//! Original provider publication/recovery assertions; no native publisher supplies their answers.
use crate::source_git_inventory::Source;
use crate::source_temporary::Directories;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

#[path = "source_provider_artifacts.rs"]
mod artifacts;
#[path = "source_provider_cases.rs"]
mod cases;
#[path = "source_coin_map_modules.rs"]
mod map_modules;
#[path = "source_remap_modules.rs"]
mod remap_modules;
#[path = "source_verify_modules.rs"]
mod verify_modules;
#[path = "source_verify_replacements.rs"]
mod verify_replacements;
use cases::{
    ALIAS, BASELINE, CANONICAL, COMMAND, IDENTITIES, INVENTORY, MAP, OUTAGE, RECOVERY, REMAP_CACHE,
    REMAP_DOWNLOAD, REMAP_FAMILY, REMAP_MAIN, REMAP_NEAREST, REMAP_SIGNATURE, SHARED, VERIFIED,
    VERIFY_DISTANCE, VERIFY_FIX, VERIFY_MAIN,
};

pub fn source_paths() -> [&'static str; 25] {
    IDENTITIES.map(|(path, _)| path)
}
pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("provider assertions require exactly twenty-five pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("provider assertion source identity drift; refusing execution".into());
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
    temporary.setattr("mkdtemp", ledger.bind(py).getattr("mkdtemp")?)?;
    let bindings = crate::source_provider_modules::build(py, workspace, sources, &temporary)?;
    bindings.set_item("tempfile", temporary)?;
    let publisher = required(&bindings, "pp")?;
    let source_path = workspace.join("provider-source.txt");
    let mut staged = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&source_path)?;
    staged.write_all(sources[0].text.as_bytes())?;
    staged.sync_all()?;
    drop(staged);
    publisher.setattr("__file__", &source_path)?;
    let clock = required(&bindings, "clock")?;
    let sleep = clock.getattr("sleep")?;
    let environment = required(&bindings, "command_environment")?;
    let environment_before = environment.cast::<PyDict>()?.copy()?;
    let http_get = required(&bindings, "_http")?.getattr("get")?;
    let argv = py.import("sys")?.getattr("argv")?;
    let command = required(&bindings, "fp")?;
    let command_names = [
        "TICKER_IDS",
        "CACHE",
        "INV",
        "OUT_INV",
        "setup_logging",
        "resolve_phase",
        "http_bytes",
        "Telegram",
        "search_match",
    ];
    let command_previous = command_names
        .iter()
        .map(|name| command.getattr(*name))
        .collect::<PyResult<Vec<_>>>()?;
    let names = [
        "EMOJI",
        "STATE",
        "TICKER_IDS",
        "KEYWORDS_CSV",
        "PACK_LOCK",
        "USER_ID",
        "_RUN_TOKEN",
        "canonical_map_lock",
        "exclusive_lock",
    ];
    let previous = names
        .iter()
        .map(|name| publisher.getattr(*name))
        .collect::<PyResult<Vec<_>>>()?;
    let options = PyDict::new(py);
    options.set_item("wraps", publisher.getattr("_content_matches")?)?;
    let proof = py
        .import("unittest.mock")?
        .getattr("Mock")?
        .call((), Some(&options))?;
    publisher.setattr("_content_matches", &proof)?;
    let original = crate::source_oracle_runtime::namespace(
        py,
        sources[6],
        &[
            "VerifiedPublish",
            "TheCanonicalMapIsRereadUnderTheLock",
            "TheProvidersAndTheRebuildShareOneStateFile",
            "_ProviderRun",
            "CommandExitCodes",
            "AnOutageIsNotNoMatch",
        ],
        &[
            "__build_class__",
            "RuntimeError",
            "len",
            "dict",
            "str",
            "super",
            "enumerate",
        ],
        &bindings,
    )?;
    let recovery = crate::source_oracle_runtime::namespace(
        py,
        sources[7],
        &["UnverifiedUploadIsRecovered"],
        &[
            "__build_class__",
            "RuntimeError",
            "OSError",
            "len",
            "setattr",
            "str",
            "dict",
        ],
        &bindings,
    )?;
    let inventory_bindings = bindings.copy()?;
    let shutil = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    shutil.setattr("rmtree", ledger.bind(py).getattr("rmtree")?)?;
    inventory_bindings.set_item("shutil", shutil)?;
    inventory_bindings.set_item(
        "ALIAS_INVENTORY",
        crate::source_logging_oracle::literal(py, sources[16], "ALIAS_INVENTORY", false)?,
    )?;
    let inventory = crate::source_oracle_runtime::namespace(
        py,
        sources[16],
        &["OneInventoryImplementation"],
        &["__build_class__"],
        &inventory_bindings,
    )?;
    let remap_bindings = inventory_bindings.copy()?;
    let remap_module = remap_modules::build(py, workspace, sources, &remap_bindings)?;
    let map_bindings =
        map_modules::build(py, workspace, sources, &inventory_bindings, &remap_module)?;
    let map_tests = crate::source_oracle_runtime::namespace(
        py,
        sources[16],
        &["AliasMapAmbiguity", "CanonicalMapWritersCannotLoseAnUpdate"],
        &["__build_class__", "classmethod", "str"],
        &map_bindings,
    )?;
    let map_names = [
        "ROOT",
        "INV",
        "OUT_INV",
        "KEYWORDS_CSV",
        "canonical_map_lock",
        "write_json_atomic",
    ];
    let map_modules = [
        required(&map_bindings, "map_alias")?,
        required(&map_bindings, "map_enhance")?,
    ];
    let map_previous = map_modules
        .iter()
        .map(|module| {
            map_names
                .iter()
                .map(|name| module.getattr(*name))
                .collect::<PyResult<Vec<_>>>()
        })
        .collect::<PyResult<Vec<_>>>()?;
    remap_bindings.set_item("mock", required(&remap_bindings, "remap_mock")?)?;
    let remap = crate::source_oracle_runtime::namespace(
        py,
        sources[20],
        &[
            "DownloadLiveTest",
            "NearestTest",
            "MainApplyTest",
            "ApplyIsSerialisedAgainstThePackFamily",
            "TheSignatureMustNotBeBlindToBlackOnTransparent",
            "ACacheFromAnOlderSignatureIsDiscarded",
            "PackFamilyLockTest",
            "NumpyIsAnExtra",
        ],
        &[
            "__build_class__",
            "staticmethod",
            "bytes",
            "list",
            "sorted",
            "str",
            "float",
            "len",
            "range",
            "ValueError",
        ],
        &remap_bindings,
    )?;
    let remap_names = [
        "download_live",
        "nearest",
        "Telegram",
        "PACK_LOCK",
        "load_env",
        "setup_logging",
        "canonical_map_lock",
    ];
    let remap_previous = remap_names
        .iter()
        .map(|name| remap_module.getattr(*name))
        .collect::<PyResult<Vec<_>>>()?;
    let verify_bindings = inventory_bindings.copy()?;
    let verify_module = verify_modules::build(py, workspace, sources, &verify_bindings)?;
    let verify = verify_modules::tests(py, sources, &verify_bindings)?;
    let verify_names = [
        "ROOT",
        "PACK_LOCK",
        "load_env",
        "setup_logging",
        "fetch_markets",
        "fix_one",
        "reconcile_intent",
        "exclusive_lock",
    ];
    let verify_previous = verify_names
        .iter()
        .map(|name| verify_module.getattr(*name))
        .collect::<PyResult<Vec<_>>>()?;
    let (replacement_module, replacements) =
        verify_replacements::build(py, workspace, sources, &inventory_bindings, &ledger)?;
    let replacement_names = ["ROOT", "fetch_markets", "fetch_image", "canonical_map_lock"];
    let replacement_previous = replacement_names
        .iter()
        .map(|name| replacement_module.getattr(*name))
        .collect::<PyResult<Vec<_>>>()?;
    let replacement_clock = required(&replacements, "replacement_clock")?;
    let replacement_sleep = replacement_clock.getattr("sleep")?;
    let unittest = py.import("unittest")?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut canonical = VERIFIED.to_vec();
    canonical.extend_from_slice(CANONICAL);
    canonical.sort_unstable();
    let mut primary = BTreeSet::new();
    let mut inventory_paths = BTreeSet::new();
    let mut map_paths = BTreeSet::new();
    let mut remap_paths = BTreeSet::new();
    let mut verify_paths = BTreeSet::new();
    let mut replacement_paths = BTreeSet::new();
    let mut cases = Vec::new();
    let mut excluded = Vec::new();
    for (globals, class_name, methods) in [
        (&original, "VerifiedPublish", VERIFIED),
        (
            &original,
            "TheCanonicalMapIsRereadUnderTheLock",
            canonical.as_slice(),
        ),
        (&recovery, "UnverifiedUploadIsRecovered", RECOVERY),
        (
            &original,
            "TheProvidersAndTheRebuildShareOneStateFile",
            SHARED,
        ),
        (&original, "CommandExitCodes", COMMAND),
        (&original, "AnOutageIsNotNoMatch", OUTAGE),
        (&inventory, "OneInventoryImplementation", INVENTORY),
        (&map_tests, "AliasMapAmbiguity", ALIAS),
        (&map_tests, "CanonicalMapWritersCannotLoseAnUpdate", MAP),
        (&remap, "DownloadLiveTest", REMAP_DOWNLOAD),
        (&remap, "NearestTest", REMAP_NEAREST),
        (&remap, "MainApplyTest", REMAP_MAIN),
        (
            &remap,
            "ApplyIsSerialisedAgainstThePackFamily",
            REMAP_FAMILY,
        ),
        (
            &remap,
            "TheSignatureMustNotBeBlindToBlackOnTransparent",
            REMAP_SIGNATURE,
        ),
        (&remap, "ACacheFromAnOlderSignatureIsDiscarded", REMAP_CACHE),
        (&remap, "PackFamilyLockTest", &[]),
        (&remap, "NumpyIsAnExtra", &[]),
        (&verify, "VerifyLogosMainContracts", VERIFY_MAIN),
        (&replacements, "VerifyLogosFix", VERIFY_FIX),
        (&replacements, "LogoReviewDistance", VERIFY_DISTANCE),
    ] {
        let class = required(globals, class_name)?;
        let collected: Vec<String> = loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        let exclusions = cases::exclusions(class_name);
        let mut all = methods.to_vec();
        all.extend_from_slice(exclusions);
        all.sort_unstable();
        if collected != all {
            return Err(PyValueError::new_err(
                "original provider method collection drift",
            ));
        }
        excluded.extend(exclusions.iter().map(|name| format!("{class_name}.{name}")));
        let collected = collected
            .into_iter()
            .filter(|name| methods.contains(&name.as_str()))
            .collect::<Vec<_>>();
        if collected != methods {
            return Err(PyValueError::new_err(
                "original provider method collection drift",
            ));
        }
        class.call_method0("setUpClass")?;
        for method in methods {
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
                        "original provider {class_name}.{method} reported {field}: {}; retained",
                        crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                    )));
                }
            }
            if class_name == "VerifyLogosFix" {
                let path = PathBuf::from(test.getattr("tmp")?.str()?.extract::<String>()?);
                if !replacement_paths.insert(path.clone()) || !primary.insert(path) {
                    return Err(PyValueError::new_err(
                        "original replacement reused a test workspace",
                    ));
                }
            } else if class_name == "VerifyLogosMainContracts" {
                let path = PathBuf::from(test.getattr("tmp")?.str()?.extract::<String>()?);
                if !verify_paths.insert(path.clone()) || !primary.insert(path) {
                    return Err(PyValueError::new_err(
                        "original verification reused a test workspace",
                    ));
                }
            } else if class_name == "OneInventoryImplementation" {
                let path = PathBuf::from(test.getattr("tmp")?.str()?.extract::<String>()?);
                if !inventory_paths.insert(path.clone()) || !primary.insert(path) {
                    return Err(PyValueError::new_err(
                        "original inventory reused a test workspace",
                    ));
                }
            } else if matches!(
                class_name,
                "AliasMapAmbiguity" | "CanonicalMapWritersCannotLoseAnUpdate"
            ) {
                let path = PathBuf::from(test.getattr("tmp")?.str()?.extract::<String>()?);
                if !map_paths.insert(path.clone()) || !primary.insert(path) {
                    return Err(PyValueError::new_err(
                        "original map reused a test workspace",
                    ));
                }
            } else if matches!(
                class_name,
                "DownloadLiveTest" | "MainApplyTest" | "ApplyIsSerialisedAgainstThePackFamily"
            ) {
                let path =
                    PathBuf::from(test.getattr("tmp")?.getattr("name")?.extract::<String>()?);
                if !remap_paths.insert(path.clone()) || !primary.insert(path) {
                    return Err(PyValueError::new_err(
                        "original remap reused a test workspace",
                    ));
                }
            } else if !matches!(
                class_name,
                "TheProvidersAndTheRebuildShareOneStateFile"
                    | "NearestTest"
                    | "TheSignatureMustNotBeBlindToBlackOnTransparent"
                    | "ACacheFromAnOlderSignatureIsDiscarded"
                    | "LogoReviewDistance"
            ) {
                let path =
                    PathBuf::from(test.getattr("tmp")?.getattr("name")?.extract::<String>()?);
                if !primary.insert(path) {
                    return Err(PyValueError::new_err(
                        "original provider reused a test workspace",
                    ));
                }
            }
            if !environment.eq(&environment_before)?
                || !required(&bindings, "_http")?.getattr("get")?.is(&http_get)
                || !py.import("sys")?.getattr("argv")?.is(&argv)
            {
                return Err(PyValueError::new_err(
                    "original provider command patches not restored",
                ));
            }
            for (name, value) in command_names.iter().zip(&command_previous) {
                if !command.getattr(*name)?.eq(value)? {
                    return Err(PyValueError::new_err(
                        "original provider command attributes not restored",
                    ));
                }
            }
            for (module, previous) in map_modules.iter().zip(&map_previous) {
                for (name, value) in map_names.iter().zip(previous) {
                    if !module.getattr(*name)?.eq(value)? {
                        return Err(PyValueError::new_err(
                            "original map module attributes not restored",
                        ));
                    }
                }
            }
            if !replacement_clock.getattr("sleep")?.is(&replacement_sleep) {
                return Err(PyValueError::new_err(
                    "original replacement clock patch not restored",
                ));
            }
            for (name, value) in replacement_names.iter().zip(&replacement_previous) {
                if !replacement_module.getattr(*name)?.is(value) {
                    return Err(PyValueError::new_err(
                        "original replacement patches not restored",
                    ));
                }
            }
            for (name, value) in verify_names.iter().zip(&verify_previous) {
                if !verify_module.getattr(*name)?.is(value) {
                    return Err(PyValueError::new_err(
                        "original verification patches not restored",
                    ));
                }
            }
            for (name, value) in remap_names.iter().zip(&remap_previous) {
                if !remap_module.getattr(*name)?.is(value) {
                    return Err(PyValueError::new_err("original remap patches not restored"));
                }
            }
            if !clock.getattr("sleep")?.is(&sleep) {
                return Err(PyValueError::new_err(
                    "original provider sleep patch not restored",
                ));
            }
            for (name, value) in names.iter().zip(&previous) {
                if !publisher.getattr(*name)?.eq(value)? {
                    return Err(PyValueError::new_err(
                        "original provider globals not restored",
                    ));
                }
            }
            cases.push(format!("{class_name}.{method}"));
        }
        class.call_method0("tearDownClass")?;
    }
    let loads: usize = required(&bindings, "command_load")?
        .getattr("calls")?
        .extract()?;
    let http_patches: usize = required(&bindings, "command_patch")?
        .getattr("calls")?
        .extract()?;
    let map_loads: usize = required(&map_bindings, "map_loader")?
        .getattr("calls")?
        .extract()?;
    let verify_loads: usize = required(&verify, "verify_loader")?
        .getattr("calls")?
        .extract()?;
    let replacement_proofs = verify_replacements::finish(&replacements)?;
    let checks = [
        ("tests", result.getattr("testsRun")?.extract::<usize>()?, 97),
        ("primary_paths", primary.len(), 85),
        ("replacement_paths", replacement_paths.len(), 19),
        ("verify_paths", verify_paths.len(), 8),
        ("verify_loads", verify_loads, 1),
        (
            "verify_refusals",
            required(&verify_bindings, "verify_refusal")?
                .getattr("call_count")?
                .extract()?,
            0,
        ),
        (
            "verify_constructors",
            required(&verify_bindings, "verify_constructor")?
                .getattr("call_count")?
                .extract()?,
            5,
        ),
        (
            "verify_telegram",
            required(&verify_bindings, "verify_telegram")?
                .getattr("mock_calls")?
                .len()?,
            0,
        ),
        ("remap_paths", remap_paths.len(), 13),
        (
            "remap_refusals",
            required(&remap_bindings, "remap_refusal")?
                .getattr("call_count")?
                .extract()?,
            0,
        ),
        (
            "remap_loads",
            required(&remap_bindings, "remap_load")?
                .getattr("call_count")?
                .extract()?,
            0,
        ),
        ("inventory_paths", inventory_paths.len(), 4),
        ("map_paths", map_paths.len(), 6),
        ("map_loads", map_loads, 6),
        ("loads", loads, 4),
        ("http_patches", http_patches, 1),
        (
            "command_refusals",
            required(&bindings, "command_refusal")?
                .getattr("call_count")?
                .extract()?,
            0,
        ),
    ];
    let mismatches = checks
        .into_iter()
        .filter(|(_, actual, expected)| actual != expected)
        .collect::<Vec<_>>();
    if !result.call_method0("wasSuccessful")?.extract::<bool>()? || !mismatches.is_empty() {
        return Err(PyValueError::new_err(format!(
            "original provider assertions incomplete: {mismatches:?}"
        )));
    }
    let proof_calls: usize = proof.getattr("call_count")?.extract()?;
    let ledger = ledger.borrow(py);
    let paths = ledger
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("provider temporary accounting poisoned"))?;
    if proof_calls == 0
        || paths.len() != 86 + proof_calls + replacement_proofs
        || paths
            .iter()
            .any(|(_, cleaned)| !cleaned.load(Ordering::SeqCst))
        || primary
            .iter()
            .any(|path| !paths.iter().any(|(owned, _)| owned == path))
    {
        return Err(PyValueError::new_err(
            "original provider teardown accounting incomplete; retained",
        ));
    }
    let replacement_downloads = paths
        .iter()
        .filter(|(path, _)| !primary.contains(path) && path.join("live.png").is_file())
        .count();
    if replacement_downloads != replacement_proofs {
        return Err(PyValueError::new_err(
            "original replacement download accounting drift",
        ));
    }
    // The old-signature case creates its sole context inside the method, not setUp.
    let cache_contexts = paths
        .iter()
        .map(|(path, _)| path)
        .filter(|path| !primary.contains(*path) && path.join("cache.json").is_file())
        .cloned()
        .collect::<Vec<_>>();
    if cache_contexts.len() != 1 {
        return Err(PyValueError::new_err(
            "original remap cache context accounting drift",
        ));
    }
    remap_paths.insert(cache_contexts[0].clone());
    let owned = paths
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    if publisher.getattr("__file__")?.extract::<PathBuf>()? != source_path {
        return Err(PyValueError::new_err("original provider source path drift"));
    }
    artifacts::clean(
        workspace,
        sources[0],
        &bindings,
        &owned,
        &artifacts::Groups {
            primary: &primary,
            inventory: &inventory_paths,
            maps: &map_paths,
            remap: &remap_paths,
            verify: &verify_paths,
            replacements: &replacement_paths,
        },
    )?;
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":"12.3.0",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":97,"primary_contexts":85,"replacement_contexts":19,"replacement_proof_contexts":replacement_proofs,"remap_contexts":14,"verify_contexts":8,"verify_loads":verify_loads,"NumPy":"2.4.6","proof_contexts":proof_calls,"temporary_calls":paths.len(),"cases":cases,"excluded":excluded,"environment_loads":loads,"http_patches":http_patches,"map_loads":map_loads,
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"unchanged original classes/inherited assertions/publisher/rebuild validator/identity/OS locks/atomic writer/converter/fakes; private modules, exact no-wait clock and delayed project-contained test/download teardown; own-PID lock diagnostic; native errors remain separate"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("provider assertion workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect provider assertion workspace")?
        .next()
        .is_some()
    {
        return Err("provider assertion workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect provider assertion bytecode policy")?
        {
            return Err("provider assertions require disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original provider assertions failed: {}",
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
        "provider-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
