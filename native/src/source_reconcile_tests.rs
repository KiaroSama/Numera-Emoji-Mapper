//! Exact original live reconciliation assertions; real historical SQLite/images, no Telegram network.
use crate::source_git_inventory::Source;
use crate::source_temporary::Directories;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use crate::source_reconcile_namespace as namespace;
#[path = "source_plan_apply_replay.rs"]
mod plan_apply;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
pub(crate) const IDENTITIES: [(&str, &str); 34] = [
    (
        "tests/reference/collection_reconcile.py",
        "5f9aa1cd99411c508d860133711bd08919faf904d6c13f387993e76887bda4e7",
    ),
    (
        "tests/test_build_collection_state.py",
        "479e52e0e83c9e0e4f2edb4a29198d9360e3312eb6ce279f39d31090a8288469",
    ),
    (
        "tests/_bc_fixtures.py",
        "63ccf0e7cada4aa77af627f6ca5070a7318d26ef246d215372c2e482d94d60c0",
    ),
    (
        "tests/_media_fixtures.py",
        "1c45ebbb66c1d06bab2fb897e1c24166646974cf02e6fd0d8b54c93c1df8f5e4",
    ),
    (
        "emojikit/media.py",
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
    ),
    (
        "emojikit/identity.py",
        "8cbefd550e306e671df1e52af2dd9fe6b4e3647c3aa1b13dc98bf35080d36e89",
    ),
    (
        "tests/reference/catalog.py",
        "5dab2e475754dc219dfe3fd8e98012fffd59ab0c760f646752ff9d35f7188ee2",
    ),
    (
        "emojikit/media_paths.py",
        "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3",
    ),
    (
        "tests/reference/ingest.py",
        "6654f129fba59b600d8498c7d734a36973fd9761276075d1f23a28974ba69162",
    ),
    (
        "tests/_similarity_oracle.py",
        "9be48166b64ab2052798c67e2f7426c2dd0008fc27b4d668a785877fba7ac44e",
    ),
    (
        "tests/reference/collection_state.py",
        "b8bf2695954cbe61a3e3cb68c8da0c583aa80a1b645abba01eabe9a28776074a",
    ),
    (
        "tests/reference/build_collection.py",
        "e651186017023c8adf7a4a5525717f29e0ca78842a91c08ec25d0230b2c3a62e",
    ),
    (
        "emojikit/telegram_api.py",
        "712554abbfeb63f72f38c4a27261b973bc81a7497991615223df44bec29265e8",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
    (
        "tests/test_reconcile_identity.py",
        "effc8d513f5ad333f69ef2c360f21e75b6e70136b4e7dd10e47e982dac2d53cf",
    ),
    (
        "tests/test_reconcile_uniqueness.py",
        "f35df641b326dc2d4c782974083287de2b47804d85f04aa06b29507bc29e099a",
    ),
    (
        "tests/test_plan_apply.py",
        "d25069fb9133c1fcfe28ea2ab0eb6b250516aedcee9b07fe50c2372480356928",
    ),
    (
        "tests/reference/plan_apply.py",
        "4bff9d8f12a64d1b04d204dcca25f9737df912f408b559bfc9241254341b8f62",
    ),
    (
        "tests/reference/panel_plan.py",
        "14d4df245dbb13a13c768f405ba0217b2ea885b204d19c2c4d8a691177a7100e",
    ),
    (
        "tests/reference/sync_order.py",
        "00a0765118e3e0a2ff2999b7c1e4a649434147786da2adb9a9c1e710864112e0",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
    (
        "emojikit/state_artifacts.py",
        "3b0faf872a3d7ee9ebfd8324060e7c2ff7541ead4b743fa30e5ebd9939248001",
    ),
    (
        "tests/oracles/telegram_api.py",
        "59f5c958a2f7a2f7ed32b93669861066bf3a3f6a6a859623f7456405b5c56aa3",
    ),
    (
        "tests/reference/collection_media_check.py",
        "c7a9baa959cc796f68db5b1c633ae12e8caadd6e21a32e9e9d6c7a4b04b6d50e",
    ),
    (
        "tests/reference/collection_names.py",
        "b3b7925d7a1e92147d07a296c91156f59dc6518298e641bc8bd16c5edb0fc9d5",
    ),
    (
        "emojikit/operator_config.py",
        "a19f804a7bdd37baccb474eac96df6aa4d46439dd956e457c06adf3c180b9591",
    ),
    (
        "emojikit/cli_env.py",
        "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
    ),
    (
        "tests/test_telegram_client.py",
        "3cba78d05dc735bf116f98095b09393d70db8b4ae4d2980eefabdb75a7fd8981",
    ),
    (
        "tests/test_telegram_body_shape.py",
        "6d9e2b2afa7877aff000290f9365203f7a94393d1105b4696a62b66b1d808b4c",
    ),
    (
        "tests/_pack_fixtures.py",
        "bbac616f05c178647206528c015f4ba85df29a36c93fb69de52f2a0d505368d8",
    ),
    (
        "tests/reference/build_pack.py",
        "11e7e3bca19e9625efd4055823d40aa4da2f711aec942460546edcf0917e6306",
    ),
    (
        "tests/test_resume_safety.py",
        "0af175ef5368ed7a6613ce487f65ff5b8f42e1751fcfa9e787b17faea956b3d4",
    ),
    (
        "tests/test_unresolved_mutation.py",
        "41261628bace24747c9c205c0d1c632547c4d02b3883e89422b368fab9807d16",
    ),
    (
        "tests/test_build_pack_announce.py",
        "5e6e86e98e21d4d52e75fa1df44de19e439dee9b77d3e116d7c3000d3fcd544a",
    ),
];
const CLASSES: [(&str, usize); 4] = [
    ("LiveSetDrift", 11),
    ("ForeignIdentityOnARecordedPosition", 6),
    ("UnattributedTail", 4),
    ("OurEmojiBehindAForeignSticker", 3),
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("reconciliation oracle requires exactly thirty-four pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("reconciliation source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let ports = namespace::build(py, workspace, sources)?;
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
    let unittest = py.import("unittest")?;
    let fixture_bindings = required(&ports, "fixture_bindings")?.cast_into::<PyDict>()?;
    fixture_bindings.set_item("unittest", &unittest)?;
    fixture_bindings.set_item("tempfile", &temporary_api)?;
    let fixture = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["_CatalogFixture"],
        &["__build_class__", "range", "list", "str", "enumerate"],
        &fixture_bindings,
    )?;
    ports.set_item("_CatalogFixture", required(&fixture, "_CatalogFixture")?)?;
    let bindings = PyDict::new(py);
    bindings.set_item("_CatalogFixture", required(&fixture, "_CatalogFixture")?)?;
    for name in ["Catalog", "cr", "bc", "cs", "LiveStateUnknown"] {
        bindings.set_item(name, required(&ports, name)?)?;
    }
    let fakes = required(&ports, "fakes")?.cast_into::<PyDict>()?;
    for name in ["FakeTG", "DownloadingTG", "_sticker"] {
        bindings.set_item(name, required(&fakes, name)?)?;
    }
    bindings.set_item("SET", "pks1_by_YourEmojiBot")?;
    bindings.set_item("mock", py.import("unittest.mock")?)?;
    let classes = CLASSES.map(|(name, _)| name);
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &classes,
        &["__build_class__", "str", "zip"],
        &bindings,
    )?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    let mut observations = Vec::new();
    for (name, expected) in CLASSES {
        let class = required(&tests, name)?;
        let methods: Vec<String> = loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        if methods.len() != expected {
            return Err(PyValueError::new_err(
                "original reconciliation collection drift",
            ));
        }
        for method in methods {
            let test = class.call1((&method,))?;
            test.call_method1("run", (&result,))?;
            for field in [
                "errors",
                "failures",
                "skipped",
                "expectedFailures",
                "unexpectedSuccesses",
            ] {
                if result.getattr(field)?.len()? != 0 {
                    return Err(PyValueError::new_err(format!(
                        "original reconciliation {name}.{method} reported {field}; evidence retained"
                    )));
                }
            }
            let data: PathBuf = test.getattr("data")?.extract()?;
            let data =
                crate::tooling_environment::physical_root(&data).map_err(PyValueError::new_err)?;
            if data.parent() != Some(workspace) {
                return Err(PyValueError::new_err("foreign reconciliation observation"));
            }
            crate::tooling_environment::reject_links(
                &data.join("catalog.db"),
                "linked original reconciliation database",
            )
            .map_err(PyValueError::new_err)?;
            let keys: Vec<String> = test.getattr("keys")?.extract()?;
            if keys.len() != 2 {
                return Err(PyValueError::new_err(
                    "original reconciliation key fixture drift",
                ));
            }
            let db = py
                .import("sqlite3")?
                .call_method1("connect", (data.join("catalog.db"),))?;
            let rows = (|| -> PyResult<Value> {
                let mut publications = Vec::new();
                for row in db.call_method1("execute", ("SELECT base,content_key,set_name,custom_emoji_id FROM publications ORDER BY base,content_key",))?.try_iter()? {
                    let row = row?;
                    let key: String = row.get_item(1)?.extract()?;
                    let key_index = keys.iter().position(|candidate| candidate == &key)
                        .ok_or_else(|| PyValueError::new_err("foreign original publication key"))?;
                    publications.push(json!({"base":row.get_item(0)?.extract::<String>()?,
                        "key_index":key_index,
                        "set_name":row.get_item(2)?.extract::<Option<String>>()?,"custom_emoji_id":row.get_item(3)?.extract::<Option<String>>()?}));
                }
                let count: usize = db
                    .call_method1("execute", ("SELECT COUNT(*) FROM items",))?
                    .call_method0("fetchone")?
                    .get_item(0)?
                    .extract()?;
                Ok(json!({"items":count,"publications":publications}))
            })();
            let closed = db.call_method0("close");
            let rows = rows?;
            closed?;
            cases.push(format!("{name}.{method}"));
            observations.push(json!({"case":format!("{name}.{method}"),"catalog":rows}));
        }
    }
    let extra_bindings = PyDict::new(py);
    for name in ["Catalog", "cr", "identity"] {
        extra_bindings.set_item(name, required(&ports, name)?)?;
    }
    extra_bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    extra_bindings.set_item("Image", py.import("PIL.Image")?)?;
    extra_bindings.set_item("tempfile", &temporary_api)?;
    extra_bindings.set_item("unittest", &unittest)?;
    extra_bindings.set_item("mock", py.import("unittest.mock")?)?;
    extra_bindings.set_item("sqlite3", py.import("sqlite3")?)?;
    for (index, classes) in [
        (
            14,
            vec![
                ("NearCatalogMatchVerifiesBeforeItAttributes", 8),
                ("TheThreeNonKeyAnswersStayApart", 1),
                ("TheResolverActsOnEachAnswerDifferently", 4),
                ("ScratchFilesArePrivateAndAlwaysRemoved", 4),
            ],
        ),
        (
            15,
            vec![
                ("UniqueRequiresExcludingEveryRival", 10),
                ("NothingAmbiguousIsEverWritten", 1),
            ],
        ),
    ] {
        let definitions = classes.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        let tests = crate::source_oracle_runtime::namespace(
            py,
            sources[index],
            &definitions,
            &["__build_class__", "list", "len", "type", "ValueError"],
            &extra_bindings,
        )?;
        for (name, count) in classes {
            let class = required(&tests, name)?;
            let methods: Vec<String> = loader
                .call_method1("getTestCaseNames", (&class,))?
                .extract()?;
            if methods.len() != count {
                return Err(PyValueError::new_err("original identity collection drift"));
            }
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
                            "original identity {name}.{method} reported {field}; evidence retained"
                        )));
                    }
                }
                cases.push(format!("{name}.{method}"));
            }
        }
    }
    cases.extend(plan_apply::run(
        py,
        workspace,
        sources,
        &ports,
        &temporary_api,
        &result,
    )?);
    if result.getattr("testsRun")?.extract::<usize>()? != 145
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original reconciliation assertion execution incomplete",
        ));
    }
    let state = temporary.borrow(py);
    let paths = state
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("reconciliation cleanup accounting poisoned"))?;
    if paths.len() != 127
        || paths
            .iter()
            .any(|(_, cleaned)| !cleaned.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original reconciliation teardown incomplete",
        ));
    }
    // Keep the earlier 51-context inventory budget; added Family fixtures have
    // at most 17 source images and get their own bounded per-context inventory.
    let mut original_budget = 512;
    for (index, (path, _)) in paths.iter().enumerate() {
        let mut family_budget = 64;
        let budget = if index < 51 {
            &mut original_budget
        } else {
            &mut family_budget
        };
        directories(workspace, path, budget, 0, &mut Vec::new())?;
    }
    let expected = paths
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
    if actual != expected {
        return Err(PyValueError::new_err(
            "unexpected original reconciliation scratch; retained",
        ));
    }
    for (index, (path, _)) in paths.iter().enumerate() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let name = entry.file_name();
            let permitted = if index < 24 {
                matches!(
                    name.to_str(),
                    Some("catalog.db" | "catalog.db-wal" | "catalog.db-shm" | "media" | "tmp")
                )
            } else if index >= 125 {
                matches!(
                    name.to_str(),
                    Some("src" | "state_TELEGRAM_BOT_TOKEN.json" | "state_GENERAL_BOT_TOKEN.json")
                )
            } else if index >= 92 {
                matches!(name.to_str(), Some("src" | "state.json"))
            } else if index >= 90 {
                name == "logo.png"
            } else if index >= 74 {
                name == "ours.png"
            } else if index >= 51 {
                matches!(
                    name.to_str(),
                    Some(
                        "catalog.db"
                            | "catalog.db-wal"
                            | "catalog.db-shm"
                            | "media"
                            | "tmp"
                            | "brand"
                            | "brand-logo.png"
                            | "pack_plan.json"
                            | "publish_cryptoemoji.json"
                            | "publish_cryptoemoji.lock"
                            | "plan_apply_cryptoemoji.json"
                            | "publish_pk.json"
                            | "publish_plan_pk.json"
                            | "publish_pk.lock"
                    )
                )
            } else {
                let name = name
                    .to_str()
                    .ok_or_else(|| PyValueError::new_err("non-UTF8 original identity evidence"))?;
                matches!(
                    name,
                    "dl" | "red.png"
                        | "blue.png"
                        | "a.png"
                        | "b.png"
                        | "probe.png"
                        | "ghost.png"
                        | "red_reencoded.webp"
                        | "broken.png"
                        | "x.tgs"
                ) || [
                    "red.db", "ab.db", "ghost.db", "empty.db", "two.db", "one.db", "cat.db",
                ]
                .iter()
                .any(|base| {
                    name == *base || name == format!("{base}-wal") || name == format!("{base}-shm")
                })
            };
            if !permitted {
                return Err(PyValueError::new_err(
                    "unexpected original reconciliation evidence; retained",
                ));
            }
            if matches!(name.to_str(), Some("tmp" | "dl"))
                && std::fs::read_dir(entry.path())?.next().is_some()
            {
                return Err(PyValueError::new_err(
                    "original reconciliation left download scratch",
                ));
            }
        }
    }
    for (path, _) in paths.iter() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":"12.3.0",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":145,"cases":cases,"observations":observations,"temporary_calls":127,
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"unchanged original52live/foreign/tail/look-behind/identity/uniqueness/scratch,20applier,5fresh-upload,33transport and35ticker-bookkeeping/resume/capacity/unresolved/announcer assertion methods; original fakes/catalog/static identity/publisher/removal/reorder/atomic state and OS family lock; real SQLite/Pillow; no-op native prerequisite/catalog ownership/manifest output, private synthetic environment/HTTP/sleep/own-PID diagnostic, delayed verified teardown; native enumeration/private-context/cancellation/wire seams remain adaptations, not native exception identity or production transport equivalence"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("reconciliation workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect reconciliation workspace")?
        .next()
        .is_some()
    {
        return Err("reconciliation workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect reconciliation bytecode policy")?
        {
            return Err("reconciliation oracle requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original reconciliation replay failed: {}",
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
        "reconcile-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
