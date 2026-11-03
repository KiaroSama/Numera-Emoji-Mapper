//! Fixed original archive assertions retain real SQLite, paths, file moves and independent renderers.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{directories, observed, required};
use pyo3::exceptions::{PyImportError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 7] = [
    (
        "tests/reference/pack_archive.py",
        "392bd78db1321ef135706c22309facb22b417e1119a7e854717e9dde806ed0ad",
    ),
    (
        "tests/test_pack_archive.py",
        "86c35e985a8a6a3c7729d7ef873ff717b371fbdd9c34a1cc0d4e9268412a2c9c",
    ),
    (
        "tests/oracles/pack_rows.py",
        "1c0c0fc15ab5b6abcecb182f138f89da55c566c7210102c1eaaa5ad115343121",
    ),
    (
        "emojikit/media_paths.py",
        "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3",
    ),
    (
        "emojikit/operator_config.py",
        "a19f804a7bdd37baccb474eac96df6aa4d46439dd956e457c06adf3c180b9591",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
];
const CLASSES: [(&str, usize); 5] = [
    ("OnlyAFinishedPackIsArchived", 4),
    ("TheArchiveHasToFollowThePack", 3),
    ("TheNameCarriesTheSlot", 4),
    ("FolderNames", 2),
    ("AMovedFileStaysRecorded", 1),
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("archive tests require exactly seven pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("archive tests source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

#[pyclass]
struct Temporary {
    name: String,
    exited: Arc<AtomicBool>,
}
#[pymethods]
impl Temporary {
    fn __enter__(&self) -> &str {
        &self.name
    }
    fn __exit__(
        &self,
        _kind: &Bound<'_, PyAny>,
        _error: &Bound<'_, PyAny>,
        _trace: &Bound<'_, PyAny>,
    ) {
        self.exited.store(true, Ordering::SeqCst);
    }
}
#[pyclass]
struct Directories {
    root: PathBuf,
    paths: Mutex<Vec<(PathBuf, Arc<AtomicBool>)>>,
}
#[pymethods]
impl Directories {
    fn temporary(&self, py: Python<'_>) -> PyResult<Py<Temporary>> {
        let options = PyDict::new(py);
        options.set_item("dir", &self.root)?;
        options.set_item("prefix", "original-archive-")?;
        let name: String = py
            .import("tempfile")?
            .call_method("mkdtemp", (), Some(&options))?
            .extract()?;
        let path = crate::tooling_environment::physical_root(Path::new(&name))
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(self.root.as_path()) {
            return Err(PyValueError::new_err("original archive temporary escaped"));
        }
        let exited = Arc::new(AtomicBool::new(false));
        self.paths
            .lock()
            .map_err(|_| PyRuntimeError::new_err("archive temporary accounting poisoned"))?
            .push((path, exited.clone()));
        Py::new(py, Temporary { name, exited })
    }
}
#[pyclass]
struct Imports;
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
    ) -> PyResult<Bound<'py, PyModule>> {
        if level == 0
            && name == "emojikit.telegram_api"
            && fromlist.extract::<Vec<String>>()? == ["Telegram"]
        {
            let module = PyModule::new(py, "_original_unused_telegram_type")?;
            module.add("Telegram", py.import("builtins")?.getattr("object")?)?;
            return Ok(module);
        }
        Err(PyImportError::new_err("unexpected original archive import"))
    }
}
#[pyclass]
struct Check {
    function: Py<PyAny>,
    observations: Mutex<Vec<Value>>,
}
#[pymethods]
impl Check {
    fn __call__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let result = self.function.bind(py).call0()?;
        self.observations
            .lock()
            .map_err(|_| PyRuntimeError::new_err("archive check observations poisoned"))?
            .push(observed(&result)?);
        Ok(result)
    }
}

