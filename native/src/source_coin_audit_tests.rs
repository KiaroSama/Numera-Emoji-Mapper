//! Unchanged original audit assertions run against admitted historical functions and private ports.
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 6] = [
    (
        "coins/check_all_packs.py",
        "f62da85bd939464a2673a758740cab9001ae0920a000df32f7827d42485b5272",
    ),
    (
        "emojikit/media.py",
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
    ),
    (
        "emojikit/cli_env.py",
        "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
    ),
    (
        "tests/test_check_all_packs.py",
        "33e02cce8a29db515d9babd96bc35de9452b687b7715e2c6cc59bdbc9f0214e0",
    ),
    (
        "tests/_coin_fixtures.py",
        "eb95df6b040a2b57ba521ec7f84405dad33cf3be7c7de61d8ccffb3d50475389",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
];
const CLASSES: [(&str, &[&str]); 2] = [
    (
        "CheckAllPacksTest",
        &[
            "test_deleted_sticker_leaves_totals_and_duplicate_groups",
            "test_analyse_failure_is_an_error_that_is_retried_not_a_blank",
            "test_blank_sticker_is_still_reported_with_live_position",
            "test_blank_is_the_pipeline_wide_rule_not_a_local_copy",
        ],
    ),
    (
        "CheckAllPacksListingRetry",
        &[
            "test_a_throttled_listing_is_retried_instead_of_ending_the_run",
            "test_work_already_analysed_survives_a_set_that_will_not_list",
            "test_each_set_boundary_checkpoints_the_audit",
        ],
    ),
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("audit tests require exactly six pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("audit tests source identity drift; refusing execution".into());
        }
    }
    Ok(())
}
fn required<'py>(values: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    values
        .get_item(name)?
        .ok_or_else(|| PyValueError::new_err("missing original audit definition"))
}

#[pyclass]
struct Temporary {
    #[pyo3(get)]
    name: String,
    cleaned: Arc<AtomicBool>,
}
#[pymethods]
impl Temporary {
    fn cleanup(&self) {
        // Original teardown is recorded now, but failed test evidence must survive it.
        self.cleaned.store(true, Ordering::SeqCst);
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
        options.set_item("prefix", "original-audit-")?;
        let name: String = crate::source_oracle_runtime::host_import(py, "tempfile")?
            .call_method("mkdtemp", (), Some(&options))?
            .extract()?;
        let path = crate::tooling_environment::physical_root(Path::new(&name))
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(self.root.as_path()) {
            return Err(PyValueError::new_err(
                "audit temporary escaped its workspace",
            ));
        }
        let cleaned = Arc::new(AtomicBool::new(false));
        self.paths
            .lock()
            .map_err(|_| PyValueError::new_err("audit temporary accounting poisoned"))?
            .push((path, cleaned.clone()));
        Py::new(py, Temporary { name, cleaned })
    }
}

