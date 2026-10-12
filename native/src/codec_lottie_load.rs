//! Bounded TGS decoding through external gzip; document and acceptance policy remain Rust-owned.
use pyo3::exceptions::{PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyTuple};
use serde_json::Value;
use std::path::Path;

const MAX_UNPACKED: usize = 8 * 1024 * 1024;

#[derive(Debug)]
pub struct LoadError {
    pub kind: String,
    pub message: String,
    pub raw_fallback: bool,
}

struct Closed<'py>(Bound<'py, PyAny>);
impl Drop for Closed<'_> {
    fn drop(&mut self) {
        let _ = self.0.call_method0("close");
    }
}

fn failure(kind: &str, message: String) -> LoadError {
    LoadError {
        kind: kind.into(),
        message,
        raw_fallback: false,
    }
}

fn external(error: PyErr, py: Python<'_>) -> LoadError {
    LoadError {
        raw_fallback: error.is_instance_of::<PyOSError>(py)
            || error.is_instance_of::<PyValueError>(py),
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

fn read_source(path: &Path) -> Result<Vec<u8>, LoadError> {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return Err(failure(
            "ValueError",
            "Lottie source must be an absolute regular non-symlink file".into(),
        ));
    }
    std::fs::read(path).map_err(|_| failure("OSError", "cannot read Lottie source".into()))
}

fn decode<'py>(py: Python<'py>, raw: &[u8], path: &Path) -> Result<Bound<'py, PyAny>, LoadError> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let decoded = (|| -> PyResult<Vec<u8>> {
        if !raw.starts_with(&[0x1f, 0x8b]) {
            return Ok(raw.to_vec());
        }
        let stream = Closed(
            py.import("io")?
                .call_method1("BytesIO", (PyBytes::new(py, raw),))?,
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
    Ok(doc)
}

pub fn load(path: &Path) -> Result<Value, LoadError> {
    let raw = read_source(path)?;
    Python::initialize();
    Python::attach(|py| {
        let doc = decode(py, &raw, path)?;
        let encoded = py
            .import("json")
            .and_then(|module| module.call_method1("dumps", (doc,)))
            .and_then(|text| text.extract::<String>())
            .map_err(|error| external(error, py))?;
        serde_json::from_str(&encoded).map_err(|_| {
            failure(
                "CompatibilityError",
                "non-finite Lottie JSON compatibility remains unverified".into(),
            )
        })
    })
}

pub fn canonical_bytes(path: &Path) -> Result<Vec<u8>, LoadError> {
    let raw = read_source(path)?;
    Python::initialize();
    Python::attach(|py| {
        let doc = decode(py, &raw, path)?;
        // Keep the decoded format object: a serde round trip changes nonfinite and float identity.
        let encoded = (|| -> PyResult<Vec<u8>> {
            let options = PyDict::new(py);
            options.set_item("sort_keys", true)?;
            options.set_item("separators", PyTuple::new(py, [",", ":"])?)?;
            py.import("json")?
                .call_method("dumps", (doc,), Some(&options))?
                .extract::<String>()
                .map(String::into_bytes)
        })();
        encoded.map_err(|error| external(error, py))
    })
}
