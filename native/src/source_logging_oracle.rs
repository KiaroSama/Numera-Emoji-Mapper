//! Fixed original redaction observations; no global logger, environment or operator secrets.
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PySet};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 2] = [
    (
        "emojikit/logsetup.py",
        "97c34490fde4b280ac005e0095d187ff1a38904a25521f5680930d915367eca6",
    ),
    (
        "tests/test_logsetup.py",
        "93a6bfb0cb7fea9679dfbab3014aaa1e9c273d59235c4c807e2e83a1cd8c4405",
    ),
];
const METHODS: [&str; 7] = [
    "test_token_shaped_is_masked",
    "test_bot_url_is_masked",
    "test_registered_secret_value_is_masked",
    "test_short_value_not_registered",
    "test_content_key_and_ids_preserved",
    "test_a_token_glued_to_bot_is_masked",
    "test_the_worker_secrets_are_masked_by_name",
];

pub fn source_identity(source: &Source, test: &Source) -> Result<(), String> {
    for (source, (path, hash)) in [source, test].into_iter().zip(IDENTITIES) {
        let actual = Sha256::digest(source.text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || actual != hash
        {
            return Err("logging oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub(crate) fn required<'py>(
    globals: &Bound<'py, PyDict>,
    name: &str,
) -> PyResult<Bound<'py, PyAny>> {
    globals
        .get_item(name)?
        .ok_or_else(|| PyValueError::new_err("missing original logging definition"))
}

fn integer_product(node: &Bound<'_, PyAny>, depth: usize) -> PyResult<u64> {
    if depth > 8 {
        return Err(PyValueError::new_err(
            "constant product is too deeply nested",
        ));
    }
    let value = match crate::source_test_inventory::kind(node)?.as_str() {
        "Constant" => {
            let value = node.getattr("value")?;
            if !value
                .get_type()
                .is(node.py().get_type::<pyo3::types::PyInt>())
            {
                return Err(PyValueError::new_err(
                    "constant product requires integer literals",
                ));
            }
            value.extract::<u64>()?
        }
        "BinOp" if crate::source_test_inventory::kind(&node.getattr("op")?)? == "Mult" => {
            integer_product(&node.getattr("left")?, depth + 1)?
                .checked_mul(integer_product(&node.getattr("right")?, depth + 1)?)
                .ok_or_else(|| PyValueError::new_err("constant product overflow"))?
        }
        _ => {
            return Err(PyValueError::new_err(
                "only literal integer multiplication is admitted",
            ));
        }
    };
    if value > 64 * 1024 * 1024 {
        return Err(PyValueError::new_err(
            "constant product exceeds its admission bound",
        ));
    }
    Ok(value)
}

// Only literal assignments, bounded integer products or the first re.compile literal are admitted.
pub(crate) fn literal<'py>(
    py: Python<'py>,
    source: &Source,
    name: &str,
    regex: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (&source.text, &source.path))?;
    let mut found = None;
    for node in crate::source_test_inventory::body(&tree)? {
        if crate::source_test_inventory::kind(&node)? != "Assign" {
            continue;
        }
        let targets = node.getattr("targets")?;
        if targets.len()? != 1 {
            continue;
        }
        let target = targets.get_item(0)?;
        if crate::source_test_inventory::kind(&target)? != "Name"
            || target.getattr("id")?.extract::<String>()? != name
        {
            continue;
        }
        if found.is_some() {
            return Err(PyValueError::new_err("duplicate original logging constant"));
        }
        let mut value = node.getattr("value")?;
        if regex {
            if crate::source_test_inventory::kind(&value)? != "Call" {
                return Err(PyValueError::new_err(
                    "original logging pattern is not a call",
                ));
            }
            let func = value.getattr("func")?;
            if crate::source_test_inventory::kind(&func)? != "Attribute"
                || func.getattr("attr")?.extract::<String>()? != "compile"
                || func.getattr("value")?.getattr("id")?.extract::<String>()? != "re"
                || value.getattr("args")?.len()? != 1
                || value.getattr("keywords")?.len()? != 0
            {
                return Err(PyValueError::new_err(
                    "original logging pattern assignment drift",
                ));
            }
            value = value.getattr("args")?.get_item(0)?;
        }
        found = Some(if crate::source_test_inventory::kind(&value)? == "BinOp" {
            pyo3::types::PyInt::new(py, integer_product(&value, 0)?).into_any()
        } else {
            ast.call_method1("literal_eval", (value,))?
        });
    }
    found.ok_or_else(|| PyValueError::new_err("missing original logging constant"))
}

fn execute(py: Python<'_>, source: &Source, test: &Source) -> PyResult<Value> {
    let secrets = PySet::empty(py)?;
    let bindings = PyDict::new(py);
    bindings.set_item("_SECRETS", &secrets)?;
    let regex = py.import("re")?;
    for name in ["_TOKEN_RE", "_BOT_URL_RE"] {
        bindings.set_item(
            name,
            regex.call_method1("compile", (literal(py, source, name, true)?,))?,
        )?;
    }
    let globals = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["register_secret", "redact"],
        &["len", "sorted"],
        &bindings,
    )?;
    let token: String = literal(py, test, "TOKEN", false)?.extract()?;
    let formatting = crate::source_logging_format::observe(py, source, test, &globals, &token)?;
    let api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    api.setattr("redact", required(&globals, "redact")?)?;
    api.setattr("register_secret", required(&globals, "register_secret")?)?;
    let secret_keys = literal(py, source, "SECRET_ENV_KEYS", false)?;
    api.setattr("SECRET_ENV_KEYS", &secret_keys)?;
    let secret_keys: Vec<String> = secret_keys.extract()?;
    let test_bindings = PyDict::new(py);
    let unittest = py.import("unittest")?;
    test_bindings.set_item("unittest", &unittest)?;
    test_bindings.set_item("L", api)?;
    test_bindings.set_item("TOKEN", &token)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &["TestRedaction"],
        &["__build_class__"],
        &test_bindings,
    )?;
    let class = required(&tests, "TestRedaction")?;
    let result = unittest.getattr("TestResult")?.call0()?;
    for method in METHODS {
        secrets.clear();
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
                "original redaction methods reported {field}"
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != METHODS.len()
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original redaction replay incomplete",
        ));
    }
    let suffix = "ABCDEFGHIJKLMNOPQRSTUVWXYZ012";
    let literal_secret = "synthetic-registered-log-value";
    let mut recipes: Vec<(String, String, Vec<String>, bool)> = vec![
        ("plain".into(), "normal é 😀".into(), vec![], true),
        (
            "original-token".into(),
            format!("using {token} now"),
            vec![],
            true,
        ),
        (
            "original-bot-url".into(),
            format!("GET /bot{token}/getMe"),
            vec![],
            true,
        ),
        (
            "original-registered-value".into(),
            format!("key={}", "0".repeat(32)),
            vec!["0".repeat(32)],
            true,
        ),
        (
            "original-unregistered-short".into(),
            "abc".into(),
            vec![],
            true,
        ),
        (
            "original-glued-bot".into(),
            format!("POST https://api.telegram.org/bot123456789:{suffix}"),
            vec![],
            true,
        ),
        (
            "standalone-bot".into(),
            format!("bot123456:{suffix}"),
            vec![],
            true,
        ),
        (
            "minimum-suffix".into(),
            format!("123456:{}", "A".repeat(20)),
            vec![],
            true,
        ),
        (
            "short-suffix".into(),
            format!("123456:{}", "A".repeat(19)),
            vec![],
            true,
        ),
        ("short-id".into(), format!("12345:{suffix}"), vec![], true),
        (
            "identity-text".into(),
            "key s:01f6c1284261975d0000934b7fe6f3c3 cid 111111111".into(),
            vec![],
            true,
        ),
        (
            "registered".into(),
            format!("x{literal_secret}"),
            vec![literal_secret.into()],
            true,
        ),
        (
            "longest-first".into(),
            "prefix-synthetic-long-secret-value suffix".into(),
            vec![
                "synthetic-long".into(),
                "synthetic-long-secret-value".into(),
            ],
            true,
        ),
    ];
    for digits in ["۱۲۳۴۵۶", "１２３４５６", "١٢٣٤٥٦"] {
        for prefix in ["", "bot", "/bot"] {
            recipes.push((
                format!("unicode-{}-{}", recipes.len(), prefix.len()),
                format!("{prefix}{digits}:{suffix}/"),
                vec![],
                true,
            ));
        }
    }
    for reversed in [false, true] {
        let mut registered = vec!["é".repeat(8), "éabcdefghi".into()];
        if reversed {
            registered.reverse();
        }
        recipes.push((
            format!("unicode-literal-order-{reversed}"),
            format!("{}abcdefghi", "é".repeat(8)),
            registered,
            true,
        ));
    }
    for prefix in ["x", "_", "-"] {
        recipes.push((
            format!("conservative-boundary-{prefix}"),
            format!("{prefix}123456:{suffix}"),
            vec![],
            false,
        ));
    }
    let mut observations = Vec::new();
    for (name, input, registered, parity) in recipes {
        secrets.clear();
        for value in &registered {
            required(&globals, "register_secret")?.call1((value,))?;
        }
        let output: String = required(&globals, "redact")?.call1((&input,))?.extract()?;
        observations.push(json!({"name":name,"input":input,"secrets":registered,
            "output":output,"parity":parity}));
    }
    Ok(
        json!({"baseline_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hash":source.sha256,"test_source_hash":test.sha256,
        "original_methods":METHODS.map(|method| format!("TestRedaction.{method}")),
        "tests_run":METHODS.len(),"observations":observations,"secret_keys":secret_keys,
        "formatting":formatting,
        "coverage":"six original pure redaction methods plus secret-key names, original formatter, recorded exit, counters, private summary and exception hooks; no global logger setup or original CLI execution",
        "known_difference":"native conservative adjacent-ASCII masking remains stronger than original lookbehind"}),
    )
}

