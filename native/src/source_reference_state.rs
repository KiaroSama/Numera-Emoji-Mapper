//! Shared original state writer and OS-lock namespace; callers admit its fixed source first.
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::path::Path;

#[pyclass]
struct OwnPid;
#[pymethods]
impl OwnPid {
    fn __call__(&self, pid: u32) -> PyResult<bool> {
        if pid != std::process::id() {
            return Err(PyValueError::new_err(
                "foreign original lock diagnostic PID",
            ));
        }
        Ok(true)
    }
}

pub(crate) fn ownership<'py>(
    py: Python<'py>,
    source: &Source,
    state: &Bound<'py, PyDict>,
    name: &str,
) -> PyResult<Bound<'py, PyModule>> {
    let values = PyDict::new(py);
    values.set_item("os", py.import("os")?)?;
    values.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    values.set_item(
        "contextmanager",
        py.import("contextlib")?.getattr("contextmanager")?,
    )?;
    values.set_item(
        "_owners",
        py.import("threading")?.getattr("local")?.call0()?,
    )?;
    for key in ["LockBusy", "exclusive_lock"] {
        values.set_item(key, crate::source_tooling_oracle::required(state, key)?)?;
    }
    values.set_item(
        "JOURNAL_NAME",
        crate::source_logging_oracle::literal(py, source, "JOURNAL_NAME", false)?,
    )?;
    crate::source_oracle_runtime::module(
        py,
        source,
        &[
            "canonical_directory",
            "lock_path",
            "_ownership",
            "writer",
            "maintenance",
        ],
        &["str", "getattr"],
        &values,
        name,
    )
}

pub(crate) fn namespace<'py>(
    py: Python<'py>,
    source: &Source,
    lock_dir: Option<&Path>,
) -> PyResult<Bound<'py, PyDict>> {
    let bindings = PyDict::new(py);
    let path = py.import("pathlib")?.getattr("Path")?;
    bindings.set_item("Path", &path)?;
    for name in [
        "contextlib",
        "json",
        "os",
        "stat",
        "tempfile",
        "time",
        "secrets",
        "re",
    ] {
        bindings.set_item(name, py.import(name)?)?;
    }
    if let Some(directory) = lock_dir {
        bindings.set_item("LOCK_DIR", path.call1((directory,))?)?;
    }
    bindings.set_item("datetime", py.import("datetime")?.getattr("datetime")?)?;
    bindings.set_item("timezone", py.import("datetime")?.getattr("timezone")?)?;
    bindings.set_item("_lock_owner_is_alive", Py::new(py, OwnPid)?)?;
    for name in [
        "LOCK_STALE_AFTER",
        "_LOCK_BYTE",
        "_RECORD_WIDTH",
        "_REPLACE_DEADLINE",
    ] {
        bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, source, name, false)?,
        )?;
    }
    let platform: String = py.import("os")?.getattr("name")?.extract()?;
    let lock_module = if platform == "nt" { "msvcrt" } else { "fcntl" };
    bindings.set_item(lock_module, py.import(lock_module)?)?;
    crate::source_oracle_runtime::namespace(
        py,
        source,
        &[
            "LockBusy",
            "StateInvalid",
            "_intent_key",
            "validate_state_shape",
            "make_intent",
            "pack_family_lock_path",
            "canonical_map_lock",
            "exclusive_lock",
            "_take_os_lock",
            "_drop_os_lock",
            "_busy_message",
            "_utc_now",
            "_replace",
            "write_json_atomic",
        ],
        &[
            "__build_class__",
            "RuntimeError",
            "BaseException",
            "OSError",
            "ValueError",
            "TypeError",
            "PermissionError",
            "FileNotFoundError",
            "str",
            "int",
            "min",
            "isinstance",
            "dict",
            "list",
            "all",
            "set",
            "enumerate",
        ],
        &bindings,
    )
}
