//! Original offline status assertions retain real read-only SQLite and private patchable modules.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{directories, observed, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 9] = [
    (
        "tests/reference/status.py",
        "fa12779a7275a2447479f9e131f5f9f0fa14cce75524cbcf0be3b42c615af32c",
    ),
    (
        "tests/test_status.py",
        "3b014accd3b6abc2aec6ff99af1eabb4e41c7af6d53b8247ead683f41e6bbc08",
    ),
    (
        "emojikit/operator_config.py",
        "a19f804a7bdd37baccb474eac96df6aa4d46439dd956e457c06adf3c180b9591",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
    (
        "tests/reference/plan_status.py",
        "3e1eb749032af4de33283c731c9c1f3ca29baa89204fea807fc6559a16439ee9",
    ),
    (
        "tests/test_plan_status.py",
        "a4b0a6c677af4b7412ea4a7434bc52def7989b7788e384085f580077953bdd50",
    ),
    (
        "tests/reference/panel_plan.py",
        "14d4df245dbb13a13c768f405ba0217b2ea885b204d19c2c4d8a691177a7100e",
    ),
    (
        "emojikit/state_artifacts.py",
        "3b0faf872a3d7ee9ebfd8324060e7c2ff7541ead4b743fa30e5ebd9939248001",
    ),
    (
        "tests/test_native_plan_status_cli.py",
        "4ad04ddd11093c8ffc78ee75bdb6d05c032e7a05adc5d500215630688505f514",
    ),
];
const METHODS: [&str; 4] = [
    "test_a_stale_roster_exits_3_and_names_the_fix",
    "test_everything_current_exits_0",
    "test_a_stale_archive_is_stale_too",
    "test_unpublished_emoji_are_counted_without_writing_the_catalog",
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("status tests require exactly nine pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("status tests source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

#[pyclass]
struct Temporary {
    name: String,
    exited: Arc<AtomicBool>,
}
#[pymethods]
impl Temporary {
    fn __enter__(&self) -> &str {
        &self.name
    }
    fn __exit__(
        &self,
        _kind: &Bound<'_, PyAny>,
        _error: &Bound<'_, PyAny>,
        _trace: &Bound<'_, PyAny>,
    ) {
        self.exited.store(true, Ordering::SeqCst);
    }
}
#[pyclass]
struct Directories {
    root: PathBuf,
    paths: Mutex<Vec<(PathBuf, Arc<AtomicBool>)>>,
}
#[pymethods]
impl Directories {
    fn gettempdir(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }
    fn temporary(&self, py: Python<'_>) -> PyResult<Py<Temporary>> {
        let options = PyDict::new(py);
        options.set_item("dir", &self.root)?;
        options.set_item("prefix", "original-status-")?;
        let name: String = crate::source_oracle_runtime::host_import(py, "tempfile")?
            .call_method("mkdtemp", (), Some(&options))?
            .extract()?;
        let path = crate::tooling_environment::physical_root(Path::new(&name))
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(self.root.as_path()) {
            return Err(PyValueError::new_err("original status temporary escaped"));
        }
        let exited = Arc::new(AtomicBool::new(false));
        self.paths
            .lock()
            .map_err(|_| PyRuntimeError::new_err("status temporary accounting poisoned"))?
            .push((path, exited.clone()));
        Py::new(py, Temporary { name, exited })
    }
}
#[pyclass]
struct Report {
    function: Py<PyAny>,
    reports: Mutex<Vec<Value>>,
}
#[pymethods]
impl Report {
    fn __call__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let result = self.function.bind(py).call0()?;
        self.reports
            .lock()
            .map_err(|_| PyRuntimeError::new_err("status report accounting poisoned"))?
            .push(observed(&result)?);
        Ok(result)
    }
}
fn forbidden<'py>(py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    let options = PyDict::new(py);
    options.set_item(
        "side_effect",
        PyRuntimeError::new_err(format!("unpatched original status {name}")).value(py),
    )?;
    py.import("unittest.mock")?
        .getattr("Mock")?
        .call((), Some(&options))
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let path = py.import("pathlib")?.getattr("Path")?;
    let environment = PyDict::new(py);
    let os = ns.call0()?;
    os.setattr("environ", &environment)?;
    let operator_bindings = PyDict::new(py);
    operator_bindings.set_item("os", os)?;
    let operator = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["value"],
        &[],
        &operator_bindings,
    )?;
    let operator_api = ns.call0()?;
    operator_api.setattr("value", required(&operator, "value")?)?;
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["OperatorConfigMissing"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let roster = ns.call0()?;
    roster.setattr("CATALOG", path.call1((workspace.join("absent.db"),))?)?;
    roster.setattr("check_stale", forbidden(py, "roster check")?)?;
    let archive = ns.call0()?;
    archive.setattr("check", forbidden(py, "archive check")?)?;
    let bindings = PyDict::new(py);
    for (name, value) in [
        ("pack_manifest", roster.clone()),
        ("pack_archive", archive.clone()),
        ("operator_config", operator_api.clone()),
        ("sqlite3", py.import("sqlite3")?.into_any()),
        ("argparse", py.import("argparse")?.into_any()),
        (
            "OperatorConfigMissing",
            required(&errors, "OperatorConfigMissing")?,
        ),
        ("setup_logging", forbidden(py, "logging setup")?),
        ("load_env", forbidden(py, "environment loader")?),
    ] {
        bindings.set_item(name, value)?;
    }
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (&sources[0].text, &sources[0].path))?;
    bindings.set_item("__doc__", ast.call_method1("get_docstring", (tree,))?)?;
    bindings.set_item("EXIT_OK", 0)?;
    bindings.set_item("EXIT_STALE", 3)?;
    let functions = ["_unpublished", "report", "main"];
    let original = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &functions,
        &["int", "print"],
        &bindings,
    )?;
    let status = PyModule::new(py, "_original_status_assertions")?;
    status.dict().update(original.as_mapping())?;
    let function_type = py.import("types")?.getattr("FunctionType")?;
    for name in functions {
        let original = required(&original, name)?;
        let rebound = function_type.call1((
            original.getattr("__code__")?,
            status.dict(),
            original.getattr("__name__")?,
            original.getattr("__defaults__")?,
            original.getattr("__closure__")?,
        ))?;
        for attribute in ["__kwdefaults__", "__annotations__"] {
            rebound.setattr(attribute, original.getattr(attribute)?)?;
        }
        if !rebound.getattr("__globals__")?.is(status.dict()) {
            return Err(PyValueError::new_err(
                "status globals are not patchable module storage",
            ));
        }
        status.add(name, rebound)?;
    }
    let report = Py::new(
        py,
        Report {
            function: status.getattr("report")?.unbind(),
            reports: Mutex::new(vec![]),
        },
    )?;
    status.add("report", report.bind(py))?;
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
    tempfile.setattr("gettempdir", temporary.bind(py).getattr("gettempdir")?)?;
    let private_patch = ns.call0()?;
    private_patch.setattr(
        "object",
        py.import("unittest.mock")?
            .getattr("patch")?
            .getattr("object")?,
    )?;
    private_patch.setattr(
        "dict",
        crate::source_config_oracle::private_environment_patch(py, &environment)?
            .getattr("dict")?,
    )?;
    let mock = ns.call0()?;
    mock.setattr("patch", private_patch)?;
    let unittest = py.import("unittest")?;
    let tests = PyDict::new(py);
    for (name, value) in [
        ("status", status.into_any()),
        ("pack_manifest", roster),
        ("pack_archive", archive),
        ("tempfile", tempfile.clone()),
        ("mock", mock),
        ("unittest", unittest.clone().into_any()),
        ("Path", path),
        ("sqlite3", py.import("sqlite3")?.into_any()),
        ("contextlib", py.import("contextlib")?.into_any()),
        ("io", py.import("io")?.into_any()),
    ] {
        tests.set_item(name, value)?;
    }
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["TheStatusCommand"],
        &["__build_class__"],
        &tests,
    )?;
    let class = required(&tests, "TheStatusCommand")?;
    let names: Vec<String> = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    let mut expected = METHODS.to_vec();
    expected.sort_unstable();
    if names != expected {
        return Err(PyValueError::new_err(
            "original status method collection drift",
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
                    "original status {method} reported {field}: {}; retained",
                    crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                )));
            }
        }
        if !environment.is_empty() {
            return Err(PyValueError::new_err(
                "original status environment not restored",
            ));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 4
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original status execution incomplete",
        ));
    }
    let reports = report
        .borrow(py)
        .reports
        .lock()
        .map_err(|_| PyRuntimeError::new_err("status observations poisoned"))?
        .clone();
    if reports.len() != 4 {
        return Err(PyValueError::new_err("original status report count drift"));
    }
    let plans =
        crate::source_plan_report_tests::run(py, workspace, sources, &tempfile, &operator_api)?;
    let temporary = temporary.borrow(py);
    let paths = temporary
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("status cleanup accounting poisoned"))?;
    if paths.len() != 3
        || paths
            .iter()
            .any(|(_, exited)| !exited.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original status temporary exit incomplete",
        ));
    }
    directories(workspace, workspace, &mut 16, 0, &mut vec![])?;
    let entries = std::fs::read_dir(workspace)?.collect::<std::io::Result<Vec<_>>>()?;
    if entries.len() != 3
        || entries
            .iter()
            .any(|entry| !paths.iter().any(|(path, _)| *path == entry.path()))
    {
        return Err(PyValueError::new_err(
            "unexpected original status workspace; retained",
        ));
    }
    let mut cleanup = Vec::new();
    for (index, (path, _)) in paths.iter().enumerate() {
        let expected: &[&str] = match index {
            0 => &["catalog.db"],
            1 => &["pack_plan.json", "publish_mine.json"],
            2 => &[],
            _ => unreachable!(),
        };
        let mut members = std::fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        members.sort();
        let wanted = expected
            .iter()
            .map(|name| path.join(name))
            .collect::<Vec<_>>();
        if members != wanted
            || members.iter().any(|member| {
                !std::fs::metadata(member)
                    .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 65536)
            })
        {
            return Err(PyValueError::new_err(
                "unexpected original status members; retained",
            ));
        }
        cleanup.extend(members);
    }
    for path in cleanup {
        std::fs::remove_file(path)?;
    }
    for (path, _) in paths.iter() {
        std::fs::remove_dir(path)?;
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":4,"cases":METHODS.map(|name|format!("TheStatusCommand.{name}")),"temporary_calls":3,"reports":reports,"plans":plans,
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"four unchanged original methods/private module/environment/literal os.environ patch adapter/real SQLite URI read-only; mocked roster/archive/log/environment ports do not prove native checks or OS isolation; native command spelling adapted; delayed success-only cleanup"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("status workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect status workspace")?
        .next()
        .is_some()
    {
        return Err("status workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect status bytecode policy")?
        {
            return Err("status tests require disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original status tests failed: {}",
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
        "status-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
