//! Fixed historical storage observation; writer/native/dedup seams are explicit stubs, never parity proof.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet, PyTuple};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const CATALOG_HASH: &str = "5dab2e475754dc219dfe3fd8e98012fffd59ab0c760f646752ff9d35f7188ee2";
const PATHS_HASH: &str = "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3";
const TEST_HASH: &str = "2680e795ff159b3fc1a61f3b5c959c689902adc62311da3d51fe518939453a96";
const CASE: &str =
    "NativeCatalogParity.test_order_inclusion_signed_hash_and_family_publications_match_source";

pub fn source_identity(catalog: &Source, paths: &Source, test: &Source) -> Result<(), String> {
    for (source, path, hash) in [
        (catalog, "tests/reference/catalog.py", CATALOG_HASH),
        (paths, "emojikit/media_paths.py", PATHS_HASH),
        (test, "tests/test_native_catalog.py", TEST_HASH),
    ] {
        let actual = Sha256::digest(source.text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || actual != hash
        {
            return Err("catalog oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

#[derive(Default)]
struct Calls {
    native: usize,
    writer: usize,
    enter: usize,
    exit: usize,
    keys: Vec<String>,
}
struct State {
    workspace: PathBuf,
    calls: Mutex<Calls>,
}
impl State {
    fn locked(&self) -> PyResult<std::sync::MutexGuard<'_, Calls>> {
        self.calls
            .lock()
            .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("catalog stub recording failed"))
    }
}
#[pyclass]
struct Prerequisite(Arc<State>);
#[pymethods]
impl Prerequisite {
    fn __call__(&self) -> PyResult<()> {
        self.0.locked()?.native += 1;
        Ok(())
    }
}
#[pyclass]
struct Ownership(Arc<State>);
#[pymethods]
impl Ownership {
    fn __enter__(&self) -> PyResult<()> {
        self.0.locked()?.enter += 1;
        Ok(())
    }
    fn __exit__(
        &self,
        _kind: &Bound<'_, PyAny>,
        _error: &Bound<'_, PyAny>,
        _trace: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.0.locked()?.exit += 1;
        Ok(())
    }
}
#[pyclass]
struct Writer(Arc<State>);
#[pymethods]
impl Writer {
    fn __call__(&self, py: Python<'_>, path: PathBuf) -> PyResult<Py<Ownership>> {
        if path != self.0.workspace.join("old") {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "catalog writer stub escaped its supplied directory",
            ));
        }
        self.0.locked()?.writer += 1;
        Py::new(py, Ownership(self.0.clone()))
    }
}
#[pyclass]
struct Identity(Arc<State>);
#[pymethods]
impl Identity {
    fn __call__(
        &self,
        _catalog: &Bound<'_, PyAny>,
        key: &str,
        format: &str,
        path: PathBuf,
        phash: Option<u64>,
    ) -> PyResult<(String, bool)> {
        let mut calls = self.0.locked()?;
        let index = calls.keys.len();
        let expected = [("a", Some(u64::MAX)), ("b", Some(0)), ("c", None)]
            .get(index)
            .copied()
            .ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err("extra catalog identity call")
            })?;
        if key != expected.0
            || phash != expected.1
            || format != "static"
            || path != self.0.workspace.join(format!("{key}.png"))
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "catalog identity stub input drift",
            ));
        }
        calls.keys.push(key.into());
        Ok((key.into(), true))
    }
}
struct Catalog<'py> {
    value: Bound<'py, PyAny>,
    closed: bool,
}
impl Catalog<'_> {
    fn close(&mut self) -> PyResult<()> {
        if !self.closed {
            self.value.call_method0("close")?;
            self.closed = true;
        }
        Ok(())
    }
}
impl Drop for Catalog<'_> {
    fn drop(&mut self) {
        if self.close().is_err() {
            eprintln!("ERROR: historical catalog could not close; caller scratch retained");
        }
    }
}
struct DatabaseFiles {
    path: PathBuf,
    retain: bool,
}
impl DatabaseFiles {
    fn clean(&self) -> Result<(), String> {
        std::fs::remove_dir_all(&self.path)
            .or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Ok(())
                } else {
                    Err(error)
                }
            })
            .map_err(|_| "cannot clean historical catalog database".into())
    }
}
impl Drop for DatabaseFiles {
    fn drop(&mut self) {
        if !self.retain
            && let Err(error) = self.clean()
        {
            eprintln!("ERROR: {error}");
        }
    }
}
fn required<'py>(dictionary: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    dictionary.get_item(name)?.ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(format!("missing catalog oracle {name}"))
    })
}
fn items<'py>(py: Python<'py>, rows: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyList>> {
    let result = PyList::empty(py);
    let asdict = py.import("dataclasses")?.getattr("asdict")?;
    for row in rows.try_iter()? {
        result.append(asdict.call1((row?,))?)?;
    }
    Ok(result)
}
fn namespace<'py>(
    py: Python<'py>,
    catalog: &Source,
    paths: &Source,
    root: &Path,
    state: Arc<State>,
) -> PyResult<Bound<'py, PyDict>> {
    let ports = PyDict::new(py);
    ports.set_item("require_native", Py::new(py, Prerequisite(state.clone()))?)?;
    ports.set_item("writer", Py::new(py, Writer(state.clone()))?)?;
    ports.set_item("catalog_identity", Py::new(py, Identity(state))?)?;
    namespace_with_ports(py, catalog, paths, root, &ports)
}

