//! Shared unchanged historical static identity assembly; callers admit source identities first.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::prelude::*;
use pyo3::types::PyDict;

pub(crate) struct Modules<'py> {
    pub globals: Bound<'py, PyDict>,
    pub identity: Bound<'py, PyAny>,
    pub media: Bound<'py, PyAny>,
    pub log: Bound<'py, PyAny>,
}
pub(crate) fn build<'py>(
    py: Python<'py>,
    media: &Source,
    identity: &Source,
) -> PyResult<Modules<'py>> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let image = py.import("PIL.Image")?;
    if image.getattr("__version__")?.extract::<String>()? != "12.3.0" {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "original image identity requires Pillow12.3.0",
        ));
    }
    let log = py
        .import("logging")?
        .getattr("Logger")?
        .call1(("original-image-identity",))?;
    log.setattr("disabled", true)?;
    let original_media = crate::source_static_oracle::media_namespace(py, media)?;
    let media_api = ns.call0()?;
    for name in ["to_static_png", "fit_100", "is_blank_image", "SIZE"] {
        media_api.setattr(name, required(&original_media, name)?)?;
    }
    let formats = crate::source_oracle_runtime::namespace(
        py,
        media,
        &["telegram_sticker_format"],
        &[],
        &PyDict::new(py),
    )?;
    media_api.setattr(
        "telegram_sticker_format",
        required(&formats, "telegram_sticker_format")?,
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("Image", image)?;
    bindings.set_item("ImageChops", py.import("PIL.ImageChops")?)?;
    bindings.set_item("ImageStat", py.import("PIL.ImageStat")?)?;
    bindings.set_item("hashlib", py.import("hashlib")?)?;
    bindings.set_item("json", py.import("json")?)?;
    bindings.set_item("media", &media_api)?;
    bindings.set_item("log", &log)?;
    for name in ["UPLOAD_PHASH_TOLERANCE", "UPLOAD_MEAN_DELTA"] {
        bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, identity, name, false)?,
        )?;
    }
    let globals = crate::source_oracle_runtime::namespace(
        py,
        identity,
        &[
            "_norm_pixels",
            "_dhash",
            "fingerprint",
            "content_key",
            "perceptual_hash",
            "collision_key",
            "_premultiply",
            "_premultiplied",
            "same_image",
            "hamming",
        ],
        &["range", "sum", "Exception", "OSError", "ValueError"],
        &bindings,
    )?;
    let identity_api = ns.call0()?;
    for name in [
        "fingerprint",
        "content_key",
        "perceptual_hash",
        "collision_key",
        "same_image",
        "hamming",
    ] {
        identity_api.setattr(name, required(&globals, name)?)?;
    }
    Ok(Modules {
        globals,
        identity: identity_api,
        media: media_api,
        log,
    })
}
