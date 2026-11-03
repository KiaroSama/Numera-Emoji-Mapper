//! Original operator assertions use a private environment and the shared historical source owner.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 3] = [
    (
        "emojikit/operator_config.py",
        "a19f804a7bdd37baccb474eac96df6aa4d46439dd956e457c06adf3c180b9591",
    ),
    (
        "tests/test_operator_config.py",
        "d039ab09301a4f9aeee52c48a37aeda15913c1d3365e9cf79baea8530f537388",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
];
const METHODS: [&str; 8] = [
    "test_an_unset_key_is_named_in_the_stop",
    "test_unset_logo_bots_stops_but_empty_means_none",
    "test_bot_list_ignores_case_and_at",
    "test_a_listed_bot_needs_an_existing_logo",
    "test_a_configured_logo_is_used",
    "test_a_relative_logo_path_is_under_the_project_root",
    "test_disabled_needs_no_configuration",
    "test_keywords_default_to_logo",
];
const FUNCTIONS: [&str; 7] = [
    "value",
    "require",
    "brand_logo_bots",
    "brand_logo_path",
    "brand_logo_keywords",
    "brand_logo",
    "stop_unless",
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("operator oracle requires exactly three pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("operator configuration source identity drift; refusing execution".into());
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
struct TemporaryFactory {
    root: PathBuf,
    paths: Mutex<Vec<(PathBuf, Arc<AtomicBool>)>>,
}
#[pymethods]
impl TemporaryFactory {
    fn temporary(&self, py: Python<'_>) -> PyResult<Py<Temporary>> {
        let options = PyDict::new(py);
        options.set_item("dir", &self.root)?;
        options.set_item("prefix", "original-operator-")?;
        let name: String = py
            .import("tempfile")?
            .call_method("mkdtemp", (), Some(&options))?
            .extract()?;
        let path = crate::tooling_environment::physical_root(Path::new(&name))
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(self.root.as_path()) {
            return Err(PyValueError::new_err("original operator temporary escaped"));
        }
        let exited = Arc::new(AtomicBool::new(false));
        self.paths
            .lock()
            .map_err(|_| PyRuntimeError::new_err("operator temporary accounting poisoned"))?
            .push((path, exited.clone()));
        Py::new(py, Temporary { name, exited })
    }
}
fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let environment = PyDict::new(py);
    let os = ns.call0()?;
    os.setattr("environ", &environment)?;
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["OperatorConfigMissing"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let bindings = PyDict::new(py);
    let path = py.import("pathlib")?.getattr("Path")?;
    bindings.set_item("Path", &path)?;
    bindings.set_item("ROOT", path.call1((workspace,))?)?;
    bindings.set_item("os", &os)?;
    bindings.set_item(
        "WHAT",
        crate::source_logging_oracle::literal(py, sources[0], "WHAT", false)?,
    )?;
    bindings.set_item(
        "OperatorConfigMissing",
        required(&errors, "OperatorConfigMissing")?,
    )?;
    let original = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &FUNCTIONS,
        &["str", "frozenset", "SystemExit"],
        &bindings,
    )?;
    let api = ns.call0()?;
    for name in FUNCTIONS {
        api.setattr(name, required(&original, name)?)?;
    }
    api.setattr("ROOT", required(&original, "ROOT")?)?;
    let temporary = Py::new(
        py,
        TemporaryFactory {
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
    let tests = PyDict::new(py);
    tests.set_item("Path", path)?;
    tests.set_item("os", os)?;
    tests.set_item("oc", &api)?;
    tests.set_item("unittest", &unittest)?;
    tests.set_item("mock", py.import("unittest.mock")?)?;
    tests.set_item("tempfile", tempfile)?;
    tests.set_item(
        "KEYS",
        crate::source_logging_oracle::literal(py, sources[1], "KEYS", false)?,
    )?;
    tests.set_item(
        "OperatorConfigMissing",
        required(&errors, "OperatorConfigMissing")?,
    )?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["OperatorConfigTest"],
        &["__build_class__", "str", "frozenset"],
        &tests,
    )?;
    let class = required(&tests, "OperatorConfigTest")?;
    let names: Vec<String> = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    let mut expected = METHODS.to_vec();
    expected.sort_unstable();
    if names != expected {
        return Err(PyValueError::new_err("original operator collection drift"));
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
                    "original operator {method} reported {field}; retained"
                )));
            }
        }
        if !environment.is_empty() {
            return Err(PyValueError::new_err(
                "original private operator environment was not restored",
            ));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 8
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original operator execution incomplete",
        ));
    }
    let mut observations = Vec::new();
    for raw in [
        "@YourEmojiBot, other_bot",
        "\u{1c}@@YourEmojiBot, @YOURemojiBOT\u{1f}other_bot\u{1e}",
        "",
    ] {
        environment.set_item("BRAND_LOGO_BOTS", raw)?;
        let bots = api.call_method0("brand_logo_bots")?;
        let bots: Vec<String> = py
            .import("builtins")?
            .call_method1("sorted", (bots,))?
            .extract()?;
        observations.push(json!({"raw":raw,"bots":bots}));
    }
    environment.clear();
    for raw in [
        "yourbrand, logo",
        "\u{1c}yourbrand\u{1f}, ,\u{1e}logo\u{1c}",
        ", \u{1c},",
    ] {
        environment.set_item("BRAND_LOGO_KEYWORDS", raw)?;
        let keywords: Vec<String> = api.call_method0("brand_logo_keywords")?.extract()?;
        observations.push(json!({"raw":raw,"keywords":keywords}));
    }
    environment.clear();
    let state = temporary.borrow(py);
    let paths = state
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("operator cleanup accounting poisoned"))?;
    if paths.len() != 1 || !paths[0].1.load(Ordering::SeqCst) {
        return Err(PyValueError::new_err(
            "original operator context exit incomplete",
        ));
    }
    directories(workspace, workspace, &mut 8, 0, &mut vec![])?;
    let entries = std::fs::read_dir(workspace)?.collect::<std::io::Result<Vec<_>>>()?;
    if entries.len() != 1 || entries[0].path() != paths[0].0 {
        return Err(PyValueError::new_err(
            "unexpected original operator evidence; retained",
        ));
    }
    let logo = paths[0].0.join("logo.png");
    let children = std::fs::read_dir(&paths[0].0)?.collect::<std::io::Result<Vec<_>>>()?;
    if children.len() != 1 || children[0].path() != logo || std::fs::read(&logo)? != b"png" {
        return Err(PyValueError::new_err(
            "original operator temporary contents drift; retained",
        ));
    }
    std::fs::remove_file(logo)?;
    std::fs::remove_dir(&paths[0].0)?;
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":8,"cases":METHODS.map(|method|format!("OperatorConfigTest.{method}")),
        "observations":observations,"temporary_calls":1,"errors":0,"failures":0,"skipped":0,
        "ports":"unchanged original eight methods/private os.environ/mock restoration/real pathlib; delayed checked temporary context cleanup; original exception preserved; no native command or decoder equivalence"}),
    )
}
pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("operator workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect operator workspace")?
        .next()
        .is_some()
    {
        return Err("operator workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect operator bytecode policy")?
        {
            return Err("operator oracle requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original operator replay failed: {}",
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
        "operator-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
