//! Original Python entry signatures and the no-catalog path remain source observations, not native APIs.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyclass]
struct Temporary(Py<PyAny>);
#[pymethods]
impl Temporary {
    #[pyo3(signature=(*, prefix))]
    fn mkdtemp(&self, py: Python<'_>, prefix: &str) -> PyResult<String> {
        if prefix != "sandbox-nocat-" {
            return Err(PyValueError::new_err(
                "unexpected original panel entry prefix",
            ));
        }
        self.0.bind(py).call_method0("mkdtemp")?.extract()
    }
    fn rmtree(
        &self,
        py: Python<'_>,
        path: std::path::PathBuf,
        ignore_errors: bool,
    ) -> PyResult<()> {
        let options = PyDict::new(py);
        options.set_item("ignore_errors", ignore_errors)?;
        self.0
            .bind(py)
            .call_method("rmtree", (path,), Some(&options))?;
        Ok(())
    }
}

#[pyclass]
struct Imports {
    panel: Py<PyAny>,
    environment: Py<PyAny>,
}
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
            if name == "tests.reference" && from == ["panel"] {
                let package = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
                package.setattr("panel", self.panel.bind(py))?;
                return Ok(package);
            }
            if name == "emojikit.cli_env" && from == ["load_env"] {
                return Ok(self.environment.bind(py).clone());
            }
            if name == "inspect" && from.is_empty() {
                return Ok(py.import("inspect")?.into_any());
            }
        }
        Err(PyImportError::new_err(
            "unexpected original panel entry import",
        ))
    }
}

pub(super) fn run(
    py: Python<'_>,
    workspace: &std::path::Path,
    sources: &[&Source],
    ledger: &Bound<'_, PyAny>,
    result: &Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let mock = py.import("unittest.mock")?;
    let options = PyDict::new(py);
    options.set_item("return_value", py.None())?;
    let load = mock.getattr("Mock")?.call((), Some(&options))?;
    let setup = mock.getattr("Mock")?.call((), Some(&options))?;
    let environment = ns.call0()?;
    environment.setattr("load_env", &load)?;
    let reopen = mock.getattr("Mock")?.call0()?;
    let bindings = PyDict::new(py);
    bindings.set_item("reopen_existing", &reopen)?;
    bindings.set_item("argparse", py.import("argparse")?)?;
    bindings.set_item("os", py.import("os")?)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("ROOT", workspace)?;
    bindings.set_item("DEFAULT_PORT", 8798)?;
    bindings.set_item(
        "PREVIEW_FPS",
        crate::source_logging_oracle::literal(py, sources[30], "PREVIEW_FPS", false)?,
    )?;
    bindings.set_item("setup_logging", &setup)?;
    let log = py
        .import("logging")?
        .getattr("Logger")?
        .call1(("original-panel-entry",))?;
    log.setattr("disabled", true)?;
    bindings.set_item("log", log)?;
    let main = crate::source_oracle_runtime::namespace(
        py,
        sources[11],
        &["main"],
        &["int", "range"],
        &bindings,
    )?;
    let panel = crate::source_roster_modules::rebind(py, &main, "_original_panel_entry")?;
    let imports = Py::new(
        py,
        Imports {
            panel: panel.clone().into_any().unbind(),
            environment: environment.unbind(),
        },
    )?;
    panel
        .dict()
        .get_item("__builtins__")?
        .ok_or_else(|| PyValueError::new_err("missing original entry builtins"))?
        .set_item("__import__", imports.bind(py))?;
    let temporary = Py::new(py, Temporary(ledger.clone().unbind()))?;
    let values = PyDict::new(py);
    values.set_item("tempfile", temporary.bind(py))?;
    values.set_item("shutil", temporary.bind(py))?;
    values.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    values.set_item("unittest", py.import("unittest")?)?;
    values.set_item("mock", &mock)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[35],
        &["ThePanelCanRunInProcessWithoutAdoptingAListener"],
        &["__build_class__", "AssertionError"],
        &values,
    )?;
    required(&tests, "__builtins__")?.set_item("__import__", imports.bind(py))?;
    let name = "ThePanelCanRunInProcessWithoutAdoptingAListener";
    let class = required(&tests, name)?;
    let methods: Vec<String> = py
        .import("unittest")?
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods
        != [
            "test_main_still_reads_sys_argv_when_given_none",
            "test_reuse_disabled_reaches_neither_reopen_call_site",
        ]
    {
        return Err(PyValueError::new_err(
            "original panel entry collection drift",
        ));
    }
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
                    "original panel entry {method} reported {field}; retained"
                )));
            }
        }
        let directory = instance
            .getattr("no_catalog")?
            .extract::<std::path::PathBuf>()?;
        if directory.parent() != Some(workspace)
            || std::fs::read_dir(directory)?.next().is_some()
            || !panel.getattr("reopen_existing")?.is(&reopen)
        {
            return Err(PyValueError::new_err(
                "original panel entry cleanup/patch differs; retained",
            ));
        }
        cases.push(format!("{name}.{method}"));
    }
    load.call_method0("assert_called_once")?;
    setup.call_method1("assert_called_once_with", ("panel",))?;
    reopen.call_method0("assert_not_called")?;
    Ok(cases)
}
