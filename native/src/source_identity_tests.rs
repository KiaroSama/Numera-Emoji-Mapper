//! Unchanged identity/provenance assertions share the pinned corpus and bounded historical worker.
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

#[path = "source_identity_bundle.rs"]
mod bundle;
#[path = "source_identity_modules.rs"]
mod modules;
#[path = "source_identity_report.rs"]
mod report;
const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 21] = [
    (
        "tests/test_identity_migration.py",
        "7a17796f09d44a35d3bbfe10822c9102cc34f7af0ad14259e6f066d573dcba12",
    ),
    (
        "tests/test_migration_provenance.py",
        "ff81d2c44a7aacb84c656e5b4e5514829b53ef7e1ffb06085616e8f089761489",
    ),
    (
        "tests/reference/collection_migrate.py",
        "b4cdee1858f5f5ef5b4f3e9d187159155efa7d3af7c191e64e0f81dc0e809dd9",
    ),
    (
        "tests/reference/migration_bundle.py",
        "c42b661d5e88f64a00780d95cbf88bbf38647c2ae2c661e25c7bd420a2cc0241",
    ),
    (
        "tests/reference/identity_repair.py",
        "40ba02c5dfe0bc26381956831be181a771a5ec674689cec16205851c725e26bb",
    ),
    (
        "tests/reference/maintenance.py",
        "d145a381f6da57a03551eeec76d1812e5e521af707e6dc7e7925dc0852b86278",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
    (
        "tests/reference/catalog.py",
        "5dab2e475754dc219dfe3fd8e98012fffd59ab0c760f646752ff9d35f7188ee2",
    ),
    (
        "tests/reference/collection_state.py",
        "b8bf2695954cbe61a3e3cb68c8da0c583aa80a1b645abba01eabe9a28776074a",
    ),
    (
        "emojikit/media_paths.py",
        "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3",
    ),
    (
        "emojikit/sqlite_snapshot.py",
        "8e3dd41f6607f1f1e4da4b777192088b9b1b370cf429774968a0771a1f78ca4d",
    ),
    (
        "emojikit/state_artifacts.py",
        "3b0faf872a3d7ee9ebfd8324060e7c2ff7541ead4b743fa30e5ebd9939248001",
    ),
    (
        "emojikit/identity.py",
        "8cbefd550e306e671df1e52af2dd9fe6b4e3647c3aa1b13dc98bf35080d36e89",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
    (
        "tests/test_migration_lifecycle.py",
        "7ba50a6cb12c9e6408239ff76d13e0edd34d909c5d6323e72527e283ca7439c9",
    ),
    (
        "tests/_migration_process.py",
        "67b9749507571b358eb0e5f78fc86434220ef0b8703aa9f373177d74c1fadc46",
    ),
    (
        "tests/reference/collection_reconcile.py",
        "5f9aa1cd99411c508d860133711bd08919faf904d6c13f387993e76887bda4e7",
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
        "emojikit/telegram_api.py",
        "712554abbfeb63f72f38c4a27261b973bc81a7497991615223df44bec29265e8",
    ),
    (
        "emojikit/media.py",
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
    ),
];
const EXCLUDED: [&str; 3] = [
    "MigrationCannotCertifyDamage.test_native_death_after_second_move_replays_via_cli",
    "ActualWriterOwnership.test_first_family_publisher_and_catalog_exclude_migration_in_real_children",
    "ActualWriterOwnership.test_pending_journal_blocks_ingest_archive_publisher_and_panel_save",
];
pub fn source_paths() -> [&'static str; 21] {
    IDENTITIES.map(|(path, _)| path)
}
pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("identity replay requires exactly21pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("identity replay source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn execute(py: Python<'_>, root: &Path, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let modules = modules::build(py, workspace, sources)?;
    let ledger = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let temporary = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    temporary.setattr("TemporaryDirectory", ledger.bind(py).getattr("temporary")?)?;
    let values = PyDict::new(py);
    for name in [
        "json",
        "sqlite3",
        "threading",
        "unittest",
        "io",
        "contextlib",
    ] {
        values.set_item(name, py.import(name)?)?;
    }
    values.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    values.set_item("mock", py.import("unittest.mock")?)?;
    values.set_item(
        "ROOT",
        py.import("pathlib")?.getattr("Path")?.call1((workspace,))?,
    )?;
    values.set_item("cm", &modules.migration)?;
    values.set_item("ir", &modules.repair)?;
    values.set_item("packstate", &modules.state)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &[
            "_Catalog",
            "MigrationCase",
            "EveryDurableReferenceMoves",
            "MigratedPathsStayDataRelative",
            "NothingIsAppliedFromAnIncompletePicture",
            "APreexistingPartialMigrationIsRecoverable",
            "TheBackupIsWalSafe",
            "AnInterruptedMigrationIsResumable",
        ],
        &[
            "__build_class__",
            "str",
            "dict",
            "list",
            "sorted",
            "len",
            "Exception",
            "RuntimeError",
            "KeyboardInterrupt",
        ],
        &values,
    )?;
    let imports = required(&tests, "__builtins__")?;
    imports.set_item("__import__", modules.imports.bind(py))?;
    // Only the inherited original setup's lazy tempfile import uses the shared ledger.
    let original_import = modules.imports.clone_ref(py);
    let setup_globals = required(&tests, "MigrationCase")?
        .getattr("setUp")?
        .getattr("__globals__")?;
    let setup_imports = Py::new(
        py,
        TestImports {
            base: original_import,
            temporary: temporary.clone().unbind(),
        },
    )?;
    setup_globals
        .get_item("__builtins__")?
        .set_item("__import__", setup_imports.bind(py))?;
    let provenance_values = values.copy()?;
    provenance_values.set_item("MigrationCase", required(&tests, "MigrationCase")?)?;
    provenance_values.set_item("_Catalog", required(&tests, "_Catalog")?)?;
    let provenance = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["LegacyRecoveryRequiresProvenance"],
        &["__build_class__", "str"],
        &provenance_values,
    )?;
    required(&provenance, "__builtins__")?.set_item("__import__", setup_imports.bind(py))?;
    let lifecycle_values = provenance_values.copy()?;
    lifecycle_values.set_item("tempfile", temporary)?;
    lifecycle_values.set_item("bundle", &modules.bundle)?;
    lifecycle_values.set_item("_lock_path", modules.collection.getattr("_lock_path")?)?;
    lifecycle_values.set_item("media_paths", modules.migration.getattr("media_paths")?)?;
    lifecycle_values.set_item("maintenance", modules.migration.getattr("maintenance")?)?;
    lifecycle_values.set_item("Catalog", &modules.catalog)?;
    let lifecycle = crate::source_oracle_runtime::namespace(
        py,
        sources[14],
        &["MigrationCannotCertifyDamage", "ActualWriterOwnership"],
        &[
            "__build_class__",
            "bytes",
            "str",
            "next",
            "type",
            "range",
            "enumerate",
            "RuntimeError",
            "KeyboardInterrupt",
        ],
        &lifecycle_values,
    )?;
    required(&lifecycle, "__builtins__")?.set_item("__import__", setup_imports.bind(py))?;
    let unittest = py.import("unittest")?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let writer = modules.state.getattr("write_json_atomic")?;
    let fingerprint = modules.identity.getattr("fingerprint")?;
    let migration_functions = [
        "_apply_files",
        "_apply_database",
        "_write_journal",
        "_verify_backup",
    ]
    .map(|name| Ok((name, modules.migration.getattr(name)?)))
    .into_iter()
    .collect::<PyResult<Vec<_>>>()?;
    let move_file = modules.bundle.getattr("move_file")?;
    let os = modules.bundle.getattr("os")?;
    let link = os.getattr("link")?;
    let sys = py.import("sys")?;
    let stdout = sys.getattr("stdout")?;
    let threading = py.import("threading")?;
    let threads = threading.call_method0("enumerate")?;
    let mut cases = Vec::new();
    let mut alias_directories = BTreeSet::new();
    for (globals, classes) in [
        (
            &tests,
            vec![
                ("EveryDurableReferenceMoves", 7),
                ("MigratedPathsStayDataRelative", 2),
                ("NothingIsAppliedFromAnIncompletePicture", 10),
                ("APreexistingPartialMigrationIsRecoverable", 4),
                ("TheBackupIsWalSafe", 5),
                ("AnInterruptedMigrationIsResumable", 5),
            ],
        ),
        (&provenance, vec![("LegacyRecoveryRequiresProvenance", 9)]),
        (
            &lifecycle,
            vec![
                ("MigrationCannotCertifyDamage", 16),
                ("ActualWriterOwnership", 4),
            ],
        ),
    ] {
        for (name, count) in classes {
            let class = required(globals, name)?;
            let methods: Vec<String> = unittest
                .getattr("TestLoader")?
                .call0()?
                .call_method1("getTestCaseNames", (&class,))?
                .extract()?;
            if methods.len() != count {
                return Err(PyValueError::new_err(
                    "original identity method collection drift",
                ));
            }
            for method in methods {
                let case = format!("{name}.{method}");
                if EXCLUDED.contains(&case.as_str()) {
                    continue;
                }
                let instance = class.call1((&method,))?;
                let output = py.import("io")?.getattr("StringIO")?.call0()?;
                let redirect = py
                    .import("contextlib")?
                    .getattr("redirect_stdout")?
                    .call1((&output,))?;
                redirect.call_method0("__enter__")?;
                let executed = instance.call_method1("run", (&result,));
                redirect.call_method1("__exit__", (py.None(), py.None(), py.None()))?;
                if !sys.getattr("stdout")?.is(&stdout)
                    || !threading.call_method0("enumerate")?.eq(&threads)?
                {
                    return Err(PyValueError::new_err(
                        "original identity stream/thread custody not restored; retained",
                    ));
                }
                executed?;
                for field in [
                    "errors",
                    "failures",
                    "skipped",
                    "expectedFailures",
                    "unexpectedSuccesses",
                ] {
                    if result.getattr(field)?.len()? != 0 {
                        return Err(PyValueError::new_err(format!(
                            "original {name}.{method} reported {field}: {}; retained",
                            crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                        )));
                    }
                }
                if !modules.state.getattr("write_json_atomic")?.is(&writer)
                    || !modules.identity.getattr("fingerprint")?.is(&fingerprint)
                    || !modules.bundle.getattr("move_file")?.is(&move_file)
                    || !os.getattr("link")?.is(&link)
                {
                    return Err(PyValueError::new_err(
                        "original identity patch not restored",
                    ));
                }
                for (function, original) in &migration_functions {
                    if !modules.migration.getattr(*function)?.is(original) {
                        return Err(PyValueError::new_err(
                            "original migration function patch not restored",
                        ));
                    }
                }
                if case
                    == "ActualWriterOwnership.test_canonical_alias_and_second_migration_use_the_same_native_lock"
                {
                    let data = instance.getattr("data")?.extract::<std::path::PathBuf>()?;
                    let alias = data.join("nested");
                    if !alias.is_dir() || std::fs::read_dir(&alias)?.next().is_some() {
                        return Err(PyValueError::new_err(
                            "original canonical alias fixture drift; retained",
                        ));
                    }
                    alias_directories.insert(alias);
                }
                cases.push(case);
            }
        }
    }
    if cases.len() != 59
        || result.getattr("testsRun")?.extract::<usize>()? != 59
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err("original identity replay incomplete"));
    }
    let reports = report::run(
        py,
        root,
        [sources[20], sources[12]],
        &modules,
        ledger.bind(py).as_any(),
    )?;
    let ledger = ledger.borrow(py);
    let paths = ledger
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("identity temporary accounting poisoned"))?;
    if paths.len() != 65 || paths.iter().any(|(_, done)| !done.load(Ordering::SeqCst)) {
        return Err(PyValueError::new_err(
            "identity teardown accounting incomplete; retained",
        ));
    }
    let roots = std::fs::read_dir(workspace)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    let owned = paths
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
    let mut expected = paths
        .iter()
        .filter(|(path, _)| path.parent() == Some(workspace))
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
    if expected.len() != 60
        || alias_directories.len() != 1
        || alias_directories
            .iter()
            .any(|path| !path.parent().is_some_and(|parent| owned.contains(parent)))
        || paths.iter().any(|(path, _)| {
            path.parent() != Some(workspace)
                && !path.parent().is_some_and(|parent| owned.contains(parent))
        })
    {
        return Err(PyValueError::new_err(
            "identity temporary parent inventory differs; retained",
        ));
    }
    if workspace.join("locks").exists() {
        expected.insert(workspace.join("locks"));
    }
    if roots != expected || crate::process_owner::cleanup_pending(workspace) {
        return Err(PyValueError::new_err(
            "identity workspace inventory differs; retained",
        ));
    }
    let mut count = 0;
    for (path, _) in paths.iter() {
        directories(workspace, path, &mut 256, 0, &mut Vec::new())?;
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| PyValueError::new_err("identity fixture name not UTF8"))?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                if owned.contains(&entry.path()) {
                    continue;
                }
                if alias_directories.contains(&entry.path())
                    && std::fs::read_dir(entry.path())?.next().is_none()
                {
                    continue;
                }
            }
            if !metadata.is_file()
                || metadata.len() > 4 * 1024 * 1024
                || !(name.ends_with(".db")
                    || name.ends_with(".db-wal")
                    || name.ends_with(".db-shm")
                    || name.ends_with(".webm")
                    || name == "source.png"
                    || name.ends_with(".json")
                    || name.ends_with(".lock"))
            {
                return Err(PyValueError::new_err(
                    "unexpected original identity member; retained",
                ));
            }
            count += 1;
        }
    }
    if !(64..=1024).contains(&count) {
        return Err(PyValueError::new_err(
            "identity fixture inventory count drift; retained",
        ));
    }
    if workspace.join("locks").exists() {
        directories(
            workspace,
            &workspace.join("locks"),
            &mut 32,
            0,
            &mut Vec::new(),
        )?;
        for entry in std::fs::read_dir(workspace.join("locks"))? {
            let entry = entry?;
            if !entry.file_type()?.is_file()
                || !entry
                    .file_name()
                    .to_str()
                    .is_some_and(|s| s.starts_with("pack_") && s.ends_with(".lock"))
            {
                return Err(PyValueError::new_err(
                    "unexpected original family-lock member; retained",
                ));
            }
        }
    }
    for path in roots {
        std::fs::remove_dir_all(path)?;
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","source_hashes":IDENTITIES.into_iter().collect::<BTreeMap<_,_>>(),
        "tests_run":59,"temporary_calls":65,"reports":reports,"cases":cases,"excluded":EXCLUDED,"errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"59unchanged original identity/provenance/recovery/resume methods and inherited setup/teardown; real original SQL/locks/atomic/snapshot/bundle/state/path/hash/ingest/reconcile functions; original Catalog uses actual original writer with private native prerequisite; original test fixture decoders plus original real public-logo fingerprint for two complete report/no-op outputs; private fingerprint and stdout restored; unused content/download paths reject; private logging initialization mock; three process/HTTP methods explicitly excluded; combined original native-CLI method not executed; no native result oracle"}),
    )
}

