//! Actual stdlib reload of one private config module, executing only its admitted owner assignment.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct Loader {
    source: Source,
    module: Py<PyModule>,
    calls: AtomicUsize,
}
#[pymethods]
impl Loader {
    fn exec_module(&self, py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
        if !module.is(self.module.bind(py)) {
            return Err(PyValueError::new_err(
                "unexpected original rebuild reload module",
            ));
        }
        crate::source_oracle_runtime::assignment(py, &self.source, "USER_ID", &module.dict())?;
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
#[pyclass]
struct Finder {
    module: Py<PyModule>,
    spec: Py<PyAny>,
}
#[pymethods]
impl Finder {
    fn find_spec<'py>(
        &self,
        py: Python<'py>,
        fullname: &str,
        path: &Bound<'_, PyAny>,
        target: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if fullname != "_original_rebuild_config" {
            return Ok(py.None().into_bound(py));
        }
        if !path.is_none() || !target.is(self.module.bind(py)) {
            return Err(PyValueError::new_err(
                "unexpected original rebuild reload target",
            ));
        }
        Ok(self.spec.bind(py).clone())
    }
}
struct Registration<'py> {
    sys: Bound<'py, PyModule>,
    modules: Bound<'py, PyDict>,
    previous_path: Bound<'py, PyAny>,
    cfg: Bound<'py, PyModule>,
    attributes: Vec<(&'static str, Option<Bound<'py, PyAny>>)>,
    restored: bool,
}
impl Registration<'_> {
    fn restore(&mut self) -> PyResult<()> {
        if self.restored {
            return Ok(());
        }
        self.sys.setattr("meta_path", &self.previous_path)?;
        if self.modules.contains("_original_rebuild_config")? {
            self.modules.del_item("_original_rebuild_config")?;
        }
        for (name, value) in &self.attributes {
            match value {
                Some(value) => self.cfg.setattr(*name, value)?,
                None => {
                    if self.cfg.hasattr(*name)? {
                        self.cfg.delattr(*name)?;
                    }
                }
            }
        }
        if !self.sys.getattr("meta_path")?.is(&self.previous_path)
            || self.modules.contains("_original_rebuild_config")?
        {
            return Err(PyValueError::new_err(
                "original rebuild reload registration survived cleanup",
            ));
        }
        for (name, value) in &self.attributes {
            let actual = self.cfg.dict().get_item(*name)?;
            if match (actual, value) {
                (Some(actual), Some(expected)) => !actual.is(expected),
                (None, None) => false,
                _ => true,
            } {
                return Err(PyValueError::new_err(
                    "original rebuild reload attribute not restored",
                ));
            }
        }
        self.restored = true;
        Ok(())
    }
}
impl Drop for Registration<'_> {
    fn drop(&mut self) {
        if self.restore().is_err() {
            eprintln!("ERROR: original rebuild reload registration could not be restored");
        }
    }
}

pub(super) fn run<'py>(
    py: Python<'py>,
    config: &Source,
    environment_source: &Source,
    cfg: &Bound<'py, PyModule>,
    class: &Bound<'py, PyAny>,
    bindings: &Bound<'py, PyDict>,
    result: &Bound<'py, PyAny>,
) -> PyResult<(Vec<String>, usize)> {
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?.cast_into::<PyDict>()?;
    if modules.contains("_original_rebuild_config")? {
        return Err(PyValueError::new_err(
            "original rebuild reload registration is occupied",
        ));
    }
    let previous_path = sys.getattr("meta_path")?;
    let attributes = [
        "__name__",
        "__spec__",
        "__loader__",
        "__package__",
        "USER_ID",
        "safe_int_env",
    ]
    .into_iter()
    .map(|name| Ok((name, cfg.dict().get_item(name)?)))
    .collect::<PyResult<Vec<_>>>()?;
    let os = required(bindings, "os")?;
    let environment = os.getattr("environ")?.cast_into::<PyDict>()?;
    if !environment.is_empty()
        || cfg.getattr("__name__")?.extract::<String>()? != "_original_rebuild_config"
    {
        return Err(PyValueError::new_err(
            "original rebuild reload environment is not private and empty",
        ));
    }
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let stderr = py.import("io")?.getattr("StringIO")?.call0()?;
    let private_sys = ns.call0()?;
    private_sys.setattr("stderr", &stderr)?;
    let numeric_bindings = PyDict::new(py);
    numeric_bindings.set_item("os", &os)?;
    numeric_bindings.set_item("sys", private_sys)?;
    let numeric = crate::source_oracle_runtime::namespace(
        py,
        environment_source,
        &["safe_int_env"],
        &["int", "print", "ValueError"],
        &numeric_bindings,
    )?;
    let loader = Py::new(
        py,
        Loader {
            source: Source {
                baseline_revision: config.baseline_revision.clone(),
                path: config.path.clone(),
                sha256: config.sha256.clone(),
                text: config.text.clone(),
            },
            module: cfg.clone().unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    let spec = py
        .import("importlib.machinery")?
        .getattr("ModuleSpec")?
        .call1(("_original_rebuild_config", &loader))?;
    let finder = Py::new(
        py,
        Finder {
            module: cfg.clone().unbind(),
            spec: spec.unbind(),
        },
    )?;
    let path = PyList::new(py, previous_path.try_iter()?.collect::<PyResult<Vec<_>>>()?)?;
    path.insert(0, finder)?;
    modules.set_item("_original_rebuild_config", cfg)?;
    let mut registration = Registration {
        sys,
        modules,
        previous_path,
        cfg: cfg.clone(),
        attributes,
        restored: false,
    };
    cfg.setattr("safe_int_env", required(&numeric, "safe_int_env")?)?;
    registration.sys.setattr("meta_path", path)?;
    let outcome = (|| {
        let methods = super::cases::OWNER;
        let before = result.getattr("testsRun")?.extract::<usize>()?;
        class.call_method0("setUpClass")?;
        for (index, method) in methods.iter().enumerate() {
            class.call1((*method,))?.call_method1("run", (result,))?;
            for field in [
                "errors",
                "failures",
                "skipped",
                "expectedFailures",
                "unexpectedSuccesses",
            ] {
                if result.getattr(field)?.len()? != 0 {
                    return Err(PyValueError::new_err(format!(
                        "original owner reload reported {field}"
                    )));
                }
            }
            if !environment.is_empty()
                || cfg.getattr("USER_ID")?.extract::<u64>()? != 0
                || loader.borrow(py).calls.load(Ordering::SeqCst) != (index + 1) * 2
                || !registration
                    .modules
                    .get_item("_original_rebuild_config")?
                    .is_some_and(|value| value.is(cfg))
            {
                return Err(PyValueError::new_err("original owner reload cleanup drift"));
            }
        }
        class.call_method0("tearDownClass")?;
        let calls = loader.borrow(py).calls.load(Ordering::SeqCst);
        if calls != 4
            || result.getattr("testsRun")?.extract::<usize>()? != before + methods.len()
            || !result.call_method0("wasSuccessful")?.extract::<bool>()?
            || stderr.call_method0("getvalue")?.extract::<String>()?
                != "WARNING: PACK_OWNER_USER_ID is not a whole number; using 0.\n"
        {
            return Err(PyValueError::new_err(
                "original owner reload accounting drift",
            ));
        }
        Ok((
            methods
                .iter()
                .map(|method| format!("OwnerIdIsParsedSafely.{method}"))
                .collect(),
            calls,
        ))
    })();
    let restored = registration.restore();
    let closed = stderr.call_method0("close");
    restored?;
    closed?;
    outcome
}