const ENV_IDENTITY: (&str, &str) = (
    "emojikit/cli_env.py",
    "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
);

pub fn retention_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != 3 {
        return Err("logging retention requires exactly three pinned sources".into());
    }
    source_identity(sources[0], sources[1])?;
    let environment = sources[2];
    let actual = Sha256::digest(environment.text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if environment.baseline_revision != BASELINE
        || environment.path != ENV_IDENTITY.0
        || environment.sha256 != ENV_IDENTITY.1
        || actual != ENV_IDENTITY.1
    {
        return Err("logging retention source identity drift; refusing execution".into());
    }
    Ok(())
}

#[pyclass]
struct RetentionModule(Py<PyDict>);
#[pymethods]
impl RetentionModule {
    fn __getattr__<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        required(self.0.bind(py), name)
    }
    fn __setattr__(&self, py: Python<'_>, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        if name != "LOG_DIR" {
            return Err(PyValueError::new_err(
                "unexpected original retention assignment",
            ));
        }
        self.0.bind(py).set_item(name, value)
    }
}

#[pyclass]
struct RetentionTemporary {
    workspace: std::path::PathBuf,
    calls: std::sync::atomic::AtomicUsize,
}
#[pymethods]
impl RetentionTemporary {
    fn temporary_directory<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let options = PyDict::new(py);
        options.set_item("dir", &self.workspace)?;
        let temporary =
            py.import("tempfile")?
                .call_method("TemporaryDirectory", (), Some(&options))?;
        let name: std::path::PathBuf = temporary.getattr("name")?.extract()?;
        let physical =
            crate::tooling_environment::physical_root(&name).map_err(PyValueError::new_err)?;
        if physical == self.workspace || !physical.starts_with(&self.workspace) {
            return Err(PyValueError::new_err(
                "original retention temporary escaped",
            ));
        }
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(temporary)
    }
}

