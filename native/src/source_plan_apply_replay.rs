//! Original applier and fresh-upload assertions, sharing the reconciliation source owner and ledger.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::source_plan_apply_modules as modules;
#[path = "source_ticker_replay.rs"]
mod ticker;
#[path = "source_transport_replay.rs"]
mod transport;

#[pyclass]
struct Imports {
    telegram: Py<PyAny>,
    calls: AtomicUsize,
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
        if level != 0
            || name != "tests.oracles.telegram_api"
            || fromlist.extract::<Vec<String>>()? != ["Telegram"]
        {
            return Err(PyImportError::new_err("unexpected original applier import"));
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(self.telegram.bind(py).clone())
    }
}

#[pyclass]
struct Patch {
    patch: Py<PyAny>,
    time: Py<PyAny>,
    calls: AtomicUsize,
}
#[pymethods]
impl Patch {
    #[getter]
    fn object<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.patch.bind(py).getattr("object")
    }
    #[getter]
    fn dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.patch.bind(py).getattr("dict")
    }
    fn __call__<'py>(&self, py: Python<'py>, target: &str) -> PyResult<Bound<'py, PyAny>> {
        if target != "tests.oracles.telegram_api.time.sleep" {
            return Err(PyValueError::new_err(
                "unexpected original applier patch target",
            ));
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.patch
            .bind(py)
            .call_method1("object", (self.time.bind(py), "sleep"))
    }
}

const CLASSES: [(&str, usize); 6] = [
    ("CatalogUnpublish", 1),
    ("DryRun", 4),
    ("ApplyInBoundedRuns", 4),
    ("LongFloodWaitEndsTheRunCleanly", 2),
    ("ClientFloodCeiling", 2),
    ("RefuseBeforeTouchingAnything", 7),
];

