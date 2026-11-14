//! Fixed original prebuilt environment assertions; never executes an installation path.
use crate::source_git_inventory::Source;
use pyo3::exceptions::{PyImportError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
#[path = "source_build_job_tests.rs"]
mod build_job_tests;
const IDENTITIES: [(&str, &str); 5] = [
    (
        "scripts/install_prebuilt.py",
        "e47fbac5e60d5f8c6f0468ba7e69fd70483d18aa058ea660d52b2e68dee8dc53",
    ),
    (
        "scripts/local_environment.py",
        "b568feffc0c75e27b7f08cb69aaf546be4ce544427234379316b8f708b619766",
    ),
    (
        "tests/test_prebuilt_environment.py",
        "325ea859959d4fd5b86036cd2fac7e9efd38638acdad0129f794813af0f90c6c",
    ),
    (
        "scripts/build_job.py",
        "165c15916bfd3e7f8d4b4229db70f76a15ef4468b53e201c95f2b1106ff4d7a7",
    ),
    (
        "tests/test_build_process_owner.py",
        "c773d3c5bec0573f726e1ecd4ebb6595a0d2d4d56270a69e609d6826a0e6625d",
    ),
];
const METHODS: [&str; 3] = [
    "test_existing_installation_refuses_before_writing",
    "test_owned_temp_and_caches_override_external_values",
    "test_linked_cache_refuses_before_using_outside_directory",
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("prebuilt test oracle requires exactly five pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("prebuilt tests source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn required<'py>(globals: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    globals
        .get_item(name)?
        .ok_or_else(|| PyValueError::new_err("missing original prebuilt definition"))
}

#[pyclass]
struct Temporary {
    name: String,
    exited: Arc<AtomicBool>,
}
#[pymethods]
impl Temporary {
    fn __enter__(&self) -> String {
        self.name.clone()
    }
    fn __exit__(
        &self,
        _kind: &Bound<'_, PyAny>,
        _error: &Bound<'_, PyAny>,
        _trace: &Bound<'_, PyAny>,
    ) {
        // A failing original assertion retains its files instead of erasing evidence.
        self.exited.store(true, Ordering::SeqCst);
    }
}
#[pyclass]
struct Directories {
    parent: PathBuf,
    paths: Mutex<Vec<(PathBuf, Arc<AtomicBool>)>>,
}
#[pymethods]
impl Directories {
    #[pyo3(signature = (*, dir))]
    fn temporary_directory(&self, py: Python<'_>, dir: PathBuf) -> PyResult<Py<Temporary>> {
        let dir = crate::tooling_environment::physical_root(&dir).map_err(PyValueError::new_err)?;
        if dir != self.parent {
            return Err(PyValueError::new_err(
                "unexpected original prebuilt temporary root",
            ));
        }
        let options = PyDict::new(py);
        options.set_item("dir", &dir)?;
        options.set_item("prefix", "original-prebuilt-")?;
        let name: String = crate::source_oracle_runtime::host_import(py, "tempfile")?
            .call_method("mkdtemp", (), Some(&options))?
            .extract()?;
        let path = crate::tooling_environment::physical_root(Path::new(&name))
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(self.parent.as_path()) {
            return Err(PyValueError::new_err("original prebuilt temporary escaped"));
        }
        let exited = Arc::new(AtomicBool::new(false));
        self.paths
            .lock()
            .map_err(|_| PyRuntimeError::new_err("prebuilt temporary accounting poisoned"))?
            .push((path, exited.clone()));
        Py::new(py, Temporary { name, exited })
    }
}

#[pyclass]
struct Imports {
    installer: Py<PyModule>,
}
#[pymethods]
impl Imports {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        _globals: &Bound<'_, PyAny>,
        _locals: &Bound<'_, PyAny>,
        fromlist: &Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<Bound<'py, PyModule>> {
        if level == 0
            && name == "scripts"
            && fromlist.extract::<Vec<String>>()? == ["install_prebuilt"]
        {
            let scripts = PyModule::new(py, "_original_prebuilt_scripts")?;
            scripts.add("install_prebuilt", self.installer.bind(py))?;
            return Ok(scripts);
        }
        Err(PyImportError::new_err(
            "unexpected import in original prebuilt assertion",
        ))
    }
}

#[cfg(windows)]
#[pyclass]
struct Junction {
    workspace: PathBuf,
}
#[cfg(windows)]
#[pymethods]
impl Junction {
    #[pyo3(signature = (args, *, check, capture_output, timeout))]
    fn run(
        &self,
        py: Python<'_>,
        args: Vec<String>,
        check: bool,
        capture_output: bool,
        timeout: u64,
    ) -> PyResult<()> {
        if args.len() != 6
            || args[..4] != ["cmd.exe", "/c", "mklink", "/J"]
            || !check
            || !capture_output
            || timeout != 5
        {
            return Err(PyValueError::new_err(
                "unexpected original prebuilt subprocess",
            ));
        }
        let destination = Path::new(&args[4]);
        let target = Path::new(&args[5]);
        crate::tooling_environment::reject_links(destination, "linked junction destination")
            .map_err(PyValueError::new_err)?;
        let parent = crate::tooling_environment::physical_root(
            destination
                .parent()
                .ok_or_else(|| PyValueError::new_err("junction destination has no parent"))?,
        )
        .map_err(PyValueError::new_err)?;
        let physical =
            crate::tooling_environment::physical_root(target).map_err(PyValueError::new_err)?;
        if !parent.starts_with(&self.workspace)
            || !physical.starts_with(&self.workspace)
            || destination.file_name().and_then(|name| name.to_str()) != Some(".venv")
            || std::fs::symlink_metadata(destination).is_ok()
        {
            return Err(PyValueError::new_err(
                "original junction escaped its workspace",
            ));
        }
        let log = crate::logging::RunLog::open(&parent, "original-junction", vec![]);
        let command = crate::codec_process::executable("cmd").map_err(PyRuntimeError::new_err)?;
        let environment =
            crate::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
        let mut argv = vec![command.into_os_string(), "/d".into()];
        argv.extend(args[1..].iter().map(std::ffi::OsString::from));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| PyRuntimeError::new_err("cannot initialize original junction owner"))?;
        let result = py
            .detach(|| {
                runtime.block_on(crate::tooling_process::capture(
                    &parent,
                    &argv,
                    &environment,
                    std::time::Duration::from_secs(5),
                    std::time::Duration::from_secs(3),
                    &log,
                ))
            })
            .map_err(PyRuntimeError::new_err)?;
        if result.code != 0 {
            return Err(PyRuntimeError::new_err("original junction command failed"));
        }
        Ok(())
    }
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let path = py.import("pathlib")?.getattr("Path")?;
    let os = py.import("os")?;
    let bindings = PyDict::new(py);
    bindings.set_item("os", &os)?;
    bindings.set_item("Path", &path)?;
    bindings.set_item("stat", py.import("stat")?)?;
    let environment = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["_linked", "confined_directory", "environment"],
        &["bool", "getattr", "ValueError", "dict", "str"],
        &bindings,
    )?;
    let mock = py.import("unittest.mock")?;
    let forbidden = PyDict::new(py);
    forbidden.set_item(
        "side_effect",
        PyRuntimeError::new_err("original installer crossed its refusal boundary").value(py),
    )?;
    let stop = mock.getattr("Mock")?.call((), Some(&forbidden))?;
    let module = PyModule::new(py, "_original_prebuilt_install")?;
    let original = PyDict::new(py);
    original.set_item("ROOT", path.call1((workspace,))?)?;
    for name in ["verify", "local_environment", "setup_logging"] {
        original.set_item(name, &stop)?;
    }
    let functions = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &["install"],
        &["ValueError"],
        &original,
    )?;
    module.dict().update(functions.as_mapping())?;
    let function = required(&functions, "install")?;
    let rebound = py.import("types")?.getattr("FunctionType")?.call1((
        function.getattr("__code__")?,
        module.dict(),
        function.getattr("__name__")?,
        function.getattr("__defaults__")?,
        function.getattr("__closure__")?,
    ))?;
    module.add("install", rebound)?;
    let logs = workspace.join("logs");
    std::fs::create_dir(&logs)?;
    let directories = Py::new(
        py,
        Directories {
            parent: logs,
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let temporary = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    temporary.setattr(
        "TemporaryDirectory",
        directories.bind(py).getattr("temporary_directory")?,
    )?;
    let tests = PyDict::new(py);
    tests.set_item("Path", &path)?;
    tests.set_item("ROOT", path.call1((workspace,))?)?;
    tests.set_item("os", &os)?;
    tests.set_item("tempfile", temporary)?;
    let unittest = py.import("unittest")?;
    tests.set_item("unittest", &unittest)?;
    tests.set_item("patch", mock.getattr("patch")?)?;
    for name in ["confined_directory", "environment"] {
        tests.set_item(name, required(&environment, name)?)?;
    }
    #[cfg(windows)]
    {
        let subprocess = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
        let junction = Py::new(
            py,
            Junction {
                workspace: workspace.into(),
            },
        )?;
        subprocess.setattr("run", junction.bind(py).getattr("run")?)?;
        tests.set_item("subprocess", subprocess)?;
    }
    let classes = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["PrebuiltEnvironment"],
        &["__build_class__", "str", "sorted", "list", "ValueError"],
        &tests,
    )?;
    let imports = Py::new(
        py,
        Imports {
            installer: module.clone().unbind(),
        },
    )?;
    classes
        .get_item("__builtins__")?
        .ok_or_else(|| PyValueError::new_err("missing prebuilt builtins"))?
        .set_item("__import__", imports)?;
    let class = required(&classes, "PrebuiltEnvironment")?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let actual: Vec<String> = loader
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    let mut expected = METHODS.to_vec();
    expected.sort_unstable();
    if actual != expected {
        return Err(PyValueError::new_err("original prebuilt collection drift"));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    for name in METHODS {
        class.call1((name,))?.call_method1("run", (&result,))?;
        for field in [
            "errors",
            "failures",
            "skipped",
            "expectedFailures",
            "unexpectedSuccesses",
        ] {
            if result.getattr(field)?.len()? != 0 {
                return Err(PyRuntimeError::new_err(format!(
                    "original prebuilt {name} reported {field}: {}",
                    crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                )));
            }
        }
    }
    let process_case = build_job_tests::run(py, [sources[3], sources[4]], &result)?;
    stop.call_method0("assert_not_called")?;
    if result.getattr("testsRun")?.extract::<usize>()? != 4
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original prebuilt execution incomplete",
        ));
    }
    let state = directories.bind(py).borrow();
    let tracked = state
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("prebuilt temporary accounting poisoned"))?;
    if tracked.len() != 3
        || tracked
            .iter()
            .any(|(_, exited)| !exited.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original prebuilt context cleanup drift",
        ));
    }
    for (directory, _) in tracked.iter() {
        if directory.parent() != Some(state.parent.as_path()) {
            return Err(PyValueError::new_err("foreign prebuilt cleanup"));
        }
        validate_tree(directory, 0).map_err(PyValueError::new_err)?;
    }
    let expected = tracked
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let actual = std::fs::read_dir(&state.parent)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
    if actual != expected {
        return Err(PyValueError::new_err(
            "unexpected original prebuilt scratch; retained",
        ));
    }
    for (directory, _) in tracked.iter() {
        std::fs::remove_dir_all(directory)?;
    }
    std::fs::remove_dir(&state.parent)?;
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "cases":METHODS.map(|name|format!("PrebuiltEnvironment.{name}")),"process_case":process_case,"tests_run":4,
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"unchanged original confinement/environment and all three prebuilt assertion methods plus unchanged partial member-inventory fault against actual original Job.finish; no real WinDLL/Job call in that fault; fixed importer/private install globals, original mocked verification and refusal-only install, project-contained delayed temporary cleanup; Windows junction uses existing native child owner; no successful install/signing or native parity claim"}),
    )
}

