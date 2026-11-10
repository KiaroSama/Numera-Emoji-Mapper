//! Private original roster modules; admission belongs to the fixed assertion cohort.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::path::Path;

pub(crate) fn rebind<'py>(
    py: Python<'py>,
    globals: &Bound<'py, PyDict>,
    name: &str,
) -> PyResult<Bound<'py, PyModule>> {
    let module = PyModule::new(py, name)?;
    module.dict().update(globals.as_mapping())?;
    let function_type = py.import("types")?.getattr("FunctionType")?;
    for (key, value) in globals.iter() {
        if value.is_instance(&function_type)? && value.getattr("__globals__")?.is(globals) {
            let rebound = function_type.call1((
                value.getattr("__code__")?,
                module.dict(),
                value.getattr("__name__")?,
                value.getattr("__defaults__")?,
                value.getattr("__closure__")?,
            ))?;
            for attribute in ["__kwdefaults__", "__annotations__"] {
                rebound.setattr(attribute, value.getattr(attribute)?)?;
            }
            if !rebound.getattr("__globals__")?.is(module.dict()) {
                return Err(PyValueError::new_err(
                    "original roster globals not patchable",
                ));
            }
            module.dict().set_item(key, rebound)?;
        }
    }
    Ok(module)
}

pub(crate) fn build<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
    environment: &Bound<'py, PyDict>,
    gallery_media: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyModule>, Bound<'py, PyModule>)> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let path = py.import("pathlib")?.getattr("Path")?;
    let gallery_bindings = crate::source_gallery_oracle::bindings(py, sources[2], sources[4])?;
    for (name, value) in [("THUMB", 88), ("THUMB_FPS", 9), ("THUMB_QUALITY", 65)] {
        gallery_bindings.set_item(name, value)?;
    }
    gallery_bindings.set_item("Image", py.import("PIL.Image")?)?;
    gallery_bindings.set_item("base64", py.import("base64")?)?;
    gallery_bindings.set_item(
        "log",
        py.import("logging")?
            .call_method1("getLogger", ("original_roster_gallery",))?,
    )?;
    gallery_bindings.set_item(
        "_MIME",
        crate::source_logging_oracle::literal(py, sources[2], "_MIME", false)?,
    )?;
    gallery_bindings.set_item("media", gallery_media)?;
    let gallery_globals = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["_thumb_file", "_still_file", "_uri", "thumb_uri", "render"],
        &["chr", "Exception"],
        &gallery_bindings,
    )?;
    let gallery = rebind(py, &gallery_globals, "_original_roster_gallery")?;

    let bindings = PyDict::new(py);
    let os = ns.call0()?;
    os.setattr("environ", environment)?;
    bindings.set_item("os", &os)?;
    let operator =
        crate::source_oracle_runtime::namespace(py, sources[5], &["value"], &[], &bindings)?;
    let api = ns.call0()?;
    api.setattr("value", required(&operator, "value")?)?;
    bindings.set_item("operator_config", api)?;
    bindings.set_item("Path", &path)?;
    bindings.set_item("DATA_PREFIX", "./")?;
    bindings.set_item("PROJECT_ROOT", path.call1((workspace,))?)?;
    let paths = crate::source_oracle_runtime::namespace(
        py,
        sources[7],
        &["store", "resolve"],
        &["str", "len"],
        &bindings,
    )?;
    let media_paths = ns.call0()?;
    media_paths.setattr("resolve", required(&paths, "resolve")?)?;
    bindings.set_item("media_paths", media_paths)?;
    let format_bindings = PyDict::new(py);
    let format = crate::source_oracle_runtime::namespace(
        py,
        sources[8],
        &["telegram_sticker_format"],
        &[],
        &format_bindings,
    )?;
    let media = ns.call0()?;
    media.setattr(
        "telegram_sticker_format",
        required(&format, "telegram_sticker_format")?,
    )?;
    bindings.set_item("media", media)?;
    bindings.set_item("ROOT", path.call1((workspace,))?)?;
    for (name, relative) in [
        ("OUT_DIR", "packs"),
        ("THUMBS", "packs/.thumbs"),
        ("DATA_DIR", "collection"),
        ("CATALOG", "collection/catalog.db"),
        ("COINS_STATE", "coins/rebuild_dedup_state.json"),
        ("TICKER_MAP", "coins/ticker_to_id.json"),
    ] {
        bindings.set_item(name, path.call1((workspace.join(relative),))?)?;
    }
    bindings.set_item("COIN_ART_ENV", "COIN_EMOJI_DIR")?;
    bindings.set_item("EXIT_OK", 0)?;
    bindings.set_item("EXIT_FAILED", 1)?;
    bindings.set_item("EXIT_STALE", 3)?;
    bindings.set_item("pack_gallery", &gallery)?;
    bindings.set_item(
        "datetime",
        py.get_type::<crate::source_intent_oracle::FixedClock>(),
    )?;
    bindings.set_item("timezone", py.import("datetime")?.getattr("timezone")?)?;
    bindings.set_item(
        "defaultdict",
        py.import("collections")?.getattr("defaultdict")?,
    )?;
    for name in ["json", "sqlite3", "argparse"] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item(
        "log",
        py.import("logging")?
            .call_method1("getLogger", ("original_roster_assertions",))?,
    )?;
    let mock = py.import("unittest.mock")?.getattr("Mock")?;
    for name in ["Telegram", "load_env", "setup_logging"] {
        let options = PyDict::new(py);
        options.set_item(
            "side_effect",
            PyValueError::new_err(format!("unpatched original roster {name}")).value(py),
        )?;
        bindings.set_item(name, mock.call((), Some(&options))?)?;
    }
    let table_bindings = PyDict::new(py);
    table_bindings.set_item("EMPTY", "—")?;
    let table = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["_cell", "markdown_table"],
        &["str", "max", "len"],
        &table_bindings,
    )?;
    bindings.set_item("markdown_table", required(&table, "markdown_table")?)?;
    let writer_bindings = PyDict::new(py);
    for name in [
        "Path",
        "contextlib",
        "json",
        "os",
        "stat",
        "tempfile",
        "time",
    ] {
        writer_bindings.set_item(
            name,
            if name == "Path" {
                path.clone()
            } else {
                py.import(name)?.into_any()
            },
        )?;
    }
    writer_bindings.set_item(
        "_REPLACE_DEADLINE",
        crate::source_logging_oracle::literal(py, sources[6], "_REPLACE_DEADLINE", false)?,
    )?;
    let writer = crate::source_oracle_runtime::namespace(
        py,
        sources[6],
        &["_replace", "write_json_atomic"],
        &["PermissionError", "FileNotFoundError", "min"],
        &writer_bindings,
    )?;
    bindings.set_item("write_json_atomic", required(&writer, "write_json_atomic")?)?;
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (&sources[0].text, &sources[0].path))?;
    bindings.set_item("__doc__", ast.call_method1("get_docstring", (tree,))?)?;
    let globals = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &[
            "general_state",
            "_sets",
            "general_provenance",
            "coin_provenance",
            "previous_ids",
            "coin_art_dir",
            "general_art",
            "_row",
            "build_pack",
            "_zero_note",
            "render_markdown",
            "render_index_markdown",
            "fingerprint",
            "check_stale",
            "refresh",
            "_family_inputs",
            "_keep_other_family",
            "_Prefetched",
            "main",
        ],
        &[
            "__build_class__",
            "str",
            "dict",
            "list",
            "bool",
            "int",
            "enumerate",
            "len",
            "OSError",
            "ValueError",
            "Exception",
            "set",
            "sorted",
            "sum",
            "print",
        ],
        &bindings,
    )?;
    Ok((
        rebind(py, &globals, "_original_roster_assertions")?,
        gallery,
    ))
}
