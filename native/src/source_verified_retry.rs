//! Original real-content retry assertions share the admitted publisher client and temporary ledger.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct Imports {
    telegram: Py<PyAny>,
    media: Py<PyAny>,
    calls: AtomicUsize,
    comparisons: AtomicUsize,
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
    ) -> PyResult<Bound<'py, PyAny>> {
        let requested: Vec<String> = fromlist.extract()?;
        if level == 0 && name == "tests.oracles.telegram_api" && requested == ["Telegram"] {
            self.calls.fetch_add(1, Ordering::Relaxed);
            return Ok(self.telegram.bind(py).clone());
        }
        if level == 0 && name == "emojikit" && requested == ["identity", "media"] {
            self.comparisons.fetch_add(1, Ordering::Relaxed);
            return Ok(self.media.bind(py).clone());
        }
        Err(PyImportError::new_err(
            "unexpected original verified-retry import",
        ))
    }
}
#[pyclass]
struct Patch {
    clock: Py<PyAny>,
    calls: AtomicUsize,
}
#[pymethods]
impl Patch {
    #[getter]
    fn object<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::source_oracle_runtime::host_import(py, "unittest.mock")?
            .getattr("patch")?
            .getattr("object")
    }
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        target: &str,
        replacement: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if target != "tests.reference.build_pack.time.sleep" || !replacement.is_callable() {
            return Err(PyValueError::new_err(
                "unexpected original verified-retry patch",
            ));
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        crate::source_oracle_runtime::host_import(py, "unittest.mock")?
            .getattr("patch")?
            .call_method1("object", (self.clock.bind(py), "sleep", replacement))
    }
}

pub(super) fn run(
    py: Python<'_>,
    source: &Source,
    telegram: &Bound<'_, PyDict>,
    ports: &Bound<'_, PyDict>,
    temporary: &Bound<'_, PyAny>,
    result: &Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let clock = required(telegram, "time")?;
    let previous_sleep = clock.getattr("sleep")?;
    let api = ns.call0()?;
    api.setattr("Telegram", required(telegram, "Telegram")?)?;
    let media = ns.call0()?;
    media.setattr("identity", required(ports, "identity")?)?;
    media.setattr("media", required(ports, "cr")?.getattr("media")?)?;
    let imports = Py::new(
        py,
        Imports {
            telegram: api.unbind(),
            media: media.unbind(),
            calls: AtomicUsize::new(0),
            comparisons: AtomicUsize::new(0),
        },
    )?;
    let patch = Py::new(
        py,
        Patch {
            clock: clock.clone().unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    let mock = ns.call0()?;
    mock.setattr("patch", &patch)?;
    mock.setattr("Mock", py.import("unittest.mock")?.getattr("Mock")?)?;
    let bindings = PyDict::new(py);
    for (name, value) in [
        ("Path", py.import("pathlib")?.getattr("Path")?),
        ("unittest", py.import("unittest")?.into_any()),
        ("requests", py.import("requests")?.into_any()),
        ("io", py.import("io")?.into_any()),
        ("Image", py.import("PIL.Image")?.into_any()),
        ("tempfile", temporary.clone()),
        ("mock", mock),
        (
            "_make_png",
            required(
                &required(ports, "fixture_bindings")?.cast_into::<PyDict>()?,
                "_make_png",
            )?,
        ),
        (
            "SourceAmbiguousUploadError",
            required(telegram, "AmbiguousUploadError")?,
        ),
    ] {
        bindings.set_item(name, value)?;
    }
    let tests = crate::source_oracle_runtime::namespace(
        py,
        source,
        &[
            "_Resp",
            "_png_bytes",
            "_FileResp",
            "_FakeServer",
            "VerifiedRetryTest",
        ],
        &[
            "__build_class__",
            "range",
            "dict",
            "list",
            "set",
            "str",
            "bytes",
            "len",
            "next",
            "isinstance",
            "AssertionError",
            "RuntimeError",
        ],
        &bindings,
    )?;
    required(&tests, "__builtins__")?.set_item("__import__", &imports)?;
    let builtin = required(telegram, "__builtins__")?.cast_into::<PyDict>()?;
    let previous_import = required(&builtin, "__import__")?;
    let previous_tempfile = required(telegram, "tempfile")?;
    let values = PyDict::new(py);
    values.set_item("__import__", &imports)?;
    let guard = py
        .import("unittest.mock")?
        .getattr("patch")?
        .call_method1("dict", (&builtin, values))?;
    let temp_values = PyDict::new(py);
    temp_values.set_item("tempfile", temporary)?;
    let temp_guard = py
        .import("unittest.mock")?
        .getattr("patch")?
        .call_method1("dict", (telegram, temp_values))?;
    let stack = py.import("contextlib")?.getattr("ExitStack")?.call0()?;
    let outcome = (|| -> PyResult<Vec<String>> {
        stack.call_method1("enter_context", (guard,))?;
        stack.call_method1("enter_context", (temp_guard,))?;
        let before: usize = result.getattr("testsRun")?.extract()?;
        let class = required(&tests, "VerifiedRetryTest")?;
        let names: Vec<String> = py
            .import("unittest")?
            .getattr("TestLoader")?
            .call0()?
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        if names.len() != 6 {
            return Err(PyValueError::new_err(
                "original verified-retry collection drift",
            ));
        }
        let mut cases = Vec::new();
        for method in names {
            // Historical behavior only: native transport still refuses no-snapshot blind retry.
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
                        "original verified-retry {method} reported {field}: {}; retained",
                        crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                    )));
                }
            }
            cases.push(format!("VerifiedRetryTest.{method}"));
        }
        if result.getattr("testsRun")?.extract::<usize>()? != before + 6
            || cases.len() != 6
            || imports.borrow(py).calls.load(Ordering::Relaxed) != 6
            || imports.borrow(py).comparisons.load(Ordering::Relaxed) != 7
            || patch.borrow(py).calls.load(Ordering::Relaxed) != 1
        {
            return Err(PyValueError::new_err(
                "original verified-retry execution/port counts incomplete",
            ));
        }
        Ok(cases)
    })();
    stack.call_method0("close")?;
    if !required(&builtin, "__import__")?.is(&previous_import)
        || !required(telegram, "tempfile")?.is(&previous_tempfile)
        || !clock.getattr("sleep")?.is(&previous_sleep)
    {
        return Err(PyValueError::new_err(
            "original verified-retry port restoration incomplete",
        ));
    }
    outcome
}
