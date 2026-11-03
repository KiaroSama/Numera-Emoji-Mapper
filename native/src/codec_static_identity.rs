//! Static metadata and identity preserve typed external errors; callers choose their reporting boundary.
use crate::codec_error::CodecError;
use crate::codec_raster::{
    OpenImage, blank_rgba, dhash, dhash_checked, digest_hex, normalized_pixels, normalized_rgba,
    pixels,
};
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::path::Path;

fn inspect(py: Python<'_>, path: &Path) -> PyResult<Value> {
    let image_module = py.import("PIL.Image")?;
    let image = OpenImage(image_module.call_method1("open", (path.to_path_buf(),))?);
    let mode = image.0.getattr("mode")?.extract::<String>()?;
    let size = image.0.getattr("size")?.extract::<(u32, u32)>()?;
    let rgba = OpenImage(image.0.call_method1("convert", ("RGBA",))?);
    let blank = blank_rgba(&pixels(&rgba.0)?);
    let normalized = normalized_rgba(py, &rgba.0)?;
    let key = format!("s:{}", &digest_hex(&pixels(&normalized.0)?)[..32]);
    let phash = match dhash(py, &image.0) {
        Ok(hash) => Some(hash),
        Err(error) if error.is_instance_of::<pyo3::exceptions::PyException>(py) => None,
        Err(error) => return Err(error),
    };
    Ok(json!({"size":[size.0,size.1],"mode":mode,"blank":blank,"content_key":key,"phash":phash}))
}

pub fn metadata(path: &Path, include_blank: bool) -> Result<Value, CodecError> {
    metadata_with(path, None, include_blank)
}

pub(crate) fn metadata_operation(path: &Path, operation: &str) -> Result<Value, CodecError> {
    metadata_with(path, Some(operation), operation == "image_info")
}

fn metadata_with(
    path: &Path,
    operation: Option<&str>,
    include_blank: bool,
) -> Result<Value, CodecError> {
    Python::initialize();
    let result = Python::attach(|py| -> PyResult<Option<Value>> {
        let image = py
            .import("PIL.Image")?
            .call_method1("open", (path.to_path_buf(),))?;
        crate::codec_raster::with_image_context(image, |image| {
            // Generic usability does not inspect metadata before the source blank-image predicate.
            if operation == Some("usable") {
                return Ok(crate::codec_raster::blank_image_external(py, image)?
                    .map(|blank| json!(!blank)));
            }
            image.call_method0("load")?;
            let size = image.getattr("size")?.extract::<(u32, u32)>()?;
            if operation == Some("dimensions") {
                return Ok(Some(json!([size.0, size.1])));
            }
            let mode = image.getattr("mode")?.extract::<String>()?;
            let mut result = json!({"size":[size.0,size.1],"mode":mode});
            if include_blank {
                let Some(blank) = crate::codec_raster::blank_image_external(py, image)? else {
                    return Ok(None);
                };
                result["blank"] = json!(blank);
            }
            Ok(Some(result))
        })
    })
    .map_err(|error| Python::attach(|py| CodecError::from_python(py, error)))?;
    result.ok_or_else(|| CodecError::unverified("native alpha histogram is unverified"))
}

fn classify(py: Python<'_>, error: PyErr) -> (CodecError, bool) {
    let caught = error.is_instance_of::<pyo3::exceptions::PyException>(py);
    (CodecError::from_python(py, error), caught)
}

pub(crate) fn content_key_classified(
    py: Python<'_>,
    path: &Path,
) -> Result<String, (CodecError, bool)> {
    let image = OpenImage(
        py.import("PIL.Image")
            .and_then(|module| module.call_method1("open", (path.to_path_buf(),)))
            .map_err(|error| classify(py, error))?,
    );
    content_key_image(py, &image.0)
}

fn content_key_image<'py>(
    py: Python<'py>,
    image: &Bound<'py, PyAny>,
) -> Result<String, (CodecError, bool)> {
    let pixels = normalized_pixels(py, image).map_err(|error| classify(py, error))?;
    if pixels.len() != 64 * 64 * 4 {
        return Err((
            CodecError::unverified("native normalized raster is incomplete"),
            false,
        ));
    }
    Ok(format!("s:{}", &digest_hex(&pixels)[..32]))
}

pub(crate) fn perceptual_hash_classified(
    py: Python<'_>,
    path: &Path,
) -> Result<Option<u64>, (CodecError, bool)> {
    let image = match py
        .import("PIL.Image")
        .and_then(|module| module.call_method1("open", (path.to_path_buf(),)))
    {
        Ok(image) => OpenImage(image),
        Err(error) if error.is_instance_of::<pyo3::exceptions::PyException>(py) => return Ok(None),
        Err(error) => return Err(classify(py, error)),
    };
    // The source catches open failures, not _dhash failures: same_image owns that outer catch.
    dhash_checked(py, &image.0)
        .map_err(|error| classify(py, error))?
        .map(Some)
        .ok_or_else(|| {
            (
                CodecError::unverified("native dHash raster is incomplete"),
                false,
            )
        })
}

pub(crate) fn validate_identity_source(path: &Path) -> Result<(), CodecError> {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    {
        return Err(CodecError::unverified(
            "static codec source must be an absolute regular non-symlink file",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if !std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.file_attributes() & 0x400 == 0)
        {
            return Err(CodecError::unverified(
                "static codec source must not be a reparse point",
            ));
        }
    }
    Ok(())
}

/// The source's one-pass key/hash contract, without metadata probes in its exception boundary.
pub fn fingerprint_typed(path: &Path) -> Result<Value, CodecError> {
    validate_identity_source(path)?;
    Python::initialize();
    Python::attach(|py| {
        let initial = (|| -> Result<_, (CodecError, bool)> {
            let image = OpenImage(
                py.import("PIL.Image")
                    .and_then(|module| module.call_method1("open", (path.to_path_buf(),)))
                    .map_err(|error| classify(py, error))?,
            );
            let key = content_key_image(py, &image.0)?;
            Ok((image, key))
        })();
        let (image, key) = match initial {
            Ok(value) => value,
            Err((_, true)) => {
                let key = content_key_classified(py, path).map_err(|(error, _)| error)?;
                return Ok(json!({"content_key":key,"phash":null}));
            }
            Err((error, false)) => return Err(error),
        };
        let phash = match dhash_checked(py, &image.0) {
            Ok(Some(hash)) => Some(hash),
            Ok(None) => return Err(CodecError::unverified("native dHash raster is incomplete")),
            Err(error) if error.is_instance_of::<pyo3::exceptions::PyException>(py) => None,
            Err(error) => return Err(CodecError::from_python(py, error)),
        };
        Ok(json!({"content_key":key,"phash":phash}))
    })
}

pub fn fingerprint(path: &Path) -> Result<Value, CodecError> {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    {
        return Err(CodecError::value(
            "static codec source must be an absolute regular non-symlink file",
        ));
    }
    Python::initialize();
    Python::attach(|py| inspect(py, path).map_err(|error| CodecError::from_python(py, error)))
}

pub fn inspect_legacy(path: &Path) -> Result<Value, String> {
    fingerprint(path).map_err(|error| {
        if error.kind == "ValueError"
            && error.message == "static codec source must be an absolute regular non-symlink file"
        {
            error.message
        } else {
            format!("external static codec failed: {}", error.kind)
        }
    })
}
