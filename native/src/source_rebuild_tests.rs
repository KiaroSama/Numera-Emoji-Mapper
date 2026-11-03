//! Unchanged original rebuild assertions; native rebuild code supplies no oracle answers.
use crate::source_git_inventory::Source;
use crate::source_temporary::Directories;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

#[path = "source_rebuild_cases.rs"]
mod cases;
#[path = "source_rebuild_modules.rs"]
mod modules;
#[path = "source_rebuild_reload.rs"]
mod reload;
use cases::{BASELINE, CLASSES, EXCLUDED_MAP, IDENTITIES, RESUME, SCHEMA};

pub fn source_paths() -> [&'static str; 16] {
    IDENTITIES.map(|(path, _)| path)
}
pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("rebuild assertions require exactly sixteen pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("rebuild assertion source identity drift; refusing execution".into());
        }
    }
    Ok(())
}
fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let ledger = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let temporary = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    temporary.setattr("TemporaryDirectory", ledger.bind(py).getattr("temporary")?)?;
    let modules = modules::build(py, workspace, sources, &temporary)?;
    let cfg_names = [
        "ROOT",
        "EMOJI",
        "STATE",
        "PLAN",
        "OLD_STATE",
        "GROUPS_REPORT",
        "INV",
        "OUT_INV",
        "TICKER_IDS",
        "KEYWORDS_CSV",
        "LOCK",
        "USER_ID",
        "write_json_atomic",
    ];
    let previous = cfg_names
        .iter()
        .map(|name| modules.cfg.getattr(*name))
        .collect::<PyResult<Vec<_>>>()?;
    let approved = modules.map.getattr("approved_shared_tickers")?;
    let map_imports = required(&modules.bindings, "map_imports")?;
    let lock_dir = required(&modules.locks, "LOCK_DIR")?;
    let sleep = modules.clock.getattr("sleep")?;
    let announce = modules.rebuild.getattr("announce_packs")?;
    let sys = py.import("sys")?;
    let stdout = sys.getattr("stdout")?;
    let stderr = sys.getattr("stderr")?;
    let mut namespaces = std::collections::BTreeMap::new();
    for (index, definitions) in [
        (
            8,
            vec![
                "LinkMessagesAreBounded",
                "OwnerIdIsParsedSafely",
                "OldPackDeletionMustBeConfirmed",
            ],
        ),
        (
            9,
            vec![
                "RebuildTakesThePackFamilyLock",
                "_LockWatchingTelegram",
                "ConcurrentRunsAreLockedOut",
            ],
        ),
        (
            10,
            vec![
                "BuildFinishesWhenThePlanIsWalked",
                "CursorOrderAndPlanMustDescribeOneWalk",
                "RetryableFailureKeepsThePlanPosition",
                "TruncatedPlanFailsClosed",
                "StateSchemaIsValidatedBeforeAnyMutation",
            ],
        ),
        (11, RESUME.to_vec()),
        (14, vec!["MapIsResolvedByImageIdentity", "SharedLogoGuard"]),
    ] {
        let namespace = crate::source_oracle_runtime::namespace(
            py,
            sources[index],
            &definitions,
            &[
                "__build_class__",
                "super",
                "len",
                "RuntimeError",
                "ValueError",
                "SystemExit",
                "str",
                "sorted",
                "list",
                "set",
                "dict",
            ],
            &modules.bindings,
        )?;
        if index == 14 {
            required(&namespace, "__builtins__")?
                .set_item("__import__", required(&modules.bindings, "map_imports")?)?;
        }
        namespaces.insert(index, namespace);
    }
    let unittest = py.import("unittest")?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut primary = BTreeSet::new();
    let mut proofs = std::collections::BTreeMap::new();
    let mut executed = Vec::new();
    let mut owner_reloads = 0;
    let mut excluded = Vec::new();
    for (index, name, methods) in CLASSES.into_iter().chain(std::iter::once((
        10,
        "StateSchemaIsValidatedBeforeAnyMutation",
        SCHEMA,
    ))) {
        let namespace = &namespaces[&index];
        let class = required(namespace, name)?;
        let collected: Vec<String> = loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        let mut expected = methods.to_vec();
        if name == "SharedLogoGuard" {
            expected.push(EXCLUDED_MAP);
            expected.sort();
            excluded.push(format!("{name}.{EXCLUDED_MAP}"));
        }
        if collected != expected {
            return Err(PyValueError::new_err("original rebuild collection drift"));
        }
        if name == "OwnerIdIsParsedSafely" {
            let (names, calls) = reload::run(
                py,
                sources[1],
                sources[15],
                &modules.cfg,
                &class,
                &modules.bindings,
                &result,
            )?;
            owner_reloads += calls;
            executed.extend(names);
            for (field, value) in cfg_names.iter().zip(&previous) {
                if !modules.cfg.getattr(*field)?.is(value) {
                    return Err(PyValueError::new_err(
                        "original owner reload configuration not restored",
                    ));
                }
            }
            continue;
        }
        class.call_method0("setUpClass")?;
        for method in methods {
            let start = ledger
                .borrow(py)
                .paths
                .lock()
                .map_err(|_| PyRuntimeError::new_err("rebuild temporary accounting poisoned"))?
                .len();
            let imports = modules.imports.getattr("calls")?.extract::<usize>()?;
            let test = class.call1((*method,))?;
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
                        "original rebuild {name}.{method} reported {field}; retained"
                    )));
                }
            }
            if !matches!(name, "RebuildTakesThePackFamilyLock" | "SharedLogoGuard") {
                let path = PathBuf::from(test.getattr("dir")?.str()?.extract::<String>()?);
                if !primary.insert(path.clone()) {
                    return Err(PyValueError::new_err("original rebuild reused a workspace"));
                }
                let (add_proofs, create_proofs) = cases::proof_counts(name, method);
                let map_proofs = if name == "MapIsResolvedByImageIdentity" {
                    cases::map_proofs(method)
                } else {
                    0
                };
                let accounting = ledger.borrow(py);
                let paths = accounting.paths.lock().map_err(|_| {
                    PyRuntimeError::new_err("rebuild temporary accounting poisoned")
                })?;
                let current = paths
                    .get(start..)
                    .ok_or_else(|| PyValueError::new_err("rebuild temporary ledger shrank"))?;
                if current.len() != 1 + add_proofs + create_proofs + map_proofs
                    || current[0].0 != path
                {
                    return Err(PyValueError::new_err(
                        "original rebuild per-method context drift",
                    ));
                }
                for (proof, _) in &current[1..] {
                    let create = proof
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("emoji-verify-"));
                    let kind = if create {
                        "create"
                    } else if map_proofs != 0 {
                        "map"
                    } else {
                        "add"
                    };
                    if create != (create_proofs != 0)
                        || proofs.insert(proof.clone(), kind).is_some()
                    {
                        return Err(PyValueError::new_err(
                            "original rebuild proof context drift",
                        ));
                    }
                }
                drop(paths);
                drop(accounting);
                let lock = path.join("state.json.lock");
                if lock.is_file() {
                    let lease = required(&modules.bindings, "ps")?
                        .call_method1("exclusive_lock", (&lock,))?;
                    lease.call_method0("__enter__")?;
                    lease.call_method1("__exit__", (py.None(), py.None(), py.None()))?;
                }
            }
            if modules.imports.getattr("calls")?.extract::<usize>()?
                != imports + cases::proof_counts(name, method).1
                || modules.imports.getattr("refused")?.extract::<usize>()? != 0
            {
                return Err(PyValueError::new_err(
                    "original rebuild comparator import drift",
                ));
            }
            for (field, value) in cfg_names.iter().zip(&previous) {
                if !modules.cfg.getattr(*field)?.is(value) {
                    return Err(PyValueError::new_err(
                        "original rebuild configuration patch not restored",
                    ));
                }
            }
            if !modules.clock.getattr("sleep")?.is(&sleep)
                || !modules.rebuild.getattr("announce_packs")?.is(&announce)
                || !sys.getattr("stdout")?.is(&stdout)
                || !sys.getattr("stderr")?.is(&stderr)
                || !modules
                    .map
                    .getattr("approved_shared_tickers")?
                    .is(&approved)
                || !modules.state.getattr("LOCK_DIR")?.is(&lock_dir)
            {
                return Err(PyValueError::new_err(
                    "original rebuild output/effect patch not restored",
                ));
            }
            executed.push(format!("{name}.{method}"));
        }
        class.call_method0("tearDownClass")?;
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 76
        || owner_reloads != 4
        || primary.len() != 70
        || proofs.len() != 16
        || proofs.values().filter(|kind| **kind == "create").count() != 4
        || proofs.values().filter(|kind| **kind == "add").count() != 3
        || proofs.values().filter(|kind| **kind == "map").count() != 9
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || modules.refusal.getattr("call_count")?.extract::<usize>()? != 0
        || modules.imports.getattr("calls")?.extract::<usize>()? != 4
        || modules.imports.getattr("refused")?.extract::<usize>()? != 0
        || map_imports.getattr("calls")?.extract::<usize>()? != 0
        || map_imports.getattr("maps")?.extract::<usize>()? != 3
        || map_imports.getattr("refused")?.extract::<usize>()? != 0
    {
        return Err(PyValueError::new_err(
            "original rebuild assertions incomplete; retained",
        ));
    }
    let ledger = ledger.borrow(py);
    let paths = ledger
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("rebuild temporary accounting poisoned"))?;
    if paths.len() != 86
        || paths.iter().any(|(path, cleaned)| {
            (!primary.contains(path) && !proofs.contains_key(path))
                || !cleaned.load(Ordering::SeqCst)
        })
    {
        return Err(PyValueError::new_err(
            "original rebuild teardown accounting incomplete; retained",
        ));
    }
    let mut expected: BTreeSet<_> = primary.iter().chain(proofs.keys()).cloned().collect();
    expected.insert(workspace.join(".locks"));
    let canonical = workspace.join(".locks/canonical_map.lock");
    let lock_members = std::fs::read_dir(workspace.join(".locks"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    if lock_members != [canonical.clone()] || !canonical.is_file() {
        return Err(PyValueError::new_err(
            "unexpected original rebuild map lock artifacts",
        ));
    }
    let lease = modules.state.call_method0("canonical_map_lock")?;
    lease.call_method0("__enter__")?;
    lease.call_method1("__exit__", (py.None(), py.None(), py.None()))?;
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    if actual != expected || crate::process_owner::cleanup_pending(workspace) {
        return Err(PyValueError::new_err(
            "unexpected original rebuild workspace members; retained",
        ));
    }
    let mut budget = 1024;
    directories(workspace, workspace, &mut budget, 0, &mut Vec::new())?;
    for (path, kind) in &proofs {
        let members = std::fs::read_dir(path)?
            .map(|entry| entry.and_then(|entry| Ok((entry.file_name(), entry.file_type()?))))
            .collect::<std::io::Result<Vec<_>>>()?;
        let names = members
            .iter()
            .map(|(name, _)| name.to_str().unwrap_or(""))
            .collect::<Vec<_>>();
        if members.iter().any(|(_, kind)| !kind.is_file())
            || match *kind {
                "create" => names != ["sticker.dl"],
                "map" => names != ["live.png"],
                _ => !matches!(names.as_slice(), [] | ["s1#1"]),
            }
        {
            return Err(PyValueError::new_err(
                "unexpected original rebuild proof artifact; retained",
            ));
        }
    }
    for path in &primary {
        let members = std::fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<Vec<_>>>()?;
        let allowed = [
            "emoji",
            "state.json",
            "plan.json",
            "old_state.json",
            "inventory.md",
            "inventory.filled.md",
            "shared_logo_groups.json",
            "state.json.lock",
            "ticker_to_id.json",
            "ticker_to_id.candidate.json",
        ];
        if !path.join("inventory.md").is_file()
            || members
                .iter()
                .any(|name| !allowed.contains(&name.to_str().unwrap_or("")))
        {
            return Err(PyValueError::new_err(
                "unexpected original rebuild artifact; retained",
            ));
        }
        let images = path.join("emoji");
        if images.is_dir() {
            for entry in std::fs::read_dir(&images)? {
                let entry = entry?;
                if !entry.file_type()?.is_file()
                    || !["seed.png", "aaa.png", "bbb.png", "ccc.png", "ddd.png"]
                        .contains(&entry.file_name().to_str().unwrap_or(""))
                {
                    return Err(PyValueError::new_err(
                        "unexpected original rebuild image; retained",
                    ));
                }
            }
        }
    }
    // Only after assertions, restoration, cleanup acknowledgements and real lock reacquisition.
    for (path, _) in paths.iter() {
        std::fs::remove_dir_all(path)?;
    }
    std::fs::remove_file(canonical)?;
    std::fs::remove_dir(workspace.join(".locks"))?;
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":"12.3.0",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":76,"owner_reloads":owner_reloads,"primary_contexts":70,"proof_contexts":proofs.len(),"comparator_imports":4,"map_imports":3,"temporary_calls":paths.len(),"cases":executed,"excluded":excluded,
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"unchanged original classes/setup/cleanup/engine/config/writer/OS locks/image recipes/send_message; private clock/environment/closed comparator/map imports and delayed test teardown; real stdlib reload executes original owner assignment; operator-only map assertion excluded"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("rebuild assertion workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect rebuild assertion workspace")?
        .next()
        .is_some()
    {
        return Err("rebuild assertion workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect rebuild bytecode policy")?
        {
            return Err("rebuild assertions require disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original rebuild assertions failed: {}",
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
        "rebuild-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
