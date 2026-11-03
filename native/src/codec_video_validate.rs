//! Telegram video validation preserves source refusal order and complete alpha-sample blank checks.
use crate::codec_error::CodecError;
use crate::codec_video::{VideoDecoder, VideoInfo};
use pyo3::prelude::*;
use pyo3::types::{PyInt, PyString};
use serde_json::Value;
use std::path::Path;

pub fn validate_info(info: &Value, bytes: u64, filename: &str) -> Result<(), String> {
    validate_info_typed(info, bytes, filename).map_err(|error| error.message)
}

pub fn validate_info_typed(info: &Value, bytes: u64, filename: &str) -> Result<(), CodecError> {
    let width = info["width"]
        .as_u64()
        .ok_or_else(|| CodecError::unverified("video width is unknown"))?;
    let height = info["height"]
        .as_u64()
        .ok_or_else(|| CodecError::unverified("video height is unknown"))?;
    let duration = info["duration"]
        .as_f64()
        .ok_or_else(|| CodecError::unverified("video duration is unknown"))?;
    let codec = info["codec"]
        .as_str()
        .ok_or_else(|| CodecError::unverified("video codec is unknown"))?;
    let fps = info["fps"]
        .as_f64()
        .ok_or_else(|| CodecError::unverified("video frame rate is unknown"))?;
    let audio = info["has_audio"]
        .as_bool()
        .ok_or_else(|| CodecError::unverified("video audio state is unknown"))?;
    let container = info["container"]
        .as_str()
        .ok_or_else(|| CodecError::unverified("video container is unknown"))?;
    if !duration.is_finite() || !fps.is_finite() {
        return Err(CodecError::unverified("nonfinite video metadata"));
    }
    Python::initialize();
    let info = Python::attach(|py| VideoInfo {
        width: PyInt::new(py, width).unbind(),
        height: PyInt::new(py, height).unbind(),
        duration,
        codec: PyString::new(py, codec).unbind(),
        fps,
        has_audio: audio,
        container: PyString::new(py, container).unbind(),
    });
    validate_source_info(&info, bytes, filename)
}

/// Source policy consumes lossless probe values; JSON admission above remains intentionally strict.
pub fn validate_source_info(
    info: &VideoInfo,
    bytes: u64,
    filename: &str,
) -> Result<(), CodecError> {
    Python::initialize();
    Python::attach(|py| validate_source_info_size(info, PyInt::new(py, bytes).as_any(), filename))
}

/// Preserve the source size object through comparison and diagnostic formatting.
pub fn validate_source_info_size(
    info: &VideoInfo,
    bytes: &Bound<'_, PyAny>,
    filename: &str,
) -> Result<(), CodecError> {
    let py = bytes.py();
    let problems = (|| -> PyResult<Vec<String>> {
        let mut problems = Vec::new();
        if !info.width.bind(py).as_any().eq(100)? || !info.height.bind(py).as_any().eq(100)? {
            problems.push(
                PyString::new(py, "dimensions {}x{} != 100x100")
                    .call_method1("format", (info.width.bind(py), info.height.bind(py)))?
                    .extract()?,
            );
        }
        if info.duration <= 0.0 {
            problems.push("duration is zero or unknown".into());
        } else if info.duration > 3.05 {
            problems.push(
                PyString::new(py, "duration {:.2f}s > 3.0s")
                    .call_method1("format", (info.duration,))?
                    .extract()?,
            );
        }
        if !info.codec.bind(py).as_any().eq("vp9")? {
            problems.push(
                PyString::new(py, "codec {!r} != 'vp9'")
                    .call_method1("format", (info.codec.bind(py),))?
                    .extract()?,
            );
        }
        if info.has_audio {
            problems.push("contains an audio stream (video emoji must have none)".into());
        }
        if info.fps > 30.01 {
            problems.push(
                PyString::new(py, "{:.2f} fps > 30 fps")
                    .call_method1("format", (info.fps,))?
                    .extract()?,
            );
        }
        if !info
            .container
            .bind(py)
            .call_method1("split", (",",))?
            .contains("webm")?
        {
            problems.push(
                PyString::new(py, "container {!r} is not webm")
                    .call_method1("format", (info.container.bind(py),))?
                    .extract()?,
            );
        }
        if bytes.gt(256 * 1024)? {
            problems.push(
                PyString::new(py, "size {} > 262144 bytes")
                    .call_method1("format", (bytes,))?
                    .extract()?,
            );
        }
        Ok(problems)
    })()
    .map_err(|error| CodecError::from_python(py, error))?;
    if problems.is_empty() {
        Ok(())
    } else {
        Err(CodecError::media(format!(
            "{filename}: {}",
            problems.join("; ")
        )))
    }
}