#[pyclass]
struct TestImports {
    base: Py<modules::Imports>,
    temporary: Py<PyAny>,
}
#[pymethods]
impl TestImports {
    #[pyo3(signature = (name, globals=None, locals=None, fromlist=None, level=0))]
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        globals: Option<&Bound<'_, PyAny>>,
        locals: Option<&Bound<'_, PyAny>>,
        fromlist: Option<&Bound<'_, PyAny>>,
        level: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        if name == "tempfile"
            && level == 0
            && fromlist.is_none_or(|v| v.is_none() || v.len().is_ok_and(|n| n == 0))
        {
            return Ok(self.temporary.bind(py).clone());
        }
        let value = |v: Option<&Bound<'_, PyAny>>| {
            v.map(|v| v.clone().unbind()).unwrap_or_else(|| py.None())
        };
        self.base
            .bind(py)
            .call1((name, value(globals), value(locals), value(fromlist), level))
    }
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("identity workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect identity workspace")?
        .next()
        .is_some()
    {
        return Err("identity workspace must be empty".into());
    }
    report::admit(&root)?;
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|s| s.getattr("dont_write_bytecode"))
            .and_then(|v| v.extract::<bool>())
            .map_err(|_| "cannot inspect identity bytecode policy")?
        {
            return Err("identity replay requires disabled import bytecode".into());
        }
        execute(py, &root, &workspace, sources).map_err(|e| {
            format!(
                "original identity replay failed: {}",
                crate::logging::redact(&e.to_string(), &[])
            )
        })
    })
}
pub fn observe_bundle(
    root: &Path,
    data: &Path,
    manifest: Option<&Path>,
    action: &str,
    sources: &[&Source],
) -> Result<Value, String> {
    source_identity(sources)?;
    if ![
        "prepare",
        "verify",
        "restore",
        "stage",
        "finish-native",
        "stage-restore",
    ]
    .contains(&action)
        || (action == "prepare") != manifest.is_none()
    {
        return Err("invalid original bundle replay action".into());
    }
    let root = crate::tooling_environment::physical_root(root)?;
    let data = bundle::admit(&root, data, manifest)?;
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect bundle replay bytecode policy")?
        {
            return Err("bundle replay requires disabled import bytecode".into());
        }
        bundle::run(py, &data, manifest, action, sources).map_err(|error| {
            format!(
                "original bundle replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })
}

pub async fn bundle_owned(
    root: &Path,
    executable: &Path,
    data: &Path,
    manifest: Option<&Path>,
    action: &str,
    sources: &[&Source],
) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    if ![
        "prepare",
        "verify",
        "restore",
        "stage",
        "finish-native",
        "stage-restore",
    ]
    .contains(&action)
        || (action == "prepare") != manifest.is_none()
    {
        return Err("invalid original bundle replay action".into());
    }
    bundle::admit(&root, data, manifest)?;
    crate::source_oracle_process::check(
        &root,
        executable,
        "identity-bundle",
        sources[0],
        &sources[1..],
        &json!({"data":data,"manifest":manifest,"action":action}),
    )
    .await
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
        "identity-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
