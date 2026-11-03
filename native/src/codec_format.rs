//! Source media sniffing and collision identities, distinct from local target-format selection.
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub fn detect_bytes(head: &[u8]) -> &'static str {
    if head.starts_with(b"\x1a\x45\xdf\xa3") {
        "video"
    } else if head.starts_with(b"\x1f\x8b") {
        "animated"
    } else if head.starts_with(b"\x89PNG\r\n\x1a\n")
        || head.starts_with(b"GIF87a")
        || head.starts_with(b"GIF89a")
        || (head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP"))
    {
        "static"
    } else {
        "unknown"
    }
}

fn header(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(16).read_to_end(&mut bytes))
        .map_err(|_| "cannot read media header")?;
    Ok(bytes)
}

fn header_external(path: &Path) -> pyo3::PyResult<Vec<u8>> {
    use pyo3::prelude::*;
    struct OpenFile<'py>(Option<Bound<'py, PyAny>>);
    impl Drop for OpenFile<'_> {
        fn drop(&mut self) {
            if let Some(file) = &self.0 {
                let _ = file.call_method0("close");
            }
        }
    }
    Python::initialize();
    Python::attach(|py| {
        (|| -> PyResult<Vec<u8>> {
            let file = py
                .import("builtins")?
                .getattr("open")?
                .call1((path.to_path_buf(), "rb"))?;
            let mut guard = OpenFile(Some(file.clone()));
            let kind = file.get_type();
            let enter = kind.getattr("__enter__")?;
            let exit = kind.getattr("__exit__")?;
            let stream = enter.call1((&file,))?;
            let result = stream.call_method1("read", (16,));
            // Once explicit exit is attempted, a failed close must not be retried by Drop.
            let _ = guard.0.take();
            if let Err(error) = &result {
                exit.call1((
                    &file,
                    error.get_type(py),
                    error.value(py),
                    error.traceback(py),
                ))?;
            } else {
                exit.call1((&file, py.None(), py.None(), py.None()))?;
            }
            Ok(result?
                .cast_into::<pyo3::types::PyBytes>()?
                .as_bytes()
                .to_vec())
        })()
    })
}

fn header_typed(path: &Path) -> Result<Vec<u8>, crate::codec_error::CodecError> {
    header_external(path).map_err(|error| {
        pyo3::Python::attach(|py| crate::codec_error::CodecError::from_python(py, error))
    })
}

pub fn detect(path: &Path) -> Result<&'static str, String> {
    let head = header(path)?;
    Ok(detected(path, &head))
}

pub(crate) fn detect_external(path: &Path) -> pyo3::PyResult<&'static str> {
    Ok(detected(path, &header_external(path)?))
}

pub fn detect_typed(path: &Path) -> Result<&'static str, crate::codec_error::CodecError> {
    detect_external(path).map_err(|error| {
        pyo3::Python::attach(|py| crate::codec_error::CodecError::from_python(py, error))
    })
}

fn detected(path: &Path, head: &[u8]) -> &'static str {
    let format = detect_bytes(head);
    if format != "unknown" {
        return format;
    }
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_lowercase();
    match extension.as_str() {
        "tgs" => "animated",
        "webm" | "mp4" | "mov" | "mkv" | "m4v" | "avi" => "video",
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "apng" | "svg" => "static",
        _ => "unknown",
    }
}

pub fn extension(path: &Path, format: &str) -> Result<&'static str, String> {
    match format {
        "video" => return Ok(".webm"),
        "animated" => return Ok(".tgs"),
        _ => {}
    }
    Ok(static_extension(&header(path)?))
}

pub fn extension_typed(
    path: &Path,
    format: &str,
) -> Result<&'static str, crate::codec_error::CodecError> {
    match format {
        "video" => Ok(".webm"),
        "animated" => Ok(".tgs"),
        _ => Ok(static_extension(&header_typed(path)?)),
    }
}

fn static_extension(head: &[u8]) -> &'static str {
    if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        ".webp"
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        ".gif"
    } else {
        ".png"
    }
}

pub fn collision_key(path: &Path, sampled_key: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|_| "cannot read collision identity source")?;
    Ok(collision_bytes(&bytes, sampled_key))
}

pub fn collision_key_typed(
    path: &Path,
    sampled_key: &str,
) -> Result<String, crate::codec_error::CodecError> {
    pyo3::Python::initialize();
    pyo3::Python::attach(|py| {
        crate::codec_lottie_load::source_bytes(py, path)
            .map(|bytes| collision_bytes(&bytes, sampled_key))
            .map_err(|error| crate::codec_error::CodecError::from_python(py, error))
    })
}

pub fn collision_request(
    path: &Path,
    key: &serde_json::Value,
) -> Result<serde_json::Value, crate::codec_error::CodecError> {
    let key = key
        .as_str()
        .filter(|key| !key.is_empty())
        .ok_or_else(|| crate::codec_error::CodecError::value("collision requires a content key"))?;
    collision_key_typed(path, key).map(serde_json::Value::String)
}

fn collision_bytes(bytes: &[u8], sampled_key: &str) -> String {
    let (prefix, remainder) = sampled_key.split_once(':').unwrap_or((sampled_key, ""));
    let lookup = remainder.rsplit(':').next().unwrap_or("");
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    format!("{prefix}:{}:{lookup}", &digest[..32])
}
