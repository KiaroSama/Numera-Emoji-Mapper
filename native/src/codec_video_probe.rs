//! Lossless FFprobe metadata; the source's Python values survive until a consumer chooses policy.
use crate::codec_error::CodecError;
use crate::codec_video_error::VideoError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyInt, PyList, PyString};
use serde_json::Value;

/// Unlike JSON Value, these fields retain signed big integers, lone surrogates and NaN/infinity.
#[derive(Debug)]
pub struct VideoInfo {
    pub width: Py<PyInt>,
    pub height: Py<PyInt>,
    pub duration: f64,
    pub codec: Py<PyString>,
    pub fps: f64,
    pub has_audio: bool,
    pub container: Py<PyString>,
}

fn or_zero<'py>(value: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if value.is_truthy()? {
        Ok(value)
    } else {
        Ok(PyInt::new(value.py(), 0).into_any())
    }
}

fn fps<'py>(py: Python<'py>, rate: &Bound<'py, PyAny>) -> PyResult<f64> {
    let result = (|| {
        let parts = rate.call_method1("partition", ("/",))?;
        let numerator = parts.get_item(0)?;
        let denominator = parts.get_item(2)?;
        let numerator = py
            .import("builtins")?
            .getattr("float")?
            .call1((numerator,))?;
        let denominator = if denominator.is_truthy()? {
            denominator
        } else {
            PyInt::new(py, 1).into_any()
        };
        let denominator = py
            .import("builtins")?
            .getattr("float")?
            .call1((denominator,))?;
        numerator.div(denominator)?.extract::<f64>()
    })();
    match result {
        Err(error)
            if error.is_instance_of::<pyo3::exceptions::PyTypeError>(py)
                || error.is_instance_of::<pyo3::exceptions::PyValueError>(py)
                || error.is_instance_of::<pyo3::exceptions::PyZeroDivisionError>(py) =>
        {
            Ok(0.0)
        }
        result => result,
    }
}

fn parse(py: Python<'_>, stdout: &[u8]) -> PyResult<VideoInfo> {
    let text = PyBytes::new(py, stdout).call_method1("decode", ("utf-8", "replace"))?;
    let data = py.import("json")?.getattr("loads")?.call1((text,))?;
    let streams = data.call_method1("get", ("streams",))?;
    let streams = if streams.is_truthy()? {
        streams
    } else {
        PyList::empty(py).into_any()
    };
    let mut video = PyDict::new(py).into_any();
    for stream in streams.try_iter()? {
        let stream = stream?;
        if stream.call_method1("get", ("codec_type",))?.eq("video")? {
            video = stream;
            break;
        }
    }
    // The source makes a second pass; finding a video must not conceal a later malformed stream.
    let mut has_audio = false;
    for stream in streams.try_iter()? {
        if stream?.call_method1("get", ("codec_type",))?.eq("audio")? {
            has_audio = true;
            break;
        }
    }
    let format = data.call_method1("get", ("format", PyDict::new(py)))?;
    let width = py
        .import("builtins")?
        .getattr("int")?
        .call1((or_zero(video.call_method1("get", ("width", 0))?)?,))?
        .cast_into::<PyInt>()?
        .unbind();
    let height = py
        .import("builtins")?
        .getattr("int")?
        .call1((or_zero(video.call_method1("get", ("height", 0))?)?,))?
        .cast_into::<PyInt>()?
        .unbind();
    let duration = py
        .import("builtins")?
        .getattr("float")?
        .call1((or_zero(format.call_method1("get", ("duration", 0))?)?,))?
        .extract::<f64>()?;
    let codec = py
        .import("builtins")?
        .getattr("str")?
        .call1((video.call_method1("get", ("codec_name", ""))?,))?
        .cast_into::<PyString>()?
        .unbind();
    let rate = py
        .import("builtins")?
        .getattr("str")?
        .call1((video.call_method1("get", ("avg_frame_rate", "0/1"))?,))?;
    let fps = fps(py, &rate)?;
    let container = py
        .import("builtins")?
        .getattr("str")?
        .call1((format.call_method1("get", ("format_name", ""))?,))?
        .cast_into::<PyString>()?
        .unbind();
    Ok(VideoInfo {
        width,
        height,
        duration,
        codec,
        fps,
        has_audio,
        container,
    })
}

pub(crate) fn parse_stdout(stdout: &[u8]) -> Result<VideoInfo, VideoError> {
    Python::initialize();
    Python::attach(|py| parse(py, stdout).map_err(|error| VideoError::from_python(py, error)))
}

