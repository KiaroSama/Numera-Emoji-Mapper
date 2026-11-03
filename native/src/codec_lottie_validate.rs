//! Rust validation policy over external decoded JSON; no lossy serde round trip for file inputs.
use crate::codec_error::CodecError;
use crate::codec_lottie_load::source_bytes;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
use serde_json::Value;

fn external<T>(py: Python<'_>, result: PyResult<T>) -> Result<T, CodecError> {
    result.map_err(|error| CodecError::from_python(py, error))
}

pub(crate) fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}

fn items<'py>(
    py: Python<'py>,
    value: Option<Bound<'py, PyAny>>,
) -> Result<Vec<Bound<'py, PyAny>>, CodecError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if !external(py, value.is_truthy())? {
        return Ok(Vec::new());
    }
    external(py, value.try_iter())?
        .map(|value| external(py, value))
        .collect()
}

pub fn validate_file(path: &std::path::Path) -> Result<(), String> {
    validate_file_typed(path).map_err(|error| error.message)
}

pub fn validate_file_typed(path: &std::path::Path) -> Result<(), CodecError> {
    Python::initialize();
    let filename = path
        .file_name()
        .ok_or_else(|| CodecError::value("TGS source has no filename"))?
        .to_string_lossy();
    Python::attach(|py| {
        let size = external(
            py,
            (|| -> PyResult<_> {
                py.import("pathlib")?
                    .getattr("Path")?
                    .call1((path.to_path_buf(),))?
                    .call_method0("stat")?
                    .getattr("st_size")
            })(),
        )?;
        validate_size(py, &size, &filename)?;
        let magic = external(py, source_bytes(py, path))?;
        if !magic.starts_with(&[0x1f, 0x8b]) {
            return Err(CodecError::media(format!(
                "{filename}: TGS must be gzip-compressed Lottie"
            )));
        }
        // The second read is inside the original loader's narrower catch;
        // stat/magic errors above retain their actual filesystem exception.
        let raw = source_bytes(py, path).map_err(|error| {
            let caught = error.is_instance_of::<pyo3::exceptions::PyOSError>(py)
                || error.is_instance_of::<pyo3::exceptions::PyValueError>(py);
            let error = CodecError::from_python(py, error);
            if caught {
                CodecError::media(format!("{filename}: not a valid TGS/Lottie ({error})"))
            } else {
                error
            }
        })?;
        validate_bytes(py, &raw, path, &filename)
    })
}

pub(crate) fn validate_encoded(
    py: Python<'_>,
    raw: &[u8],
    path: &std::path::Path,
    compressed_size: u64,
) -> Result<(), CodecError> {
    let filename = path
        .file_name()
        .ok_or_else(|| CodecError::value("TGS source has no filename"))?
        .to_string_lossy();
    validate_size(
        py,
        pyo3::types::PyInt::new(py, compressed_size).as_any(),
        &filename,
    )?;
    validate_bytes(py, raw, path, &filename)
}

fn validate_size(
    py: Python<'_>,
    size: &Bound<'_, PyAny>,
    filename: &str,
) -> Result<(), CodecError> {
    if external(py, size.gt(65536))? {
        let message = external(
            py,
            PyString::new(py, "{}: TGS {} > 65536 bytes").call_method1("format", (filename, size)),
        )?;
        return Err(CodecError::media(external(
            py,
            message.extract::<String>(),
        )?));
    }
    Ok(())
}

fn validate_bytes(
    py: Python<'_>,
    raw: &[u8],
    path: &std::path::Path,
    filename: &str,
) -> Result<(), CodecError> {
    let document = crate::codec_lottie_load::decode(py, raw, path).map_err(|error| {
        if error.raw_fallback {
            CodecError::media(format!(
                "{filename}: not a valid TGS/Lottie ({})",
                error.message
            ))
        } else {
            CodecError {
                kind: error.kind,
                message: error.message,
                handled: error.handled,
            }
        }
    })?;
    validate_loaded(py, &document, filename)
}

pub fn validate_document(data: &Value, filename: &str, compressed_size: u64) -> Result<(), String> {
    validate_document_typed(data, filename, compressed_size).map_err(|error| error.message)
}

pub fn validate_document_typed(
    data: &Value,
    filename: &str,
    compressed_size: u64,
) -> Result<(), CodecError> {
    Python::initialize();
    Python::attach(|py| {
        let document = external(py, py.import("json"))?;
        let document = external(py, document.call_method1("loads", (data.to_string(),)))?;
        validate_size(
            py,
            pyo3::types::PyInt::new(py, compressed_size).as_any(),
            filename,
        )?;
        validate_loaded(py, &document, filename)
    })
}

