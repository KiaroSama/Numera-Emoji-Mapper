//! Bounded TGS decoding through external gzip; document and acceptance policy remain Rust-owned.
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::Value;
use std::path::Path;

const MAX_UNPACKED: usize = 8 * 1024 * 1024;

#[derive(Debug)]
pub struct LoadError {
    pub kind: String,
    pub message: String,
}

struct Closed<'py>(Bound<'py, PyAny>);
impl Drop for Closed<'_> {
    fn drop(&mut self) {
        let _ = self.0.call_method0("close");
    }
}

fn external(error: PyErr, py: Python<'_>) -> LoadError {
    LoadError {
        kind: error
            .get_type(py)
            .name()
            .map(|name| name.to_string())
            .unwrap_or_else(|_| "PythonError".into()),
        message: error
            .value(py)
            .str()
            .map(|text| text.to_string())
            .unwrap_or_else(|_| "external TGS decoder failed".into()),
    }
}

pub fn load(path: &Path) -> Result<Value, LoadError> {
    let failure = |kind: &str, message: String| LoadError {
        kind: kind.into(),
        message,
    };
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return Err(failure(
            "ValueError",
            "Lottie source must be an absolute regular non-symlink file".into(),
        ));
    }
    let raw =
        std::fs::read(path).map_err(|_| failure("OSError", "cannot read Lottie source".into()))?;
    Python::initialize();
    Python::attach(|py| {
        let decoded = (|| -> PyResult<Vec<u8>> {
            if !raw.starts_with(&[0x1f, 0x8b]) {
                return Ok(raw.clone());
            }
            let stream = Closed(
                py.import("io")?
                    .call_method1("BytesIO", (PyBytes::new(py, &raw),))?,
            );
            let options = PyDict::new(py);
            options.set_item("fileobj", &stream.0)?;
            let gzip = Closed(
                py.import("gzip")?
                    .call_method("GzipFile", (), Some(&options))?,
            );
            Ok(gzip
                .0
                .call_method1("read", (MAX_UNPACKED + 1,))?
                .cast_into::<PyBytes>()?
                .as_bytes()
                .to_vec())
        })()
        .map_err(|error| external(error, py))?;
        if raw.starts_with(&[0x1f, 0x8b]) && decoded.len() > MAX_UNPACKED {
            return Err(failure(
                "MediaError",
                format!("{name}: Lottie expands beyond {MAX_UNPACKED} bytes"),
            ));
        }
        let doc = (|| -> PyResult<Bound<'_, PyAny>> {
            let text = PyBytes::new(py, &decoded).call_method1("decode", ("utf-8",))?;
            py.import("json")?.call_method1("loads", (text,))
        })()
        .map_err(|error| external(error, py))?;
        if !doc.is_instance_of::<PyDict>() {
            let kind = doc
                .get_type()
                .name()
                .map(|name| name.to_string())
                .unwrap_or_else(|_| "unknown".into());
            return Err(failure(
                "MediaError",
                format!("{name}: Lottie root is {kind}, expected an object"),
            ));
        }
        let encoded = py
            .import("json")
            .and_then(|module| module.call_method1("dumps", (doc,)))
            .and_then(|text| text.extract::<String>())
            .map_err(|error| external(error, py))?;
        serde_json::from_str(&encoded).map_err(|_| {
            failure(
                "ValueError",
                "non-finite Lottie JSON compatibility remains unverified".into(),
            )
        })
    })
}
