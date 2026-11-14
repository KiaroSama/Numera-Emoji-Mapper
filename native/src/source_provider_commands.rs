//! Original Paprika command dependencies; no application imports or native policy answers.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct EnvironmentLoad {
    calls: AtomicUsize,
}
#[pymethods]
impl EnvironmentLoad {
    fn __call__(&self) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
    #[getter]
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[pyclass]
struct PrivatePatch {
    http: Py<PyAny>,
    environment: Py<PyAny>,
    calls: Arc<AtomicUsize>,
}
#[pyclass]
struct ObjectPatch {
    http: Py<PyAny>,
    object: Py<PyAny>,
    calls: Arc<AtomicUsize>,
}
#[pymethods]
impl ObjectPatch {
    #[pyo3(signature = (target, attribute, *args, **kwargs))]
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        target: &Bound<'py, PyAny>,
        attribute: &str,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if target.is(self.http.bind(py)) && attribute == "get" {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
        let mut values = vec![target.clone(), attribute.into_pyobject(py)?.into_any()];
        values.extend(args.iter());
        self.object.bind(py).call(PyTuple::new(py, values)?, kwargs)
    }
}
#[pymethods]
impl PrivatePatch {
    #[pyo3(signature = (target, *args, **kwargs))]
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        target: &str,
        args: &Bound<'py, PyTuple>,
        kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if target != "coins._http.get" {
            return Err(PyValueError::new_err(
                "unexpected original provider string patch",
            ));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut values = vec![
            self.http.bind(py).clone(),
            "get".into_pyobject(py)?.into_any(),
        ];
        values.extend(args.iter());
        let values = PyTuple::new(py, values)?;
        crate::source_oracle_runtime::host_import(py, "unittest.mock")?
            .getattr("patch")?
            .getattr("object")?
            .call(values, kwargs)
    }
    #[getter]
    fn object<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let object = crate::source_oracle_runtime::host_import(py, "unittest.mock")?
            .getattr("patch")?
            .getattr("object")?;
        Ok(Py::new(
            py,
            ObjectPatch {
                http: self.http.clone_ref(py),
                object: object.unbind(),
                calls: self.calls.clone(),
            },
        )?
        .into_bound(py)
        .into_any())
    }
    #[getter]
    fn multiple<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::source_oracle_runtime::host_import(py, "unittest.mock")?
            .getattr("patch")?
            .getattr("multiple")
    }
    #[getter]
    fn dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.environment.bind(py).getattr("dict")
    }
    #[getter]
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

