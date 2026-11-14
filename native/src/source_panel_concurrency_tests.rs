//! Original event-pinned reorder/save/page assertions share the admitted historical handler.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyclass]
struct SaveImport {
    module: Py<PyAny>,
}
#[pymethods]
impl SaveImport {
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
            && name == "tests.reference.panel_save"
            && fromlist.extract::<Vec<String>>()? == ["replace_map"]
        {
            return Ok(self.module.bind(py).clone());
        }
        Err(PyImportError::new_err(
            "unexpected original shared-map import",
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
    let (panel, save) = crate::source_panel_scope_tests::handler(
        py,
        sources,
        bindings,
        &required(bindings, "Catalog")?,
        &required(&state, "LockBusy")?,
    )?;
    // The original patch must not replace Lock inside stdlib Event/Thread itself.
    let threading = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    let real = py.import("threading")?;
    for name in ["Lock", "Event", "Thread"] {
        threading.setattr(name, real.getattr(name)?)?;
    }
    panel.dict().set_item("threading", &threading)?;
    let values = PyDict::new(py);
    for name in [
        "tempfile",
        "Path",
        "unittest",
        "json",
        "mock",
        "Catalog",
        "_make_png",
    ] {
        values.set_item(name, required(bindings, name)?)?;
    }
    values.set_item("threading", &threading)?;
    values.set_item("p", panel)?;
    values.set_item(
        "ThreadingHTTPServer",
        py.import("http.server")?.getattr("ThreadingHTTPServer")?,
    )?;
    values.set_item("request", py.import("urllib.request")?)?;
    values.set_item("error", py.import("urllib.error")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[22],
        &[
            "_WatchedLock",
            "_Tripwire",
            "SaveDuringReorder",
            "TheMediaMapIsNeverEmpty",
        ],
        &[
            "__build_class__",
            "dict",
            "super",
            "range",
            "list",
            "reversed",
            "len",
            "set",
        ],
        &values,
    )?;
    let class = required(&tests, "SaveDuringReorder")?;
    let methods: Vec<String> = py
        .import("unittest")?
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods.len() != 2 {
        return Err(PyValueError::new_err(
            "original panel interleaving collection drift",
        ));
    }
    let baseline = crate::source_panel_scope_tests::threads(py)?;
    let original_lock = threading.getattr("Lock")?;
    let mut cases = Vec::new();
    for method in methods {
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
                    "original SaveDuringReorder.{method} reported {field}: {}; retained",
                    crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                )));
            }
        }
        if !threading.getattr("Lock")?.is(&original_lock)
            || !instance
                .getattr("release")?
                .call_method0("is_set")?
                .extract::<bool>()?
        {
            return Err(PyValueError::new_err(
                "original interleaving patch or release not restored",
            ));
        }
        crate::source_panel_scope_tests::settled(
            py,
            &baseline,
            &instance.getattr("httpd")?,
            &instance.getattr("thread")?,
        )?;
        cases.push(format!("SaveDuringReorder.{method}"));
    }
    required(&tests, "__builtins__")?.set_item(
        "__import__",
        Py::new(
            py,
            SaveImport {
                module: save.into_any().unbind(),
            },
        )?,
    )?;
    let class = required(&tests, "TheMediaMapIsNeverEmpty")?;
    let methods: Vec<String> = py
        .import("unittest")?
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods != ["test_a_reader_never_misses_a_key_both_maps_hold"] {
        return Err(PyValueError::new_err(
            "original shared-map collection drift",
        ));
    }
    let executed = class.call1((&methods[0],))?.call_method1("run", (result,));
    let settled = crate::source_panel_scope_tests::settled_threads(py, &baseline);
    executed?;
    settled?;
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyValueError::new_err(format!(
                "original shared-map assertion reported {field}: {}; retained",
                crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
            )));
        }
    }
    cases.push(format!("TheMediaMapIsNeverEmpty.{}", methods[0]));
    Ok(cases)
}
