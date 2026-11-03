//! Fixed original ordering assertions; source bodies and private globals, never target policies.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{directories, observed, python, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 4] = [
    (
        "tests/reference/sync_order.py",
        "00a0765118e3e0a2ff2999b7c1e4a649434147786da2adb9a9c1e710864112e0",
    ),
    (
        "tests/test_sync_order.py",
        "98f5c7971a166333bacee7245af8154116ea90a775244cc16f8ace1edd260447",
    ),
    (
        "tests/reference/collection_state.py",
        "b8bf2695954cbe61a3e3cb68c8da0c583aa80a1b645abba01eabe9a28776074a",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
];
const CLASSES: [(&str, usize); 4] = [
    ("PlanMoves", 4),
    ("ReorderIsIdempotent", 1),
    ("TheRecordedOrderFollowsTheLiveOne", 4),
    ("OnePackAtATime", 4),
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("order oracle requires exactly four pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("sync-order source identity drift; refusing execution".into());
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
        options.set_item("prefix", "original-order-")?;
        let name: String = py
            .import("tempfile")?
            .call_method("mkdtemp", (), Some(&options))?
            .extract()?;
        let path = crate::tooling_environment::physical_root(Path::new(&name))
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(self.root.as_path()) {
            return Err(PyValueError::new_err("original order temporary escaped"));
        }
        let cleaned = Arc::new(AtomicBool::new(false));
        self.paths
            .lock()
            .map_err(|_| PyRuntimeError::new_err("order temporary accounting poisoned"))?
            .push((path, cleaned.clone()));
        Py::new(py, Temporary { name, cleaned })
    }
}

