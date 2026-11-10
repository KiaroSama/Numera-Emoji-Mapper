//! Original intent assertions use the same historical handler with real writer ownership.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

const VIDEO: &str = "test_actual_video_plan_survives_migration_reload_and_restore";

#[pyclass]
struct Imports {
    signature: Py<PyAny>,
}
#[pymethods]
impl Imports {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        _globals: &Bound<'_, PyAny>,
        _locals: &Bound<'_, PyAny>,
        fromlist: &Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        if level == 0
            && name == "tests.reference.migration_bundle"
            && fromlist.extract::<Vec<String>>()? == ["signature"]
        {
            return Ok(self.signature.bind(py).clone());
        }
        Err(PyImportError::new_err(
            "unexpected original panel intent assertion import",
        ))
    }
}

pub(crate) fn run<'py>(
    py: Python<'py>,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
    result: &Bound<'py, PyAny>,
) -> PyResult<Vec<String>> {
    let state = crate::source_reference_state::namespace(py, sources[15], None)?;
    let ownership = crate::source_reference_state::ownership(
        py,
        sources[32],
        &state,
        "_original_panel_intent_ownership",
    )?
    .dict();
    let catalog_ports = PyDict::new(py);
    let existing_catalog = required(bindings, "Catalog")?
        .getattr("__init__")?
        .getattr("__globals__")?;
    for name in ["require_native", "catalog_identity"] {
        catalog_ports.set_item(name, existing_catalog.get_item(name)?)?;
    }
    catalog_ports.set_item("writer", required(&ownership, "writer")?)?;
    let catalog = crate::source_catalog_oracle::namespace_with_ports(
        py,
        sources[2],
        sources[3],
        &required(bindings, "ROOT")?.extract::<std::path::PathBuf>()?,
        &catalog_ports,
    )?;
    let catalog = required(&catalog, "Catalog")?;
    let (panel, save) = crate::source_panel_scope_tests::handler(
        py,
        sources,
        bindings,
        &catalog,
        &required(&state, "LockBusy")?,
    )?;
    let signature = PyDict::new(py);
    for name in ["hashlib", "json", "sqlite3"] {
        signature.set_item(name, py.import(name)?)?;
    }
    let signature = crate::source_oracle_runtime::namespace(
        py,
        sources[33],
        &["_quote", "database_signature", "signature"],
        &["list", "sorted", "repr"],
        &signature,
    )?;
    let signature_api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    signature_api.setattr("signature", required(&signature, "signature")?)?;
    let values = PyDict::new(py);
    for name in ["tempfile", "Path", "unittest", "json"] {
        values.set_item(name, required(bindings, name)?)?;
    }
    values.set_item("re", py.import("re")?)?;
    values.set_item("threading", py.import("threading")?)?;
    values.set_item("Image", py.import("PIL.Image")?)?;
    values.set_item("mock", py.import("unittest.mock")?)?;
    values.set_item(
        "ThreadingHTTPServer",
        py.import("http.server")?.getattr("ThreadingHTTPServer")?,
    )?;
    values.set_item("request", py.import("urllib.request")?)?;
    values.set_item("error", py.import("urllib.error")?)?;
    values.set_item(
        "identity",
        existing_catalog
            .get_item("catalog_identity")?
            .getattr("__globals__")?
            .get_item("identity")?,
    )?;
    values.set_item("Catalog", catalog)?;
    values.set_item("panel", &panel)?;
    values.set_item("panel_save", &save)?;
    values.set_item("maintenance", required(&ownership, "maintenance")?)?;
    values.set_item("LockBusy", required(&state, "LockBusy")?)?;
    values.set_item(
        "PLAN_NAME",
        save.getattr("write_plan")?
            .getattr("__globals__")?
            .get_item("PLAN_NAME")?,
    )?;
    values.set_item("merge_plan", save.getattr("merge_plan")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[31],
        &["PanelIntentPersistence"],
        &[
            "__build_class__",
            "range",
            "list",
            "set",
            "dict",
            "next",
            "OSError",
        ],
        &values,
    )?;
    required(&tests, "__builtins__")?.set_item(
        "__import__",
        Py::new(
            py,
            Imports {
                signature: signature_api.unbind(),
            },
        )?,
    )?;
    let class = required(&tests, "PanelIntentPersistence")?;
    let methods: Vec<String> = py
        .import("unittest")?
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods.len() != 18 || !methods.iter().any(|method| method == VIDEO) {
        return Err(PyValueError::new_err(
            "original panel intent collection drift",
        ));
    }
    let baseline = crate::source_panel_scope_tests::threads(py)?;
    let writer = save.getattr("write_plan")?;
    let mut cases = Vec::new();
    for method in methods.into_iter().filter(|method| method != VIDEO) {
        let instance = class.call1((&method,))?;
        instance.call_method1("run", (result,))?;
        for field in [
            "errors",
            "failures",
            "skipped",
            "expectedFailures",
            "unexpectedSuccesses",
        ] {
            if result.getattr(field)?.len()? != 0 {
                return Err(PyValueError::new_err(format!(
                    "original PanelIntentPersistence.{method} reported {field}: {}; retained",
                    crate::logging::redact(&result.getattr(field)?.repr()?.to_string(), &[])
                )));
            }
        }
        crate::source_panel_scope_tests::settled(
            py,
            &baseline,
            &instance.getattr("server")?,
            &instance.getattr("thread")?,
        )?;
        if !save.getattr("write_plan")?.is(&writer) {
            return Err(PyValueError::new_err(
                "original panel plan-write patch was not restored",
            ));
        }
        let lease = required(&ownership, "maintenance")?.call1((instance.getattr("root")?,))?;
        lease.call_method0("__enter__")?;
        lease.call_method1("__exit__", (py.None(), py.None(), py.None()))?;
        cases.push(format!("PanelIntentPersistence.{method}"));
    }
    Ok(cases)
}