fn validate_tree(path: &Path, depth: usize) -> Result<(), String> {
    if depth > 16 {
        return Err("original prebuilt scratch exceeds cleanup depth".into());
    }
    crate::tooling_environment::reject_links(path, "linked original prebuilt evidence")?;
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "cannot inspect prebuilt evidence")?;
    if metadata.is_file() {
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err("unsupported original prebuilt scratch member".into());
    }
    if crate::process_owner::cleanup_pending(path) {
        return Err("original prebuilt child ownership unresolved".into());
    }
    for (index, entry) in std::fs::read_dir(path)
        .map_err(|_| "cannot inspect prebuilt scratch")?
        .enumerate()
    {
        if index >= 128 {
            return Err("original prebuilt scratch exceeds cleanup count".into());
        }
        validate_tree(
            &entry
                .map_err(|_| "cannot inspect prebuilt scratch member")?
                .path(),
            depth + 1,
        )?;
    }
    Ok(())
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("prebuilt test workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect prebuilt workspace")?
        .next()
        .is_some()
    {
        return Err("prebuilt test workspace must be empty".into());
    }
    Python::initialize();
    let report = Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect historical bytecode policy")?
        {
            return Err("original prebuilt tests require disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original prebuilt replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })?;
    if std::fs::read_dir(workspace)
        .map_err(|_| "cannot verify prebuilt cleanup")?
        .next()
        .is_some()
    {
        return Err("original prebuilt left scratch; retained".into());
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_prebuilt_sources_refuse_before_workspace_or_runtime_access() {
        let sources = [
            (
                "scripts/install_prebuilt.py",
                "e47fbac5e60d5f8c6f0468ba7e69fd70483d18aa058ea660d52b2e68dee8dc53",
            ),
            (
                "scripts/local_environment.py",
                "b568feffc0c75e27b7f08cb69aaf546be4ce544427234379316b8f708b619766",
            ),
            (
                "tests/test_prebuilt_environment.py",
                "325ea859959d4fd5b86036cd2fac7e9efd38638acdad0129f794813af0f90c6c",
            ),
            (
                "scripts/build_job.py",
                "165c15916bfd3e7f8d4b4229db70f76a15ef4468b53e201c95f2b1106ff4d7a7",
            ),
            (
                "tests/test_build_process_owner.py",
                "c773d3c5bec0573f726e1ecd4ebb6595a0d2d4d56270a69e609d6826a0e6625d",
            ),
        ]
        .map(|(path, hash)| Source {
            baseline_revision: BASELINE.into(),
            path: path.into(),
            sha256: hash.into(),
            text: "raise RuntimeError('untrusted source must never execute')".into(),
        });
        let refs = sources.iter().collect::<Vec<_>>();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let missing = root.join(format!("logs/tmp/prebuilt refusal {}", std::process::id()));
        assert!(!missing.exists());
        assert!(
            source_identity(&refs)
                .unwrap_err()
                .contains("source identity drift")
        );
        assert!(
            observe(root, &missing, &refs)
                .unwrap_err()
                .contains("source identity drift")
        );
        assert!(!missing.exists());
        for count in 0..sources.len() {
            assert!(source_identity(&refs[..count]).is_err());
        }
    }
}
