//! Original bot transport/access/poll/cursor assertions with private globals and no network.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{observed, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 4] = [
    (
        "tests/reference/emoji_bot.py",
        "0ae857dd8607f1e054c6825b3f1e6ecd19eaf2347cfc69a330ae0f6aefd0071f",
    ),
    (
        "tests/test_emoji_bot.py",
        "609454dce3ce7624975d4e56bfa4b05548dbc7ccfc3a2e0c48b8f87d923a5e19",
    ),
    (
        "emojikit/cli_env.py",
        "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
];
const CLASSES: [(&str, usize); 4] = [
    ("TestIdsTypedAsText", 2),
    ("TestAccessControl", 11),
    ("TestMainWiring", 2),
    ("TestOffsetPersistence", 3),
];
const PURE_TYPED: [&str; 2] = [
    "test_every_shape_people_actually_paste",
    "test_prose_holding_a_long_number_is_not_a_lookup",
];
const FUNCTIONS: [&str; 9] = [
    "enrich_labels",
    "send_reply",
    "answer_typed_ids",
    "allowed_user_ids",
    "allowed_channel_ids",
    "handle_update",
    "_load_offset",
    "_save_offset",
    "main",
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("bot command oracle requires exactly four pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("bot command source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

#[pyclass]
struct Temporary {
    name: String,
    cleaned: Arc<AtomicBool>,
}
#[pymethods]
impl Temporary {
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }
    fn cleanup(&self) {
        // Original tearDown still runs, but only verified success may discard its evidence.
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
        options.set_item("prefix", "original-bot-")?;
        let name: String = crate::source_oracle_runtime::host_import(py, "tempfile")?
            .call_method("mkdtemp", (), Some(&options))?
            .extract()?;
        let path = crate::tooling_environment::physical_root(Path::new(&name))
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(self.root.as_path()) {
            return Err(PyValueError::new_err(
                "original bot temporary escaped its workspace",
            ));
        }
        let cleaned = Arc::new(AtomicBool::new(false));
        self.paths
            .lock()
            .map_err(|_| PyRuntimeError::new_err("bot temporary accounting poisoned"))?
            .push((path, cleaned.clone()));
        Py::new(py, Temporary { name, cleaned })
    }
}

fn constants(py: Python<'_>, source: &Source, bindings: &Bound<'_, PyDict>) -> PyResult<()> {
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (&source.text, &source.path))?;
    let mut names = std::collections::BTreeSet::new();
    for node in crate::source_test_inventory::body(&tree)? {
        if crate::source_test_inventory::kind(&node)? != "Assign" {
            continue;
        }
        let targets = node.getattr("targets")?;
        if targets.len()? != 1 {
            continue;
        }
        let target = targets.get_item(0)?;
        if crate::source_test_inventory::kind(&target)? != "Name" {
            continue;
        }
        let name: String = target.getattr("id")?.extract()?;
        if !["START_TEXT", "DENIED_TEXT"].contains(&name.as_str()) {
            continue;
        }
        if !names.insert(name.clone()) {
            return Err(PyValueError::new_err("duplicate original bot constant"));
        }
        bindings.set_item(
            name,
            ast.call_method1("literal_eval", (node.getattr("value")?,))?,
        )?;
    }
    if names.len() != 2 {
        return Err(PyValueError::new_err("missing original bot constants"));
    }
    Ok(())
}