pub(crate) fn module<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: [&Source; 6],
) -> PyResult<Bound<'py, PyModule>> {
    let [archive, rows, path_source, operator_source, errors, atomic] = sources;
    let path = py.import("pathlib")?.getattr("Path")?;
    let bindings = PyDict::new(py);
    bindings.set_item("Path", &path)?;
    bindings.set_item("PROJECT_ROOT", path.call1((workspace,))?)?;
    bindings.set_item("DATA_PREFIX", "./")?;
    let paths = crate::source_oracle_runtime::namespace(
        py,
        path_source,
        &["store", "resolve"],
        &["str", "len"],
        &bindings,
    )?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let media_paths = ns.call0()?;
    for name in ["store", "resolve"] {
        media_paths.setattr(name, required(&paths, name)?)?;
    }
    let errors = crate::source_oracle_runtime::namespace(
        py,
        errors,
        &["OperatorConfigMissing"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let os = ns.call0()?;
    os.setattr("environ", PyDict::new(py))?;
    bindings.set_item("os", &os)?;
    bindings.set_item("ROOT", path.call1((workspace,))?)?;
    bindings.set_item(
        "WHAT",
        crate::source_logging_oracle::literal(py, operator_source, "WHAT", false)?,
    )?;
    bindings.set_item(
        "OperatorConfigMissing",
        required(&errors, "OperatorConfigMissing")?,
    )?;
    let operator = crate::source_oracle_runtime::namespace(
        py,
        operator_source,
        &["value", "require", "brand_logo_path"],
        &["str"],
        &bindings,
    )?;
    let operator_api = ns.call0()?;
    for name in ["value", "require", "brand_logo_path"] {
        operator_api.setattr(name, required(&operator, name)?)?;
    }
    for name in ["contextlib", "json", "os", "stat", "tempfile", "time"] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item(
        "_REPLACE_DEADLINE",
        crate::source_logging_oracle::literal(py, atomic, "_REPLACE_DEADLINE", false)?,
    )?;
    let writer = crate::source_oracle_runtime::namespace(
        py,
        atomic,
        &["_replace", "write_json_atomic"],
        &["PermissionError", "FileNotFoundError", "min"],
        &bindings,
    )?;
    bindings.set_item("write_json_atomic", required(&writer, "write_json_atomic")?)?;
    bindings.set_item("operator_config", operator_api)?;
    bindings.set_item("media_paths", media_paths)?;
    bindings.set_item("sqlite3", py.import("sqlite3")?)?;
    bindings.set_item("shutil", py.import("shutil")?)?;
    bindings.set_item("os", os)?;
    bindings.set_item("writer", py.import("contextlib")?.getattr("nullcontext")?)?;
    bindings.set_item("PER_SET", 200)?;
    bindings.set_item("ROOT", path.call1((workspace,))?)?;
    bindings.set_item("DATA_DIR", path.call1((workspace.join("collection"),))?)?;
    bindings.set_item(
        "CATALOG",
        path.call1((workspace.join("collection/catalog.db"),))?,
    )?;
    bindings.set_item(
        "ARCHIVE_ENV",
        crate::source_logging_oracle::literal(py, archive, "ARCHIVE_ENV", false)?,
    )?;
    bindings.set_item(
        "META",
        crate::source_logging_oracle::literal(py, archive, "META", false)?,
    )?;
    bindings.set_item(
        "NAME_RE",
        py.import("re")?.call_method1(
            "compile",
            (crate::source_logging_oracle::literal(
                py, archive, "NAME_RE", true,
            )?,),
        )?,
    )?;
    bindings.set_item("EXIT_OK", 0)?;
    bindings.set_item("EXIT_FAILED", 1)?;
    let render = crate::source_archive_oracle::namespace(py, archive, rows)?;
    for (key, value) in render.iter() {
        let key: String = key.extract()?;
        if !["__builtins__", "__name__"].contains(&key.as_str()) {
            bindings.set_item(key, value)?;
        }
    }
    let names = [
        "archive_root",
        "_base",
        "_state_file",
        "_sets",
        "_catalog",
        "check",
        "_sync",
    ];
    let original = crate::source_oracle_runtime::namespace(
        py,
        archive,
        &names,
        &[
            "str",
            "dict",
            "list",
            "int",
            "bool",
            "len",
            "sorted",
            "enumerate",
            "print",
            "OSError",
            "ValueError",
        ],
        &bindings,
    )?;
    let module = PyModule::new(py, "_original_archive_assertions")?;
    module.dict().update(original.as_mapping())?;
    let functions = names.into_iter().chain([
        "archive_name",
        "_folder_name",
        "_rows",
        "render_history_md",
        "render_manifest_md",
    ]);
    let function_type = py.import("types")?.getattr("FunctionType")?;
    for name in functions {
        let original = required(&original, name)?;
        let rebound = function_type.call1((
            original.getattr("__code__")?,
            module.dict(),
            original.getattr("__name__")?,
            original.getattr("__defaults__")?,
            original.getattr("__closure__")?,
        ))?;
        for attribute in ["__kwdefaults__", "__annotations__"] {
            rebound.setattr(attribute, original.getattr(attribute)?)?;
        }
        if !rebound.getattr("__globals__")?.is(&module.dict()) {
            return Err(PyValueError::new_err(
                "archive globals are not patchable module storage",
            ));
        }
        module.add(name, rebound)?;
    }
    module
        .dict()
        .get_item("__builtins__")?
        .ok_or_else(|| PyValueError::new_err("missing archive builtins"))?
        .set_item("__import__", Py::new(py, Imports)?)?;
    Ok(module)
}

fn inventory(workspace: &Path, paths: &[(PathBuf, Arc<AtomicBool>)]) -> PyResult<Vec<PathBuf>> {
    if paths.len() != 8
        || paths
            .iter()
            .any(|(_, exited)| !exited.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original archive temporary exit incomplete",
        ));
    }
    directories(workspace, workspace, &mut 256, 0, &mut vec![])?;
    let expected = paths
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    if actual != expected {
        return Err(PyValueError::new_err(
            "unexpected original archive workspace; retained",
        ));
    }
    let allowed = [
        "catalog.db",
        "state.json",
        "logo.png",
        "archive",
        "archive/Pack One",
        "collection",
        "collection/media",
        "collection/media/a.webp",
        "collection/media/x.webp",
        "archive/Pack One/001_logo.png",
        "archive/Pack One/_history.md",
        "archive/Pack One/_history.json",
        "archive/Pack One/_manifest.md",
        "archive/Pack One/002_static_aaaaaaaaaaaa.webp",
    ];
    let mut members = Vec::new();
    for root in expected {
        let mut pending = vec![root.clone()];
        while let Some(path) = pending.pop() {
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| PyValueError::new_err("foreign archive evidence"))?
                .to_str()
                .ok_or_else(|| PyValueError::new_err("non-UTF8 archive evidence"))?
                .replace('\\', "/");
            if path != root && !allowed.contains(&relative.as_str()) {
                return Err(PyValueError::new_err(
                    "unexpected original archive member; retained",
                ));
            }
            if path.is_dir() {
                pending.extend(
                    std::fs::read_dir(&path)?
                        .map(|entry| entry.map(|entry| entry.path()))
                        .collect::<std::io::Result<Vec<_>>>()?,
                );
            } else if std::fs::metadata(&path)?.len() > 65536 {
                return Err(PyValueError::new_err(
                    "excessive original archive evidence; retained",
                ));
            }
            members.push(path);
        }
    }
    Ok(members)
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let archive = module(
        py,
        workspace,
        [
            sources[0], sources[2], sources[3], sources[4], sources[5], sources[6],
        ],
    )?;
    let check = Py::new(
        py,
        Check {
            function: archive.getattr("check")?.unbind(),
            observations: Mutex::new(vec![]),
        },
    )?;
    archive.add("check", check.bind(py))?;
    let temporary = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(vec![]),
        },
    )?;
    let tempfile = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    tempfile.setattr(
        "TemporaryDirectory",
        temporary.bind(py).getattr("temporary")?,
    )?;
    let unittest = py.import("unittest")?;
    let bindings = PyDict::new(py);
    for (name, value) in [
        ("pa", archive.clone().into_any()),
        ("tempfile", tempfile),
        ("unittest", unittest.clone().into_any()),
        ("Path", py.import("pathlib")?.getattr("Path")?),
        ("ExitStack", py.import("contextlib")?.getattr("ExitStack")?),
        ("patch", py.import("unittest.mock")?.getattr("patch")?),
        ("sqlite3", py.import("sqlite3")?.into_any()),
        ("json", py.import("json")?.into_any()),
    ] {
        bindings.set_item(name, value)?;
    }
    bindings.set_item("KEY", format!("s:{}", "a".repeat(32)))?;
    bindings.set_item("ARCHIVED_NAME", "002_static_aaaaaaaaaaaa.webp")?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &[
            "_Env",
            "OnlyAFinishedPackIsArchived",
            "TheArchiveHasToFollowThePack",
            "TheNameCarriesTheSlot",
            "FolderNames",
            "AMovedFileStaysRecorded",
        ],
        &[
            "__build_class__",
            "str",
            "dict",
            "list",
            "len",
            "sorted",
            "any",
            "zip",
            "enumerate",
        ],
        &bindings,
    )?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    for (name, count) in CLASSES {
        let class = required(&tests, name)?;
        let names: Vec<String> = unittest
            .getattr("TestLoader")?
            .call0()?
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        if names.len() != count {
            return Err(PyValueError::new_err(
                "original archive method collection drift",
            ));
        }
        for method in names {
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
                        "original archive {name}.{method} reported {field}; retained"
                    )));
                }
            }
            cases.push(format!("{name}.{method}"));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 14
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original archive execution incomplete",
        ));
    }
    let checks = check
        .borrow(py)
        .observations
        .lock()
        .map_err(|_| PyRuntimeError::new_err("archive check observations poisoned"))?
        .clone();
    if checks.len() != 7 {
        return Err(PyValueError::new_err("original archive check count drift"));
    }
    let temporary = temporary.borrow(py);
    let paths = temporary
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("archive cleanup accounting poisoned"))?;
    let members = inventory(workspace, &paths)?;
    let last = &paths[7].0;
    let moved = last.join("archive/Pack One/002_static_aaaaaaaaaaaa.webp");
    if std::fs::read(&moved)? != b"a"
        || last.join("collection/media/a.webp").exists()
        || std::fs::read(last.join("logo.png"))? != b"logo"
    {
        return Err(PyValueError::new_err(
            "original partial archive evidence drift; retained",
        ));
    }
    let db = rusqlite::Connection::open_with_flags(
        last.join("catalog.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|_| PyValueError::new_err("cannot inspect original partial catalog"))?;
    let stored: String = db
        .query_row(
            "SELECT file_path FROM items WHERE content_key=?",
            [format!("s:{}", "a".repeat(32))],
            |row| row.get(0),
        )
        .map_err(|_| PyValueError::new_err("cannot read original committed path"))?;
    if Path::new(&stored) != moved {
        return Err(PyValueError::new_err(
            "original partial path was not committed; retained",
        ));
    }
    drop(db);
    let mut members = members;
    members.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in members {
        if path.is_dir() {
            std::fs::remove_dir(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":14,"cases":cases,"temporary_calls":8,"checks":checks,"partial_move":{"bytes":"a","stored":"archive/Pack One/002_static_aaaaaaaaaaaa.webp"},
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"14 unchanged original methods/real SQLite/path/shutil/atomic writer/original pure table renderer/private patchable archive/operator globals; unused Telegram import is inert, source writer ownership is nullcontext; original temporary deletion delayed until all assertions and known-member inventory; native warning/lock/transport evidence separate"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("archive workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect archive workspace")?
        .next()
        .is_some()
    {
        return Err("archive workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect archive bytecode policy")?
        {
            return Err("archive tests require disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original archive tests failed: {}",
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
        "archive-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