pub async fn validate(decoder: &VideoDecoder, path: &Path) -> Result<(), String> {
    let info = decoder
        .probe_core(path, crate::codec_video_error::CommandContext::Direct)
        .await?
        .map_err(crate::codec_video_error::VideoFailure::legacy)?;
    validate_probed(path, &info, SizePolicy::Legacy).map_err(|error| error.message)
}

pub async fn validate_typed(decoder: &VideoDecoder, path: &Path) -> Result<(), CodecError> {
    let info = decoder.probe_info_typed(path).await?;
    validate_probed(path, &info, SizePolicy::Source)
}

#[derive(Clone, Copy)]
enum SizePolicy {
    Legacy,
    Source,
}

fn validate_probed(path: &Path, info: &VideoInfo, policy: SizePolicy) -> Result<(), CodecError> {
    Python::initialize();
    Python::attach(|py| {
        let bytes = match policy {
            SizePolicy::Legacy => PyInt::new(
                py,
                std::fs::metadata(path)
                    .map_err(|_| CodecError::unverified("cannot inspect video size"))?
                    .len(),
            )
            .into_any(),
            // Source reads the size after its successful probe, before any refusal checks.
            SizePolicy::Source => (|| -> PyResult<_> {
                py.import("pathlib")?
                    .getattr("Path")?
                    .call1((path.to_path_buf(),))?
                    .call_method0("stat")?
                    .getattr("st_size")
            })()
            .map_err(|error| CodecError::from_python(py, error))?,
        };
        let filename = path
            .file_name()
            .ok_or_else(|| CodecError::unverified("video has no filename"))?
            .to_string_lossy();
        validate_source_info_size(info, &bytes, &filename)
    })
}