pub(crate) fn namespace_with_ports<'py>(
    py: Python<'py>,
    catalog: &Source,
    paths: &Source,
    root: &Path,
    ports: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyDict>> {
    let path_bindings = PyDict::new(py);
    path_bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    path_bindings.set_item("PROJECT_ROOT", root.to_path_buf())?;
    path_bindings.set_item("DATA_PREFIX", "./")?;
    let media_paths = crate::source_oracle_runtime::namespace(
        py,
        paths,
        &["store", "resolve"],
        &["str", "len"],
        &path_bindings,
    )?;
    let media = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    for name in ["store", "resolve"] {
        media.setattr(name, required(&media_paths, name)?)?;
    }
    let bindings = PyDict::new(py);
    bindings.set_item("media_paths", media)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("json", py.import("json")?)?;
    bindings.set_item("sqlite3", py.import("sqlite3")?)?;
    bindings.set_item("dataclass", py.import("dataclasses")?.getattr("dataclass")?)?;
    bindings.set_item("datetime", py.import("datetime")?.getattr("datetime")?)?;
    bindings.set_item("timezone", py.import("datetime")?.getattr("timezone")?)?;
    bindings.set_item(
        "log",
        py.import("logging")?
            .call_method1("getLogger", ("source_catalog_oracle",))?,
    )?;
    for (key, value) in [
        ("SCHEMA_VERSION", 1),
        ("DEFAULT_PHASH_THRESHOLD", -1),
        ("PHASH_BITS", 64),
        ("PHASH_MAX_THRESHOLD", 16),
    ] {
        bindings.set_item(key, value)?;
    }
    bindings.set_item("MEDIA_PATHS_DONE", "data-relative-2")?;
    bindings.set_item("_U64", u64::MAX)?;
    for (name, port) in ports.iter() {
        bindings.set_item(name, port)?;
    }
    let mut registration = crate::source_oracle_runtime::ModuleRegistration::new(py, &bindings)?;
    let result = crate::source_oracle_runtime::namespace(
        py,
        catalog,
        &[
            "Item",
            "Catalog",
            "check_phash_threshold",
            "_now",
            "_phash_to_db",
            "_phash_from_db",
            "_row_to_item",
            "_merge_unique",
        ],
        &[
            "__build_class__",
            "int",
            "str",
            "bool",
            "len",
            "list",
            "set",
            "enumerate",
            "hasattr",
            "any",
            "ValueError",
            "OSError",
            "BaseException",
        ],
        &bindings,
    );
    registration.restore()?;
    result
}
fn run<'py>(py: Python<'py>, globals: &Bound<'py, PyDict>, workspace: &Path) -> PyResult<Value> {
    let mut catalog = Catalog {
        value: required(globals, "Catalog")?.call1((workspace.join("old/catalog.db"),))?,
        closed: false,
    };
    let result = (|| -> PyResult<Value> {
        for (key, phash) in [("a", Some(u64::MAX)), ("b", Some(0)), ("c", None)] {
            let kwargs = PyDict::new(py);
            kwargs.set_item("content_key", key)?;
            kwargs.set_item("fmt", "static")?;
            kwargs.set_item("file_path", workspace.join(format!("{key}.png")))?;
            kwargs.set_item("emojis", PyList::new(py, ["😀"])?)?;
            kwargs.set_item("phash", phash)?;
            let added = catalog
                .value
                .call_method("add", (), Some(&kwargs))?
                .cast_into::<PyTuple>()?;
            if added.get_item(0)?.extract::<String>()? != key
                || !added.get_item(1)?.extract::<bool>()?
            {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "catalog storage seam unexpectedly merged",
                ));
            }
        }
        let observed = PyList::empty(py);
        observed.append(
            catalog
                .value
                .call_method1("set_order", (PyList::new(py, ["c", "a", "missing"])?,))?,
        )?;
        let inclusion = catalog
            .value
            .call_method1("set_inclusion", (PySet::new(py, ["b"])?,))?;
        observed.append(
            py.import("builtins")?
                .getattr("list")?
                .call1((inclusion,))?,
        )?;
        for (id, name) in [(Some("111111111"), Some("fixture1")), (None, None)] {
            let kwargs = PyDict::new(py);
            kwargs.set_item("base", "fixture")?;
            kwargs.set_item("set_name", name)?;
            catalog
                .value
                .call_method("mark_uploaded", ("a", id), Some(&kwargs))?;
        }
        observed.append(items(py, &catalog.value.call_method0("all_items")?)?)?;
        for base in ["fixture", "other"] {
            let kwargs = PyDict::new(py);
            kwargs.set_item("base", base)?;
            observed.append(items(
                py,
                &catalog.value.call_method("pending", (), Some(&kwargs))?,
            )?)?;
        }
        observed.append(
            catalog
                .value
                .call_method1("custom_emoji_id_for", ("fixture", "a"))?,
        )?;
        observed.append(catalog.value.call_method0("stats")?)?;
        let text: String = py
            .import("json")?
            .call_method1("dumps", (observed,))?
            .extract()?;
        serde_json::from_str(&text).map_err(|_| {
            pyo3::exceptions::PyValueError::new_err("invalid historical catalog observation")
        })
    })();
    catalog.close()?;
    result
}

