//! Original count bookkeeping shares the admitted transport, writer and temporary ledger.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct Patch {
    environment: Py<PyDict>,
    calls: AtomicUsize,
}
#[pymethods]
impl Patch {
    #[getter]
    fn object<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("unittest.mock")?
            .getattr("patch")?
            .getattr("object")
    }
    #[pyo3(signature = (target, values, clear=false))]
    fn dict<'py>(
        &self,
        py: Python<'py>,
        target: &str,
        values: &Bound<'_, PyDict>,
        clear: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        if target != "os.environ"
            || clear
            || values.len() != 1
            || !values.iter().all(|(key, value)| {
                key.extract::<String>().is_ok_and(|key| {
                    ["FAKE_TOKEN", "TELEGRAM_BOT_TOKEN", "GENERAL_BOT_TOKEN"]
                        .contains(&key.as_str())
                }) && value.extract::<String>().is_ok_and(|value| value == "x")
            })
        {
            return Err(PyValueError::new_err(
                "unexpected original ticker environment patch",
            ));
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        py.import("unittest.mock")?
            .getattr("patch")?
            .call_method1("dict", (self.environment.bind(py), values))
    }
}

pub(super) fn run(
    py: Python<'_>,
    workspace: &Path,
    sources: &[&Source],
    publication: (&Bound<'_, PyDict>, &Bound<'_, PyDict>, &Bound<'_, PyAny>),
    temporary: &Bound<'_, PyAny>,
    result: &Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    let (telegram, locks, os) = publication;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let real_mock = py.import("unittest.mock")?;
    let environment = os.getattr("environ")?.cast_into::<PyDict>()?;
    let previous_environment = environment.copy()?;
    let sys = py.import("sys")?;
    let argv = sys.getattr("argv")?;
    let patch = Py::new(
        py,
        Patch {
            environment: environment.clone().unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    let mock = ns.call0()?;
    mock.setattr("patch", &patch)?;
    mock.setattr("Mock", real_mock.getattr("Mock")?)?;
    let path = py.import("pathlib")?.getattr("Path")?;
    let bindings = PyDict::new(py);
    for (name, value) in [
        ("os", os.clone()),
        ("sys", sys.clone().into_any()),
        ("Path", path.clone()),
        ("json", py.import("json")?.into_any()),
        ("argparse", py.import("argparse")?.into_any()),
        ("Telegram", required(telegram, "Telegram")?),
        ("SetState", required(telegram, "SetState")?),
        (
            "AmbiguousUploadError",
            required(telegram, "AmbiguousUploadError")?,
        ),
        ("time", required(telegram, "time")?),
    ] {
        bindings.set_item(name, value)?;
    }
    for name in [
        "StateInvalid",
        "validate_state_shape",
        "_intent_key",
        "make_intent",
        "write_json_atomic",
        "LockBusy",
    ] {
        bindings.set_item(name, required(locks, name)?)?;
    }
    for name in ["COIN_DEFAULT_EMOJI", "MAX_PER_SET"] {
        bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[22], name, false)?,
        )?;
    }
    let tree = py
        .import("ast")?
        .call_method1("parse", (&sources[22].text, &sources[22].path))?;
    let mut aliases = 0;
    for node in crate::source_test_inventory::body(&tree)? {
        if crate::source_test_inventory::kind(&node)? != "Assign" {
            continue;
        }
        let targets = node.getattr("targets")?;
        if targets.len() != 1 {
            continue;
        }
        let target = targets.get_item(0)?;
        if crate::source_test_inventory::kind(&target)? == "Name"
            && target.getattr("id")?.extract::<String>()? == "PER_SET"
        {
            let value = node.getattr("value")?;
            if crate::source_test_inventory::kind(&value)? != "Name"
                || value.getattr("id")?.extract::<String>()? != "MAX_PER_SET"
            {
                return Err(PyValueError::new_err(
                    "original ticker capacity alias drift",
                ));
            }
            aliases += 1;
        }
    }
    if aliases != 1 {
        return Err(PyValueError::new_err(
            "original ticker capacity alias missing or repeated",
        ));
    }
    bindings.set_item("PER_SET", required(&bindings, "MAX_PER_SET")?)?;
    bindings.set_item("ROOT", path.call1((workspace,))?)?;
    bindings.set_item(
        "EMOJI_DIR",
        path.call1((workspace.join("coins/logos/emoji"),))?,
    )?;
    bindings.set_item(
        "KEYWORDS_CSV",
        path.call1((workspace.join("coins/keywords.csv"),))?,
    )?;
    let operator = ns.call0()?;
    let empty = PyDict::new(py);
    empty.set_item("return_value", "")?;
    operator.setattr("value", real_mock.getattr("Mock")?.call((), Some(&empty))?)?;
    bindings.set_item("operator_config", operator)?;
    for name in [
        "load_env",
        "setup_logging",
        "register_secret",
        "announce_packs",
    ] {
        bindings.set_item(name, real_mock.getattr("Mock")?.call0()?)?;
    }
    let blocked = PyDict::new(py);
    blocked.set_item(
        "side_effect",
        pyo3::exceptions::PyRuntimeError::new_err("unexpected original ticker keyword read")
            .value(py),
    )?;
    bindings.set_item(
        "load_keywords",
        real_mock.getattr("Mock")?.call((), Some(&blocked))?,
    )?;
    bindings.set_item("redact", py.import("builtins")?.getattr("str")?)?;
    bindings.set_item(
        "exclusive_lock",
        py.import("contextlib")?.getattr("nullcontext")?,
    )?;
    let lock_path = PyDict::new(py);
    lock_path.set_item(
        "return_value",
        path.call1((workspace.join("ticker.lock"),))?,
    )?;
    bindings.set_item(
        "pack_family_lock_path",
        real_mock.getattr("Mock")?.call((), Some(&lock_path))?,
    )?;
    for (name, value) in [("EXIT_OK", 0), ("EXIT_PARTIAL", 3), ("EXIT_FAILED", 4)] {
        bindings.set_item(name, value)?;
    }
    let exit_bindings = PyDict::new(py);
    for name in ["EXIT_OK", "EXIT_PARTIAL", "EXIT_FAILED"] {
        exit_bindings.set_item(name, required(&bindings, name)?)?;
    }
    exit_bindings.set_item("os", os)?;
    exit_bindings.set_item("sys", &sys)?;
    let exits = crate::source_oracle_runtime::namespace(
        py,
        sources[26],
        &["ingest_exit_code", "safe_int_env"],
        &["int", "ValueError", "print"],
        &exit_bindings,
    )?;
    for name in ["ingest_exit_code", "safe_int_env"] {
        bindings.set_item(name, required(&exits, name)?)?;
    }
    let original = crate::source_oracle_runtime::namespace(
        py,
        sources[30],
        &["main", "_run_build", "_is_coin_source", "announcing_bot"],
        &[
            "set",
            "list",
            "dict",
            "str",
            "int",
            "bool",
            "len",
            "sorted",
            "next",
            "max",
            "enumerate",
            "isinstance",
            "any",
            "print",
            "OSError",
            "ValueError",
            "RuntimeError",
            "Exception",
        ],
        &bindings,
    )?;
    let build = crate::source_roster_modules::rebind(py, &original, "_original_ticker_builder")?;
    let test_bindings = PyDict::new(py);
    let api = crate::source_roster_modules::rebind(py, telegram, "_original_ticker_transport")?;
    for name in ["PER_SET", "MAX_PER_SET"] {
        api.setattr(name, required(&bindings, name)?)?;
    }
    for (name, value) in [
        ("bp", build.into_any()),
        ("tg_api", api.into_any()),
        (
            "ps",
            crate::source_roster_modules::rebind(py, locks, "_original_ticker_state")?.into_any(),
        ),
        ("tempfile", temporary.clone()),
        ("Path", path),
        ("mock", mock),
        ("sys", sys.clone().into_any()),
        ("json", py.import("json")?.into_any()),
        ("requests", py.import("requests")?.into_any()),
        ("unittest", py.import("unittest")?.into_any()),
    ] {
        test_bindings.set_item(name, value)?;
    }
    let png_bindings = PyDict::new(py);
    png_bindings.set_item("Image", py.import("PIL.Image")?)?;
    let png =
        crate::source_oracle_runtime::namespace(py, sources[29], &["_png"], &[], &png_bindings)?;
    test_bindings.set_item("_png", required(&png, "_png")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[27],
        &[
            "_Resp",
            "_ForeignWriterServer",
            "_LaggingReadServer",
            "BookkeepingFollowsLiveState",
        ],
        &[
            "__build_class__",
            "range",
            "list",
            "len",
            "super",
            "str",
            "sorted",
            "AssertionError",
        ],
        &test_bindings,
    )?;
    let resume = crate::source_oracle_runtime::namespace(
        py,
        sources[31],
        &["PerSetLimit", "ResumeAfterSkippedImage"],
        &[
            "__build_class__",
            "range",
            "list",
            "dict",
            "isinstance",
            "str",
            "all",
        ],
        &test_bindings,
    )?;
    let unresolved = crate::source_oracle_runtime::namespace(
        py,
        sources[32],
        &[
            "UnresolvedMutationStopsTheRun",
            "AmbiguousCreateForLaterSets",
            "RecordedSetIntegrity",
        ],
        &["__build_class__", "str"],
        &test_bindings,
    )?;
    let announcing = crate::source_oracle_runtime::namespace(
        py,
        sources[33],
        &["_one_set_telegram", "TheAnnouncerMatchesTheToken"],
        &["__build_class__", "str"],
        &test_bindings,
    )?;
    let before = result.getattr("testsRun")?.extract::<usize>()?;
    let outcome = (|| -> PyResult<Vec<String>> {
        let mut cases = Vec::new();
        for (namespace, name, count) in [
            (&tests, "BookkeepingFollowsLiveState", 4),
            (&resume, "PerSetLimit", 5),
            (&resume, "ResumeAfterSkippedImage", 7),
            (&unresolved, "UnresolvedMutationStopsTheRun", 8),
            (&unresolved, "AmbiguousCreateForLaterSets", 6),
            (&unresolved, "RecordedSetIntegrity", 3),
            (&announcing, "TheAnnouncerMatchesTheToken", 2),
        ] {
            let class = required(namespace, name)?;
            let methods = py
                .import("unittest")?
                .getattr("TestLoader")?
                .call0()?
                .call_method1("getTestCaseNames", (&class,))?
                .extract::<Vec<String>>()?;
            if methods.len() != count {
                return Err(PyValueError::new_err("original ticker collection drift"));
            }
            for method in methods {
                class.call1((&method,))?.call_method1("run", (result,))?;
                for field in [
                    "errors",
                    "failures",
                    "skipped",
                    "expectedFailures",
                    "unexpectedSuccesses",
                ] {
                    if result.getattr(field)?.len()? != 0 {
                        return Err(PyValueError::new_err(format!(
                            "original ticker {name}.{method} reported {field}; retained"
                        )));
                    }
                }
                cases.push(format!("{name}.{method}"));
            }
        }
        Ok(cases)
    })();
    if !sys.getattr("argv")?.is(&argv)
        || !environment.eq(&previous_environment)?
        || patch.borrow(py).calls.load(Ordering::Relaxed) != 31
    {
        return Err(PyValueError::new_err(
            "original ticker private patch restoration incomplete; retained",
        ));
    }
    let cases = outcome?;
    if result.getattr("testsRun")?.extract::<usize>()? != before + 35 {
        return Err(PyValueError::new_err(
            "original ticker assertion execution incomplete",
        ));
    }
    Ok(cases)
}