fn execute_retention(
    py: Python<'_>,
    workspace: &std::path::Path,
    sources: &[&Source],
) -> PyResult<Value> {
    let environment = PyDict::new(py);
    let os = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    os.setattr("environ", &environment)?;
    os.setattr("utime", py.import("os")?.getattr("utime")?)?;
    let stderr = py.import("io")?.getattr("StringIO")?.call0()?;
    let sys = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    sys.setattr("stderr", &stderr)?;
    let bindings = PyDict::new(py);
    bindings.set_item("os", &os)?;
    bindings.set_item("sys", sys)?;
    let integers = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["safe_int_env"],
        &["int", "print", "ValueError"],
        &bindings,
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("safe_int_env", required(&integers, "safe_int_env")?)?;
    bindings.set_item(
        "LOG_RETENTION_DEFAULT_DAYS",
        literal(py, sources[0], "LOG_RETENTION_DEFAULT_DAYS", false)?,
    )?;
    bindings.set_item(
        "LOG_DIR",
        py.import("pathlib")?.getattr("Path")?.call1((workspace,))?,
    )?;
    bindings.set_item("time", py.import("time")?)?;
    // The original test checks only that setup_logging is callable, not its effects.
    bindings.set_item("setup_logging", py.import("logging")?.getattr("getLogger")?)?;
    let globals = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &["log_retention_days", "prune_old_logs"],
        &["OSError"],
        &bindings,
    )?;
    let api = Py::new(py, RetentionModule(globals.clone().unbind()))?;
    let temporary = Py::new(
        py,
        RetentionTemporary {
            workspace: workspace.into(),
            calls: std::sync::atomic::AtomicUsize::new(0),
        },
    )?;
    let tempfile = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    tempfile.setattr(
        "TemporaryDirectory",
        temporary.bind(py).getattr("temporary_directory")?,
    )?;
    let unittest = py.import("unittest")?;
    let bindings = PyDict::new(py);
    for (name, value) in [
        ("unittest", unittest.as_any()),
        ("L", api.bind(py).as_any()),
        ("os", os.as_any()),
        ("tempfile", tempfile.as_any()),
    ] {
        bindings.set_item(name, value)?;
    }
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("time", py.import("time")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["TestRetentionIsReadWhenItIsUsed", "TestLogRetention"],
        &["__build_class__", "callable"],
        &bindings,
    )?;
    let methods = [
        (
            "TestRetentionIsReadWhenItIsUsed",
            "test_a_value_set_after_import_is_honoured",
        ),
        (
            "TestRetentionIsReadWhenItIsUsed",
            "test_prune_takes_its_default_at_call_time",
        ),
        (
            "TestRetentionIsReadWhenItIsUsed",
            "test_a_malformed_value_falls_back_instead_of_raising",
        ),
        (
            "TestRetentionIsReadWhenItIsUsed",
            "test_a_negative_value_cannot_mean_prune_everything",
        ),
        (
            "TestLogRetention",
            "test_old_logs_are_pruned_and_recent_ones_kept",
        ),
        ("TestLogRetention", "test_retention_can_be_disabled"),
    ];
    let result = unittest.getattr("TestResult")?.call0()?;
    for (class, method) in methods {
        required(&tests, class)?
            .call1((method,))?
            .call_method1("run", (&result,))?;
    }
    for name in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(name)?.len()? != 0 {
            return Err(PyValueError::new_err(format!(
                "original retention methods reported {name}"
            )));
        }
    }
    let temporary_calls = temporary
        .borrow(py)
        .calls
        .load(std::sync::atomic::Ordering::Relaxed);
    if result.getattr("testsRun")?.extract::<usize>()? != methods.len()
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || temporary_calls != 2
    {
        return Err(PyValueError::new_err(
            "original retention replay incomplete",
        ));
    }
    let mut observations = Vec::new();
    for raw in [
        None,
        Some(""),
        Some("7"),
        Some("0"),
        Some("  +۷  "),
        Some("1_0"),
        Some("-۰"),
        Some("\u{1c}+۷\u{1f}"),
        Some("_1"),
        Some("1_"),
        Some("1__0"),
        Some("0x10"),
        Some("²"),
        Some("1\0"),
        Some("+"),
        Some("- 1"),
        Some("-۰۰۹"),
        Some("-1"),
        Some("not-a-number"),
    ] {
        environment.clear();
        if let Some(raw) = raw {
            environment.set_item("EMOJI_LOG_RETENTION_DAYS", raw)?;
        }
        stderr.call_method1("seek", (0,))?;
        stderr.call_method1("truncate", (0,))?;
        let days: u64 = required(&globals, "log_retention_days")?
            .call0()?
            .extract()?;
        let warning: String = stderr.call_method0("getvalue")?.extract()?;
        observations.push(json!({"raw":raw,"days":days,"warning":warning}));
    }
    stderr.call_method0("close")?;
    let pruning = pruning_observations(py, workspace, &globals)?;
    Ok(
        json!({"baseline_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":sources.iter().map(|source| (source.path.as_str(), source.sha256.as_str())).collect::<std::collections::BTreeMap<_,_>>(),
        "original_methods":methods.map(|(class, method)| format!("{class}.{method}")),
        "tests_run":methods.len(),"temporary_calls":temporary_calls,"observations":observations,
        "pruning":pruning,
        "ports":"private environment dictionary and confined external tempfile; setup callable presence only",
        "known_difference":"native pruning retains linked entries and refuses unrepresentable clock ranges"}),
    )
}

