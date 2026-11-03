//! Fixed original publisher assertions; same Git corpus, selector and bounded historical owner.
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

#[path = "source_publication_replay.rs"]
mod replay;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const EXTRA: [(&str, &str); 10] = [
    (
        "tests/test_publish_cli.py",
        "ca4d80ac36768ba1e159e7bb99b666956753f4c98e5685a204a760d189ff8af6",
    ),
    (
        "tests/test_publish_dedup.py",
        "c812bf3d425fabe64bad4f90064154aee3e4dd97c3bab5e1911ec6ffed5eb697",
    ),
    (
        "tests/test_publish_invariants.py",
        "948eb39641cbebd39397609769cd09502f031745b541d23e421a13dba6a839a1",
    ),
    (
        "tests/reference/collection_notify.py",
        "a153266b6a092b16fe28be5a2151265f4c445967330b0530c14072b824ed3b59",
    ),
    (
        "tests/reference/announce.py",
        "a10d73deec77e1de3a7b1e64b8c78dbde43bb5e5af031aee38d26fe7a85db4c8",
    ),
    (
        "tests/reference/collection_preflight.py",
        "082c0c8aacee2b1b702bc51e69d8916de3efe98d4983d7b9cb904be2fff16daa",
    ),
    (
        "tests/oracles/pack_rows.py",
        "1c0c0fc15ab5b6abcecb182f138f89da55c566c7210102c1eaaa5ad115343121",
    ),
    (
        "tests/test_brand_logo.py",
        "b2b371af99a0fce633f1f7ac8af2d7b31d34461f13a450cd2bdaf11b4ac3d358",
    ),
    (
        "tests/test_publish_contracts.py",
        "b281194e3ff9f3fed14a94ea6b2a233e0bf75245d0fb779ed717a3fa5892af61",
    ),
    (
        "emojikit/media_lottie.py",
        "5112b55f21d4d241ffe9660d5294de20487d4ea1e9772188fb6bd125f7fef63b",
    ),
];
fn identities() -> [(&'static str, &'static str); 37] {
    std::array::from_fn(|i| {
        if i < 27 {
            crate::source_reconcile_tests::IDENTITIES[i]
        } else {
            EXTRA[i - 27]
        }
    })
}
pub fn source_paths() -> [&'static str; 37] {
    identities().map(|(path, _)| path)
}
pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != 37 {
        return Err("publication oracle requires exactly thirty-seven pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(identities()) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("publication source identity drift; refusing execution".into());
        }
    }
    Ok(())
}
fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let ports = crate::source_reconcile_namespace::build(py, workspace, sources)?;
    let temporary = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let temporary_api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    temporary_api.setattr(
        "TemporaryDirectory",
        temporary.bind(py).getattr("temporary")?,
    )?;
    let bindings = required(&ports, "fixture_bindings")?.cast_into::<PyDict>()?;
    bindings.set_item("unittest", py.import("unittest")?)?;
    bindings.set_item("tempfile", &temporary_api)?;
    let fixture = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["_CatalogFixture"],
        &["__build_class__", "range", "list", "str", "enumerate"],
        &bindings,
    )?;
    ports.set_item("_CatalogFixture", required(&fixture, "_CatalogFixture")?)?;
    let cases = replay::run(py, workspace, sources, &ports, &temporary_api)?;
    let state = temporary.borrow(py);
    let paths = state
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("publication cleanup accounting poisoned"))?;
    if paths.len() != 80
        || paths
            .iter()
            .any(|(_, cleaned)| !cleaned.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original publication teardown incomplete; retained",
        ));
    }
    let source_path = workspace.join("publisher-source.txt");
    let metadata = std::fs::symlink_metadata(&source_path)?;
    if !metadata.is_file()
        || crate::tooling_environment::reject_links(&source_path, "linked publisher source")
            .is_err()
        || metadata.len() != sources[11].text.len() as u64
        || std::fs::read(&source_path)? != sources[11].text.as_bytes()
    {
        return Err(PyValueError::new_err(
            "original publisher source bytes drift; retained",
        ));
    }
    let mut expected = paths
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    expected.insert(source_path.clone());
    let actual = std::fs::read_dir(workspace)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
    if actual != expected {
        return Err(PyValueError::new_err(
            "unexpected original publication scratch; retained",
        ));
    }
    for (path, _) in paths.iter() {
        // The original 200-row capacity method creates 200 real source PNGs.
        directories(workspace, path, &mut 256, 0, &mut Vec::new())?;
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let name = entry.file_name();
            if !matches!(
                name.to_str(),
                Some(
                    "catalog.db"
                        | "catalog.db-wal"
                        | "catalog.db-shm"
                        | "media"
                        | "tmp"
                        | "brand"
                        | "logo.png"
                        | "corrupt.png"
                        | "blank.png"
                        | "manifests"
                        | "publish_pk.json"
                        | "publish_pk.lock"
                        | "publish_plan_pk.json"
                        | "publish_b.json"
                        | "publish_clos.json"
                        | "publish_c.json"
                        | "publish_plan_b.json"
                        | "e.png"
                        | "sticker.dl"
                )
            ) {
                return Err(PyValueError::new_err(
                    "unexpected original publication member; retained",
                ));
            }
            if name == "tmp" && std::fs::read_dir(entry.path())?.next().is_some() {
                return Err(PyValueError::new_err(
                    "original publication left download scratch",
                ));
            }
        }
    }
    for (path, _) in paths.iter() {
        std::fs::remove_dir_all(path)?;
    }
    std::fs::remove_file(source_path)?;
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":"12.3.0",
        "source_hashes":identities().into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":84,"cases":cases,"temporary_calls":80,"policy_differences":["VerifiedRetryTest.test_without_expected_before_keeps_legacy_retry: historical blind retry is retained as source evidence; native no-snapshot mutation remains Ambiguous and never retries"],"errors":0,"failures":0,"skipped":0,
        "expected_failures":0,"unexpected_successes":0,
        "ports":"unchanged original publisher CLI/dedup/closed-set/item-format/corrupt-logo assertions; original manifest/preflight/notification and two unchanged source-reading assertions over inert admitted publisher bytes, real original SQLite/Pillow/locks/atomic state, private synthetic environment/HTTP/sleep and scoped logger registry; no-op native prerequisite/ownership, RuntimeError media adaptation, delayed verified teardown; native mixed/wire/artwork/numeric-CID/log-sink adaptations remain separate"}),
    )
}
pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("publication workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect publication workspace")?
        .next()
        .is_some()
    {
        return Err("publication workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|s| s.getattr("dont_write_bytecode"))
            .and_then(|v| v.extract::<bool>())
            .map_err(|_| "cannot inspect publication bytecode policy")?
        {
            return Err("publication oracle requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|e| {
            format!(
                "original publication replay failed: {}",
                crate::logging::redact(&e.to_string(), &[])
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
        "publication-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
