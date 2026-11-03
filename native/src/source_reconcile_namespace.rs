//! Fixed original reconciliation dependency graph; external libraries, never native identity answers.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::path::Path;

fn api<'py>(
    py: Python<'py>,
    globals: &Bound<'py, PyDict>,
    names: &[&str],
) -> PyResult<Bound<'py, PyAny>> {
    let api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    for name in names {
        api.setattr(*name, required(globals, name)?)?;
    }
    Ok(api)
}

pub(crate) fn build<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
) -> PyResult<Bound<'py, PyDict>> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let path = py.import("pathlib")?.getattr("Path")?;
    let images = crate::source_image_identity::build(py, sources[4], sources[5])?;
    let image = required(&images.globals, "Image")?;
    let log = images.log;
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[13],
        &["MediaError"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let media_api = images.media;
    let identity_api = images.identity;
    let near = crate::source_oracle_runtime::namespace(
        py,
        sources[9],
        &["near_indices"],
        &["enumerate"],
        &PyDict::new(py),
    )?;
    let ingest_bindings = PyDict::new(py);
    ingest_bindings.set_item("Path", &path)?;
    ingest_bindings.set_item("identity", &identity_api)?;
    ingest_bindings.set_item("near_indices", required(&near, "near_indices")?)?;
    ingest_bindings.set_item("MediaError", required(&errors, "MediaError")?)?;
    let ingest = crate::source_oracle_runtime::namespace(
        py,
        sources[8],
        &["catalog_identity", "equivalent_files"],
        &["set", "enumerate", "len", "OSError"],
        &ingest_bindings,
    )?;
    let mock = py.import("unittest.mock")?;
    let none = PyDict::new(py);
    none.set_item("return_value", py.None())?;
    let context = PyDict::new(py);
    context.set_item(
        "return_value",
        py.import("contextlib")?.getattr("nullcontext")?.call0()?,
    )?;
    let ports = PyDict::new(py);
    ports.set_item(
        "require_native",
        mock.getattr("Mock")?.call((), Some(&none))?,
    )?;
    ports.set_item("writer", mock.getattr("Mock")?.call((), Some(&context))?)?;
    ports.set_item("catalog_identity", required(&ingest, "catalog_identity")?)?;
    ports.set_item("log", &log)?;
    let catalog = crate::source_catalog_oracle::namespace_with_ports(
        py, sources[6], sources[7], workspace, &ports,
    )?;
    let protocol_bindings = PyDict::new(py);
    protocol_bindings.set_item("Enum", py.import("enum")?.getattr("Enum")?)?;
    let protocol = crate::source_oracle_runtime::namespace(
        py,
        sources[12],
        &[
            "SetState",
            "LiveStateUnknown",
            "BotApiError",
            "AmbiguousUploadError",
        ],
        &["__build_class__", "RuntimeError"],
        &protocol_bindings,
    )?;
    let state_bindings = PyDict::new(py);
    state_bindings.set_item("Path", &path)?;
    let state = crate::source_oracle_runtime::namespace(
        py,
        sources[10],
        &["SetDrift", "_state_path"],
        &["__build_class__", "Exception"],
        &state_bindings,
    )?;
    let bindings = PyDict::new(py);
    for (name, value) in [
        ("Path", path.clone()),
        ("contextlib", py.import("contextlib")?.into_any()),
        ("tempfile", py.import("tempfile")?.into_any()),
        ("os", py.import("os")?.into_any()),
        ("log", log),
        ("identity", identity_api.clone()),
        ("media", media_api),
        ("SetState", required(&protocol, "SetState")?),
        ("LiveStateUnknown", required(&protocol, "LiveStateUnknown")?),
        ("SetDrift", required(&state, "SetDrift")?),
        ("_state_path", required(&state, "_state_path")?),
        ("Catalog", required(&catalog, "Catalog")?),
    ] {
        bindings.set_item(name, value)?;
    }
    // Only synthetic fixture text reaches this disclosed diagnostic port.
    bindings.set_item("redact", py.import("builtins")?.getattr("str")?)?;
    bindings.set_item("SEARCH_PHASH_TOLERANCE", 2)?;
    bindings.set_item("AMBIGUOUS", "ambiguous")?;
    bindings.set_item("UNDECIDABLE", "undecidable")?;
    bindings.set_item(
        "_PERMANENT_FILE_REJECTIONS",
        crate::source_logging_oracle::literal(py, sources[0], "_PERMANENT_FILE_REJECTIONS", false)?,
    )?;
    let helpers = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &[
            "_probe",
            "_manifest_mismatch",
            "_set_is_open",
            "Unresolvable",
            "_private_download",
            "_near_catalog_match",
            "_resolve_sticker_key",
            "_identity",
            "_live_index",
            "_same_image",
            "_confirm_new_upload",
            "_file_is_permanently_rejected",
            "reconcile_set",
        ],
        &[
            "__build_class__",
            "Exception",
            "getattr",
            "len",
            "str",
            "enumerate",
            "hasattr",
            "any",
        ],
        &bindings,
    )?;
    let reconcile = PyModule::new(py, "_original_reconcile")?;
    reconcile.dict().update(helpers.as_mapping())?;
    // Patchable original identity/media objects are shared by both dictionaries. The
    // contextmanager's stdlib wrapper keeps its original closure and globals intact.
    let record_bindings = PyDict::new(py);
    for (key, value) in helpers.iter() {
        let name: String = key.extract()?;
        if !["__builtins__", "__name__"].contains(&name.as_str()) {
            record_bindings.set_item(key, value)?;
        }
    }
    let records = crate::source_oracle_runtime::namespace(
        py,
        sources[11],
        &["_record_cids"],
        &["len", "str", "enumerate"],
        &record_bindings,
    )?;
    let build = ns.call0()?;
    build.setattr("_record_cids", required(&records, "_record_cids")?)?;
    let images = PyDict::new(py);
    images.set_item("Image", &image)?;
    let png = crate::source_oracle_runtime::namespace(py, sources[3], &["make_png"], &[], &images)?;
    let fixture_bindings = PyDict::new(py);
    fixture_bindings.set_item("Path", &path)?;
    fixture_bindings.set_item("Catalog", required(&catalog, "Catalog")?)?;
    fixture_bindings.set_item("identity", identity_api)?;
    fixture_bindings.set_item("_make_png", required(&png, "make_png")?)?;
    fixture_bindings.set_item("SetState", required(&protocol, "SetState")?)?;
    fixture_bindings.set_item("SET", "pks1_by_YourEmojiBot")?;
    fixture_bindings.set_item("SET2", "pks2_by_YourEmojiBot")?;
    let tg_api = ns.call0()?;
    tg_api.setattr("BotApiError", required(&protocol, "BotApiError")?)?;
    fixture_bindings.set_item("tg_api", tg_api)?;
    let fakes = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["FakeTG", "DownloadingTG", "_sticker"],
        &[
            "__build_class__",
            "dict",
            "set",
            "str",
            "sum",
            "len",
            "super",
            "RuntimeError",
        ],
        &fixture_bindings,
    )?;
    let tests = PyDict::new(py);
    tests.set_item("fixture_bindings", fixture_bindings)?;
    tests.set_item("fakes", &fakes)?;
    tests.set_item("Catalog", required(&catalog, "Catalog")?)?;
    tests.set_item("row_to_item", required(&catalog, "_row_to_item")?)?;
    tests.set_item("identity", reconcile.getattr("identity")?)?;
    tests.set_item("cr", reconcile)?;
    tests.set_item("protocol", &protocol)?;
    tests.set_item(
        "log",
        ports
            .get_item("log")?
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("original logger missing"))?,
    )?;
    tests.set_item("bc", build)?;
    tests.set_item("cs", api(py, &state, &["SetDrift"])?)?;
    tests.set_item("LiveStateUnknown", required(&protocol, "LiveStateUnknown")?)?;
    Ok(tests)
}