pub fn usable_at(root: Option<&Path>, path: &Path) -> Result<Value, CodecError> {
    let root =
        root.ok_or_else(|| CodecError::value("video worker requires an explicit project root"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| CodecError::unverified("cannot initialize video worker runtime"))?;
    let decoder = VideoDecoder::new(root).map_err(CodecError::unverified)?;
    runtime.block_on(async {
        validate_typed(&decoder, path).await?;
        let blank = is_blank_typed(&decoder, path).await?;
        Ok(Value::Bool(!blank))
    })
}

pub async fn is_blank(decoder: &VideoDecoder, path: &Path) -> Result<bool, String> {
    let raw = decoder.frames_rgba(path, 4).await?;
    Ok(blank_samples(&raw))
}

pub async fn is_blank_typed(decoder: &VideoDecoder, path: &Path) -> Result<bool, CodecError> {
    let raw = decoder.frames_rgba_typed(path, 4).await?;
    Ok(blank_samples(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_size_policy_keeps_python_integers_and_size_last_without_narrowing() {
        let info = VideoInfo::from_stdout(br#"{"streams":[{"codec_type":"video","width":100,"height":100,"codec_name":"vp9","avg_frame_rate":"30/1"}],"format":{"duration":1,"format_name":"webm"}}"#).unwrap();
        Python::attach(|py| {
            let integer = py.import("builtins").unwrap().getattr("int").unwrap();
            for (size, message) in [
                ("-123456789012345678901234567890", None),
                ("262144", None),
                ("262145", Some("sample.webm: size 262145 > 262144 bytes")),
                (
                    "123456789012345678901234567890",
                    Some("sample.webm: size 123456789012345678901234567890 > 262144 bytes"),
                ),
            ] {
                let size = integer.call1((size,)).unwrap();
                let result = validate_source_info_size(&info, &size, "sample.webm");
                if let Some(message) = message {
                    let error = result.unwrap_err();
                    assert_eq!(error.kind, "MediaError");
                    assert_eq!(error.message, message);
                    assert!(error.handled);
                } else {
                    result.unwrap();
                }
            }
            let error =
                validate_source_info_size(&info, PyString::new(py, "bad").as_any(), "sample.webm")
                    .unwrap_err();
            assert_eq!(error.kind, "TypeError");
            assert_eq!(
                error.message,
                "'>' not supported between instances of 'str' and 'int'"
            );
            assert!(!error.handled);
            let invalid = VideoInfo::from_stdout(br#"{"streams":[{"codec_type":"video","width":64,"height":64,"codec_name":"vp8","avg_frame_rate":"31"}],"format":{"duration":4,"format_name":"mp4"}}"#).unwrap();
            let error = validate_source_info_size(
                &invalid,
                &integer.call1(("123456789012345678901234567890",)).unwrap(),
                "sample.webm",
            )
            .unwrap_err();
            assert_eq!(
                error.message,
                "sample.webm: dimensions 64x64 != 100x100; duration 4.00s > 3.0s; codec 'vp8' != 'vp9'; 31.00 fps > 30 fps; container 'mp4' is not webm; size 123456789012345678901234567890 > 262144 bytes"
            );
            let error = validate_source_info_size(
                &invalid,
                PyString::new(py, "bad").as_any(),
                "sample.webm",
            )
            .unwrap_err();
            assert_eq!(
                error.kind, "TypeError",
                "a failed size comparison must not become a metadata refusal"
            );
            assert!(!error.handled);
        });
        validate_source_info(&info, 262144, "sample.webm").unwrap();
        assert_eq!(
            validate_source_info(&info, 262145, "sample.webm")
                .unwrap_err()
                .message,
            "sample.webm: size 262145 > 262144 bytes"
        );
    }

    #[test]
    fn source_validation_compares_nonfinite_values_without_narrowing_dimensions() {
        let metadata = |duration: &str, rate: &str| {
            VideoInfo::from_stdout(format!(
            r#"{{"streams":[{{"codec_type":"video","width":100,"height":100,"codec_name":"vp9","avg_frame_rate":"{rate}"}}],"format":{{"duration":{duration},"format_name":"webm"}}}}"#
        ).as_bytes()).unwrap()
        };
        validate_source_info(&metadata("NaN", "NaN"), 0, "sample.webm").unwrap();
        let error =
            validate_source_info(&metadata("Infinity", "Infinity"), 0, "sample.webm").unwrap_err();
        assert_eq!(error.kind, "MediaError");
        assert_eq!(
            error.message,
            "sample.webm: duration infs > 3.0s; inf fps > 30 fps"
        );
        let error =
            validate_source_info(&metadata("-Infinity", "0"), 0, "sample.webm").unwrap_err();
        assert_eq!(error.message, "sample.webm: duration is zero or unknown");
        let info = VideoInfo::from_stdout(br#"{"streams":[{"codec_type":"video","width":"-123456789012345678901234567890","height":true,"codec_name":"\ud800","avg_frame_rate":"31"}],"format":{"duration":4,"format_name":"\udfff"}}"#).unwrap();
        let error = validate_source_info(&info, 262145, "sample.webm").unwrap_err();
        assert_eq!(
            error.message,
            "sample.webm: dimensions -123456789012345678901234567890x1 != 100x100; duration 4.00s > 3.0s; codec '\\ud800' != 'vp9'; 31.00 fps > 30 fps; container '\\udfff' is not webm; size 262145 > 262144 bytes"
        );
        assert!(error.handled);
    }
}

fn blank_samples(raw: &[u8]) -> bool {
    let blank = raw
        .as_chunks::<{ crate::codec_video_timeline::FRAME_BYTES }>()
        .0
        .iter()
        .all(|frame| crate::codec_raster::blank_rgba(frame));
    !raw.is_empty() && raw.len() >= crate::codec_video_timeline::FRAME_BYTES && blank
}
