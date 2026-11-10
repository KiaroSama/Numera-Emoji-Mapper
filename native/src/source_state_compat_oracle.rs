//! Replay the admitted original state compatibility test against real native and reference locks.
use crate::source_git_inventory::Source;
use pyo3::exceptions::{PyImportError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const CASE: &str = "NativeStateCompatibility.test_exported_state_lease_callback_and_atomic_bytes";
const IDENTITIES: [(&str, &str); 6] = [
    (
        "emojikit/packstate.py",
        "a498ad8bcbc621f3184a95e77a9d98ba79ac39b2c73177c36eec91aaa5a5169a",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
    (
        "tests/test_native_state_compat.py",
        "f656a4216951aa4da735c8550e5448fc31383bd7d1d03b8289fc6e11c8b65b2e",
    ),
    (
        "tests/test_pack_locks.py",
        "91b84ceda3c046d3c084454f327028abf4be1f65c943a5965c096e98674f2c7d",
    ),
    (
        "tests/test_resume_safety.py",
        "0af175ef5368ed7a6613ce487f65ff5b8f42e1751fcfa9e787b17faea956b3d4",
    ),
    (
        "tests/test_native_storage_recovery.py",
        "80afa262b5b2d0c9ed2e8dd2309c6cac3f2ec7af0bc3eb443205f9bd681d80f4",
    ),
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("state compatibility oracle requires exactly six pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        let actual = Sha256::digest(source.text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || actual != hash
        {
            return Err(
                "state compatibility oracle source identity drift; refusing execution".into(),
            );
        }
    }
    Ok(())
}

#[pyclass]
struct Imports {
    package: Py<PyModule>,
    datetime: Py<PyModule>,
    native: AtomicUsize,
    utc: AtomicUsize,
    liveness: AtomicUsize,
}
#[pymethods]
impl Imports {
    fn __call__<'py>(
        &self,
        name: &str,
        globals: &Bound<'_, PyAny>,
        locals: &Bound<'_, PyAny>,
        fromlist: &Bound<'py, PyAny>,
        level: i32,
    ) -> PyResult<Bound<'py, PyModule>> {
        let _ = (globals, locals);
        let py = fromlist.py();
        if level == 0 && name == "time" {
            // datetime.strftime imports time through the calling frame's
            // import hook, even though the original wrapper imports datetime.
            return crate::source_oracle_runtime::host_import(py, "time");
        }
        let names = fromlist.extract::<Vec<String>>()?;
        if level == 0 && name == "emojikit" && names == ["_native"] {
            self.native.fetch_add(1, Ordering::Relaxed);
            return Ok(self.package.bind(py).clone());
        }
        if level == 0 && name == "datetime" && names == ["datetime", "timezone"] {
            self.utc.fetch_add(1, Ordering::Relaxed);
            return Ok(self.datetime.bind(py).clone());
        }
        Err(PyImportError::new_err(
            "unexpected import in original state wrapper",
        ))
    }

    fn own_pid_alive(&self, pid: u32) -> PyResult<bool> {
        if pid != std::process::id() {
            return Err(PyValueError::new_err(
                "unexpected diagnostic PID in state replay",
            ));
        }
        self.liveness.fetch_add(1, Ordering::Relaxed);
        Ok(true)
    }
}

fn required<'py>(globals: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    globals
        .get_item(name)?
        .ok_or_else(|| PyValueError::new_err("missing admitted state definition"))
}