fn module<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
    environment: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyModule>> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let os = ns.call0()?;
    os.setattr("environ", environment)?;
    let sys = ns.call0()?;
    sys.setattr("stderr", py.import("io")?.getattr("StringIO")?.call0()?)?;
    let bindings = PyDict::new(py);
    bindings.set_item("os", &os)?;
    bindings.set_item("sys", sys)?;
    let numeric = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["safe_int_env"],
        &["int", "print", "ValueError"],
        &bindings,
    )?;
    let path = py.import("pathlib")?.getattr("Path")?;
    bindings.set_item("Path", &path)?;
    bindings.set_item("contextlib", py.import("contextlib")?)?;
    bindings.set_item("json", py.import("json")?)?;
    bindings.set_item("tempfile", py.import("tempfile")?)?;
    bindings.set_item("stat", py.import("stat")?)?;
    bindings.set_item("os", py.import("os")?)?;
    bindings.set_item("time", py.import("time")?)?;
    bindings.set_item("_REPLACE_DEADLINE", 1.0)?;
    let writer = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["_replace", "write_json_atomic"],
        &["PermissionError", "FileNotFoundError", "min"],
        &bindings,
    )?;
    let original = crate::source_bot_oracle::namespace(py, sources[0])?;
    original.set_item("os", os)?;
    original.set_item("json", py.import("json")?)?;
    original.set_item("re", py.import("re")?)?;
    original.set_item("safe_int_env", required(&numeric, "safe_int_env")?)?;
    original.set_item("write_json_atomic", required(&writer, "write_json_atomic")?)?;
    original.set_item(
        "OFFSET_FILE",
        path.call1((workspace.join("never-used-default.json"),))?,
    )?;
    let mock = py.import("unittest.mock")?;
    let stop_options = PyDict::new(py);
    stop_options.set_item(
        "side_effect",
        py.get_type::<pyo3::exceptions::PyAssertionError>()
            .call1(("unpatched original bot dependency",))?,
    )?;
    for name in ["load_env", "setup_logging", "Telegram", "redact"] {
        original.set_item(name, mock.getattr("Mock")?.call((), Some(&stop_options))?)?;
    }
    let time = ns.call0()?;
    time.setattr(
        "sleep",
        mock.getattr("Mock")?.call((), Some(&stop_options))?,
    )?;
    original.set_item("time", time)?;
    // These selected cases never reject sendMessage; this class is an explicit unexercised port.
    original.set_item("BotApiError", py.get_type::<PyRuntimeError>())?;
    let logger = py
        .import("logging")?
        .getattr("Logger")?
        .call1(("original-bot",))?;
    logger.call_method1(
        "addHandler",
        (py.import("logging")?.getattr("NullHandler")?.call0()?,),
    )?;
    original.set_item("log", logger)?;
    constants(py, sources[0], &original)?;
    let ports = PyDict::new(py);
    for (key, value) in original.iter() {
        let name: String = key.extract()?;
        // A previous namespace's restricted builtins must not replace this selector's allowlist.
        if !["__builtins__", "__name__"].contains(&name.as_str()) {
            ports.set_item(key, value)?;
        }
    }
    let extra = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &FUNCTIONS,
        &[
            "int",
            "set",
            "len",
            "str",
            "max",
            "range",
            "enumerate",
            "Exception",
            "OSError",
            "ValueError",
            "KeyError",
            "TypeError",
        ],
        &ports,
    )?;
    let module = PyModule::new(py, "_original_bot_commands")?;
    module.dict().update(extra.as_mapping())?;
    let function_type = py.import("types")?.getattr("FunctionType")?;
    for (key, value) in extra.iter() {
        if !value.is_instance(&function_type)? {
            continue;
        }
        let globals = value.getattr("__globals__")?;
        // Shared numeric/writer functions retain their own private environment and real stdlib ports.
        if !globals.is(&extra) && !globals.is(&original) {
            continue;
        }
        let function = function_type.call1((
            value.getattr("__code__")?,
            module.dict(),
            value.getattr("__name__")?,
            value.getattr("__defaults__")?,
            value.getattr("__closure__")?,
        ))?;
        for attribute in ["__kwdefaults__", "__annotations__"] {
            function.setattr(attribute, value.getattr(attribute)?)?;
        }
        module.dict().set_item(key, function)?;
    }
    Ok(module)
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let environment = PyDict::new(py);
    let bot = module(py, workspace, sources, &environment)?;
    let unittest = py.import("unittest")?;
    let directories = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let temporary = ns.call0()?;
    temporary.setattr(
        "TemporaryDirectory",
        directories.bind(py).getattr("temporary")?,
    )?;
    let os = ns.call0()?;
    os.setattr("environ", &environment)?;
    let bindings = PyDict::new(py);
    bindings.set_item("unittest", &unittest)?;
    bindings.set_item("mock", py.import("unittest.mock")?)?;
    bindings.set_item("os", os)?;
    bindings.set_item("tempfile", temporary)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("b", &bot)?;
    let definitions = CLASSES.map(|(name, _)| name);
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &definitions,
        &[
            "__build_class__",
            "str",
            "len",
            "set",
            "all",
            "isinstance",
            "int",
            "KeyboardInterrupt",
        ],
        &bindings,
    )?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let mut cases = Vec::new();
    for (name, count) in CLASSES {
        let class = required(&tests, name)?;
        let methods: Vec<String> = loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        if methods.len()
            != count
                + if name == "TestIdsTypedAsText" {
                    PURE_TYPED.len()
                } else {
                    0
                }
        {
            return Err(PyValueError::new_err(
                "original bot command collection drift",
            ));
        }
        for method in methods {
            if name == "TestIdsTypedAsText" && PURE_TYPED.contains(&method.as_str()) {
                continue;
            }
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
                        "original bot {name}.{method} reported {field}: {}; evidence retained",
                        crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                    )));
                }
            }
            if !environment.is_empty() {
                return Err(PyValueError::new_err(
                    "original bot environment patch was not restored",
                ));
            }
            cases.push(format!("{name}.{method}"));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 18
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original bot command assertions incomplete",
        ));
    }
    for name in ["load_env", "setup_logging", "Telegram", "redact"] {
        bot.getattr(name)?.call_method0("assert_not_called")?;
    }
    bot.getattr("time")?
        .getattr("sleep")?
        .call_method0("assert_not_called")?;
    let mut observations = Vec::new();
    for (key, raw, function) in [
        ("BOT_ALLOWED_USER_IDS", "1, 2 ;3, oops,", "allowed_user_ids"),
        (
            "BOT_ALLOWED_USER_IDS",
            "\u{1c}+۱, ۲;۳_۰, oops,\u{1f}",
            "allowed_user_ids",
        ),
        (
            "BOT_ALLOWED_CHANNEL_IDS",
            "-1001234567890; abc,,-5",
            "allowed_channel_ids",
        ),
        (
            "BOT_ALLOWED_CHANNEL_IDS",
            "-۱۰۰۱۲۳۴۵۶۷۸۹۰;-۵;+6;7_0;abc",
            "allowed_channel_ids",
        ),
    ] {
        environment.set_item(key, raw)?;
        let values = py
            .import("builtins")?
            .getattr("sorted")?
            .call1((bot.getattr(function)?.call0()?,))?;
        observations.push(json!({"key":key,"raw":raw,"ids":observed(&values)?}));
        environment.clear();
    }
    crate::source_tooling_oracle::directories(workspace, workspace, &mut 64, 0, &mut Vec::new())?;
    let state = directories.borrow(py);
    let tracked = state
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("bot temporary accounting poisoned"))?;
    if tracked.len() != 5
        || tracked
            .iter()
            .any(|(_, cleaned)| !cleaned.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original bot teardown accounting drift",
        ));
    }
    let expected = tracked
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
    if actual != expected {
        return Err(PyValueError::new_err(
            "unexpected original bot scratch; retained",
        ));
    }
    for (path, _) in tracked.iter() {
        if path.parent() != Some(workspace) {
            return Err(PyValueError::new_err("foreign original bot cleanup"));
        }
        for (index, entry) in std::fs::read_dir(path)?.enumerate() {
            let entry = entry?;
            if index > 0
                || !entry.file_type()?.is_file()
                || !["state.json", "state_emoji_bot.json"]
                    .contains(&entry.file_name().to_str().unwrap_or(""))
                || entry.metadata()?.len() > 16384
            {
                return Err(PyValueError::new_err(
                    "unexpected original bot output; retained",
                ));
            }
        }
    }
    for (path, _) in tracked.iter() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":18,"cases":cases,"errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "temporary_calls":5,"allowlists":observations,
        "ports":"unchanged original18typed-transport/access/main/cursor assertion methods; private environment, patchable source globals, real original atomic writer and success-only delayed temporary cleanup; original KeyboardInterrupt retained; inert startup dependencies and unexercised rejection-exception port; no real transport/account or native lifecycle equivalence"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("bot command workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect bot command workspace")?
        .next()
        .is_some()
    {
        return Err("bot command workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect bot bytecode policy")?
        {
            return Err("bot command oracle requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original bot command replay failed: {}",
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
        "bot-command-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
