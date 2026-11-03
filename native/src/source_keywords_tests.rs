//! Unchanged original keyword writer assertions, with private replacement and delayed cleanup ports.
use crate::source_git_inventory::Source;
use crate::source_temporary::Directories;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 2] = [
    (
        "coins/_keywords.py",
        "7c157ebff8fd5dbb8ec28c87ce8e85b70559601a8671b01c2510fe4e9f70f75f",
    ),
    (
        "tests/test_coin_keywords.py",
        "9da5cd00b4cf3fefbb04bde4b91ba7bb4118ace863cc606f8136bedecab4ef62",
    ),
];
const METHODS: [&str; 3] = [
    "test_a_crash_before_the_swap_leaves_the_old_file_intact",
    "test_an_empty_new_name_keeps_the_old_one",
    "test_an_invalid_cached_png_is_not_listed",
];

pub fn source_paths() -> [&'static str; 2] {
    IDENTITIES.map(|(path, _)| path)
}
pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("keyword replay requires exactly two pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("keyword source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let image = py.import("PIL.Image")?;
    if image.getattr("__version__")?.extract::<String>()? != "12.3.0" {
        return Err(PyValueError::new_err(
            "keyword replay requires Pillow12.3.0",
        ));
    }
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let private_os = ns.call0()?;
    let replace = py.import("os")?.getattr("replace")?;
    private_os.setattr("replace", &replace)?;
    let bindings = PyDict::new(py);
    for (name, value) in [
        ("csv", py.import("csv")?.into_any()),
        ("Image", image.into_any()),
        ("Path", py.import("pathlib")?.getattr("Path")?),
        ("os", private_os.clone()),
    ] {
        bindings.set_item(name, value)?;
    }
    bindings.set_item(
        "HEADER",
        crate::source_logging_oracle::literal(py, sources[0], "HEADER", false)?,
    )?;
    let original = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &["_valid_image", "_old_names", "write_keywords_csv"],
        &["open", "sorted", "print", "Exception"],
        &bindings,
    )?;
    let module = crate::source_roster_modules::rebind(py, &original, "_original_keyword_writer")?;
    let ledger = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let tempfile = ns.call0()?;
    tempfile.setattr("mkdtemp", ledger.bind(py).getattr("mkdtemp")?)?;
    let shutil = ns.call0()?;
    shutil.setattr("rmtree", ledger.bind(py).getattr("rmtree")?)?;
    for (name, value) in [
        ("_keywords", module.into_any()),
        ("tempfile", tempfile),
        ("shutil", shutil),
        ("unittest", py.import("unittest")?.into_any()),
        ("mock", py.import("unittest.mock")?.into_any()),
        ("contextlib", py.import("contextlib")?.into_any()),
        ("io", py.import("io")?.into_any()),
    ] {
        bindings.set_item(name, value)?;
    }
    bindings.set_item(
        "OLD",
        crate::source_logging_oracle::literal(py, sources[1], "OLD", false)?,
    )?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["KeywordsFileHasOneWriter"],
        &["__build_class__", "open", "set", "list", "OSError"],
        &bindings,
    )?;
    let class = required(&tests, "KeywordsFileHasOneWriter")?;
    let unittest = py.import("unittest")?;
    let methods: Vec<String> = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods != METHODS {
        return Err(PyValueError::new_err("original keyword collection drift"));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    for method in METHODS {
        class.call1((method,))?.call_method1("run", (&result,))?;
        for field in [
            "errors",
            "failures",
            "skipped",
            "expectedFailures",
            "unexpectedSuccesses",
        ] {
            if result.getattr(field)?.len()? != 0 {
                return Err(PyValueError::new_err(format!(
                    "original keyword {method} reported {field}; retained"
                )));
            }
        }
        if !private_os.getattr("replace")?.is(&replace) {
            return Err(PyValueError::new_err(
                "original keyword replacement patch was not restored",
            ));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 3
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original keyword execution incomplete",
        ));
    }
    let ledger = ledger.borrow(py);
    let paths = ledger
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("keyword temporary accounting poisoned"))?;
    if paths.len() != 3
        || paths
            .iter()
            .any(|(_, cleaned)| !cleaned.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original keyword teardown incomplete; retained",
        ));
    }
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
    let expected = paths
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if actual != expected || crate::process_owner::cleanup_pending(workspace) {
        return Err(PyValueError::new_err(
            "unexpected original keyword scratch; retained",
        ));
    }
    for (path, _) in paths.iter() {
        directories(workspace, path, &mut 32, 0, &mut Vec::new())?;
        let members = std::fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
        if members
            != ["keywords.csv", "logos"]
                .map(std::ffi::OsString::from)
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
            || path.join("keywords.csv.tmp").exists()
        {
            return Err(PyValueError::new_err(
                "unexpected original keyword evidence; retained",
            ));
        }
    }
    for (path, _) in paths.iter() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":"12.3.0",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":3,"temporary_calls":3,"cases":METHODS.map(|method|format!("KeywordsFileHasOneWriter.{method}")),
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"unchanged original writer/classes/setup/teardown; real csv/Pillow/files and original injected replace OSError; private os replacement, delayed project-contained mkdtemp/rmtree ledger; native shared-atomic crash assertion remains separate"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("keyword workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect keyword workspace")?
        .next()
        .is_some()
    {
        return Err("keyword workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect keyword bytecode policy")?
        {
            return Err("keyword replay requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original keyword replay failed: {}",
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
        "keyword-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