fn observe_shapes(py: Python<'_>, test: &Source, reference: &Bound<'_, PyDict>) -> PyResult<Value> {
    let function = required(reference, "validate_state_shape")?;
    let invalid = required(reference, "StateInvalid")?;
    let options = PyDict::new(py);
    options.set_item("wraps", &function)?;
    let spy = py
        .import("unittest.mock")?
        .getattr("Mock")?
        .call((), Some(&options))?;
    let api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    api.setattr("validate_state_shape", &spy)?;
    api.setattr("StateInvalid", &invalid)?;
    let bindings = PyDict::new(py);
    let unittest = py.import("unittest")?;
    bindings.set_item("unittest", &unittest)?;
    bindings.set_item("ps", api)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &["StateShapeValidation"],
        &["__build_class__"],
        &bindings,
    )?;
    let class = required(&tests, "StateShapeValidation")?;
    let methods = [
        ("test_a_sound_state_passes", 1),
        ("test_wrong_base_is_rejected", 1),
        ("test_negative_and_oversized_counts_are_rejected", 2),
        ("test_duplicate_and_backwards_indexes_are_rejected", 2),
        ("test_malformed_intent_is_rejected", 3),
        ("test_done_must_hold_strings", 1),
    ];
    let collected = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract::<Vec<String>>()?;
    let mut expected = methods.map(|(name, _)| name).to_vec();
    expected.sort_unstable();
    if collected != expected {
        return Err(PyValueError::new_err(
            "original state-shape collection drift",
        ));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    let json_module = py.import("json")?;
    let mut cases = Vec::new();
    let mut observations = Vec::new();
    for (method, count) in methods {
        spy.call_method0("reset_mock")?;
        class.call1((method,))?.call_method1("run", (&result,))?;
        let calls = spy.getattr("call_args_list")?;
        if calls.len()? != count {
            return Err(PyValueError::new_err(
                "original state-shape validation-call drift",
            ));
        }
        let case = format!("StateShapeValidation.{method}");
        cases.push(case.clone());
        for call in calls.try_iter()? {
            let call = call?;
            let args = call.getattr("args")?;
            let kwargs = call.getattr("kwargs")?.cast_into::<PyDict>()?;
            if args.len()? != 1 || kwargs.len() != 2 {
                return Err(PyValueError::new_err("original state-shape argument drift"));
            }
            let state = args.get_item(0)?;
            let before: String = json_module.call_method1("dumps", (&state,))?.extract()?;
            let base: String = required(&kwargs, "base")?.extract()?;
            let per_set: i64 = required(&kwargs, "per_set")?.extract()?;
            let validation = function.call((&state,), Some(&kwargs));
            let (accepted, error) = match validation {
                Ok(value) if value.is_none() => (true, None),
                Err(error) if error.is_instance(py, &invalid) => {
                    (false, Some(error.value(py).str()?.extract::<String>()?))
                }
                _ => {
                    return Err(PyValueError::new_err(
                        "unexpected original state-shape outcome",
                    ));
                }
            };
            let after: String = json_module.call_method1("dumps", (&state,))?.extract()?;
            if before != after {
                return Err(PyValueError::new_err(
                    "original state validator mutated its input",
                ));
            }
            let state: Value = serde_json::from_str(&before)
                .map_err(|_| PyValueError::new_err("invalid original state-shape JSON"))?;
            observations.push(json!({"case":case,"state":state,"base":base,
                "per_set":per_set,"accepted":accepted,"error":error}));
        }
    }
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyValueError::new_err(format!(
                "original state-shape reported {field}"
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != methods.len()
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || observations.len() != 10
    {
        return Err(PyValueError::new_err(
            "original state-shape execution incomplete",
        ));
    }
    Ok(
        json!({"tests_run":methods.len(),"cases":cases,"observations":observations,
        "ports":"unchanged original class/helper; real Mock(wraps) captures arguments without replacing validation; native PyValueError/text adaptation remains distinct from original StateInvalid"}),
    )
}

fn observe_atomic(
    py: Python<'_>,
    workspace: &Path,
    test: &Source,
    reference: &Bound<'_, PyDict>,
) -> PyResult<Value> {
    let mock = py.import("unittest.mock")?.getattr("Mock")?;
    let options = PyDict::new(py);
    options.set_item("wraps", required(reference, "write_json_atomic")?)?;
    let writer = mock.call((), Some(&options))?;
    let api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    api.setattr("write_json_atomic", &writer)?;
    let options = PyDict::new(py);
    options.set_item("dir", workspace)?;
    let temporary = py.import("functools")?.getattr("partial")?.call(
        (py.import("tempfile")?.getattr("TemporaryDirectory")?,),
        Some(&options),
    )?;
    let options = PyDict::new(py);
    options.set_item("wraps", temporary)?;
    let temporary = mock.call((), Some(&options))?;
    let tempfile = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    tempfile.setattr("TemporaryDirectory", &temporary)?;
    let bindings = PyDict::new(py);
    let unittest = py.import("unittest")?;
    bindings.set_item("unittest", &unittest)?;
    bindings.set_item("json", py.import("json")?)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("tempfile", tempfile)?;
    bindings.set_item("ps", api)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &["AtomicJsonWrite"],
        &["__build_class__", "list", "TypeError"],
        &bindings,
    )?;
    let class = required(&tests, "AtomicJsonWrite")?;
    let methods = [
        ("test_writes_and_leaves_no_temp_file", 1),
        ("test_previous_content_survives_a_failed_write", 2),
        ("test_overwrites_in_place", 2),
    ];
    let collected = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract::<Vec<String>>()?;
    let mut expected = methods.map(|(name, _)| name).to_vec();
    expected.sort_unstable();
    if collected != expected {
        return Err(PyValueError::new_err(
            "original atomic JSON collection drift",
        ));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    for (method, calls) in methods {
        writer.call_method0("reset_mock")?;
        class.call1((method,))?.call_method1("run", (&result,))?;
        if writer.getattr("call_count")?.extract::<usize>()? != calls {
            return Err(PyValueError::new_err(
                "original atomic JSON writer-call drift",
            ));
        }
        cases.push(format!("AtomicJsonWrite.{method}"));
    }
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyValueError::new_err(format!(
                "original atomic JSON reported {field}"
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != methods.len()
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || temporary.getattr("call_count")?.extract::<usize>()? != 3
    {
        return Err(PyValueError::new_err(
            "original atomic JSON execution incomplete",
        ));
    }
    Ok(
        json!({"tests_run":methods.len(),"cases":cases,"writer_calls":5,"temporary_calls":3,
        "ports":"unchanged original class/writer and actual filesystem/serialization; only TemporaryDirectory defaults confined to caller workspace; native Serialize-error type and pre-staging serialization remain adaptations"}),
    )
}

fn storage(
    py: Python<'_>,
    source: &Source,
    project: &Path,
    reference: &Bound<'_, PyDict>,
    imports: &Bound<'_, Imports>,
) -> PyResult<Value> {
    let bindings = PyDict::new(py);
    for module in ["json", "tempfile", "unittest"] {
        bindings.set_item(module, py.import(module)?)?;
    }
    let pathlib = py.import("pathlib")?;
    bindings.set_item("Path", pathlib.getattr("Path")?)?;
    bindings.set_item("ROOT", pathlib.getattr("Path")?.call1((project,))?)?;
    for name in ["exclusive_lock", "LockBusy"] {
        bindings.set_item(name, required(reference, name)?)?;
    }
    let tests = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["NativeStorage"],
        &["__build_class__", "RuntimeError", "str", "list"],
        &bindings,
    )?;
    tests
        .get_item("__builtins__")?
        .ok_or_else(|| PyValueError::new_err("missing storage test builtins"))?
        .cast_into::<PyDict>()?
        .set_item("__import__", imports)?;
    let unittest = py.import("unittest")?;
    let suite = unittest.getattr("TestLoader")?.call0()?.call_method1(
        "loadTestsFromTestCase",
        (required(&tests, "NativeStorage")?,),
    )?;
    let cases = suite
        .try_iter()?
        .map(|test| {
            let method: String = test?.getattr("_testMethodName")?.extract()?;
            Ok(format!("NativeStorage.{method}"))
        })
        .collect::<PyResult<Vec<_>>>()?;
    if cases
        != [
            "NativeStorage.test_atomic_utf8_replaces_and_leaves_no_sibling",
            "NativeStorage.test_legacy_and_native_exclude_each_other_and_release",
        ]
    {
        return Err(PyValueError::new_err("original storage collection drift"));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    suite.call_method1("run", (&result,))?;
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyRuntimeError::new_err(format!(
                "original registered storage reported {field}"
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != cases.len()
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyRuntimeError::new_err(
            "original storage execution incomplete",
        ));
    }
    Ok(
        json!({"cases":cases,"tests_run":2,"errors":0,"failures":0,"skipped":0,
        "ports":"registered NativeLease/write_state_json; original reference exclusive_lock/LockBusy; private emojikit import and project-contained ROOT"}),
    )
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let project = workspace.join("project");
    std::fs::create_dir(&project)?;
    std::fs::create_dir(project.join("logs"))?;
    let native = PyModule::new(py, "_native_state_source_test")?;
    crate::_native(&native)?;
    let package = PyModule::new(py, "_state_source_package")?;
    package.add("_native", &native)?;
    let datetime = py.import("datetime")?;
    let imports = Py::new(
        py,
        Imports {
            package: package.unbind(),
            datetime: datetime.clone().unbind(),
            native: AtomicUsize::new(0),
            utc: AtomicUsize::new(0),
            liveness: AtomicUsize::new(0),
        },
    )?;
    let imports = imports.bind(py);
    let bindings = PyDict::new(py);
    let pathlib = py.import("pathlib")?;
    for module in [
        "contextlib",
        "json",
        "os",
        "re",
        "time",
        "tempfile",
        "stat",
        "secrets",
    ] {
        bindings.set_item(module, py.import(module)?)?;
    }
    bindings.set_item("Path", pathlib.getattr("Path")?)?;
    bindings.set_item("datetime", datetime.getattr("datetime")?)?;
    bindings.set_item("timezone", datetime.getattr("timezone")?)?;
    bindings.set_item("LOCK_STALE_AFTER", 120)?;
    bindings.set_item("_LOCK_BYTE", 4096)?;
    bindings.set_item("_RECORD_WIDTH", 512)?;
    bindings.set_item("_REPLACE_DEADLINE", 1.0)?;
    bindings.set_item(
        "LOCK_DIR",
        pathlib.getattr("Path")?.call1((project.join(".locks"),))?,
    )?;
    bindings.set_item("_lock_owner_is_alive", imports.getattr("own_pid_alive")?)?;
    let platform: String = py.import("sys")?.getattr("platform")?.extract()?;
    let lock_module = if platform == "win32" {
        "msvcrt"
    } else {
        "fcntl"
    };
    bindings.set_item(lock_module, py.import(lock_module)?)?;
    let builtins = [
        "__build_class__",
        "RuntimeError",
        "BaseException",
        "OSError",
        "TypeError",
        "ValueError",
        "PermissionError",
        "FileNotFoundError",
        "str",
        "dict",
        "list",
        "int",
        "isinstance",
        "all",
        "set",
        "enumerate",
        "min",
    ];
    let reference = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &[
            "StateInvalid",
            "LockBusy",
            "validate_state_shape",
            "make_intent",
            "pack_family_lock_path",
            "canonical_map_lock",
            "exclusive_lock",
            "_take_os_lock",
            "_drop_os_lock",
            "_busy_message",
            "_utc_now",
            "_replace",
            "write_json_atomic",
        ],
        &builtins,
        &bindings,
    )?;
    let wrapper = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &[
            "StateInvalid",
            "LockBusy",
            "validate_state_shape",
            "make_intent",
            "pack_family_lock_path",
            "canonical_map_lock",
            "exclusive_lock",
            "_utc_now",
            "write_json_atomic",
        ],
        &builtins,
        &bindings,
    )?;
    wrapper
        .get_item("__builtins__")?
        .ok_or_else(|| PyValueError::new_err("missing wrapper builtins"))?
        .cast_into::<PyDict>()?
        .set_item("__import__", imports)?;
    let as_module = |name: &str, globals: &Bound<'_, PyDict>| -> PyResult<Py<PyModule>> {
        let module = PyModule::new(py, name)?;
        module.dict().update(globals.as_mapping())?;
        Ok(module.unbind())
    };
    let tests = PyDict::new(py);
    for module in ["json", "os", "tempfile", "unittest"] {
        tests.set_item(module, py.import(module)?)?;
    }
    tests.set_item("Path", pathlib.getattr("Path")?)?;
    tests.set_item("ROOT", pathlib.getattr("Path")?.call1((&project,))?)?;
    tests.set_item("packstate", as_module("_original_state_wrapper", &wrapper)?)?;
    tests.set_item(
        "source",
        as_module("_original_state_reference", &reference)?,
    )?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["NativeStateCompatibility"],
        &[
            "__build_class__",
            "callable",
            "float",
            "list",
            "object",
            "LookupError",
            "OSError",
            "TypeError",
        ],
        &tests,
    )?;
    let unittest = py.import("unittest")?;
    let suite = unittest.getattr("TestLoader")?.call0()?.call_method1(
        "loadTestsFromTestCase",
        (required(&tests, "NativeStateCompatibility")?,),
    )?;
    let cases = suite
        .try_iter()?
        .map(|test| {
            let method: String = test?.getattr("_testMethodName")?.extract()?;
            Ok(format!("NativeStateCompatibility.{method}"))
        })
        .collect::<PyResult<Vec<_>>>()?;
    if cases != [CASE] {
        return Err(PyValueError::new_err(
            "original state compatibility collection drift",
        ));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    suite.call_method1("run", (&result,))?;
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyRuntimeError::new_err(format!(
                "original state compatibility reported {field}: {}; scratch retained",
                crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 1
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyRuntimeError::new_err(
            "original state compatibility execution incomplete",
        ));
    }
    let counts = imports.borrow();
    let native_calls = counts.native.load(Ordering::Relaxed);
    let utc_calls = counts.utc.load(Ordering::Relaxed);
    let liveness_calls = counts.liveness.load(Ordering::Relaxed);
    if native_calls != 14 || utc_calls != 5 || liveness_calls != 1 {
        return Err(PyRuntimeError::new_err(format!(
            "state compatibility port accounting differs: native={native_calls} utc={utc_calls} liveness={liveness_calls}"
        )));
    }
    drop(counts);
    let storage = storage(py, sources[5], &project, &reference, imports)?;
    let counts = imports.borrow();
    let storage_calls = json!({
        "native_import":counts.native.load(Ordering::Relaxed) - native_calls,
        "utc_import":counts.utc.load(Ordering::Relaxed) - utc_calls,
        "own_pid_liveness":counts.liveness.load(Ordering::Relaxed) - liveness_calls,
    });
    if storage_calls != json!({"native_import":2,"utc_import":0,"own_pid_liveness":1}) {
        return Err(PyRuntimeError::new_err(
            "original storage boundary accounting differs",
        ));
    }
    let diagnostics = crate::source_lock_diagnostics::observe(py, sources[1], sources[3])?;
    let shapes = observe_shapes(py, sources[4], &reference)?;
    let atomic = observe_atomic(py, workspace, sources[4], &reference)?;
    std::fs::remove_dir(project.join("logs"))?;
    std::fs::remove_dir(project)?;
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","source_platform":platform,
        "diagnostics":diagnostics,"shapes":shapes,"atomic":atomic,
        "storage":storage,"storage_boundary_calls":storage_calls,
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "cases":cases,"tests_run":1,"errors":0,"failures":0,"skipped":0,
        "boundary_calls":{"native_import":native_calls,"utc_import":utc_calls,"own_pid_liveness":liveness_calls},
        "coverage":{"wrapper":"exact original callback/finally, serialization and exception assertions",
        "locks":"real native and original fcntl/msvcrt exclusion in both directions",
        "liveness":"own worker PID diagnostic port only; no original process probe"}}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root || !workspace.starts_with(&root) {
        return Err("state compatibility requires its project-contained workspace".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect state compatibility workspace")?
        .next()
        .is_some()
    {
        return Err("state compatibility workspace must be empty".into());
    }
    Python::initialize();
    let report = Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original state compatibility failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })?;
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot verify state compatibility cleanup")?
        .next()
        .is_some()
    {
        return Err("original state compatibility left scratch; retained for diagnosis".into());
    }
    Ok(report)
}
