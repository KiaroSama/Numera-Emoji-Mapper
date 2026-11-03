//! Original formatter/outcome observations; admitted only by the fixed logging source owner.
use crate::source_git_inventory::Source;
use crate::source_logging_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};

pub(crate) fn observe(
    py: Python<'_>,
    source: &Source,
    test: &Source,
    redaction: &Bound<'_, PyDict>,
    token: &str,
) -> PyResult<Value> {
    let logging = py.import("logging")?;
    let state = PyDict::new(py);
    state.set_item("id", "deadbeef")?;
    state.set_item("exit_code", py.None())?;
    state.set_item("start", 10.0)?;
    let counts = PyDict::new(py);
    for level in ["WARNING", "ERROR", "CRITICAL"] {
        counts.set_item(level, 0)?;
    }
    state.set_item("counts", &counts)?;
    let namespace = py.import("types")?.getattr("SimpleNamespace")?;
    let mock = py.import("unittest.mock")?.getattr("Mock")?;
    let options = PyDict::new(py);
    options.set_item("return_value", 12.5)?;
    let clock = namespace.call0()?;
    clock.setattr("time", mock.call((), Some(&options))?)?;
    let registration = mock.call0()?;
    let atexit = namespace.call0()?;
    atexit.setattr("register", &registration)?;
    let shutdown = mock.call0()?;
    let private_logging = namespace.call0()?;
    private_logging.setattr("Handler", logging.getattr("Handler")?)?;
    private_logging.setattr("Formatter", logging.getattr("Formatter")?)?;
    private_logging.setattr("shutdown", &shutdown)?;
    let previous_hook = mock.call0()?;
    let hook_output = py.import("io")?.getattr("StringIO")?.call0()?;
    let private_sys = namespace.call0()?;
    private_sys.setattr("excepthook", &previous_hook)?;
    private_sys.setattr("stderr", &hook_output)?;
    let private_threading = namespace.call0()?;
    private_threading.setattr("excepthook", py.None())?;
    let bindings = PyDict::new(py);
    bindings.set_item("logging", &private_logging)?;
    bindings.set_item("time", clock)?;
    bindings.set_item("atexit", atexit)?;
    bindings.set_item("sys", &private_sys)?;
    bindings.set_item("threading", &private_threading)?;
    bindings.set_item("traceback", py.import("traceback")?)?;
    bindings.set_item("re", py.import("re")?)?;
    bindings.set_item("_RUN", &state)?;
    bindings.set_item("redact", required(redaction, "redact")?)?;
    bindings.set_item("_LEVEL_COLOR", PyDict::new(py))?;
    bindings.set_item("_RESET", "")?;
    let datetime = py.import("datetime")?;
    bindings.set_item("datetime", datetime.getattr("datetime")?)?;
    bindings.set_item("timezone", datetime.getattr("timezone")?)?;
    let globals = crate::source_oracle_runtime::namespace(
        py,
        source,
        &[
            "_HumanFormatter",
            "_CounterHandler",
            "_install_summary",
            "_install_excepthooks",
            "_sanitize",
            "record_exit_code",
        ],
        &[
            "__build_class__",
            "super",
            "int",
            "str",
            "issubclass",
            "KeyboardInterrupt",
            "hasattr",
        ],
        &bindings,
    )?;
    let api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    api.setattr("_RUN", &state)?;
    api.setattr("_HumanFormatter", required(&globals, "_HumanFormatter")?)?;
    api.setattr("record_exit_code", required(&globals, "record_exit_code")?)?;
    let bindings = PyDict::new(py);
    bindings.set_item("unittest", py.import("unittest")?)?;
    bindings.set_item("logging", &logging)?;
    bindings.set_item("L", api)?;
    bindings.set_item("TOKEN", token)?;
    bindings.set_item("__file__", "tests/test_logsetup.py")?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &["TestRedaction", "TestRunOutcome"],
        &["__build_class__"],
        &bindings,
    )?;
    let result = py.import("unittest")?.getattr("TestResult")?.call0()?;
    let methods = [
        ("TestRedaction", "test_formatter_redacts_record"),
        ("TestRunOutcome", "test_exit_code_is_recorded_and_returned"),
    ];
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
                "original log formatter reported {name}"
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != methods.len()
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original formatter/outcome replay incomplete",
        ));
    }
    let formatter = required(&globals, "_HumanFormatter")?
        .call1(("[%(levelname)s] [%(run_id)s] %(message)s",))?;
    let record = logging.getattr("LogRecord")?.call1((
        "fixture",
        logging.getattr("INFO")?,
        "tests/test_logsetup.py",
        1,
        "token %s",
        (token,),
        py.None(),
    ))?;
    let formatted: String = formatter.call_method1("format", (record,))?.extract()?;
    let mut outcomes = Vec::new();
    for code in [0, 3] {
        let returned: i32 = required(&globals, "record_exit_code")?
            .call1((code,))?
            .extract()?;
        let recorded: i32 = state
            .get_item("exit_code")?
            .ok_or_else(|| PyValueError::new_err("original outcome disappeared"))?
            .extract()?;
        outcomes.push(json!({"input":code,"returned":returned,"recorded":recorded}));
    }
    let logger = logging
        .getattr("Logger")?
        .call1(("private-summary-fixture", logging.getattr("DEBUG")?))?;
    let counter = required(&globals, "_CounterHandler")?.call0()?;
    let capture = py.import("io")?.getattr("StringIO")?.call0()?;
    let stream = logging.getattr("StreamHandler")?.call1((&capture,))?;
    stream.call_method1("setFormatter", (&formatter,))?;
    logger.call_method1("addHandler", (&counter,))?;
    logger.call_method1("addHandler", (&stream,))?;
    let observed = (|| -> PyResult<Value> {
        let levels = ["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL", "WARNING"];
        for level in levels {
            logger.call_method1(
                "log",
                (
                    logging.getattr(level)?,
                    format!("diagnostic {level} {token}"),
                ),
            )?;
        }
        let diagnostic_counts: std::collections::BTreeMap<String, u64> = counts.extract()?;
        required(&globals, "_install_summary")?.call1((&logger,))?;
        if registration.getattr("call_count")?.extract::<usize>()? != 1 {
            return Err(PyValueError::new_err("original summary registration drift"));
        }
        let callback = registration
            .getattr("call_args")?
            .getattr("args")?
            .get_item(0)?;
        let mut summaries = Vec::new();
        for code in [0, 3] {
            required(&globals, "record_exit_code")?.call1((code,))?;
            capture.call_method1("seek", (0,))?;
            capture.call_method1("truncate", (0,))?;
            callback.call0()?;
            let text: String = capture.call_method0("getvalue")?.extract()?;
            summaries.push(json!({"code":code,"text":text}));
        }
        if shutdown.getattr("call_count")?.extract::<usize>()? != 2
            || counts.extract::<std::collections::BTreeMap<String, u64>>()? != diagnostic_counts
        {
            return Err(PyValueError::new_err(
                "original summary count or shutdown drift",
            ));
        }
        required(&globals, "_install_excepthooks")?.call1((&logger,))?;
        let literal = "synthetic-never-issued-exception-credential";
        required(redaction, "register_secret")?.call1((literal,))?;
        let builtins = py.import("builtins")?;
        let exception_type = builtins.getattr("ValueError")?;
        let exception = exception_type.call1((format!("controlled {literal} {token}"),))?;
        let mut exceptions = Vec::new();
        for mode in [
            "ordinary",
            "thread",
            "unnamed-thread",
            "traceback",
            "keyboard-interrupt",
        ] {
            capture.call_method1("seek", (0,))?;
            capture.call_method1("truncate", (0,))?;
            hook_output.call_method1("seek", (0,))?;
            hook_output.call_method1("truncate", (0,))?;
            match mode {
                "ordinary" => {
                    private_sys.getattr("excepthook")?.call1((
                        &exception_type,
                        &exception,
                        py.None(),
                    ))?;
                }
                "traceback" => {
                    let error = required(&globals, "record_exit_code")?
                        .call1((format!("controlled {literal} {token}"),))
                        .err()
                        .ok_or_else(|| {
                            PyValueError::new_err("original integer conversion did not raise")
                        })?;
                    if !error.is_instance_of::<pyo3::exceptions::PyValueError>(py)
                        || error.traceback(py).is_none()
                    {
                        return Err(PyValueError::new_err(
                            "original exception traceback was not captured",
                        ));
                    }
                    private_sys.getattr("excepthook")?.call1((
                        error.get_type(py),
                        error.value(py),
                        error.traceback(py),
                    ))?;
                }
                "keyboard-interrupt" => {
                    let class = builtins.getattr("KeyboardInterrupt")?;
                    private_sys.getattr("excepthook")?.call1((
                        &class,
                        class.call0()?,
                        py.None(),
                    ))?;
                }
                _ => {
                    let args = namespace.call0()?;
                    args.setattr("exc_type", &exception_type)?;
                    args.setattr("exc_value", &exception)?;
                    args.setattr("exc_traceback", py.None())?;
                    let thread = if mode == "thread" {
                        let thread = namespace.call0()?;
                        thread.setattr("name", "fixture-thread")?;
                        thread
                    } else {
                        py.None().into_bound(py)
                    };
                    args.setattr("thread", thread)?;
                    private_threading.getattr("excepthook")?.call1((args,))?;
                }
            }
            let logged: String = capture.call_method0("getvalue")?.extract()?;
            let stderr: String = hook_output.call_method0("getvalue")?.extract()?;
            let previous_calls: usize = previous_hook.getattr("call_count")?.extract()?;
            exceptions.push(json!({"mode":mode,"logged":logged,"stderr":stderr,"previous_calls":previous_calls}));
        }
        Ok(
            json!({"levels":levels,"counts":diagnostic_counts,"summaries":summaries,
            "shutdown_calls":2,"duration_seconds":2.5,"exceptions":exceptions,
            "ports":"private real Logger and handlers; fixed clock and captured atexit/shutdown callbacks; exception hooks assigned only to private sys/threading namespaces; no global hook installed"}),
        )
    })();
    logger.call_method1("removeHandler", (&stream,))?;
    logger.call_method1("removeHandler", (&counter,))?;
    stream.call_method0("close")?;
    counter.call_method0("close")?;
    capture.call_method0("close")?;
    hook_output.call_method0("close")?;
    let summary = observed?;
    let mut labels = Vec::new();
    for input in [
        "",
        "  ",
        "collect.py",
        "__name__",
        "../outside",
        "foo/bar\\baz : 😀.rs",
        "a__b",
        "😀😀alphaسلامbeta",
        "._-",
        "\0\n",
    ] {
        let expected: String = required(&globals, "_sanitize")?
            .call1((input,))?
            .extract()?;
        labels.push(json!({"input":input,"expected":expected}));
    }
    Ok(json!({"tests_run":methods.len(),
        "original_methods":methods.map(|(class, method)| format!("{class}.{method}")),
        "formatted":formatted,"message":format!("token {token}"),"outcomes":outcomes,
        "summary":summary,"labels":labels,
        "coverage":"original formatter, recorded outcome, counters, summary, five private exception-hook observations and ten filename labels; no global root setup or process hook installation"}))
}
