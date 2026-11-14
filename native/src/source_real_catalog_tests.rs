//! Local-only real data stays inside one historical worker; responses never contain operator rows.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::Path;

#[pyclass]
struct Imports {
    package: Py<PyModule>,
    paths: Py<PyAny>,
}

#[pyclass]
struct Temporary {
    root: std::path::PathBuf,
    ledger: Py<PyAny>,
}
#[pymethods]
impl Temporary {
    #[pyo3(signature=(*, dir))]
    fn temporary<'py>(
        &self,
        py: Python<'py>,
        dir: std::path::PathBuf,
    ) -> PyResult<Bound<'py, PyAny>> {
        if dir != self.root {
            return Err(PyValueError::new_err(
                "unexpected original real-catalog temporary root",
            ));
        }
        self.ledger.bind(py).call_method0("temporary")
    }
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
        if level == 0 && name == "emojikit" && fromlist.extract::<Vec<String>>()? == ["_native"] {
            return Ok(self.package.bind(py).clone().into_any());
        }
        if level == 0
            && name == "emojikit.media_paths"
            && fromlist.extract::<Vec<String>>()? == ["resolve"]
        {
            return Ok(self.paths.bind(py).clone());
        }
        Err(PyImportError::new_err(
            "unexpected original real-catalog import",
        ))
    }
}

pub(super) fn admit(
    root: &Path,
    selected: Option<&Path>,
) -> Result<Option<std::path::PathBuf>, String> {
    let Some(path) = selected else {
        return Ok(None);
    };
    if !path.is_absolute() {
        return Err("real catalog fixture must be an absolute project-contained path".into());
    }
    crate::tooling_environment::reject_links(path, "real catalog fixture cannot follow links")?;
    let physical = crate::migration_bundle::resolved(path)
        .map_err(|_| "cannot resolve real catalog fixture")?;
    if !physical.starts_with(root)
        || !std::fs::metadata(&physical)
            .map_err(|_| "cannot inspect real catalog fixture")?
            .is_file()
    {
        return Err("real catalog fixture must be a regular project-contained file".into());
    }
    Ok(Some(physical))
}