fn pruning_observations(
    py: Python<'_>,
    workspace: &std::path::Path,
    globals: &Bound<'_, PyDict>,
) -> PyResult<Value> {
    let project = workspace.join("pruning");
    let directory = project.join("logs");
    std::fs::create_dir(&project)?;
    std::fs::create_dir(&directory)?;
    let path_type = py.import("pathlib")?.getattr("Path")?;
    let now: f64 = py.import("time")?.call_method0("time")?.extract()?;
    let os = py.import("os")?;
    for (name, days) in [
        ("old.log", 90),
        ("new.log", 1),
        ("caller.txt", 90),
        ("upper.LOG", 90),
        (".log", 90),
    ] {
        let path = directory.join(name);
        std::fs::write(&path, b"synthetic retained bytes")?;
        let stamp = now - f64::from(days) * 86400.0;
        os.call_method1("utime", (path_type.call1((&path,))?, (stamp, stamp)))?;
    }
    std::fs::create_dir(directory.join("directory.log"))?;
    globals.set_item("LOG_DIR", path_type.call1((&directory,))?)?;
    let prune = required(globals, "prune_old_logs")?;
    let mut observations = Vec::new();
    for days in [0, 30] {
        let removed: usize = prune.call1((days,))?.extract()?;
        let mut members = std::fs::read_dir(&directory)?
            .map(|entry| {
                entry?
                    .file_name()
                    .into_string()
                    .map_err(|_| std::io::Error::other("original retention filename is not UTF-8"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        members.sort();
        observations.push(json!({"days":days,"removed":removed,"members":members}));
    }
    for entry in std::fs::read_dir(&directory)? {
        let path = entry?.path();
        if path.is_dir() {
            std::fs::remove_dir(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    std::fs::remove_dir(&directory)?;
    std::fs::remove_dir(project)?;
    Ok(json!(observations))
}

pub fn generate_retention(
    root: &std::path::Path,
    workspace: &std::path::Path,
    sources: &[&Source],
) -> Result<Value, String> {
    retention_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root || !workspace.starts_with(&root) {
        return Err("logging retention requires its project-contained workspace".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect retention workspace")?
        .next()
        .is_some()
    {
        return Err("logging retention workspace must be empty".into());
    }
    Python::initialize();
    let report = Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        execute_retention(py, &workspace, sources).map_err(|error| {
            format!(
                "original logging retention failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })?;
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot verify retention cleanup")?
        .next()
        .is_some()
    {
        return Err("original logging retention left scratch; retained for diagnosis".into());
    }
    Ok(report)
}

pub fn generate(source: &Source, test: &Source) -> Result<Value, String> {
    source_identity(source, test)?;
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        execute(py, source, test).map_err(|error| {
            format!(
                "original logging replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_body_limit_product_is_read_without_executing_names_or_calls() {
        Python::initialize();
        Python::attach(|py| {
            let ast = py.import("ast").unwrap();
            let expression = |text: &str| {
                ast.call_method1("parse", (text, "fixture", "eval"))
                    .unwrap()
                    .getattr("body")
                    .unwrap()
            };
            assert_eq!(
                integer_product(&expression("4 * 1024 * 1024"), 0).unwrap(),
                4_194_304
            );
            for refused in [
                "True * 1",
                "'text' * 4",
                "limit * 1024",
                "4 + 1024",
                "2 ** 20",
                "64 * 1024 * 1024 * 2",
                "0 * __import__('os')",
            ] {
                assert!(
                    integer_product(&expression(refused), 0).is_err(),
                    "{refused}"
                );
            }
        });
    }
}
