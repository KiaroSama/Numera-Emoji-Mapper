//! Original PID-probe assertions with private patch/import ports; no live process is probed.
use crate::source_git_inventory::Source;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct Imports {
    package: Py<PyAny>,
    calls: AtomicUsize,
}
#[pymethods]
impl Imports {
    fn __call__<'py>(
        &self,
        name: &str,
        globals: &Bound<'_, PyAny>,
        locals: &Bound<'_, PyAny>,
        fromlist: &Bound<'py, PyAny>,
        level: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = (globals, locals);
        if level != 0
            || name != "tests.reference"
            || fromlist.extract::<Vec<String>>()? != ["packstate"]
        {
            return Err(PyImportError::new_err(
                "unexpected original PID diagnostic import",
            ));
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(self.package.bind(fromlist.py()).clone())
    }
}

pub(crate) fn observe(py: Python<'_>, source: &Source, test: &Source) -> PyResult<Value> {
    let namespace = py.import("types")?.getattr("SimpleNamespace")?;
    let mock = py.import("unittest.mock")?;
    let private_os = namespace.call0()?;
    private_os.setattr("name", "private-unpatched-platform")?;
    let sentinel = mock.getattr("Mock")?.call0()?;
    private_os.setattr("kill", &sentinel)?;
    let bindings = PyDict::new(py);
    bindings.set_item("os", &private_os)?;
    let original = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["_lock_owner_is_alive"],
        &["ProcessLookupError", "PermissionError", "OSError", "str"],
        &bindings,
    )?;
    let function = original
        .get_item("_lock_owner_is_alive")?
        .ok_or_else(|| PyValueError::new_err("missing original PID diagnostic"))?;
    let api = namespace.call0()?;
    api.setattr("_lock_owner_is_alive", &function)?;
    let package = namespace.call0()?;
    package.setattr("packstate", api)?;
    let imports = Py::new(
        py,
        Imports {
            package: package.unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    let bp = namespace.call0()?;
    bp.setattr("os", &private_os)?;
    let bindings = PyDict::new(py);
    let unittest = py.import("unittest")?;
    bindings.set_item("unittest", &unittest)?;
    bindings.set_item("mock", &mock)?;
    bindings.set_item("bp", bp)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &["PosixStaleLockReclaim"],
        &[
            "__build_class__",
            "ProcessLookupError",
            "PermissionError",
            "OSError",
        ],
        &bindings,
    )?;
    tests
        .get_item("__builtins__")?
        .ok_or_else(|| PyValueError::new_err("missing diagnostic builtins"))?
        .cast_into::<PyDict>()?
        .set_item("__import__", &imports)?;
    let class = tests
        .get_item("PosixStaleLockReclaim")?
        .ok_or_else(|| PyValueError::new_err("missing diagnostic test class"))?;
    let methods = [
        "test_no_such_process_is_reported_dead",
        "test_permission_denied_means_it_exists",
        "test_unknown_failure_stays_conservative",
        "test_a_running_process_is_alive",
    ];
    let collected = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract::<Vec<String>>()?;
    let mut expected = methods.to_vec();
    expected.sort_unstable();
    if collected != expected {
        return Err(PyValueError::new_err(
            "original diagnostic collection drift",
        ));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    for method in methods {
        class.call1((method,))?.call_method1("run", (&result,))?;
    }
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyValueError::new_err(format!(
                "original PID diagnostic reported {field}"
            )));
        }
    }
    let calls = imports.borrow(py).calls.load(Ordering::Relaxed);
    if result.getattr("testsRun")?.extract::<usize>()? != 4
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || calls != 4
        || private_os.getattr("name")?.extract::<String>()? != "private-unpatched-platform"
        || !private_os.getattr("kill")?.is(&sentinel)
    {
        return Err(PyValueError::new_err(
            "original PID diagnostic execution/restoration drift",
        ));
    }
    let builtins = py.import("builtins")?;
    let mut observations = Vec::new();
    private_os.setattr("name", "posix")?;
    for (method, exception) in methods.into_iter().zip([
        Some("ProcessLookupError"),
        Some("PermissionError"),
        Some("OSError"),
        None,
    ]) {
        let options = PyDict::new(py);
        if let Some(exception) = exception {
            options.set_item("side_effect", builtins.getattr(exception)?)?;
        } else {
            options.set_item("return_value", py.None())?;
        }
        let kill = mock.getattr("Mock")?.call((), Some(&options))?;
        private_os.setattr("kill", &kill)?;
        let alive = function.call1((4242,))?.extract::<bool>()?;
        kill.call_method1("assert_called_once_with", (4242, 0))?;
        observations.push(json!({"case":format!("PosixStaleLockReclaim.{method}"),"pid":4242,"signal":0,"alive":alive}));
    }
    private_os.setattr("name", "private-unpatched-platform")?;
    private_os.setattr("kill", &sentinel)?;
    Ok(
        json!({"tests_run":4,"import_calls":calls,"observations":observations,
        "ports":"original test/helper bodies unchanged; private os attributes/import return only, real external patch.object restoration, no global module mutation or live PID probe"}),
    )
}
