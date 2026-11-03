//! Original remap dependencies; unchanged numerical/cache/command functions use private effect ports.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::path::Path;

pub(super) fn build<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyModule>> {
    let globals = bindings.copy()?;
    let np = py.import("numpy")?;
    if np.getattr("__version__")?.extract::<String>()? != "2.4.6" {
        return Err(PyValueError::new_err(
            "original remap assertions require NumPy2.4.6",
        ));
    }
    globals.set_item("np", &np)?;
    for name in ["base64", "hashlib", "argparse", "shutil"] {
        globals.set_item(name, py.import(name)?)?;
    }
    globals.set_item("Counter", py.import("collections")?.getattr("Counter")?)?;
    let command = required(bindings, "fp")?;
    globals.set_item("operator_config", command.getattr("operator_config")?)?;
    globals.set_item("os", command.getattr("os")?)?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let mock = py.import("unittest.mock")?;
    let options = PyDict::new(py);
    options.set_item(
        "side_effect",
        py.import("builtins")?
            .getattr("AssertionError")?
            .call1(("unexpected original remap effect",))?,
    )?;
    let refusal = mock.getattr("Mock")?.call((), Some(&options))?;
    globals.set_item("setup_logging", &refusal)?;
    globals.set_item("Telegram", &refusal)?;
    let load = mock.getattr("Mock")?.call0()?;
    globals.set_item("load_env", &load)?;
    bindings.set_item("remap_load", load)?;
    let log = ns.call0()?;
    for name in ["info", "warning", "error"] {
        log.setattr(name, mock.getattr("Mock")?.call0()?)?;
    }
    globals.set_item("log", log)?;
    let path = required(bindings, "Path")?;
    globals.set_item("ROOT", path.call1((workspace,))?)?;
    globals.set_item(
        "PACK_LOCK",
        required(bindings, "ps")?.call_method1("pack_family_lock_path", ("cryptoemoji",))?,
    )?;
    globals.set_item("SET_BASE", "cryptoemoji")?;
    globals.set_item(
        "SIG_PX",
        crate::source_logging_oracle::literal(py, sources[19], "SIG_PX", false)?,
    )?;
    // SIG_LEN's source expression is SIG_PX * SIG_PX * 4, not an AST literal.
    globals.set_item("SIG_LEN", 1024)?;
    for (name, value) in [
        ("EXIT_OK", 0),
        ("EXIT_USAGE", 2),
        ("EXIT_PARTIAL", 3),
        ("EXIT_FAILED", 4),
    ] {
        globals.set_item(name, value)?;
        bindings.set_item(name, value)?;
    }
    let original = crate::source_oracle_runtime::namespace(
        py,
        sources[19],
        &[
            "signature",
            "manifest_digest",
            "load_cache",
            "save_cache",
            "download_live",
            "build_local",
            "nearest",
            "main",
            "_remap",
        ],
        &[
            "dict",
            "list",
            "str",
            "len",
            "set",
            "enumerate",
            "zip",
            "Exception",
            "ValueError",
            "sorted",
            "range",
            "float",
            "int",
            "round",
            "sum",
        ],
        &globals,
    )?;
    let remap = crate::source_roster_modules::rebind(py, &original, "_original_coin_remap")?;
    let requests = ns.call0()?;
    requests.setattr(
        "HTTPError",
        py.import("requests.exceptions")?.getattr("HTTPError")?,
    )?;
    globals.set_item("requests", requests)?;
    let fakes = crate::source_oracle_runtime::namespace(
        py,
        sources[5],
        &[
            "_png",
            "_sticker",
            "FakeResponse",
            "FakeSession",
            "LiveSetsTelegram",
            "_provider_top_up",
        ],
        &[
            "__build_class__",
            "frozenset",
            "set",
            "dict",
            "RuntimeError",
            "str",
        ],
        &globals,
    )?;
    for (name, original) in [
        ("FakeTelegram", "LiveSetsTelegram"),
        ("FakeSession", "FakeSession"),
        ("_png", "_png"),
        ("_sticker", "_sticker"),
        ("_provider_top_up", "_provider_top_up"),
    ] {
        bindings.set_item(name, required(&fakes, original)?)?;
    }
    bindings.set_item("np", np)?;
    bindings.set_item("base64", py.import("base64")?)?;
    bindings.set_item("remap_ids", &remap)?;
    bindings.set_item("os", command.getattr("os")?)?;
    bindings.set_item("LockBusy", required(bindings, "ps")?.getattr("LockBusy")?)?;
    // These original tests patch the private dict object, never an import string.
    bindings.set_item("remap_mock", mock)?;
    bindings.set_item("remap_refusal", refusal)?;
    Ok(remap)
}
