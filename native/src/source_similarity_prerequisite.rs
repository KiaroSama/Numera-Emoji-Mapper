//! Two unchanged missing-module assertions share the admitted original catalog cohort.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::atomic::Ordering;

#[pyclass]
struct Imports {
    similarity: Py<PyAny>,
    catalog: Py<PyAny>,
    temporary: Py<PyAny>,
}
#[pymethods]
impl Imports {
    #[pyo3(signature = (name, _globals=None, _locals=None, fromlist=None, level=0))]
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
            .filter(|value| !value.is_none())
            .map(|value| value.extract::<Vec<String>>())
            .transpose()?
            .unwrap_or_default();
        if level == 0 {
            let namespace = crate::source_oracle_runtime::host_import(py, "types")?
                .getattr("SimpleNamespace")?;
            if name == "emojikit" && from == ["similarity"] {
                let package = namespace.call0()?;
                package.setattr("similarity", self.similarity.bind(py))?;
                return Ok(package);
            }
            if name == "tests.reference.catalog" && from == ["Catalog"] {
                let module = namespace.call0()?;
                module.setattr("Catalog", self.catalog.bind(py))?;
                return Ok(module);
            }
            if name == "tempfile" && from.is_empty() {
                return Ok(self.temporary.bind(py).clone());
            }
        }
        Err(PyImportError::new_err(
            "unexpected original similarity prerequisite import",
        ))
    }
}

pub(super) fn check(
    py: Python<'_>,
    workspace: &Path,
    catalog: &Bound<'_, PyDict>,
    similarity: &Source,
    test: &Source,
) -> PyResult<Value> {
    let namespace = py.import("types")?.getattr("SimpleNamespace")?;
    let importlib = namespace.call0()?;
    let mock = py.import("unittest.mock")?;
    let options = PyDict::new(py);
    options.set_item(
        "side_effect",
        PyValueError::new_err("unexpected unpatched original native import").into_value(py),
    )?;
    let importer = mock.getattr("Mock")?.call((), Some(&options))?;
    importlib.setattr("import_module", &importer)?;
    let values = PyDict::new(py);
    values.set_item("importlib", &importlib)?;
    let original = crate::source_oracle_runtime::module(
        py,
        similarity,
        &["require_native", "greedy_indices", "near_indices"],
        &["ImportError", "RuntimeError", "getattr", "callable"],
        &values,
        "_original_missing_similarity",
    )?;
    let ledger = Py::new(
        py,
        crate::source_temporary::Directories {
            root: workspace.into(),
            paths: std::sync::Mutex::new(Vec::new()),
        },
    )?;
    let temporary = namespace.call0()?;
    temporary.setattr("TemporaryDirectory", ledger.bind(py).getattr("temporary")?)?;
    let imports = Py::new(
        py,
        Imports {
            similarity: original.clone().into_any().unbind(),
            catalog: required(catalog, "Catalog")?.unbind(),
            temporary: temporary.unbind(),
        },
    )?;
    let saved = required(catalog, "require_native")?;
    catalog.set_item("require_native", original.getattr("require_native")?)?;
    let observed = (|| -> PyResult<Value> {
        let bindings = PyDict::new(py);
        bindings.set_item("unittest", py.import("unittest")?)?;
        bindings.set_item("mock", &mock)?;
        bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
        let tests = crate::source_oracle_runtime::namespace(
            py,
            test,
            &["NativeSimilarity"],
            &["__build_class__", "ImportError", "RuntimeError"],
            &bindings,
        )?;
        required(&tests, "__builtins__")?.set_item("__import__", imports)?;
        let result = py.import("unittest")?.getattr("TestResult")?.call0()?;
        let cases = [
            "test_missing_module_never_uses_a_python_fallback",
            "test_missing_native_cannot_create_catalog_state",
        ];
        for method in cases {
            required(&tests, "NativeSimilarity")?
                .call1((method,))?
                .call_method1("run", (&result,))?;
            if !importlib.getattr("import_module")?.is(&importer) {
                return Err(PyValueError::new_err(
                    "original missing-module patch was not restored; retained",
                ));
            }
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
                    "original missing-module assertions reported {field}: {}; retained",
                    crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                )));
            }
        }
        if importer.getattr("call_count")?.extract::<usize>()? != 0 {
            return Err(PyValueError::new_err(
                "original missing-module import escaped patch",
            ));
        }
        if result.getattr("testsRun")?.extract::<usize>()? != 2 {
            return Err(PyValueError::new_err(
                "original missing-module assertions incomplete",
            ));
        }
        let owner = ledger.borrow(py);
        let paths = owner.paths.lock().map_err(|_| {
            PyValueError::new_err("original missing-module temporary accounting poisoned")
        })?;
        if paths.len() != 1 || !paths[0].1.load(Ordering::SeqCst) {
            return Err(PyValueError::new_err(
                "original missing-module cleanup incomplete",
            ));
        }
        let path = &paths[0].0;
        let physical =
            crate::tooling_environment::physical_root(path).map_err(PyValueError::new_err)?;
        if physical.parent() != Some(workspace)
            || crate::process_owner::cleanup_pending(path)
            || std::fs::read_dir(path)?.next().is_some()
        {
            return Err(PyValueError::new_err(
                "original missing-module fixture changed or created state; retained",
            ));
        }
        Ok(json!({"tests_run":2,"temporary_calls":1,
            "cases":cases.map(|method| format!("NativeSimilarity.{method}")),
            "errors":0,"failures":0,"skipped":0,
            "coverage":"unchanged original import-failure methods and original require_native/Catalog; private imports, delayed tempfile and shared catalog writer port; not compiled-native installation acceptance"}))
    })();
    let restored = catalog.set_item("require_native", &saved);
    restored?;
    if !required(catalog, "require_native")?.is(&saved) {
        return Err(PyValueError::new_err(
            "original catalog prerequisite not restored",
        ));
    }
    let observed = observed?;
    let owner = ledger.borrow(py);
    let paths = owner.paths.lock().map_err(|_| {
        PyValueError::new_err("original missing-module temporary accounting poisoned")
    })?;
    std::fs::remove_dir(&paths[0].0)?;
    Ok(observed)
}
