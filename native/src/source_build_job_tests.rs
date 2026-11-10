//! Original partial-inventory cleanup fault, with no WinDLL or operating-system Job execution.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyclass]
struct Imports(Py<PyAny>);
#[pymethods]
impl Imports {
    #[pyo3(signature=(name, _globals=None, _locals=None, fromlist=None, level=0))]
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        _globals: Option<&Bound<'_, PyAny>>,
        _locals: Option<&Bound<'_, PyAny>>,
        fromlist: Option<&Bound<'_, PyAny>>,
        level: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let from = fromlist
            .filter(|v| !v.is_none())
            .map(|v| v.extract::<Vec<String>>())
            .transpose()?
            .unwrap_or_default();
        if level == 0 {
            if name == "scripts.build_job" && from == ["Job"] {
                return Ok(self.0.bind(py).clone());
            }
            if name == "unittest" && from == ["mock"] {
                let module = crate::source_oracle_runtime::host_import(py, "types")?
                    .getattr("SimpleNamespace")?
                    .call0()?;
                module.setattr(
                    "mock",
                    crate::source_oracle_runtime::host_import(py, "unittest.mock")?,
                )?;
                return Ok(module);
            }
        }
        Err(PyImportError::new_err(
            "unexpected original partial-inventory import",
        ))
    }
}

pub(super) fn run(
    py: Python<'_>,
    sources: [&Source; 2],
    result: &Bound<'_, PyAny>,
) -> PyResult<String> {
    let job = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &["Job"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let module = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    module.setattr("Job", required(&job, "Job")?)?;
    let values = PyDict::new(py);
    values.set_item("unittest", py.import("unittest")?)?;
    values.set_item("os", py.import("os")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["BuildProcessOwner"],
        &["__build_class__", "object", "RuntimeError"],
        &values,
    )?;
    required(&tests, "__builtins__")?
        .set_item("__import__", Py::new(py, Imports(module.unbind()))?)?;
    let class = required(&tests, "BuildProcessOwner")?;
    let methods: Vec<String> = py
        .import("unittest")?
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    let method = "test_partial_member_inventory_closes_already_pinned_handles";
    if methods != [method, "test_success_reaps_detached_stdio_descendant"] {
        return Err(PyValueError::new_err(
            "original process-owner method collection drift",
        ));
    }
    class.call1((method,))?.call_method1("run", (result,))?;
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyValueError::new_err(format!(
                "original partial-inventory assertion reported {field}"
            )));
        }
    }
    Ok(format!("BuildProcessOwner.{method}"))
}
