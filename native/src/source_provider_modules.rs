//! Private original provider dependencies; identity admission belongs to the fixed assertion cohort.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::Path;

#[path = "source_provider_commands.rs"]
mod commands;

#[pyfunction]
fn no_wait(seconds: f64) -> PyResult<()> {
    if seconds == 0.3 {
        Ok(())
    } else {
        Err(PyValueError::new_err("unexpected original provider wait"))
    }
}

pub(crate) fn build<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
    temporary: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let bindings = PyDict::new(py);
    let path = py.import("pathlib")?.getattr("Path")?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let image = py.import("PIL.Image")?;
    if image.getattr("__version__")?.extract::<String>()? != "12.3.0" {
        return Err(PyValueError::new_err(
            "provider assertions require Pillow12.3.0",
        ));
    }
    for (name, value) in [
        ("Path", path.clone()),
        ("Image", image.into_any()),
        ("ImageChops", py.import("PIL.ImageChops")?.into_any()),
        ("io", py.import("io")?.into_any()),
    ] {
        bindings.set_item(name, value)?;
    }
    let identity = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["_dhash", "hamming"],
        &["range"],
        &bindings,
    )?;
    for name in ["_dhash", "hamming"] {
        bindings.set_item(name, required(&identity, name)?)?;
    }
    bindings.set_item("Enum", py.import("enum")?.getattr("Enum")?)?;
    let protocol = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["SetState", "LiveStateUnknown", "AmbiguousUploadError"],
        &["__build_class__", "RuntimeError"],
        &bindings,
    )?;
    let tg_api = ns.call0()?;
    for name in ["SetState", "LiveStateUnknown", "AmbiguousUploadError"] {
        let value = required(&protocol, name)?;
        tg_api.setattr(name, &value)?;
        bindings.set_item(name, value)?;
    }
    let locks =
        crate::source_reference_state::namespace(py, sources[1], Some(&workspace.join(".locks")))?;
    let ps = crate::source_roster_modules::rebind(py, &locks, "_original_provider_state")?;
    for name in [
        "LockBusy",
        "canonical_map_lock",
        "exclusive_lock",
        "make_intent",
        "write_json_atomic",
    ] {
        bindings.set_item(name, ps.getattr(name)?)?;
    }
    for name in ["contextlib", "json", "os", "shutil", "csv"] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item("ROOT", path.call1((workspace,))?)?;
    let keywords = crate::source_oracle_runtime::namespace(
        py,
        sources[4],
        &["load_keywords"],
        &["open"],
        &bindings,
    )?;
    bindings.set_item("load_keywords", required(&keywords, "load_keywords")?)?;
    let clock = ns.call0()?;
    clock.setattr("sleep", wrap_pyfunction!(no_wait, py)?)?;
    bindings.set_item("time", &clock)?;
    bindings.set_item("tempfile", temporary)?;
    bindings.set_item("_RUN_TOKEN", "original-provider-run")?;
    bindings.set_item("SET_BASE", "cryptoemoji")?;
    bindings.set_item("SET_TITLE", "Title")?;
    bindings.set_item("USER_ID", 1)?;
    for name in ["INTENT_KEY", "SAME_IMAGE_MAX", "EMOJI_CHAR", "PER_SET"] {
        bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[0], name, false)?,
        )?;
    }
    for name in ["EMOJI", "STATE", "TICKER_IDS", "KEYWORDS_CSV", "PACK_LOCK"] {
        bindings.set_item(
            name,
            path.call1((workspace.join(format!("unpatched-{name}")),))?,
        )?;
    }
    let publisher = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &[
            "live_stickers",
            "incoming_dir",
            "staged_or_published",
            "publish_source",
            "discard_staging",
            "source_dhash",
            "_content_matches",
            "_landed_emoji_id",
            "_mutate",
            "_recover_in_flight",
            "_record_provider_add",
            "publish_logos",
        ],
        &[
            "str",
            "int",
            "len",
            "list",
            "dict",
            "sorted",
            "set",
            "enumerate",
            "isinstance",
            "print",
            "Exception",
            "RuntimeError",
            "OSError",
        ],
        &bindings,
    )?;
    let publisher =
        crate::source_roster_modules::rebind(py, &publisher, "_original_provider_publisher")?;
    bindings.set_item("tg_api", &tg_api)?;
    bindings.set_item("SET", "cryptoemoji1_by_bot")?;
    let fakes = crate::source_oracle_runtime::namespace(
        py,
        sources[5],
        &["_png_bytes", "_gradient", "FakeTelegram"],
        &[
            "__build_class__",
            "range",
            "dict",
            "set",
            "str",
            "RuntimeError",
        ],
        &bindings,
    )?;
    for name in ["_png_bytes", "_gradient", "FakeTelegram"] {
        bindings.set_item(name, required(&fakes, name)?)?;
    }
    let (_, converter) = crate::source_static_oracle::namespaces(py, sources[8], sources[9])?;
    let fp = ns.call0()?;
    fp.setattr("to_emoji_png", required(&converter, "to_emoji_png")?)?;
    let cfg = crate::source_oracle_runtime::namespace(
        py,
        sources[10],
        &["_count", "_order_walks_plan", "_state_problem"],
        &[
            "isinstance",
            "int",
            "bool",
            "dict",
            "list",
            "str",
            "all",
            "sorted",
            "set",
            "len",
            "enumerate",
            "iter",
        ],
        &bindings,
    )?;
    let cfg = crate::source_roster_modules::rebind(py, &cfg, "_original_provider_rebuild")?;
    cfg.setattr(
        "LOCK",
        path.call1((workspace.join("unpatched-rebuild.lock"),))?,
    )?;
    bindings.set_item("ps", ps)?;
    bindings.set_item("pp", &publisher)?;
    bindings.set_item("fp", fp)?;
    bindings.set_item("cfg", cfg)?;
    bindings.set_item("unittest", py.import("unittest")?)?;
    bindings.set_item("mock", py.import("unittest.mock")?)?;
    bindings.set_item("clock", clock)?;
    commands::augment(py, workspace, sources, &bindings)?;
    Ok(bindings)
}
