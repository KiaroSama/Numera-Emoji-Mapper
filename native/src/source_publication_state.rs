//! Original publication persistence and OS locks; admitted by the reconciliation cohort.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyImportError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyclass]
struct CatalogImport {
    module: Py<PyAny>,
}
#[pymethods]
impl CatalogImport {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        _globals: &Bound<'_, PyAny>,
        _locals: &Bound<'_, PyAny>,
        fromlist: &Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        if level != 1 || name != "catalog" || fromlist.extract::<Vec<String>>()? != ["_row_to_item"]
        {
            return Err(PyImportError::new_err(
                "unexpected original publication state import",
            ));
        }
        Ok(self.module.bind(py).clone())
    }
}

pub(super) struct Persistent<'py> {
    pub state: Bound<'py, PyDict>,
    pub locks: Bound<'py, PyDict>,
}

pub(super) fn build<'py>(
    py: Python<'py>,
    sources: &[&Source],
    ports: &Bound<'py, PyDict>,
) -> PyResult<Persistent<'py>> {
    let bindings = PyDict::new(py);
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    for name in [
        "contextlib",
        "json",
        "os",
        "stat",
        "tempfile",
        "time",
        "secrets",
    ] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item("datetime", py.import("datetime")?.getattr("datetime")?)?;
    bindings.set_item("timezone", py.import("datetime")?.getattr("timezone")?)?;
    let locks = crate::source_reference_state::namespace(py, sources[20], None)?;
    bindings.set_item("writer", py.import("contextlib")?.getattr("nullcontext")?)?;
    bindings.set_item("write_json_atomic", required(&locks, "write_json_atomic")?)?;
    bindings.set_item("Image", py.import("PIL.Image")?)?;
    bindings.set_item("hashlib", py.import("hashlib")?)?;
    bindings.set_item("log", required(ports, "log")?)?;
    bindings.set_item("media", required(ports, "cr")?.getattr("media")?)?;
    bindings.set_item("SetDrift", required(ports, "cs")?.getattr("SetDrift")?)?;
    for name in [
        "PER_SET",
        "FMT_TAG",
        "MIXED",
        "DEFAULT_EMOJI",
        "BRAND_LOGO_EMOJI",
    ] {
        bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[10], name, false)?,
        )?;
    }
    let state = crate::source_oracle_runtime::namespace(
        py,
        sources[10],
        &[
            "StateError",
            "_state_path",
            "_plan_path",
            "_lock_path",
            "load_json",
            "load_plan",
            "freeze_plan",
            "_all_items",
            "save_json",
            "load_state",
            "_validate_state",
            "_usable_logo",
            "BrandLogo",
            "_static_is_blank",
        ],
        &[
            "__build_class__",
            "RuntimeError",
            "OSError",
            "ValueError",
            "Exception",
            "isinstance",
            "dict",
            "str",
            "list",
            "all",
            "bool",
            "int",
            "set",
            "enumerate",
            "sorted",
            "len",
        ],
        &bindings,
    )?;
    let module = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    module.setattr("_row_to_item", required(ports, "row_to_item")?)?;
    required(&state, "__builtins__")?.set_item(
        "__import__",
        Py::new(
            py,
            CatalogImport {
                module: module.unbind(),
            },
        )?,
    )?;
    Ok(Persistent { state, locks })
}