pub(super) fn augment<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
) -> PyResult<()> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let path = required(bindings, "Path")?;
    let environment = PyDict::new(py);
    environment.set_item("COIN_PACK_BASE", "cryptoemoji")?;
    environment.set_item("COIN_PACK_TITLE", "Title")?;
    let os = ns.call0()?;
    os.setattr("environ", &environment)?;
    let globals = bindings.copy()?;
    globals.set_item("os", os)?;
    globals.set_item("sys", py.import("sys")?)?;
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[15],
        &["OperatorConfigMissing"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    globals.set_item(
        "OperatorConfigMissing",
        required(&errors, "OperatorConfigMissing")?,
    )?;
    globals.set_item(
        "WHAT",
        crate::source_logging_oracle::literal(py, sources[14], "WHAT", false)?,
    )?;
    let operator = crate::source_oracle_runtime::namespace(
        py,
        sources[14],
        &["value", "require", "stop_unless"],
        &["SystemExit"],
        &globals,
    )?;
    globals.set_item(
        "operator_config",
        crate::source_roster_modules::rebind(py, &operator, "_original_provider_operator")?,
    )?;
    for name in ["EXIT_OK", "EXIT_USAGE", "EXIT_PARTIAL", "EXIT_FAILED"] {
        globals.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[4], name, false)?,
        )?;
    }
    let exit = crate::source_oracle_runtime::namespace(
        py,
        sources[4],
        &["ingest_exit_code"],
        &[],
        &globals,
    )?;
    globals.set_item("ingest_exit_code", required(&exit, "ingest_exit_code")?)?;
    let token = crate::source_oracle_runtime::namespace(
        py,
        sources[13],
        &["require_token"],
        &["print", "SystemExit"],
        &globals,
    )?;
    globals.set_item("require_token", required(&token, "require_token")?)?;
    let re = py.import("re")?;
    globals.set_item("re", &re)?;
    for name in ["SUFFIXES", "EXPLICIT_ALIASES"] {
        globals.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[12], name, false)?,
        )?;
    }
    for name in ["_BLOCK_RE", "_TICKER_RE", "_PREMIUM_RE"] {
        let pattern = crate::source_logging_oracle::literal(py, sources[12], name, true)?;
        globals.set_item(name, re.call_method1("compile", (pattern,))?)?;
    }
    let inventory = crate::source_oracle_runtime::namespace(
        py,
        sources[12],
        &["norm", "base_ticker", "parse_missing", "refill_inventory"],
        &["len", "enumerate"],
        &globals,
    )?;
    let inventory =
        crate::source_roster_modules::rebind(py, &inventory, "_original_provider_inventory")?;
    globals.set_item("_inventory", &inventory)?;
    for name in ["norm", "base_ticker"] {
        globals.set_item(name, inventory.getattr(name)?)?;
    }
    let mock = py.import("unittest.mock")?;
    let options = PyDict::new(py);
    options.set_item(
        "side_effect",
        py.import("builtins")?
            .getattr("AssertionError")?
            .call1(("unexpected original provider external effect",))?,
    )?;
    let refusal = mock.getattr("Mock")?.call((), Some(&options))?;
    let http = ns.call0()?;
    http.setattr("get", &refusal)?;
    globals.set_item("_http", &http)?;
    let urllib = ns.call0()?;
    urllib.setattr("parse", py.import("urllib.parse")?)?;
    globals.set_item("urllib", urllib)?;
    let object = py.import("builtins")?.getattr("object")?;
    for name in ["QUOTA_EXHAUSTED", "TRANSIENT"] {
        globals.set_item(name, object.call0()?)?;
    }
    for name in ["SEARCH", "HEADERS", "SLEEP"] {
        globals.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[9], name, false)?,
        )?;
    }
    let api = crate::source_oracle_runtime::namespace(
        py,
        sources[9],
        &["http_json", "http_bytes", "classify", "search_match"],
        &["isinstance", "dict", "str"],
        &globals,
    )?;
    let api = crate::source_roster_modules::rebind(py, &api, "_original_provider_search")?;
    for name in [
        "http_json",
        "http_bytes",
        "search_match",
        "QUOTA_EXHAUSTED",
        "TRANSIENT",
    ] {
        globals.set_item(name, api.getattr(name)?)?;
    }
    let publisher = required(bindings, "pp")?;
    for name in ["TICKER_IDS", "incoming_dir", "publish_logos"] {
        globals.set_item(name, publisher.getattr(name)?)?;
    }
    globals.set_item(
        "to_emoji_png",
        required(bindings, "fp")?.getattr("to_emoji_png")?,
    )?;
    globals.set_item("argparse", py.import("argparse")?)?;
    globals.set_item("setup_logging", &refusal)?;
    globals.set_item("Telegram", &refusal)?;
    let load = Py::new(
        py,
        EnvironmentLoad {
            calls: AtomicUsize::new(0),
        },
    )?;
    globals.set_item("load_env", load.bind(py))?;
    for name in ["INV", "OUT_INV", "CACHE"] {
        globals.set_item(
            name,
            path.call1((workspace.join(format!("unpatched-{name}")),))?,
        )?;
    }
    for name in ["COIN", "LOGO_CDN"] {
        globals.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[11], name, false)?,
        )?;
    }
    let fp = crate::source_oracle_runtime::namespace(
        py,
        sources[11],
        &[
            "parse_missing",
            "refill_inventory",
            "load_cache",
            "save_cache",
            "resolve_phase",
            "main",
        ],
        &["set", "len", "max", "print", "isinstance", "dict"],
        &globals,
    )?;
    let fp = crate::source_roster_modules::rebind(py, &fp, "_original_provider_command")?;
    let patch = Py::new(
        py,
        PrivatePatch {
            http: http.clone().unbind(),
            environment: crate::source_config_oracle::private_environment_patch(py, &environment)?
                .unbind(),
            calls: Arc::new(AtomicUsize::new(0)),
        },
    )?;
    let private_mock = ns.call0()?;
    private_mock.setattr("patch", patch.bind(py))?;
    let bp = ns.call0()?;
    for name in ["EXIT_OK", "EXIT_USAGE", "EXIT_PARTIAL", "EXIT_FAILED"] {
        bp.setattr(name, required(&globals, name)?)?;
    }
    for (name, value) in [
        ("fp", fp.into_any()),
        ("_http", http),
        ("mock", private_mock),
        ("bp", bp),
        ("sys", py.import("sys")?.into_any()),
        ("contextlib", py.import("contextlib")?.into_any()),
    ] {
        bindings.set_item(name, value)?;
    }
    bindings.set_item("_inventory", inventory)?;
    bindings.set_item("command_environment", &environment)?;
    bindings.set_item("command_patch", patch.bind(py))?;
    bindings.set_item("command_load", load.bind(py))?;
    bindings.set_item("command_refusal", refusal)?;
    Ok(())
}
