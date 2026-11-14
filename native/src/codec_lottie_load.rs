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
    pub handled: bool,
    pub(crate) caught_by_exception: bool,
}

struct Closed<'py>(Option<Bound<'py, PyAny>>);
impl Drop for Closed<'_> {
    fn drop(&mut self) {
        if let Some(stream) = self.0.take() {
            let _ = stream.call_method0("close");
        }
    }
}

pub(crate) fn with_gzip<'py, T>(
    manager: Bound<'py, PyAny>,
    body: impl FnOnce(&Bound<'py, PyAny>) -> PyResult<T>,
) -> PyResult<T> {
    let py = manager.py();
    let mut owned = Closed(Some(manager));
    let manager = owned.0.as_ref().expect("owned gzip context");
    let kind = manager.get_type();
    let enter = kind.getattr("__enter__")?;
    let exit = kind.getattr("__exit__")?;
    // Enter failure/unwind retains native fallback ownership, without masking its error.
    let entered = enter.call1((manager,))?;
    let result = body(&entered);
    // Once exit is attempted, never retry a close that may already have mutated the stream.
    let manager = owned.0.take().expect("owned gzip context");
    if let Err(error) = &result {
        exit.call1((
            &manager,
            error.get_type(py),
            error.value(py),
            error.traceback(py),
        ))?;
    } else {
        exit.call1((&manager, py.None(), py.None(), py.None()))?;
    }
    // Standard GzipFile exits never suppress. A custom truthy exit cannot invent missing bytes:
    // retain the pending error rather than treating its incomplete body as a successful encoding.
    result
}

fn failure(kind: &str, message: String) -> LoadError {
    LoadError {
        kind: kind.into(),
        message,
        raw_fallback: false,
        handled: matches!(kind, "MediaError" | "OSError" | "ValueError"),
        caught_by_exception: true,
    }
}

fn external(error: PyErr, py: Python<'_>) -> LoadError {
    LoadError {
        caught_by_exception: error.is_instance_of::<pyo3::exceptions::PyException>(py),
        raw_fallback: error.is_instance_of::<PyOSError>(py)
            || error.is_instance_of::<PyValueError>(py),
        handled: error.is_instance_of::<PyOSError>(py)
            || error.is_instance_of::<PyValueError>(py)
            || error.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py),
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

pub(crate) fn source_bytes(py: Python<'_>, path: &Path) -> PyResult<Vec<u8>> {
    Ok(py
        .import("pathlib")?
        .getattr("Path")?
        .call1((path.to_path_buf(),))?
        .call_method0("read_bytes")?
        .cast_into::<PyBytes>()?
        .as_bytes()
        .to_vec())
}

fn read_source(path: &Path) -> Result<Vec<u8>, LoadError> {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return Err(LoadError {
            kind: "UnverifiedCodecError".into(),
            message: "Lottie source must be an absolute regular non-symlink file".into(),
            raw_fallback: false,
            handled: false,
            caught_by_exception: false,
        });
    }
    Python::initialize();
    Python::attach(|py| source_bytes(py, path).map_err(|error| external(error, py)))
}

pub(crate) fn decode<'py>(
    py: Python<'py>,
    raw: &[u8],
    path: &Path,
) -> Result<Bound<'py, PyAny>, LoadError> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let decoded = (|| -> PyResult<Vec<u8>> {
        if !raw.starts_with(&[0x1f, 0x8b]) {
            return Ok(raw.to_vec());
        }
        let stream = Closed(Some(
            py.import("io")?
                .call_method1("BytesIO", (PyBytes::new(py, raw),))?,
        ));
        let options = PyDict::new(py);
        options.set_item("fileobj", stream.0.as_ref().expect("open gzip input"))?;
        let gzip = py
            .import("gzip")?
            .call_method("GzipFile", (), Some(&options))?;
        let decoded = with_gzip(gzip, |entered| {
            entered.call_method1("read", (MAX_UNPACKED + 1,))
        })?;
        Ok(decoded.cast_into::<PyBytes>()?.as_bytes().to_vec())
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

pub(crate) fn load_object<'py>(
    py: Python<'py>,
    path: &Path,
) -> Result<Bound<'py, PyAny>, LoadError> {
    let raw = read_source(path)?;
    decode(py, &raw, path)
}

pub fn load(path: &Path) -> Result<Value, LoadError> {
    Python::initialize();
    Python::attach(|py| {
        let doc = load_object(py, path)?;
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
