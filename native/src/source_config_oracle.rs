//! Pinned original dotenv and numeric assertions; synthetic files and a private environment.
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

#[path = "source_links_destination.rs"]
mod links_destination;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const METHOD: &str = "test_the_first_key_is_read_under_its_own_name";
const IDENTITIES: [(&str, &str); 4] = [
    (
        "emojikit/cli_env.py",
        "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
    ),
    (
        "tests/test_cli_env.py",
        "671c810ad43670f0fbe6cd1833b6b05b606c061a03f19899da20c1924555be99",
    ),
    (
        "tests/test_resume_safety.py",
        "0af175ef5368ed7a6613ce487f65ff5b8f42e1751fcfa9e787b17faea956b3d4",
    ),
    (
        "tests/reference/announce.py",
        "a10d73deec77e1de3a7b1e64b8c78dbde43bb5e5af031aee38d26fe7a85db4c8",
    ),
];

pub fn source_identity(
    source: &Source,
    test: &Source,
    numeric: &Source,
    announce: &Source,
) -> Result<(), String> {
    for (source, (path, hash)) in [source, test, numeric, announce]
        .into_iter()
        .zip(IDENTITIES)
    {
        let actual = Sha256::digest(source.text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || actual != hash
        {
            return Err("configuration oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

#[pyclass]
struct NumericEnvironment {
    values: Py<PyDict>,
    calls: std::sync::atomic::AtomicUsize,
}
#[pymethods]
impl NumericEnvironment {
    #[pyo3(signature = (target, values, clear=false))]
    fn dict<'py>(
        &self,
        py: Python<'py>,
        target: &str,
        values: &Bound<'py, PyDict>,
        clear: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        if target != "os.environ" {
            return Err(PyValueError::new_err(
                "unexpected original configuration patch target",
            ));
        }
        let options = PyDict::new(py);
        options.set_item("clear", clear)?;
        let manager = crate::source_oracle_runtime::host_import(py, "unittest.mock")?
            .getattr("patch")?
            .call_method("dict", (self.values.bind(py), values), Some(&options))?;
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(manager)
    }
}

pub(crate) fn private_environment_patch<'py>(
    py: Python<'py>,
    values: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyAny>> {
    Ok(Py::new(
        py,
        NumericEnvironment {
            values: values.clone().unbind(),
            calls: std::sync::atomic::AtomicUsize::new(0),
        },
    )?
    .into_bound(py)
    .into_any())
}

fn numeric_observations(py: Python<'_>, source: &Source, test: &Source) -> PyResult<Value> {
    let namespace = py.import("types")?.getattr("SimpleNamespace")?;
    let environment = PyDict::new(py);
    let os = namespace.call0()?;
    os.setattr("environ", &environment)?;
    let stderr = py.import("io")?.getattr("StringIO")?.call0()?;
    let sys = namespace.call0()?;
    sys.setattr("stderr", &stderr)?;
    let bindings = PyDict::new(py);
    bindings.set_item("os", os)?;
    bindings.set_item("sys", sys)?;
    let globals = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["safe_int_env"],
        &["int", "print", "ValueError"],
        &bindings,
    )?;
    let function = crate::source_logging_oracle::required(&globals, "safe_int_env")?;
    let api = namespace.call0()?;
    api.setattr("safe_int_env", &function)?;
    let patch = Py::new(
        py,
        NumericEnvironment {
            values: environment.clone().unbind(),
            calls: std::sync::atomic::AtomicUsize::new(0),
        },
    )?;
    let mock = namespace.call0()?;
    mock.setattr("patch", patch.bind(py))?;
    let unittest = py.import("unittest")?;
    let bindings = PyDict::new(py);
    bindings.set_item("unittest", &unittest)?;
    bindings.set_item("bp", api)?;
    bindings.set_item("mock", mock)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &["SafeConfigParsing"],
        &["__build_class__"],
        &bindings,
    )?;
    let methods = [
        "test_valid_value",
        "test_garbage_falls_back_instead_of_raising",
        "test_absent_and_blank_use_the_default",
        "test_values_are_clamped",
    ];
    let result = unittest.getattr("TestResult")?.call0()?;
    let class = crate::source_logging_oracle::required(&tests, "SafeConfigParsing")?;
    for method in methods {
        class.call1((method,))?.call_method1("run", (&result,))?;
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
                "original numeric configuration reported {field}"
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != methods.len()
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || patch
            .borrow(py)
            .calls
            .load(std::sync::atomic::Ordering::Relaxed)
            != 6
        || !environment.is_empty()
    {
        return Err(PyValueError::new_err(
            "original numeric configuration execution or private restoration drift",
        ));
    }
    let mut observations = Vec::new();
    let limit = "0".repeat(4300);
    let over_limit = "0".repeat(4301);
    let huge = "9".repeat(4300);
    for (raw, default, minimum, maximum) in [
        (None, 7, None, None),
        (Some("   "), 7, None, None),
        (Some("42"), 7, None, None),
        (Some("not-a-number"), 7, None, None),
        (Some("999"), 1, None, Some(64)),
        (Some("-5"), 1, Some(0), None),
        (Some("\u{1c}+۴_۲\u{1f}"), 7, Some(0), Some(64)),
        (Some("-۰۰۹"), 7, Some(0), Some(64)),
        (Some("9223372036854775808"), 7, None, Some(i64::MAX)),
        (Some("-9223372036854775809"), 7, Some(i64::MIN), None),
        (Some("-9223372036854775808"), 7, None, None),
        (Some("-۰"), 7, Some(0), Some(64)),
        (None, 7, Some(10), Some(64)),
        (Some("_1"), 7, None, None),
        (Some("1_"), 7, None, None),
        (Some("1__0"), 7, None, None),
        (Some("0x10"), 7, None, None),
        (Some("²"), 7, None, None),
        (Some("1\0"), 7, None, None),
        (Some("+"), 7, None, None),
        (Some("- 1"), 7, None, None),
        (Some(limit.as_str()), 7, None, None),
        (Some(over_limit.as_str()), 7, None, None),
        (Some(huge.as_str()), 7, None, Some(64)),
    ] {
        environment.clear();
        if let Some(raw) = raw {
            environment.set_item("X_TEST_NUM", raw)?;
        }
        stderr.call_method1("seek", (0,))?;
        stderr.call_method1("truncate", (0,))?;
        let options = PyDict::new(py);
        options.set_item("minimum", minimum)?;
        options.set_item("maximum", maximum)?;
        let value: String = function
            .call(("X_TEST_NUM", default), Some(&options))?
            .str()?
            .extract()?;
        let warning: String = stderr.call_method0("getvalue")?.extract()?;
        observations.push(
            json!({"raw":raw,"default":default,"minimum":minimum,"maximum":maximum,
            "value":value,"stderr":warning}),
        );
    }
    environment.clear();
    stderr.call_method0("close")?;
    Ok(
        json!({"test_source_hash":test.sha256,"tests_run":methods.len(),"patch_calls":6,
        "original_methods":methods.map(|method| format!("SafeConfigParsing.{method}")),
        "observations":observations,
        "ports":"only literal os.environ patch target redirected to a private dictionary; actual external patch.dict restoration and original assertions unchanged"}),
    )
}

fn exit_observations(py: Python<'_>, source: &Source, test: &Source) -> PyResult<Value> {
    let bindings = PyDict::new(py);
    let api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    let mut codes = BTreeMap::new();
    for name in ["EXIT_OK", "EXIT_USAGE", "EXIT_PARTIAL", "EXIT_FAILED"] {
        let value = crate::source_logging_oracle::literal(py, source, name, false)?;
        let code = value.extract::<u8>()?;
        bindings.set_item(name, &value)?;
        api.setattr(name, value)?;
        codes.insert(name, code);
    }
    let original =
        crate::source_oracle_runtime::namespace(py, source, &["ingest_exit_code"], &[], &bindings)?;
    let function = crate::source_logging_oracle::required(&original, "ingest_exit_code")?;
    api.setattr("ingest_exit_code", &function)?;
    let bindings = PyDict::new(py);
    let unittest = py.import("unittest")?;
    bindings.set_item("unittest", &unittest)?;
    bindings.set_item("bp", api)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &["ExitCodes"],
        &["__build_class__", "len"],
        &bindings,
    )?;
    let class = crate::source_logging_oracle::required(&tests, "ExitCodes")?;
    let methods = [
        "test_clean_run_is_zero",
        "test_partial_run_is_retryable",
        "test_total_failure_is_terminal",
        "test_empty_run_is_not_a_failure",
        "test_codes_are_distinct",
    ];
    let collected = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract::<Vec<String>>()?;
    let mut expected = methods.to_vec();
    expected.sort_unstable();
    if collected != expected {
        return Err(PyValueError::new_err("original exit-code collection drift"));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    for method in methods {
        class.call1((method,))?.call_method1("run", (&result,))?;
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
                "original exit-code assertion reported {field}"
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != methods.len()
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original exit-code assertions did not execute",
        ));
    }
    let mut observations = Vec::new();
    for (succeeded, failed) in [(5_u64, 0_u64), (3, 2), (0, 4), (0, 0)] {
        let code = function.call1((succeeded, failed))?.extract::<u8>()?;
        observations.push(json!({"succeeded":succeeded,"failed":failed,"code":code}));
    }
    Ok(
        json!({"tests_run":methods.len(),"original_methods":methods.map(|name|format!("ExitCodes.{name}")),
        "codes":codes,"observations":observations,
        "ports":"private bp namespace; original function/test bodies and literal constants unchanged; nonnegative batch-count boundary only"}),
    )
}

fn runner_observations(py: Python<'_>, source: &Source) -> PyResult<Value> {
    let namespace = py.import("types")?.getattr("SimpleNamespace")?;
    let mut observations = Vec::new();
    for (index, (entry, marker, expected)) in [
        (None, None, false),
        (None, Some("unittest"), true),
        (None, Some("pytest"), true),
        (Some("unittest.__main__"), None, true),
        (Some("pytest"), None, true),
        (Some("unittest_extra"), None, false),
        (Some("ordinary.unittest"), None, false),
    ]
    .into_iter()
    .enumerate()
    {
        let modules = PyDict::new(py);
        if index == 1 {
            modules.set_item("__main__", py.None())?;
        } else if index != 0 {
            let main = namespace.call0()?;
            let spec = match entry {
                Some(entry) => {
                    let spec = namespace.call0()?;
                    spec.setattr("name", entry)?;
                    spec
                }
                None => py.None().into_bound(py),
            };
            main.setattr("__spec__", spec)?;
            modules.set_item("__main__", main)?;
        }
        if let Some(marker) = marker {
            modules.set_item(marker, namespace.call0()?)?;
        }
        let before = modules.copy()?;
        let sys = namespace.call0()?;
        sys.setattr("modules", &modules)?;
        let bindings = PyDict::new(py);
        bindings.set_item("sys", sys)?;
        let globals = crate::source_oracle_runtime::namespace(
            py,
            source,
            &["_under_a_test_runner"],
            &["getattr"],
            &bindings,
        )?;
        let actual = crate::source_logging_oracle::required(&globals, "_under_a_test_runner")?
            .call0()?
            .extract::<bool>()?;
        if actual != expected || !modules.eq(&before)? {
            return Err(PyValueError::new_err(
                "original test-runner detection drift",
            ));
        }
        observations.push(json!({"entry":entry,"marker":marker,"result":actual}));
    }
    Ok(json!({"observations":observations,
        "ports":"unchanged original detector with private sys.modules/spec projections, not original subprocess imports; native runner uses explicit NO_DOTENV guard"}))
}

fn observe(
    py: Python<'_>,
    workspace: &Path,
    source: &Source,
    test: &Source,
    numeric: &Source,
    announce: &Source,
) -> PyResult<Value> {
    let globals = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["_parse_env_file"],
        &[],
        &PyDict::new(py),
    )?;
    let parser = globals
        .get_item("_parse_env_file")?
        .ok_or_else(|| PyValueError::new_err("original configuration parser is missing"))?;
    let namespace = py.import("types")?.getattr("SimpleNamespace")?;
    let api = namespace.call0()?;
    api.setattr("_parse_env_file", &parser)?;
    let temporary = namespace.call0()?;
    let options = PyDict::new(py);
    options.set_item("dir", workspace)?;
    temporary.setattr(
        "TemporaryDirectory",
        py.import("functools")?.getattr("partial")?.call(
            (py.import("tempfile")?.getattr("TemporaryDirectory")?,),
            Some(&options),
        )?,
    )?;
    let unittest = py.import("unittest")?;
    let path_type = py.import("pathlib")?.getattr("Path")?;
    let bindings = PyDict::new(py);
    bindings.set_item("cli_env", api)?;
    bindings.set_item("tempfile", temporary)?;
    bindings.set_item("Path", &path_type)?;
    bindings.set_item("unittest", &unittest)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &["ADotEnvWithAByteOrderMark"],
        &["__build_class__"],
        &bindings,
    )?;
    let result = unittest.getattr("TestResult")?.call0()?;
    tests
        .get_item("ADotEnvWithAByteOrderMark")?
        .ok_or_else(|| PyValueError::new_err("original configuration test is missing"))?
        .call1((METHOD,))?
        .call_method1("run", (&result,))?;
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyValueError::new_err(format!(
                "original configuration assertion reported {field}"
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 1
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original configuration assertion did not execute",
        ));
    }
    let mut observations = Vec::new();
    let path = workspace.join(".env");
    for (name, bytes) in [
        ("original-bom", "\u{feff}FIRST_KEY=one\n# note\nSECOND='two'\n".as_bytes()),
        ("two-bom-and-lines", "\u{feff}\u{feff}FIRST=one\rSECOND=two\u{85}THIRD='three'\u{2028}FOURTH=4\u{2029}FIFTH=5\nSECOND=ignored\n\u{1c}SIXTH = \"six\"\u{1f}\n".as_bytes()),
        ("quotes-and-first-duplicate", "KEY=\"first\"\r\nKEY=second\nEMPTY=\n# ignored=entry\nUNICODE=سلام\n".as_bytes()),
        ("invalid-utf8", b"FIRST=\xff".as_slice()),
    ] {
        std::fs::write(&path, bytes)?;
        let value = match parser.call1((path_type.call1((&path,))?,)) {
            Ok(value) => {
                let parsed: BTreeMap<String, String> = value.extract()?;
                json!({"values":parsed})
            }
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyUnicodeDecodeError>(py) => {
                json!({"error_kind":"UnicodeDecodeError"})
            }
            Err(error) => return Err(error),
        };
        observations.push(json!({"name":name,"bytes":bytes,"result":value}));
    }
    std::fs::remove_file(path)?;
    let links = links_destination::observe(py, announce, numeric)?;
    let exits = exit_observations(py, source, numeric)?;
    let numeric = numeric_observations(py, source, numeric)?;
    let runner = runner_observations(py, source)?;
    Ok(
        json!({"baseline_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hash":source.sha256,"test_source_hash":test.sha256,"tests_run":1,
        "original_method":format!("ADotEnvWithAByteOrderMark.{METHOD}"),"observations":observations,
        "numeric":numeric,"exits":exits,"links":links,"runner":runner,
        "ports":"external TemporaryDirectory confined to caller workspace; original file reads, parser and assertion unchanged; no process environment mutation"}),
    )
}

pub fn generate(
    root: &Path,
    workspace: &Path,
    source: &Source,
    test: &Source,
    numeric: &Source,
    announce: &Source,
) -> Result<Value, String> {
    source_identity(source, test, numeric, announce)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root || !workspace.starts_with(&root) {
        return Err("configuration replay requires its project-contained workspace".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect configuration workspace")?
        .next()
        .is_some()
    {
        return Err("configuration replay workspace must be empty".into());
    }
    Python::initialize();
    let report = Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        observe(py, &workspace, source, test, numeric, announce)
            .map_err(|_| "original configuration replay failed; workspace retained".to_owned())
    })?;
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot verify configuration cleanup")?
        .next()
        .is_some()
    {
        return Err("configuration replay left scratch; retained for diagnosis".into());
    }
    Ok(report)
}
