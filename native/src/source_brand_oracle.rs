//! Replay the admitted original brand-cache assertions, never native-generated expectations.
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::path::Path;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 5] = [
    (
        "emojikit/media.py",
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
    ),
    (
        "tests/reference/collection_state.py",
        "b8bf2695954cbe61a3e3cb68c8da0c583aa80a1b645abba01eabe9a28776074a",
    ),
    (
        "tests/test_brand_logo.py",
        "b2b371af99a0fce633f1f7ac8af2d7b31d34461f13a450cd2bdaf11b4ac3d358",
    ),
    (
        "tests/_media_fixtures.py",
        "1c45ebbb66c1d06bab2fb897e1c24166646974cf02e6fd0d8b54c93c1df8f5e4",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
];
const METHODS: [&str; 6] = [
    "test_static_logo_is_100x100_png",
    "test_static_png_is_cached",
    "test_missing_source_returns_none",
    "test_a_corrupt_logo_raises",
    "test_a_blank_logo_raises",
    "test_a_truncated_cache_is_rendered_again",
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("brand oracle requires exactly five pinned sources".into());
    }
    for (source, (path, expected)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != expected
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != expected
        {
            return Err("brand oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let image = py.import("PIL.Image")?;
    let pillow: String = image.getattr("__version__")?.extract()?;
    if pillow != "12.3.0" {
        return Err(PyValueError::new_err("brand oracle requires Pillow12.3.0"));
    }
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[4],
        &["MediaError"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let media = crate::source_static_oracle::media_namespace(py, sources[0])?;
    media.set_item(
        "MediaError",
        errors
            .get_item("MediaError")?
            .ok_or_else(|| PyValueError::new_err("missing original MediaError"))?,
    )?;
    media.set_item("SIZE", 100)?;
    let module = pyo3::types::PyModule::new(py, "_original_brand_media")?;
    module.dict().update(media.as_mapping())?;
    let path = py.import("pathlib")?.getattr("Path")?;
    let bindings = PyDict::new(py);
    bindings.set_item("media", &module)?;
    bindings.set_item("Image", &image)?;
    bindings.set_item("Path", &path)?;
    bindings.set_item("hashlib", py.import("hashlib")?)?;
    bindings.set_item("os", py.import("os")?)?;
    let collection = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["_usable_logo", "BrandLogo"],
        &["__build_class__", "bool", "str", "OSError", "ValueError"],
        &bindings,
    )?;
    let collection_module = pyo3::types::PyModule::new(py, "_original_brand_collection")?;
    collection_module.dict().update(collection.as_mapping())?;
    let fixtures =
        crate::source_oracle_runtime::namespace(py, sources[3], &["make_png"], &[], &bindings)?;
    let options = PyDict::new(py);
    options.set_item("dir", workspace)?;
    let temporary = py.import("functools")?.getattr("partial")?.call(
        (py.import("tempfile")?.getattr("TemporaryDirectory")?,),
        Some(&options),
    )?;
    let temporary_api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    temporary_api.setattr("TemporaryDirectory", temporary)?;
    let unittest = py.import("unittest")?;
    bindings.set_item("unittest", &unittest)?;
    bindings.set_item("tempfile", temporary_api)?;
    bindings.set_item("cs", collection_module)?;
    bindings.set_item(
        "make_png",
        fixtures
            .get_item("make_png")?
            .ok_or_else(|| PyValueError::new_err("missing original PNG fixture"))?,
    )?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["_make_png", "BrandLogoPrepare"],
        &["__build_class__", "str", "OSError"],
        &bindings,
    )?;
    let class = tests
        .get_item("BrandLogoPrepare")?
        .ok_or_else(|| PyValueError::new_err("missing original brand class"))?;
    let collected = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract::<Vec<String>>()?;
    let mut expected = METHODS.to_vec();
    expected.sort_unstable();
    if collected != expected {
        return Err(PyValueError::new_err("original brand collection drift"));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    for method in METHODS {
        class.call1((method,))?.call_method1("run", (&result,))?;
    }
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyValueError::new_err(format!(
                "original brand assertions reported {field}; scratch retained"
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != METHODS.len()
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err("original brand execution incomplete"));
    }
    Ok(json!({
        "source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":pillow,
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "cases":METHODS.map(|method|format!("BrandLogoPrepare.{method}")),
        "tests_run":METHODS.len(),"errors":0,"failures":0,"skipped":0,
        "coverage":"six unchanged original brand-preparation methods/setup/teardown and PNG builders; original media, MediaError, BrandLogo and cache validator; private modules and project-confined tempfile port; publisher/import-chain and native lifetimes not covered"
    }))
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("brand oracle workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect brand workspace")?
        .next()
        .is_some()
    {
        return Err("brand oracle workspace must be empty".into());
    }
    Python::initialize();
    let report = Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original brand replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })?;
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot verify brand cleanup")?
        .next()
        .is_some()
    {
        return Err("original brand tests left scratch; retained for diagnosis".into());
    }
    Ok(report)
}
