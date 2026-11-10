//! Delayed original fixture teardown: acknowledge cleanup, inventory before removing any evidence.
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
#[cfg(test)]
use pyo3::types::PyDict;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT: AtomicU64 = AtomicU64::new(0);

#[pyclass]
pub(crate) struct Temporary {
    name: String,
    cleaned: Arc<AtomicBool>,
}
#[pymethods]
impl Temporary {
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }
    fn cleanup(&self) {
        self.cleaned.store(true, Ordering::SeqCst);
    }
    fn __enter__(&self) -> &str {
        &self.name
    }
    fn __exit__(
        &self,
        _kind: &Bound<'_, PyAny>,
        _error: &Bound<'_, PyAny>,
        _trace: &Bound<'_, PyAny>,
    ) {
        self.cleanup();
    }
}
#[pyclass]
pub(crate) struct Directories {
    pub root: PathBuf,
    pub paths: Mutex<Vec<(PathBuf, Arc<AtomicBool>)>>,
}
#[pymethods]
impl Directories {
    #[pyo3(signature = (prefix=None, *, dir=None))]
    fn temporary(
        &self,
        py: Python<'_>,
        prefix: Option<&str>,
        dir: Option<PathBuf>,
    ) -> PyResult<Py<Temporary>> {
        let prefix = match prefix {
            None => "original-reconcile-",
            Some("emoji-verify-") => "emoji-verify-",
            _ => {
                return Err(PyValueError::new_err(
                    "unexpected original temporary prefix",
                ));
            }
        };
        let directory = match dir {
            None => self.root.clone(),
            Some(directory) => {
                let directory = crate::tooling_environment::physical_root(&directory)
                    .map_err(PyValueError::new_err)?;
                let paths = self.paths.lock().map_err(|_| {
                    PyRuntimeError::new_err("original temporary accounting poisoned")
                })?;
                if !paths
                    .iter()
                    .any(|(owned, cleaned)| owned == &directory && !cleaned.load(Ordering::SeqCst))
                {
                    return Err(PyValueError::new_err(
                        "original nested temporary requires an active owned parent",
                    ));
                }
                directory
            }
        };
        // This Rust callback may be invoked from a restricted historical
        // Python namespace. Importing tempfile here would re-enter that
        // namespace's import facade; create the owned directory directly.
        let name = loop {
            let path = directory.join(format!(
                "{prefix}{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => {
                    break path
                        .to_str()
                        .ok_or_else(|| {
                            PyValueError::new_err("original temporary path is not UTF-8")
                        })?
                        .to_owned();
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(PyRuntimeError::new_err(error.to_string())),
            }
        };
        let path = crate::tooling_environment::physical_root(Path::new(&name))
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(directory.as_path()) {
            return Err(PyValueError::new_err(
                "original reconciliation temporary escaped",
            ));
        }
        let cleaned = Arc::new(AtomicBool::new(false));
        self.paths
            .lock()
            .map_err(|_| PyRuntimeError::new_err("reconciliation temporary accounting poisoned"))?
            .push((path, cleaned.clone()));
        Py::new(py, Temporary { name, cleaned })
    }

    fn mkdtemp(&self, py: Python<'_>) -> PyResult<String> {
        Ok(self.temporary(py, None, None)?.borrow(py).name.clone())
    }

    #[pyo3(signature = (path, *, ignore_errors=false))]
    fn rmtree(&self, path: PathBuf, ignore_errors: bool) -> PyResult<()> {
        let path =
            crate::tooling_environment::physical_root(&path).map_err(PyValueError::new_err)?;
        let paths = self
            .paths
            .lock()
            .map_err(|_| PyRuntimeError::new_err("original temporary accounting poisoned"))?;
        let (_, cleaned) = paths
            .iter()
            .find(|(owned, _)| owned == &path)
            .filter(|_| ignore_errors)
            .ok_or_else(|| PyValueError::new_err("unexpected original directory cleanup"))?;
        cleaned.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_temporaries_require_active_owned_parents_and_defer_removal() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let root = parent.join(format!("original temporary ledger {}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let caller = root.join("caller");
        std::fs::create_dir(&caller).unwrap();
        std::fs::write(caller.join("sentinel"), b"caller bytes").unwrap();
        Python::initialize();
        Python::attach(|py| {
            let ledger = Py::new(
                py,
                Directories {
                    root: root.clone(),
                    paths: Mutex::new(Vec::new()),
                },
            )
            .unwrap();
            let ledger = ledger.bind(py);
            let first = ledger.call_method0("temporary").unwrap();
            let first_path =
                PathBuf::from(first.getattr("name").unwrap().extract::<String>().unwrap());
            assert_eq!(first_path.parent(), Some(root.as_path()));
            let options = PyDict::new(py);
            options.set_item("dir", &first_path).unwrap();
            let nested = ledger.call_method("temporary", (), Some(&options)).unwrap();
            let nested_path =
                PathBuf::from(nested.getattr("name").unwrap().extract::<String>().unwrap());
            assert_eq!(nested_path.parent(), Some(first_path.as_path()));
            std::fs::write(nested_path.join("evidence.json"), b"fixture evidence").unwrap();
            for refused in [&root, &caller] {
                let before = std::fs::read_dir(refused).unwrap().count();
                options.set_item("dir", refused).unwrap();
                assert!(
                    ledger
                        .call_method("temporary", (), Some(&options))
                        .unwrap_err()
                        .to_string()
                        .contains("active owned parent")
                );
                assert_eq!(std::fs::read_dir(refused).unwrap().count(), before);
            }
            nested.call_method0("cleanup").unwrap();
            options.set_item("dir", &nested_path).unwrap();
            assert!(
                ledger
                    .call_method("temporary", (), Some(&options))
                    .unwrap_err()
                    .to_string()
                    .contains("active owned parent")
            );
            assert_eq!(
                std::fs::read(nested_path.join("evidence.json")).unwrap(),
                b"fixture evidence"
            );
            first.call_method0("cleanup").unwrap();
            options.set_item("dir", &first_path).unwrap();
            assert!(
                ledger
                    .call_method("temporary", (), Some(&options))
                    .unwrap_err()
                    .to_string()
                    .contains("active owned parent")
            );
            let owned = ledger.borrow();
            let paths = owned.paths.lock().unwrap();
            assert_eq!(paths.len(), 2);
            assert!(paths.iter().all(|(_, done)| done.load(Ordering::SeqCst)));
            assert_eq!(paths[0].0, first_path);
            assert_eq!(paths[1].0, nested_path);
            drop(paths);
            drop(owned);
            assert!(!crate::process_owner::cleanup_pending(&root));
            std::fs::remove_file(nested_path.join("evidence.json")).unwrap();
            std::fs::remove_dir(nested_path).unwrap();
            std::fs::remove_dir(first_path).unwrap();
        });
        assert_eq!(
            std::fs::read(caller.join("sentinel")).unwrap(),
            b"caller bytes"
        );
        std::fs::remove_file(caller.join("sentinel")).unwrap();
        std::fs::remove_dir(caller).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