impl VideoInfo {
    /// Parse the same UTF-8-replacement/JSON/coercion contract used by a direct source probe.
    pub fn from_stdout(stdout: &[u8]) -> Result<Self, CodecError> {
        parse_stdout(stdout).map_err(|error| error.error)
    }

    /// Compatibility only: unrepresentable Python values are errors, never fabricated JSON nulls.
    pub fn to_value(&self) -> Result<Value, CodecError> {
        let unrepresentable =
            || CodecError::unverified("video metadata cannot be represented losslessly as JSON");
        if !self.duration.is_finite() || !self.fps.is_finite() {
            return Err(unrepresentable());
        }
        Python::initialize();
        let (width, height, codec, container) = Python::attach(|py| {
            (|| -> PyResult<_> {
                Ok((
                    self.width.bind(py).str()?.to_str()?.to_owned(),
                    self.height.bind(py).str()?.to_str()?.to_owned(),
                    self.codec.bind(py).to_str()?.to_owned(),
                    self.container.bind(py).to_str()?.to_owned(),
                ))
            })()
            .map_err(|error| {
                if error.is_instance_of::<pyo3::exceptions::PyUnicodeEncodeError>(py) {
                    unrepresentable()
                } else {
                    CodecError::from_python(py, error)
                }
            })
        })?;
        let mut value = serde_json::Map::new();
        value.insert(
            "width".into(),
            Value::Number(width.parse().map_err(|_| unrepresentable())?),
        );
        value.insert(
            "height".into(),
            Value::Number(height.parse().map_err(|_| unrepresentable())?),
        );
        value.insert("codec".into(), Value::String(codec));
        value.insert(
            "duration".into(),
            Value::Number(serde_json::Number::from_f64(self.duration).ok_or_else(unrepresentable)?),
        );
        value.insert(
            "fps".into(),
            Value::Number(serde_json::Number::from_f64(self.fps).ok_or_else(unrepresentable)?),
        );
        value.insert("has_audio".into(), Value::Bool(self.has_audio));
        value.insert("container".into(), Value::String(container));
        Ok(Value::Object(value))
    }