pub(super) fn run(
    py: Python<'_>,
    root: &Path,
    project: &Path,
    sources: &[&Source],
    original: &Bound<'_, PyDict>,
    selected: Option<&Path>,
) -> PyResult<Value> {
    let Some(selected) = selected else {
        return Ok(
            json!({"selected":false,"tests_run":0,"reason":"local-by-design: NUMERA_REAL_CATALOG_FIXTURE was not selected; no real-data parity claim"}),
        );
    };
    let before = crate::tooling_manifest::digest(selected)
        .map_err(|_| PyValueError::new_err("cannot hash real catalog fixture"))?;
    let state = crate::source_reference_state::namespace(py, sources[12], None)?;
    let ownership = crate::source_reference_state::ownership(
        py,
        sources[11],
        &state,
        "_real_catalog_ownership",
    )?;
    let ports = PyDict::new(py);
    ports.set_item("writer", ownership.getattr("writer")?)?;
    let options = PyDict::new(py);
    options.set_item("return_value", py.None())?;
    ports.set_item(
        "require_native",
        py.import("unittest.mock")?
            .getattr("Mock")?
            .call((), Some(&options))?,
    )?;
    ports.set_item("catalog_identity", required(original, "catalog_identity")?)?;
    let log = py
        .import("logging")?
        .getattr("Logger")?
        .call1(("private-real-catalog",))?;
    log.setattr("disabled", true)?;
    ports.set_item("log", log)?;
    let catalog = crate::source_catalog_oracle::namespace_with_ports(
        py, sources[0], sources[1], project, &ports,
    )?;
    let sqlite = py.import("sqlite3")?;
    let snapshot = PyDict::new(py);
    snapshot.set_item("sqlite3", &sqlite)?;
    snapshot.set_item("math", py.import("math")?)?;
    snapshot.set_item("time", py.import("time")?)?;
    for name in ["BACKUP_TIMEOUT", "RESTORE_TIMEOUT"] {
        snapshot.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[14], name, false)?,
        )?;
    }
    let snapshot = crate::source_oracle_runtime::namespace(
        py,
        sources[14],
        &["BackupTimeout", "backup"],
        &["__build_class__", "RuntimeError", "ValueError", "min"],
        &snapshot,
    )?;
    let ledger = Py::new(
        py,
        crate::source_temporary::Directories {
            root: project.join("logs"),
            paths: std::sync::Mutex::new(Vec::new()),
        },
    )?;
    let environment = PyDict::new(py);
    environment.set_item(
        "NUMERA_REAL_CATALOG_FIXTURE",
        selected
            .to_str()
            .ok_or_else(|| PyValueError::new_err("real catalog fixture path must be UTF-8"))?,
    )?;
    let os = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    os.setattr("environ", &environment)?;
    let temporary = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    let temporary_owner = Py::new(
        py,
        Temporary {
            root: project.join("logs"),
            ledger: ledger.clone_ref(py).into_any(),
        },
    )?;
    temporary.setattr(
        "TemporaryDirectory",
        temporary_owner.bind(py).getattr("temporary")?,
    )?;
    let values = PyDict::new(py);
    for name in ["dataclasses", "hashlib", "json", "unittest"] {
        values.set_item(name, py.import(name)?)?;
    }
    values.set_item("sqlite3", &sqlite)?;
    values.set_item("closing", py.import("contextlib")?.getattr("closing")?)?;
    values.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    values.set_item("ROOT", project)?;
    values.set_item("os", os)?;
    values.set_item("tempfile", temporary)?;
    values.set_item("Catalog", required(&catalog, "Catalog")?)?;
    values.set_item("backup", required(&snapshot, "backup")?)?;
    let native = PyModule::new(py, "_real_catalog_native")?;
    crate::_native(&native)?;
    let package = PyModule::new(py, "_real_catalog_package")?;
    package.add("_native", native)?;
    let paths = PyDict::new(py);
    paths.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    paths.set_item("PROJECT_ROOT", root)?;
    paths.set_item("DATA_PREFIX", "./")?;
    let paths = crate::source_oracle_runtime::module(
        py,
        sources[1],
        &["resolve"],
        &["len"],
        &paths,
        "_real_catalog_source_paths",
    )?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[13],
        &["RealCatalogReplay"],
        &["__build_class__", "str", "len", "list"],
        &values,
    )?;
    required(&tests, "__builtins__")?.set_item(
        "__import__",
        Py::new(
            py,
            Imports {
                package: package.unbind(),
                paths: paths.into_any().unbind(),
            },
        )?,
    )?;
    let result = py.import("unittest")?.getattr("TestResult")?.call0()?;
    let class = required(&tests, "RealCatalogReplay")?;
    let methods: Vec<String> = py
        .import("unittest")?
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    let method = "test_copied_operator_rows_replay_and_python_reads_native_changes";
    if methods != [method] {
        return Err(PyValueError::new_err(
            "original real-catalog method collection drift",
        ));
    }
    class.call1((method,))?.call_method1("run", (&result,))?;
    // Never serialize unittest failure tuples: their assertion values can contain private IDs.
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyValueError::new_err(format!(
                "original real-catalog reported {field}: {}; private evidence retained",
                crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 1
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || ownership.getattr("_owners")?.getattr("held")?.len()? != 0
    {
        return Err(PyValueError::new_err(
            "original real-catalog execution/ownership incomplete; retained",
        ));
    }
    if crate::tooling_manifest::digest(selected)
        .map_err(|_| PyValueError::new_err("cannot rehash real catalog fixture"))?
        != before
    {
        return Err(PyValueError::new_err(
            "real catalog source bytes changed; retained",
        ));
    }
    let ledger = ledger.borrow(py);
    let paths = ledger
        .paths
        .lock()
        .map_err(|_| PyValueError::new_err("real catalog temporary ledger poisoned"))?;
    if paths.len() != 1 || !paths[0].1.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(PyValueError::new_err(
            "real catalog teardown incomplete; retained",
        ));
    }
    let data = &paths[0].0;
    crate::source_tooling_oracle::directories(
        &project.join("logs"),
        data,
        &mut 16,
        0,
        &mut Vec::new(),
    )?;
    let names = std::fs::read_dir(data)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
    if names
        != ["catalog.db", ".maintenance.lock"]
            .map(std::ffi::OsString::from)
            .into_iter()
            .collect()
        || crate::process_owner::cleanup_pending(data)
    {
        return Err(PyValueError::new_err(
            "real catalog copied inventory differs; retained",
        ));
    }
    std::fs::remove_dir_all(data)?;
    Ok(
        json!({"selected":true,"tests_run":1,"source_unchanged":true,"case":format!("RealCatalogReplay.{method}"),"ports":"unchanged original method; real read-only online snapshot, original Catalog/path resolution/maintenance and registered NativeCatalog; private selected fixture environment/import/temp/root; prerequisite no-op, no rows/IDs/path in response"}),
    )
}