pub fn observe(
    root: &Path,
    workspace: &Path,
    catalog: &Source,
    paths: &Source,
    test: &Source,
) -> Result<Value, String> {
    source_identity(catalog, paths, test)?;
    if !workspace.is_absolute()
        || workspace
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("catalog oracle workspace must be absolute and contained".into());
    }
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root || !workspace.starts_with(&root) {
        return Err("catalog oracle workspace must be project-contained".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect catalog oracle workspace")?
        .next()
        .is_some()
    {
        return Err("catalog oracle workspace must be empty".into());
    }
    let old = workspace.join("old");
    std::fs::create_dir(&old).map_err(|_| "cannot create owned historical catalog directory")?;
    let mut files = DatabaseFiles {
        path: old,
        retain: true,
    };
    Python::initialize();
    let observed = Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let state = Arc::new(State {
            workspace: workspace.clone(),
            calls: Mutex::new(Calls::default()),
        });
        let globals = namespace(py, catalog, paths, &root, state.clone())
            .map_err(|error| format!("catalog oracle namespace failed: {error}"))?;
        let observation = run(py, &globals, &workspace)
            .map_err(|error| format!("catalog oracle source failed: {error}"))?;
        let calls = state
            .calls
            .lock()
            .map_err(|_| "cannot inspect catalog stub calls")?;
        if calls.native != 1
            || calls.writer != 1
            || calls.enter != 1
            || calls.exit != 1
            || calls.keys != ["a", "b", "c"]
        {
            return Err("catalog oracle supplied boundary callback drift".into());
        }
        Ok(
            json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","source_hash":CATALOG_HASH,"paths_source_hash":PATHS_HASH,"test_source_hash":TEST_HASH,"case":CASE,"observation":observation,
            "stub_calls":{"require_native":calls.native,"writer":calls.writer,"enter":calls.enter,"exit":calls.exit,"catalog_identity_keys":calls.keys},
            "coverage":{"storage":"original Catalog and sqlite3","media_paths":"original store/resolve","ownership":"supplied no-op context; NOT native ownership proof","native_prerequisite":"supplied recording stub; NOT install proof","identity":"supplied original-test (key,true) stub; NOT media/dedup proof"}}),
        )
    });
    match observed {
        Ok(observed) => {
            files.clean()?;
            files.retain = false;
            Ok(observed)
        }
        Err(error) => Err(format!(
            "{error}; owned catalog scratch retained for diagnosis"
        )),
    }
}