fn validate_loaded(
    py: Python<'_>,
    data: &Bound<'_, PyAny>,
    filename: &str,
) -> Result<(), CodecError> {
    let object = match data.cast::<PyDict>() {
        Ok(object) => object,
        Err(_) => {
            let kind = external(py, data.get_type().name())?;
            return Err(CodecError::media(format!(
                "{filename}: Lottie root is {kind}, expected an object"
            )));
        }
    };
    for key in ["v", "fr", "ip", "op", "layers"] {
        if !external(py, object.contains(key))? {
            return Err(CodecError::media(format!(
                "{filename}: Lottie missing required key '{key}'"
            )));
        }
    }
    let w = external(py, object.get_item("w"))?.unwrap_or_else(|| py.None().into_bound(py));
    let h = external(py, object.get_item("h"))?.unwrap_or_else(|| py.None().into_bound(py));
    if !external(py, w.eq(512))? || !external(py, h.eq(512))? {
        let w = external(py, w.str())?;
        let h = external(py, h.str())?;
        return Err(CodecError::media(format!(
            "{filename}: canvas {w}x{h}, expected 512x512"
        )));
    }
    let mut values = Vec::with_capacity(3);
    let mut originals = Vec::with_capacity(3);
    let float = external(py, py.import("builtins"))?;
    let float = external(py, float.getattr("float"))?;
    for key in ["fr", "ip", "op"] {
        let value = external(py, object.get_item(key))?
            .ok_or_else(|| CodecError::value("required timeline field disappeared"))?;
        let number = float.call1((&value,)).map_err(|error| {
            let caught = error.is_instance_of::<pyo3::exceptions::PyTypeError>(py)
                || error.is_instance_of::<pyo3::exceptions::PyValueError>(py);
            let error = CodecError::from_python(py, error);
            if caught {
                CodecError::media(format!("{filename}: non-numeric fr/ip/op ({error})"))
            } else {
                error
            }
        })?;
        values.push(external(py, number.extract::<f64>())?);
        originals.push(value);
    }
    let (fr, ip, op) = (values[0], values[1], values[2]);
    let frames = op - ip;
    for (name, value) in ["fr", "ip", "op"].into_iter().zip(&values) {
        if !value.is_finite() {
            let value = external(py, float.call1((*value,)))?;
            let value = external(py, value.str())?;
            return Err(CodecError::media(format!(
                "{filename}: {name} is {value}, not a finite number"
            )));
        }
    }
    if fr <= 0.0 || fr > 60.0 {
        let value = external(py, float.call1((fr,)))?;
        let value = external(py, value.str())?;
        return Err(CodecError::media(if fr <= 0.0 {
            format!("{filename}: frame rate {value} must be positive")
        } else {
            format!("{filename}: {value} fps > 60 fps")
        }));
    }
    let duration = frames / fr;
    if duration <= 0.0 {
        let ip = external(py, originals[1].str())?;
        let op = external(py, originals[2].str())?;
        return Err(CodecError::media(format!(
            "{filename}: empty timeline (ip={ip}, op={op})"
        )));
    }
    if duration > 3.01 {
        // Formatting is part of the source error contract, including tie rounding.
        let text = PyString::new(py, "{:.2f}");
        let text = external(py, text.call_method1("format", (duration,)))?;
        return Err(CodecError::media(format!("{filename}: {text}s > 3.0s")));
    }
    let mut layers = items(py, external(py, object.get_item("layers"))?)?;
    for asset in items(py, external(py, object.get_item("assets"))?)? {
        if let Ok(asset) = asset.cast::<PyDict>() {
            layers.extend(items(py, external(py, asset.get_item("layers"))?)?);
        }
    }
    for layer in layers {
        let Ok(layer) = layer.cast::<PyDict>() else {
            continue;
        };
        for mask in items(py, external(py, layer.get_item("masksProperties"))?)? {
            let mode = external(py, mask.call_method1("get", ("mode",)))?;
            if external(py, mode.eq("s"))? {
                let name = external(py, layer.get_item("nm"))?
                    .unwrap_or_else(|| PyString::new(py, "?").into_any());
                let name = external(py, name.repr())?;
                return Err(CodecError::media(format!(
                    "{filename}: layer {name} uses a SUBTRACT mask, which Telegram refuses on upload (add masks are fine). Re-export without it."
                )));
            }
        }
    }
    Ok(())
}