fn modules<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
) -> PyResult<(Bound<'py, PyModule>, Bound<'py, PyDict>)> {
    let bindings = PyDict::new(py);
    let path = py.import("pathlib")?.getattr("Path")?;
    bindings.set_item("Path", &path)?;
    for name in ["contextlib", "json", "os", "stat", "tempfile", "time"] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item(
        "_REPLACE_DEADLINE",
        crate::source_logging_oracle::literal(py, sources[3], "_REPLACE_DEADLINE", false)?,
    )?;
    let writer = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["LockBusy", "_replace", "write_json_atomic"],
        &[
            "__build_class__",
            "RuntimeError",
            "PermissionError",
            "FileNotFoundError",
            "min",
        ],
        &bindings,
    )?;
    bindings.set_item("write_json_atomic", required(&writer, "write_json_atomic")?)?;
    let contextlib = py.import("contextlib")?;
    bindings.set_item("writer", contextlib.getattr("nullcontext")?)?;
    bindings.set_item(
        "FMT_TAG",
        python(py, &json!({"static":"s","video":"v","animated":"a"}))?,
    )?;
    bindings.set_item("MIXED", "mixed")?;
    bindings.set_item("PER_SET", 200)?;
    let state = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &[
            "StateError",
            "_state_path",
            "_lock_path",
            "load_json",
            "save_json",
            "load_state",
            "_validate_state",
        ],
        &[
            "__build_class__",
            "RuntimeError",
            "OSError",
            "ValueError",
            "isinstance",
            "dict",
            "str",
            "list",
            "all",
            "bool",
            "int",
            "set",
            "enumerate",
            "sorted",
            "len",
        ],
        &bindings,
    )?;
    for name in ["_state_path", "_lock_path", "load_state", "save_json"] {
        bindings.set_item(name, required(&state, name)?)?;
    }
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let os = ns.call0()?;
    os.setattr("environ", PyDict::new(py))?;
    bindings.set_item("os", os)?;
    let mock = py.import("unittest.mock")?.getattr("Mock")?;
    let clock = ns.call0()?;
    clock.setattr("sleep", mock.call0()?)?;
    bindings.set_item("time", clock)?;
    for name in ["log", "load_env", "setup_logging"] {
        bindings.set_item(name, mock.call0()?)?;
    }
    for name in ["Catalog", "Telegram"] {
        let options = PyDict::new(py);
        options.set_item(
            "side_effect",
            py.import("builtins")?
                .getattr("AssertionError")?
                .call1(("unpatched original transport/catalog",))?,
        )?;
        bindings.set_item(name, mock.call((), Some(&options))?)?;
    }
    bindings.set_item("exclusive_lock", contextlib.getattr("nullcontext")?)?;
    bindings.set_item("LockBusy", required(&writer, "LockBusy")?)?;
    bindings.set_item("argparse", py.import("argparse")?)?;
    bindings.set_item("ROOT", path.call1((workspace,))?)?;
    bindings.set_item(
        "MOVE_DELAY",
        crate::source_logging_oracle::literal(py, sources[0], "MOVE_DELAY", false)?,
    )?;
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (&sources[0].text, &sources[0].path))?;
    bindings.set_item("__doc__", ast.call_method1("get_docstring", (tree,))?)?;
    for (name, value) in [("EXIT_OK", 0), ("EXIT_USAGE", 2), ("EXIT_FAILED", 4)] {
        bindings.set_item(name, value)?;
    }
    let names = ["desired_order", "plan_moves", "sync_set", "main"];
    let globals = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &names,
        &[
            "list",
            "enumerate",
            "next",
            "len",
            "set",
            "sorted",
            "int",
            "str",
            "map",
            "print",
        ],
        &bindings,
    )?;
    let module = PyModule::new(py, "_original_sync_order")?;
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
        if !function.getattr("__globals__")?.is(&module.dict()) {
            return Err(PyValueError::new_err(
                "order globals are not patchable module storage",
            ));
        }
        module.add(name, function)?;
    }
    Ok((module, state))
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let (order, state) = modules(py, workspace, sources)?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let state_api = ns.call0()?;
    for name in ["save_json", "_state_path"] {
        state_api.setattr(name, required(&state, name)?)?;
    }
    let temporary = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(vec![]),
        },
    )?;
    let tempfile = ns.call0()?;
    tempfile.setattr(
        "TemporaryDirectory",
        temporary.bind(py).getattr("temporary")?,
    )?;
    let unittest = py.import("unittest")?;
    let bindings = PyDict::new(py);
    for (name, value) in [
        ("so", order.clone().into_any()),
        ("cs", state_api),
        ("tempfile", tempfile),
        ("unittest", unittest.clone().into_any()),
        ("mock", py.import("unittest.mock")?.into_any()),
        ("Path", py.import("pathlib")?.getattr("Path")?),
        ("contextlib", py.import("contextlib")?.into_any()),
        ("io", py.import("io")?.into_any()),
        (
            "redirect_stdout",
            py.import("contextlib")?.getattr("redirect_stdout")?,
        ),
    ] {
        bindings.set_item(name, value)?;
    }
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &[
            "_live",
            "PlanMoves",
            "ReorderIsIdempotent",
            "TheRecordedOrderFollowsTheLiveOne",
            "OnePackAtATime",
        ],
        &[
            "__build_class__",
            "str",
            "list",
            "all",
            "range",
            "reversed",
            "len",
            "type",
            "property",
        ],
        &bindings,
    )?;
    let options = PyDict::new(py);
    options.set_item("wraps", order.getattr("sync_set")?)?;
    let synced = py
        .import("unittest.mock")?
        .getattr("Mock")?
        .call((), Some(&options))?;
    order.add("sync_set", &synced)?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    for (name, count) in CLASSES {
        let class = required(&tests, name)?;
        let methods: Vec<String> = unittest
            .getattr("TestLoader")?
            .call0()?
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        if methods.len() != count {
            return Err(PyValueError::new_err(
                "original order method inventory drift",
            ));
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
                        "original order {name}.{method} reported {field}; retained"
                    )));
                }
            }
            if !order.getattr("os")?.getattr("environ")?.is_empty()? {
                return Err(PyValueError::new_err(
                    "original order private environment not restored",
                ));
            }
            cases.push(format!("{name}.{method}"));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 13
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err("original order execution incomplete"));
    }
    let mut records = Vec::new();
    for call in synced.getattr("call_args_list")?.try_iter()? {
        let call = call?;
        let args = call.getattr("args")?;
        let tg = args.get_item(0)?;
        let mut moves = Vec::new();
        for movement in tg
            .getattr("set_sticker_position")?
            .getattr("call_args_list")?
            .try_iter()?
        {
            moves.push(observed(&movement?.getattr("args")?)?);
        }
        records.push(json!({"record":observed(&args.get_item(2)?)?,"apply":call.getattr("kwargs")?.get_item("apply")?.extract::<bool>()?,"moves":moves}));
    }
    if records.len() != 3 {
        return Err(PyValueError::new_err(
            "original record observation count drift",
        ));
    }
    let mut plans = Vec::new();
    for (ids, desired, pinned) in [
        (vec!["logo", "a", "b", "c"], vec!["a", "b", "c"], 1),
        (vec!["logo", "c", "a", "b"], vec!["a", "b", "c"], 1),
        (vec!["logo", "c", "b", "a"], vec!["a", "b", "c"], 1),
        (vec!["logo", "b", "c", "a"], vec!["a", "b", "c"], 1),
        (vec!["b", "a"], vec!["a", "b"], 0),
    ] {
        let live = ids
            .iter()
            .map(|id| json!({"custom_emoji_id":id,"file_id":format!("f-{id}")}))
            .collect::<Vec<_>>();
        let moves = order.call_method1(
            "plan_moves",
            (python(py, &json!(live))?, desired.clone(), pinned),
        )?;
        plans
            .push(json!({"live":live,"desired":desired,"pinned":pinned,"moves":observed(&moves)?}));
    }
    let state = temporary.borrow(py);
    let paths = state
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("order cleanup accounting poisoned"))?;
    if paths.len() != 4
        || paths
            .iter()
            .any(|(_, exited)| !exited.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original order temporary cleanup incomplete",
        ));
    }
    directories(workspace, workspace, &mut 32, 0, &mut vec![])?;
    let expected = paths
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let entries = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
    if entries != expected {
        return Err(PyValueError::new_err(
            "unexpected original order workspace; retained",
        ));
    }
    for directory in &expected {
        let entries = std::fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
        if entries
            != ["catalog.db", "publish_pk.json"]
                .map(std::ffi::OsString::from)
                .into_iter()
                .collect()
            || std::fs::metadata(directory.join("catalog.db"))?.len() != 0
            || std::fs::metadata(directory.join("publish_pk.json"))?.len() > 16384
        {
            return Err(PyValueError::new_err(
                "unexpected original order temporary contents; retained",
            ));
        }
        let saved: Value =
            serde_json::from_slice(&std::fs::read(directory.join("publish_pk.json"))?)
                .map_err(|_| PyValueError::new_err("original order state is not JSON"))?;
        if saved["base"] != "pk" || saved["sets"].as_array().map(Vec::len) != Some(3) {
            return Err(PyValueError::new_err(
                "original order state drift; retained",
            ));
        }
    }
    for directory in expected {
        std::fs::remove_file(directory.join("catalog.db"))?;
        std::fs::remove_file(directory.join("publish_pk.json"))?;
        std::fs::remove_dir(directory)?;
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":13,"cases":cases,"temporary_calls":4,"plans":plans,"records":records,
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"13 unchanged original methods/private module and environment/real original state validation and atomic writer; original fake catalog and mocked transport retained; prerequisite/logging/catalog writer/family lock ports do not prove native ownership; delayed checked temporary cleanup; native command evidence separate"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("order workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect order workspace")?
        .next()
        .is_some()
    {
        return Err("order workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect order bytecode policy")?
        {
            return Err("order oracle requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original sync-order replay failed: {}",
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
        "order-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
