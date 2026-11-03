//! Unchanged bounded SQLite assertions reuse the pinned corpus, AST selector and process owner.
use crate::source_git_inventory::Source;
use crate::source_temporary::Directories;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Mutex, atomic::Ordering};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 4] = [
    (
        "tests/test_sqlite_snapshot.py",
        "ab5745cf52102d0453269a1508996114a31680c123c71af79360c9930c209268",
    ),
    (
        "emojikit/sqlite_snapshot.py",
        "8e3dd41f6607f1f1e4da4b777192088b9b1b370cf429774968a0771a1f78ca4d",
    ),
    (
        "tests/reference/collection_migrate.py",
        "b4cdee1858f5f5ef5b4f3e9d187159155efa7d3af7c191e64e0f81dc0e809dd9",
    ),
    (
        "tests/test_identity_migration.py",
        "7a17796f09d44a35d3bbfe10822c9102cc34f7af0ad14259e6f066d573dcba12",
    ),
];

pub fn source_paths() -> [&'static str; 4] {
    IDENTITIES.map(|(path, _)| path)
}

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("snapshot replay requires exactly four pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("snapshot source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let values = PyDict::new(py);
    for name in ["math", "sqlite3", "time"] {
        values.set_item(name, py.import(name)?)?;
    }
    let timeout = crate::source_logging_oracle::literal(py, sources[1], "BACKUP_TIMEOUT", false)?;
    values.set_item("BACKUP_TIMEOUT", &timeout)?;
    let snapshot = crate::source_oracle_runtime::module(
        py,
        sources[1],
        &["BackupTimeout", "backup"],
        &["__build_class__", "RuntimeError", "ValueError", "min"],
        &values,
        "_original_sqlite_snapshot",
    )?;
    values.set_item("sqlite_snapshot", &snapshot)?;
    values.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    values.set_item(
        "KEY_REFERENCES",
        crate::source_logging_oracle::literal(py, sources[2], "KEY_REFERENCES", false)?,
    )?;
    let migration = crate::source_oracle_runtime::module(
        py,
        sources[2],
        &["backup_catalog", "_verify_backup"],
        &[
            "range",
            "FileExistsError",
            "BaseException",
            "RuntimeError",
            "str",
        ],
        &values,
        "_original_snapshot_migration",
    )?;
    let catalog = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["_Catalog"],
        &["__build_class__", "str"],
        &values,
    )?;
    let ledger = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let tempfile = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    tempfile.setattr("TemporaryDirectory", ledger.bind(py).getattr("temporary")?)?;
    let tests = PyDict::new(py);
    for name in ["sqlite3", "time", "Path"] {
        tests.set_item(name, required(&values, name)?)?;
    }
    tests.set_item("tempfile", tempfile)?;
    tests.set_item("unittest", py.import("unittest")?)?;
    tests.set_item("mock", py.import("unittest.mock")?)?;
    tests.set_item("cm", migration)?;
    tests.set_item("snapshot", &snapshot)?;
    tests.set_item("_Catalog", required(&catalog, "_Catalog")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &["BoundedSnapshot"],
        &["__build_class__", "list", "str", "float", "ValueError"],
        &tests,
    )?;
    let class = required(&tests, "BoundedSnapshot")?;
    let unittest = py.import("unittest")?;
    let methods: Vec<String> = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods.len() != 4 {
        return Err(PyValueError::new_err("original snapshot collection drift"));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    for method in methods {
        class.call1((&method,))?.call_method1("run", (&result,))?;
        for field in [
            "errors",
            "failures",
            "skipped",
            "expectedFailures",
            "unexpectedSuccesses",
        ] {
            if result.getattr(field)?.len()? != 0 {
                return Err(PyValueError::new_err(format!(
                    "original BoundedSnapshot.{method} reported {field}; retained"
                )));
            }
        }
        if !snapshot.getattr("BACKUP_TIMEOUT")?.eq(&timeout)? {
            return Err(PyValueError::new_err(
                "original snapshot budget patch not restored",
            ));
        }
        cases.push(format!("BoundedSnapshot.{method}"));
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 4
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original snapshot assertions incomplete",
        ));
    }
    let ledger = ledger.borrow(py);
    let paths = ledger
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("snapshot temporary accounting poisoned"))?;
    if paths.len() != 4
        || paths
            .iter()
            .any(|(_, cleaned)| !cleaned.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original snapshot cleanup incomplete; retained",
        ));
    }
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    if actual != paths.iter().map(|(path, _)| path.clone()).collect()
        || crate::process_owner::cleanup_pending(workspace)
    {
        return Err(PyValueError::new_err(
            "unexpected original snapshot workspace; retained",
        ));
    }
    let mut inventory = Vec::new();
    for (path, _) in paths.iter() {
        directories(workspace, path, &mut 8, 0, &mut Vec::new())?;
        let mut names = BTreeSet::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| PyValueError::new_err("snapshot filename is not UTF-8"))?;
            let metadata = entry.metadata()?;
            if !metadata.is_file()
                || metadata.len() > 1024 * 1024
                || !(name == "catalog.db"
                    || (name.starts_with("catalog.before-video-identity-")
                        && name.ends_with(".db")))
            {
                return Err(PyValueError::new_err(
                    "unexpected original snapshot member; retained",
                ));
            }
            names.insert(name);
        }
        if !names.contains("catalog.db") || names.len() > 3 {
            return Err(PyValueError::new_err(
                "original snapshot inventory drift; retained",
            ));
        }
        inventory.push(names.len());
    }
    inventory.sort_unstable();
    if inventory != [1, 1, 2, 3] {
        return Err(PyValueError::new_err(
            "original snapshot backup count drift; retained",
        ));
    }
    for (path, _) in paths.iter() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<BTreeMap<_,_>>(),"tests_run":4,"temporary_calls":4,"cases":cases,
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"four unchanged original BoundedSnapshot methods/setup/cleanup; real stdlib SQLite/time/math; original backup progress/deadline, original migration backup naming/verification, original minimal catalog schema; private module budget patch and shared delayed temporary ledger; no native result oracle"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("snapshot workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect snapshot workspace")?
        .next()
        .is_some()
    {
        return Err("snapshot workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect snapshot bytecode policy")?
        {
            return Err("snapshot replay requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original snapshot replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })
}

pub async fn observe_owned(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    sources: &[&Source],
) -> Result<Value, String> {
    source_identity(sources)?;
    crate::source_oracle_process::check(
        root,
        executable,
        "snapshot-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
