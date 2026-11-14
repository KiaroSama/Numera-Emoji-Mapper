//! Fixed original opacity assertions; neither native fitting nor target pixels supply the oracle.
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::path::Path;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 2] = [
    (
        "emojikit/media.py",
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
    ),
    (
        "tests/test_media_fitting.py",
        "31a01ba3b0e3311fd27d29fb27ddf9b59bc5e30a1b39b5fe3e48eb9c4e6046aa",
    ),
];
const METHODS: [&str; 6] = [
    "test_a_partly_transparent_pixel_keeps_its_colour_and_its_alpha",
    "test_opaque_art_is_the_control_that_always_passed",
    "test_a_resized_image_keeps_its_opacity",
    "test_a_cutout_stays_a_cutout",
    "test_a_transparent_border_is_trimmed_not_composited",
    "test_a_partial_alpha_gradient_survives_end_to_end",
];
const EXCLUDED: &str = "test_every_fitting_entry_point_shares_one_implementation";

pub fn source_identity(media: &Source, test: &Source) -> Result<(), String> {
    for (source, (path, expected)) in [media, test].into_iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != expected
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != expected
        {
            return Err("fitting oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn execute(py: Python<'_>, workspace: &Path, media: &Source, test: &Source) -> PyResult<Value> {
    let image = py.import("PIL.Image")?;
    let pillow: String = image.getattr("__version__")?.extract()?;
    if pillow != "12.3.0" {
        return Err(PyValueError::new_err(
            "fitting oracle requires Pillow12.3.0",
        ));
    }
    let original = crate::source_static_oracle::media_namespace(py, media)?;
    let module = pyo3::types::PyModule::new(py, "_original_fitting_media")?;
    module.dict().update(original.as_mapping())?;
    let options = PyDict::new(py);
    options.set_item("dir", workspace)?;
    let temporary = py.import("functools")?.getattr("partial")?.call(
        (py.import("tempfile")?.getattr("TemporaryDirectory")?,),
        Some(&options),
    )?;
    let temporary_api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    temporary_api.setattr("TemporaryDirectory", temporary)?;
    let unittest = py.import("unittest")?;
    let bindings = PyDict::new(py);
    bindings.set_item("unittest", &unittest)?;
    bindings.set_item("Image", image)?;
    bindings.set_item("media", module)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("tempfile", temporary_api)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &[
            "FittingPreservesOpacity",
            "CompositingOntoAnOpaqueBackdropIsStillCorrect",
        ],
        &["__build_class__", "enumerate", "range", "max"],
        &bindings,
    )?;
    let class = tests
        .get_item("FittingPreservesOpacity")?
        .ok_or_else(|| PyValueError::new_err("missing original fitting class"))?;
    let collected = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract::<Vec<String>>()?;
    let mut expected = METHODS.to_vec();
    expected.push(EXCLUDED);
    expected.sort_unstable();
    if collected != expected {
        return Err(PyValueError::new_err("original fitting collection drift"));
    }
    let backdrop = tests
        .get_item("CompositingOntoAnOpaqueBackdropIsStillCorrect")?
        .ok_or_else(|| PyValueError::new_err("missing original backdrop class"))?;
    let backdrop_method = "test_flattening_onto_white_still_blends";
    let backdrop_names = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&backdrop,))?
        .extract::<Vec<String>>()?;
    if backdrop_names != [backdrop_method] {
        return Err(PyValueError::new_err("original backdrop collection drift"));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    for method in METHODS {
        class.call1((method,))?.call_method1("run", (&result,))?;
    }
    backdrop
        .call1((backdrop_method,))?
        .call_method1("run", (&result,))?;
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyValueError::new_err(format!(
                "original fitting assertions reported {field}: {}; scratch retained",
                crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != METHODS.len() + 1
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original fitting execution incomplete",
        ));
    }
    let mut cases = METHODS
        .map(|name| format!("FittingPreservesOpacity.{name}"))
        .to_vec();
    cases.push(format!(
        "CompositingOntoAnOpaqueBackdropIsStillCorrect.{backdrop_method}"
    ));
    Ok(json!({
        "source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":pillow,
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "cases":cases,
        "tests_run":METHODS.len()+1,"errors":0,"failures":0,"skipped":0,
        "excluded":[format!("FittingPreservesOpacity.{EXCLUDED}")],
        "coverage":"seven unchanged original fitting/backdrop methods and helper; original media definitions; private module and project-confined tempfile ports; shared-import identity not executed"
    }))
}

pub fn observe(
    root: &Path,
    workspace: &Path,
    media: &Source,
    test: &Source,
) -> Result<Value, String> {
    source_identity(media, test)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("fitting oracle workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect fitting workspace")?
        .next()
        .is_some()
    {
        return Err("fitting oracle workspace must be empty".into());
    }
    Python::initialize();
    let report = Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        execute(py, &workspace, media, test).map_err(|error| {
            format!(
                "original fitting replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })?;
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot verify fitting cleanup")?
        .next()
        .is_some()
    {
        return Err("original fitting tests left scratch; retained for diagnosis".into());
    }
    Ok(report)
}
