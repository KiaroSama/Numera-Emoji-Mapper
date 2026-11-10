//! Original blank-provider assertions reuse admitted original conversion and delayed directory cleanup.
use crate::source_git_inventory::Source;
use crate::source_temporary::Directories;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 4] = [
    (
        "emojikit/media.py",
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
    ),
    (
        "coins/_paprika_api.py",
        "34742583025adaad3c7513ce0031c13d32759e29cfa8076609150a0badeb1acc",
    ),
    (
        "tests/_coin_fixtures.py",
        "eb95df6b040a2b57ba521ec7f84405dad33cf3be7c7de61d8ccffb3d50475389",
    ),
    (
        "tests/test_coin_providers.py",
        "57694a3668a8bde79ca081053ac1da5a43da3e5bdc78803e5db45e8fcf842012",
    ),
];
const METHODS: [&str; 3] = [
    "test_almost_empty_image_is_rejected",
    "test_fully_transparent_image_is_rejected",
    "test_real_logo_is_still_accepted",
];

pub fn source_paths() -> [&'static str; 4] {
    IDENTITIES.map(|(path, _)| path)
}
pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("provider image replay requires exactly four pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("provider image source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let image = py.import("PIL.Image")?;
    if image.getattr("__version__")?.extract::<String>()? != "12.3.0" {
        return Err(PyValueError::new_err(
            "provider image replay requires Pillow12.3.0",
        ));
    }
    let (_, provider) = crate::source_static_oracle::namespaces(py, sources[0], sources[1])?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let api = ns.call0()?;
    api.setattr("to_emoji_png", required(&provider, "to_emoji_png")?)?;
    let bindings = PyDict::new(py);
    bindings.set_item("Image", image)?;
    bindings.set_item("io", py.import("io")?)?;
    let helpers = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["_png_bytes", "_gradient"],
        &["range"],
        &bindings,
    )?;
    for name in ["_png_bytes", "_gradient"] {
        bindings.set_item(name, required(&helpers, name)?)?;
    }
    let ledger = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let tempfile = ns.call0()?;
    tempfile.setattr("TemporaryDirectory", ledger.bind(py).getattr("temporary")?)?;
    bindings.set_item("tempfile", tempfile)?;
    bindings.set_item("fp", api)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    let unittest = py.import("unittest")?;
    bindings.set_item("unittest", &unittest)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["BlankProviderLogo"],
        &["__build_class__", "range"],
        &bindings,
    )?;
    let class = required(&tests, "BlankProviderLogo")?;
    let methods: Vec<String> = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods != METHODS {
        return Err(PyValueError::new_err(
            "original provider image collection drift",
        ));
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
                    "original provider image {method} reported {field}: {}; retained",
                    crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                )));
            }
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 3
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original provider image execution incomplete",
        ));
    }
    let ledger = ledger.borrow(py);
    let paths = ledger
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("provider image temporary accounting poisoned"))?;
    if paths.len() != 3
        || paths
            .iter()
            .any(|(_, cleaned)| !cleaned.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original provider image teardown incomplete; retained",
        ));
    }
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    if actual
        != paths
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<BTreeSet<_>>()
        || crate::process_owner::cleanup_pending(workspace)
    {
        return Err(PyValueError::new_err(
            "unexpected provider image scratch; retained",
        ));
    }
    for (index, (path, _)) in paths.iter().enumerate() {
        directories(workspace, path, &mut 8, 0, &mut Vec::new())?;
        let actual = std::fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        let expected = if index == 2 {
            BTreeSet::from([std::ffi::OsString::from("out.png")])
        } else {
            BTreeSet::new()
        };
        if actual != expected {
            return Err(PyValueError::new_err(
                "unexpected original provider image files; retained",
            ));
        }
    }
    for (path, _) in paths.iter() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":"12.3.0",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":3,"temporary_calls":3,"cases":METHODS.map(|method|format!("BlankProviderLogo.{method}")),
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"unchanged original provider/media/image helpers/class/setup/teardown/assertions; real Pillow/files; existing MediaError=RuntimeError namespace and delayed project-contained temporary cleanup; native errors/cancellation remain separate"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("provider image workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect provider image workspace")?
        .next()
        .is_some()
    {
        return Err("provider image workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect provider image bytecode policy")?
        {
            return Err("provider image replay requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original provider image replay failed: {}",
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
        "provider-image-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