fn audit_module<'py>(py: Python<'py>, sources: &[&Source]) -> PyResult<Bound<'py, PyModule>> {
    let bindings = PyDict::new(py);
    let image = py.import("PIL.Image")?;
    if image.getattr("__version__")?.extract::<String>()? != "12.3.0" {
        return Err(PyValueError::new_err(
            "original audit tests require Pillow12.3.0",
        ));
    }
    bindings.set_item("Image", &image)?;
    for name in ["VISIBLE_ALPHA", "BLANK_MAX_VISIBLE"] {
        bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[1], name, false)?,
        )?;
    }
    let media = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["is_blank_image"],
        &["sum"],
        &bindings,
    )?;
    bindings.set_item("is_blank_image", required(&media, "is_blank_image")?)?;
    let path = py.import("pathlib")?.getattr("Path")?;
    bindings.set_item("Path", &path)?;
    for name in ["contextlib", "json", "os", "stat", "tempfile", "time"] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item(
        "_REPLACE_DEADLINE",
        crate::source_logging_oracle::literal(py, sources[5], "_REPLACE_DEADLINE", false)?,
    )?;
    let atomic = crate::source_oracle_runtime::namespace(
        py,
        sources[5],
        &["_replace", "write_json_atomic"],
        &["PermissionError", "FileNotFoundError", "min"],
        &bindings,
    )?;
    bindings.set_item("write_json_atomic", required(&atomic, "write_json_atomic")?)?;
    for (name, value) in [
        ("EXIT_OK", 0),
        ("EXIT_USAGE", 2),
        ("EXIT_PARTIAL", 3),
        ("EXIT_FAILED", 4),
    ] {
        bindings.set_item(name, value)?;
    }
    let codes = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["ingest_exit_code"],
        &[],
        &bindings,
    )?;
    bindings.set_item("ingest_exit_code", required(&codes, "ingest_exit_code")?)?;
    for name in ["io", "hashlib"] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item(
        "defaultdict",
        py.import("collections")?.getattr("defaultdict")?,
    )?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let os = ns.call0()?;
    os.setattr("environ", PyDict::new(py))?;
    bindings.set_item("os", os)?;
    let clock = ns.call0()?;
    let mock = py.import("unittest.mock")?.getattr("Mock")?;
    clock.setattr("sleep", mock.call0()?)?;
    bindings.set_item("time", clock)?;
    for name in ["load_env", "setup_logging", "Telegram"] {
        bindings.set_item(name, mock.call0()?)?;
    }
    for name in ["STATE", "AUDIT", "REPORT"] {
        bindings.set_item(name, path.call1(("unused-original-audit-path",))?)?;
    }
    let names = ["load_audit", "save_audit", "analyze", "main"];
    let globals = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &names,
        &[
            "sorted",
            "range",
            "Exception",
            "enumerate",
            "str",
            "len",
            "list",
            "sum",
            "print",
        ],
        &bindings,
    )?;
    let module = PyModule::new(py, "_original_coin_audit")?;
    module.dict().update(globals.as_mapping())?;
    let function_type = py.import("types")?.getattr("FunctionType")?;
    for name in names {
        let original = required(&globals, name)?;
        let function = function_type.call1((
            original.getattr("__code__")?,
            module.dict(),
            original.getattr("__name__")?,
            original.getattr("__defaults__")?,
            original.getattr("__closure__")?,
        ))?;
        for attribute in ["__kwdefaults__", "__annotations__"] {
            function.setattr(attribute, original.getattr(attribute)?)?;
        }
        if !function.getattr("__globals__")?.is(module.dict()) {
            return Err(PyValueError::new_err(
                "audit globals are not patchable module storage",
            ));
        }
        module.add(name, function)?;
    }
    Ok(module)
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let audit = audit_module(py, sources)?;
    let bindings = PyDict::new(py);
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let requests = ns.call0()?;
    requests.setattr(
        "HTTPError",
        py.import("requests.exceptions")?.getattr("HTTPError")?,
    )?;
    for (name, value) in [
        ("Image", py.import("PIL.Image")?.into_any()),
        ("io", py.import("io")?.into_any()),
        ("requests", requests),
        ("Path", py.import("pathlib")?.getattr("Path")?),
    ] {
        bindings.set_item(name, value)?;
    }
    let fakes = crate::source_oracle_runtime::namespace(
        py,
        sources[4],
        &[
            "_png",
            "_blank_png",
            "_sticker",
            "FakeResponse",
            "FakeSession",
            "LiveSetsTelegram",
            "ThrottledTelegram",
        ],
        &[
            "__build_class__",
            "frozenset",
            "dict",
            "set",
            "super",
            "RuntimeError",
            "KeyboardInterrupt",
            "str",
        ],
        &bindings,
    )?;
    let media = ns.call0()?;
    media.setattr("is_blank_image", audit.getattr("is_blank_image")?)?;
    let directories = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let temporary = ns.call0()?;
    temporary.setattr(
        "TemporaryDirectory",
        directories.bind(py).getattr("temporary")?,
    )?;
    let unittest = py.import("unittest")?;
    for (name, value) in [
        ("unittest", unittest.clone().into_any()),
        ("mock", py.import("unittest.mock")?.into_any()),
        ("contextlib", py.import("contextlib")?.into_any()),
        ("json", py.import("json")?.into_any()),
        ("io", py.import("io")?.into_any()),
        ("os", audit.getattr("os")?),
        ("tempfile", temporary),
        ("cap", audit.clone().into_any()),
        ("media", media),
    ] {
        bindings.set_item(name, value)?;
    }
    for (name, original) in [
        ("FakeSession", "FakeSession"),
        ("FakeTelegram", "LiveSetsTelegram"),
        ("ThrottledTelegram", "ThrottledTelegram"),
        ("_png", "_png"),
        ("_blank_png", "_blank_png"),
        ("_sticker", "_sticker"),
    ] {
        bindings.set_item(name, required(&fakes, original)?)?;
    }
    for name in ["EXIT_OK", "EXIT_FAILED", "EXIT_PARTIAL"] {
        bindings.set_item(name, audit.getattr(name)?)?;
    }
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["CheckAllPacksTest", "CheckAllPacksListingRetry"],
        &[
            "__build_class__",
            "str",
            "list",
            "enumerate",
            "object",
            "KeyboardInterrupt",
        ],
        &bindings,
    )?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    let mut observations = Vec::new();
    for (name, methods) in CLASSES {
        let class = required(&tests, name)?;
        let actual: Vec<String> = loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        let mut expected = methods.to_vec();
        expected.sort_unstable();
        if actual != expected {
            return Err(PyValueError::new_err("original audit collection drift"));
        }
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
                let failures = result.getattr(field)?;
                if failures.len()? != 0 {
                    return Err(PyValueError::new_err(format!(
                        "original audit reported {field}: {name}.{method}: {}",
                        failures.repr()?
                    )));
                }
            }
            let path: PathBuf = test.getattr("dir")?.extract()?;
            let physical =
                crate::tooling_environment::physical_root(&path).map_err(PyValueError::new_err)?;
            if physical != path || path.parent() != Some(workspace) {
                return Err(PyValueError::new_err(
                    "original audit evidence escaped its workspace",
                ));
            }
            for name in ["audit.json", "report.txt", "state.json"] {
                crate::tooling_environment::reject_links(
                    &path.join(name),
                    "linked original audit result",
                )
                .map_err(PyValueError::new_err)?;
            }
            let audit = if path.join("audit.json").is_file() {
                Some(
                    serde_json::from_slice::<Value>(&std::fs::read(path.join("audit.json"))?)
                        .map_err(|_| PyValueError::new_err("invalid original audit cache"))?,
                )
            } else {
                None
            };
            let report = if path.join("report.txt").is_file() {
                Some(
                    py.import("pathlib")?
                        .getattr("Path")?
                        .call1((path.join("report.txt"),))?
                        .call_method1("read_text", ("utf-8",))?
                        .extract::<String>()?,
                )
            } else {
                None
            };
            cases.push(format!("{name}.{method}"));
            observations
                .push(json!({"case":format!("{name}.{method}"),"audit":audit,"report":report}));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 7
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original audit assertion execution incomplete",
        ));
    }
    let directories = directories.bind(py).borrow();
    let paths = directories
        .paths
        .lock()
        .map_err(|_| PyValueError::new_err("audit temporary accounting poisoned"))?;
    if paths.len() != 7
        || paths
            .iter()
            .any(|(_, cleaned)| !cleaned.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original audit teardown accounting drift",
        ));
    }
    for (path, _) in paths.iter() {
        crate::tooling_environment::reject_links(path, "linked original audit evidence")
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(workspace) || crate::process_owner::cleanup_pending(path) {
            return Err(PyValueError::new_err(
                "original audit scratch ownership unresolved",
            ));
        }
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            crate::tooling_environment::reject_links(&entry.path(), "linked original audit file")
                .map_err(PyValueError::new_err)?;
            if !entry.file_type()?.is_file()
                || !["state.json", "audit.json", "report.txt"]
                    .contains(&entry.file_name().to_str().unwrap_or(""))
            {
                return Err(PyValueError::new_err(
                    "unexpected original audit evidence; retained",
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
        "cases":cases,"tests_run":7,"errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "observations":observations,"ports":"unchanged original audit/test/fake/image/atomic writer bodies; private patchable module/globals/environment/clock, requests HTTPError only, delayed project-confined temporary cleanup; not native interruption, shared-rule substitution or command equivalence"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("audit test workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect audit test workspace")?
        .next()
        .is_some()
    {
        return Err("audit test workspace must be empty".into());
    }
    Python::initialize();
    let report = Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        let disabled = py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect historical bytecode policy")?;
        if !disabled {
            return Err("original audit tests require disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original audit tests failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })?;
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot verify original audit cleanup")?
        .next()
        .is_some()
    {
        return Err("original audit tests left scratch; retained".into());
    }
    Ok(report)
}