pub(super) fn run(
    py: Python<'_>,
    workspace: &Path,
    sources: &[&Source],
    ports: &Bound<'_, PyDict>,
    temporary: &Bound<'_, PyAny>,
    result: &Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    let modules = modules::build(py, workspace, sources, ports)?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let real_mock = py.import("unittest.mock")?;
    let patch = Py::new(
        py,
        Patch {
            patch: real_mock.getattr("patch")?.unbind(),
            time: required(&modules.telegram, "time")?.unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    let mock = ns.call0()?;
    mock.setattr("Mock", real_mock.getattr("Mock")?)?;
    mock.setattr("patch", &patch)?;
    let telegram = ns.call0()?;
    telegram.setattr("Telegram", required(&modules.telegram, "Telegram")?)?;
    let imports = Py::new(
        py,
        Imports {
            telegram: telegram.unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    let bindings = PyDict::new(py);
    for (name, value) in [
        ("build_collection", modules.build.clone().into_any()),
        ("plan_apply", modules.apply.into_any()),
        ("sync_order", modules.order.into_any()),
        ("Catalog", required(ports, "Catalog")?),
        ("identity", required(ports, "identity")?),
        ("tempfile", temporary.clone()),
        ("os", modules.os.clone()),
        ("mock", mock),
    ] {
        bindings.set_item(name, value)?;
    }
    for name in ["_lock_path", "_state_path", "load_state", "save_json"] {
        bindings.set_item(name, required(&modules.state, name)?)?;
    }
    bindings.set_item(
        "exclusive_lock",
        required(&modules.locks, "exclusive_lock")?,
    )?;
    bindings.set_item("read_plan", required(&modules.panel, "read_plan")?)?;
    bindings.set_item(
        "FloodWaitTooLong",
        required(&modules.errors, "FloodWaitTooLong")?,
    )?;
    for name in ["SetState", "LiveStateUnknown"] {
        bindings.set_item(name, required(ports, "protocol")?.get_item(name)?)?;
    }
    for name in ["contextlib", "io", "json", "random", "time", "unittest"] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("Image", py.import("PIL.Image")?)?;
    bindings.set_item("BASE", "cryptoemoji")?;
    bindings.set_item("BOT", "YourEmojiBot")?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[16],
        &[
            "_name",
            "_image",
            "Crash",
            "FakeTelegram",
            "Family",
            "_Case",
            "CatalogUnpublish",
            "DryRun",
            "ApplyInBoundedRuns",
            "LongFloodWaitEndsTheRunCleanly",
            "ClientFloodCeiling",
            "RefuseBeforeTouchingAnything",
        ],
        &[
            "__build_class__",
            "BaseException",
            "dict",
            "list",
            "set",
            "range",
            "enumerate",
            "len",
            "str",
            "sorted",
            "frozenset",
            "zip",
            "sum",
            "map",
            "iter",
            "next",
            "RuntimeError",
            "AssertionError",
        ],
        &bindings,
    )?;
    required(&tests, "__builtins__")?.set_item("__import__", &imports)?;
    let loader = py.import("unittest")?.getattr("TestLoader")?.call0()?;
    let before: usize = result.getattr("testsRun")?.extract()?;
    let mut cases = Vec::new();
    for (name, count) in CLASSES {
        let class = required(&tests, name)?;
        let methods: Vec<String> = loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        if methods.len() != count {
            return Err(PyValueError::new_err("original applier collection drift"));
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
                        "original applier {name}.{method} reported {field}; retained"
                    )));
                }
            }
            cases.push(format!("{name}.{method}"));
        }
    }
    let fresh = PyDict::new(py);
    fresh.set_item("bc", &modules.build)?;
    fresh.set_item("mock", &real_mock)?;
    let helper =
        crate::source_oracle_runtime::namespace(py, sources[2], &["_main"], &["list"], &fresh)?;
    fresh.set_item("_main", required(&helper, "_main")?)?;
    fresh.set_item("_CatalogFixture", required(ports, "_CatalogFixture")?)?;
    let fakes = required(ports, "fakes")?.cast_into::<PyDict>()?;
    for name in ["FakeTG", "_sticker"] {
        fresh.set_item(name, required(&fakes, name)?)?;
    }
    fresh.set_item("Catalog", required(ports, "Catalog")?)?;
    fresh.set_item("os", &modules.os)?;
    fresh.set_item("SET", "pks1_by_YourEmojiBot")?;
    fresh.set_item("EXIT_FAILED", 4)?;
    fresh.set_item("EXIT_OK", 0)?;
    fresh.set_item("io", py.import("io")?)?;
    fresh.set_item(
        "redirect_stdout",
        py.import("contextlib")?.getattr("redirect_stdout")?,
    )?;
    let fresh = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["FreshUploadTG", "FreshUploadIdentity"],
        &["__build_class__", "super", "str"],
        &fresh,
    )?;
    let class = required(&fresh, "FreshUploadIdentity")?;
    let methods: Vec<String> = loader
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods.len() != 5 {
        return Err(PyValueError::new_err(
            "original fresh-upload collection drift",
        ));
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
                    "original fresh-upload {method} reported {field}; retained"
                )));
            }
        }
        cases.push(format!("FreshUploadIdentity.{method}"));
    }
    cases.extend(transport::run(
        py,
        sources,
        &modules.telegram,
        temporary,
        result,
    )?);
    cases.extend(ticker::run(
        py,
        workspace,
        sources,
        (&modules.telegram, &modules.locks, &modules.os),
        temporary,
        result,
    )?);
    let after: usize = result.getattr("testsRun")?.extract()?;
    let keys = modules.environment.keys();
    if after != before + 93
        || cases.len() != 93
        || imports.borrow(py).calls.load(Ordering::Relaxed) != 2
        || patch.borrow(py).calls.load(Ordering::Relaxed) != 2
        || keys.extract::<Vec<String>>()? != ["BRAND_LOGO_BOTS"]
        || required(&modules.environment, "BRAND_LOGO_BOTS")?.extract::<String>()? != ""
    {
        return Err(PyValueError::new_err(
            "original applier execution or private port restoration incomplete",
        ));
    }
    Ok(cases)
}
