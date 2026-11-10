//! Original verification main contracts; private ports refuse effects beyond the original tests.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct Loader {
    path: PathBuf,
    module: Py<PyModule>,
    calls: AtomicUsize,
}
#[pymethods]
impl Loader {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        path: PathBuf,
        name: &str,
    ) -> PyResult<Bound<'py, PyModule>> {
        if path != self.path || name != "coins_verify_logos_main" {
            return Err(PyValueError::new_err(
                "unexpected original verification module load",
            ));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.module.bind(py).clone())
    }
    #[getter]
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

pub(super) fn build<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyModule>> {
    let globals = bindings.copy()?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let mock = py.import("unittest.mock")?;
    let options = PyDict::new(py);
    options.set_item(
        "side_effect",
        py.import("builtins")?
            .getattr("AssertionError")?
            .call1(("unexpected original verification effect",))?,
    )?;
    let refusal = mock.getattr("Mock")?.call((), Some(&options))?;
    for name in [
        "fetch_markets",
        "fetch_image",
        "report",
        "setup_logging",
        "load_env",
        "_cids",
        "is_our_image",
        "_promote",
        "repoint",
        "_drop_stage",
        "cid_location",
    ] {
        globals.set_item(name, &refusal)?;
    }
    let telegram = mock.getattr("Mock")?.call0()?;
    let ctor_options = PyDict::new(py);
    ctor_options.set_item("return_value", &telegram)?;
    let constructor = mock.getattr("Mock")?.call((), Some(&ctor_options))?;
    globals.set_item("Telegram", &constructor)?;
    globals.set_item("argparse", py.import("argparse")?)?;
    let command = required(bindings, "fp")?;
    globals.set_item("operator_config", command.getattr("operator_config")?)?;
    let os = ns.call0()?;
    os.setattr("environ", required(bindings, "command_environment")?)?;
    os.setattr("path", py.import("os.path")?)?;
    globals.set_item("os", &os)?;
    globals.set_item("sys", py.import("sys")?)?;
    for (name, value) in [
        ("EXIT_OK", 0),
        ("EXIT_USAGE", 2),
        ("EXIT_PARTIAL", 3),
        ("EXIT_FAILED", 4),
    ] {
        globals.set_item(name, value)?;
        bindings.set_item(name, value)?;
    }
    let path = required(bindings, "Path")?;
    globals.set_item("ROOT", path.call1((workspace,))?)?;
    globals.set_item(
        "PACK_LOCK",
        path.call1((workspace.join("unpatched-verify.lock"),))?,
    )?;
    let numeric = crate::source_oracle_runtime::namespace(
        py,
        sources[4],
        &["safe_int_env"],
        &["int", "print", "ValueError"],
        &globals,
    )?;
    globals.set_item("safe_int_env", required(&numeric, "safe_int_env")?)?;
    let exit = crate::source_oracle_runtime::namespace(
        py,
        sources[4],
        &["ingest_exit_code"],
        &[],
        &globals,
    )?;
    globals.set_item("ingest_exit_code", required(&exit, "ingest_exit_code")?)?;
    let log = ns.call0()?;
    for name in ["info", "warning", "error"] {
        log.setattr(name, mock.getattr("Mock")?.call0()?)?;
    }
    globals.set_item("log", log)?;
    let original = crate::source_oracle_runtime::namespace(
        py,
        sources[21],
        &[
            "main",
            "_intent_path",
            "_run_targets",
            "reconcile_intent",
            "fix_one",
        ],
        &[
            "str",
            "int",
            "dict",
            "list",
            "len",
            "sorted",
            "next",
            "bool",
            "RuntimeError",
            "OSError",
            "ValueError",
            "KeyError",
            "TypeError",
        ],
        &globals,
    )?;
    let module = crate::source_roster_modules::rebind(py, &original, "_original_coin_verify_main")?;
    bindings.set_item("os", os)?;
    bindings.set_item("mock", mock)?;
    bindings.set_item("verify_mod", &module)?;
    bindings.set_item("verify_refusal", refusal)?;
    bindings.set_item("verify_telegram", telegram)?;
    bindings.set_item("verify_constructor", constructor)?;
    Ok(module)
}

pub(super) fn tests<'py>(
    py: Python<'py>,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyDict>> {
    let globals = bindings.copy()?;
    let module = required(bindings, "verify_mod")?;
    let root: PathBuf = required(bindings, "ROOT")?.extract()?;
    let load = Py::new(
        py,
        Loader {
            path: root.join("coins/verify_logos.py"),
            module: module.cast_into::<PyModule>()?.unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    globals.set_item("_load_standalone", load.bind(py))?;
    globals.set_item("verify_loader", load.bind(py))?;
    globals.set_item("OLD_CID", "cid-old")?;
    crate::source_oracle_runtime::namespace(
        py,
        sources[22],
        &["VerifyLogosMainContracts"],
        &["__build_class__", "classmethod", "str", "len", "reversed"],
        &globals,
    )
}