    pub(crate) fn alpha_decoder(&self) -> Result<Option<&'static str>, VideoError> {
        Python::initialize();
        Python::attach(|py| {
            let codec = self.codec.bind(py).call_method0("lower")?;
            if !codec.is_truthy()? {
                return Ok(None);
            }
            Ok(Some(if codec.eq("vp9")? {
                "libvpx-vp9"
            } else if codec.eq("vp8")? {
                "libvpx"
            } else {
                ""
            }))
        })
        .map_err(|error| Python::attach(|py| VideoError::from_python(py, error)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn probe_coercion_keeps_defaults_signed_integers_and_actual_python_strings() {
        let empty = VideoInfo::from_stdout(b"{}").unwrap().to_value().unwrap();
        assert_eq!(
            empty,
            json!({"width":0,"height":0,"duration":0.0,"codec":"",
            "fps":0.0,"has_audio":false,"container":""})
        );
        let info = VideoInfo::from_stdout(
            br#"{"streams":[{"codec_type":"video",
            "width":"-123456789012345678901234567890","height":true,"codec_name":null,
            "avg_frame_rate":" 3_0 / 2 "},{"codec_type":"audio"}],
            "format":{"duration":2,"format_name":["webm"]}}"#,
        )
        .unwrap();
        let projected = info.to_value().unwrap();
        assert_eq!(
            projected["width"].to_string(),
            "-123456789012345678901234567890"
        );
        assert_eq!(projected["height"], 1);
        assert_eq!(projected["codec"], "None");
        assert_eq!(projected["container"], "['webm']");
        assert_eq!(projected["duration"], 2.0);
        assert_eq!(projected["fps"], 15.0);
        assert_eq!(projected["has_audio"], true);
    }

    #[test]
    fn malformed_shapes_and_coercions_raise_the_first_actual_source_exception() {
        for (raw, kind, message) in [
            (
                "null",
                "AttributeError",
                "'NoneType' object has no attribute 'get'",
            ),
            (
                "[]",
                "AttributeError",
                "'list' object has no attribute 'get'",
            ),
            (
                r#"{"streams":1}"#,
                "TypeError",
                "'int' object is not iterable",
            ),
            (
                r#"{"streams":[null]}"#,
                "AttributeError",
                "'NoneType' object has no attribute 'get'",
            ),
            (
                r#"{"format":null}"#,
                "AttributeError",
                "'NoneType' object has no attribute 'get'",
            ),
            (
                r#"{"streams":[{"codec_type":"video","width":"bad","height":[]}],"format":null}"#,
                "ValueError",
                "invalid literal for int() with base 10: 'bad'",
            ),
            (
                r#"{"streams":[{"codec_type":"video","width":[1]}]}"#,
                "TypeError",
                "int() argument must be a string, a bytes-like object or a real number, not 'list'",
            ),
            (
                r#"{"streams":[{"codec_type":"video","height":"height first"}],"format":{"duration":"duration later"}}"#,
                "ValueError",
                "invalid literal for int() with base 10: 'height first'",
            ),
            (
                r#"{"streams":[{"codec_type":"video","width":"width later"},null]}"#,
                "AttributeError",
                "'NoneType' object has no attribute 'get'",
            ),
            (
                r#"{"streams":[{"codec_type":"video","width":Infinity}]}"#,
                "OverflowError",
                "cannot convert float infinity to integer",
            ),
            (
                r#"{"streams":[{"codec_type":"video","width":NaN}]}"#,
                "ValueError",
                "cannot convert float NaN to integer",
            ),
            (
                r#"{"format":{"duration":"bad"}}"#,
                "ValueError",
                "could not convert string to float: 'bad'",
            ),
            (
                r#"{"format":{"duration":[1]}}"#,
                "TypeError",
                "float() argument must be a string or a real number, not 'list'",
            ),
        ] {
            let error = parse_stdout(raw.as_bytes()).unwrap_err();
            assert_eq!(error.error.kind, kind, "{raw}");
            assert_eq!(error.error.message, message, "{raw}");
            assert!(error.caught_by_exception, "{raw}");
            assert_eq!(error.error.handled, kind == "ValueError", "{raw}");
        }
        for raw in [b"{".as_slice(), b"\xef\xbb\xbf{}".as_slice()] {
            let error = VideoInfo::from_stdout(raw).unwrap_err();
            assert_eq!(error.kind, "JSONDecodeError");
            assert!(error.handled);
        }
        // next(video) stops at the first video, but any(audio) still sees the bad tail.
        let error =
            VideoInfo::from_stdout(br#"{"streams":[{"codec_type":"video"},null]}"#).unwrap_err();
        assert_eq!(error.kind, "AttributeError");
        // Both passes stop before null when the first audio and video have already been found.
        let info = VideoInfo::from_stdout(br#"{"streams":[{"codec_type":"audio"},{"codec_type":"video","width":-2.9,"height":false},null]}"#).unwrap();
        assert_eq!(info.to_value().unwrap()["width"], -2);
        assert!(info.has_audio);
        for raw in [
            br#"{"streams":null}"#.as_slice(),
            br#"{"streams":0}"#,
            br#"{"streams":{}}"#,
            br#"{"streams":""}"#,
        ] {
            assert_eq!(
                VideoInfo::from_stdout(raw).unwrap().to_value().unwrap()["width"],
                0
            );
        }
    }

    #[test]
    fn fps_uses_python_conversion_division_and_its_three_narrow_catches() {
        for (rate, expected) in [
            ("30", 30.0),
            ("30/", 30.0),
            (" 3_0 / ٢ ", 15.0),
            ("30/0", 0.0),
            ("30/-0", 0.0),
            ("1/2/3", 0.0),
            ("", 0.0),
            ("bad", 0.0),
            ("1/Infinity", 0.0),
            ("-0/1", -0.0),
            ("1e1000/1", f64::INFINITY),
        ] {
            let raw = serde_json::json!({"streams":[{"codec_type":"video","avg_frame_rate":rate}]})
                .to_string();
            let observed = VideoInfo::from_stdout(raw.as_bytes()).unwrap().fps;
            assert_eq!(observed.to_bits(), expected.to_bits(), "{rate}");
        }
        for rate in ["NaN", "Infinity/Infinity"] {
            let raw = serde_json::json!({"streams":[{"codec_type":"video","avg_frame_rate":rate}]})
                .to_string();
            assert!(VideoInfo::from_stdout(raw.as_bytes()).unwrap().fps.is_nan());
        }
        Python::attach(|py| {
            assert_eq!(
                fps(py, &py.None().into_bound(py))
                    .unwrap_err()
                    .get_type(py)
                    .name()
                    .unwrap(),
                "AttributeError"
            );
        });
    }

    #[test]
    fn actual_parser_and_fps_faults_keep_exception_membership_and_narrow_catches() {
        use pyo3::types::PyCFunction;
        fn fault(kind: &str) -> PyErr {
            match kind {
                "TypeError" => pyo3::exceptions::PyTypeError::new_err("probe fault"),
                "ValueError" => pyo3::exceptions::PyValueError::new_err("probe fault"),
                "ZeroDivisionError" => {
                    pyo3::exceptions::PyZeroDivisionError::new_err("probe fault")
                }
                "KeyboardInterrupt" => {
                    pyo3::exceptions::PyKeyboardInterrupt::new_err("probe fault")
                }
                _ => pyo3::exceptions::PySystemExit::new_err("probe fault"),
            }
        }
        Python::initialize();
        Python::attach(|py| {
            for kind in [
                "TypeError",
                "ValueError",
                "ZeroDivisionError",
                "KeyboardInterrupt",
                "SystemExit",
            ] {
                let replacement =
                    PyCFunction::new_closure(py, None, None, move |_, _| -> PyResult<()> {
                        Err(fault(kind))
                    })
                    .unwrap();
                let json = py.import("json").unwrap();
                let original = json.getattr("loads").unwrap();
                json.setattr("loads", &replacement).unwrap();
                let result = parse_stdout(b"{}");
                json.setattr("loads", original).unwrap();
                let error = result.unwrap_err();
                assert_eq!(error.error.kind, kind);
                assert_eq!(error.error.message, "probe fault");
                assert_eq!(
                    error.caught_by_exception,
                    !matches!(kind, "KeyboardInterrupt" | "SystemExit")
                );
                assert_eq!(error.error.handled, kind == "ValueError");
                let builtins = py.import("builtins").unwrap();
                let original = builtins.getattr("float").unwrap();
                builtins.setattr("float", replacement).unwrap();
                let result = fps(py, PyString::new(py, "1/2").as_any());
                builtins.setattr("float", original).unwrap();
                if matches!(kind, "KeyboardInterrupt" | "SystemExit") {
                    assert_eq!(result.unwrap_err().get_type(py).name().unwrap(), kind);
                } else {
                    assert_eq!(result.unwrap(), 0.0);
                }
            }
        });
    }

    #[test]
    fn source_values_survive_even_when_json_projection_cannot_represent_them() {
        for duration in ["NaN", "Infinity", "-Infinity", "1e1000"] {
            let raw = format!(r#"{{"format":{{"duration":{duration}}}}}"#);
            let info = VideoInfo::from_stdout(raw.as_bytes()).unwrap();
            assert!(!info.duration.is_finite());
            let error = info.to_value().unwrap_err();
            assert_eq!(error.kind, "UnverifiedCodecError");
            assert!(!error.handled);
        }
        for (duration, expected) in [
            ("null", 0.0),
            ("false", 0.0),
            ("true", 1.0),
            ("[]", 0.0),
            ("\"\"", 0.0),
        ] {
            let raw = format!(r#"{{"format":{{"duration":{duration}}}}}"#);
            assert_eq!(
                VideoInfo::from_stdout(raw.as_bytes()).unwrap().duration,
                expected
            );
        }
        let negative_zero = VideoInfo::from_stdout(br#"{"format":{"duration":"-0.0"}}"#).unwrap();
        assert_eq!(negative_zero.duration.to_bits(), (-0.0f64).to_bits());
        let info = VideoInfo::from_stdout(br#"{"streams":[{"codec_type":"video","codec_name":"\ud800"}],"format":{"format_name":"\udfff"}}"#).unwrap();
        Python::attach(|py| {
            assert_eq!(
                info.codec
                    .bind(py)
                    .call_method1("encode", ("unicode_escape",))
                    .unwrap()
                    .extract::<Vec<u8>>()
                    .unwrap(),
                b"\\ud800"
            );
            assert_eq!(
                info.container
                    .bind(py)
                    .call_method1("encode", ("unicode_escape",))
                    .unwrap()
                    .extract::<Vec<u8>>()
                    .unwrap(),
                b"\\udfff"
            );
        });
        assert_eq!(info.alpha_decoder().unwrap(), Some(""));
        assert_eq!(info.to_value().unwrap_err().kind, "UnverifiedCodecError");
        let info = VideoInfo::from_stdout(
            b"{\"streams\":[{\"codec_type\":\"video\",\"codec_name\":\"\xff\"}]}",
        )
        .unwrap();
        assert_eq!(info.to_value().unwrap()["codec"], "�");
        for (codec, decoder) in [
            ("VP9", Some("libvpx-vp9")),
            ("Vp8", Some("libvpx")),
            ("h264", Some("")),
            ("", None),
        ] {
            let raw = serde_json::json!({"streams":[{"codec_type":"video","codec_name":codec}]})
                .to_string();
            assert_eq!(
                VideoInfo::from_stdout(raw.as_bytes())
                    .unwrap()
                    .alpha_decoder()
                    .unwrap(),
                decoder
            );
        }
    }
}
