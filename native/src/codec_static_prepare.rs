//! In-place static reencoding, tinting and their ordered prepare contract, using unchanged Pillow APIs.
use crate::codec_error::CodecError;
use crate::codec_raster::OpenImage;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::{Value, json};
use std::path::Path;

fn lossless_webp(
    py: Python<'_>,
    path: &Path,
    tint: Option<[u8; 3]>,
    snapshot: Option<&[u8]>,
) -> PyResult<Vec<u8>> {
    let module = py.import("PIL.Image")?;
    let input = snapshot
        .map(|bytes| {
            py.import("io")?
                .call_method1("BytesIO", (PyBytes::new(py, bytes),))
                .map(OpenImage)
        })
        .transpose()?;
    let source = if let Some(input) = &input {
        module.call_method1("open", (&input.0,))?
    } else {
        module.call_method1("open", (path.to_path_buf(),))?
    };
    let rgba = crate::codec_raster::with_image_context(source, |image| {
        image.call_method1("convert", ("RGBA",)).map(OpenImage)
    })?;
    drop(input);
    let flat = if let Some([r, g, b]) = tint {
        let size = rgba.0.getattr("size")?;
        let flat = OpenImage(module.call_method1("new", ("RGBA", size, (r, g, b, 255)))?);
        let putalpha = flat.0.getattr("putalpha")?;
        let alpha = OpenImage(rgba.0.call_method1("getchannel", ("A",))?);
        putalpha.call1((&alpha.0,))?;
        flat
    } else {
        rgba
    };
    let buffer = OpenImage(py.import("io")?.call_method0("BytesIO")?);
    let options = PyDict::new(py);
    options.set_item("format", "WEBP")?;
    options.set_item("lossless", true)?;
    options.set_item("quality", 100)?;
    options.set_item("method", 6)?;
    options.set_item("exact", true)?;
    flat.0.call_method("save", (&buffer.0,), Some(&options))?;
    Ok(buffer
        .0
        .call_method0("getvalue")?
        .cast_into::<PyBytes>()?
        .as_bytes()
        .to_vec())
}

pub struct StaticReencode {
    pub rewritten: bool,
    pub warning: Option<String>,
}

fn source(path: &Path, operation: &str) -> Result<(), CodecError> {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    {
        return Err(CodecError::value(format!(
            "static {operation} source must be an absolute regular non-symlink file"
        )));
    }
    Ok(())
}

fn encoded(
    path: &Path,
    tint: Option<[u8; 3]>,
    snapshot: Option<&[u8]>,
) -> Result<Result<Vec<u8>, CodecError>, CodecError> {
    Python::initialize();
    Python::attach(|py| match lossless_webp(py, path, tint, snapshot) {
        Ok(bytes) => Ok(Ok(bytes)),
        Err(error) => {
            let caught = error.is_instance_of::<pyo3::exceptions::PyException>(py);
            let error = CodecError::from_python(py, error);
            if caught { Ok(Err(error)) } else { Err(error) }
        }
    })
}

fn skipped(path: &Path, action: &str, error: CodecError) -> Result<StaticReencode, CodecError> {
    Ok(StaticReencode {
        rewritten: false,
        warning: Some(crate::logging::redact(
            &format!(
                "{action} skipped for {} (static): {}",
                path.file_name()
                    .ok_or("static source has no filename")?
                    .to_string_lossy(),
                error.message
            ),
            &[],
        )),
    })
}

pub fn reencode_static_typed(path: &Path) -> Result<StaticReencode, CodecError> {
    reencode_with(path, true)
}

fn reencode_with(path: &Path, source_read: bool) -> Result<StaticReencode, CodecError> {
    source(path, "reencode")?;
    // The initial source read is outside the optional encoding failure catch.
    let before = if source_read {
        Python::initialize();
        Python::attach(|py| {
            crate::codec_lottie_load::source_bytes(py, path)
                .map_err(|error| CodecError::from_python(py, error))
        })?
    } else {
        std::fs::read(path)
            .map_err(|_| CodecError::unverified("cannot read static reencode source"))?
    };
    let bytes = match encoded(path, None, Some(&before))? {
        Ok(bytes) => bytes,
        Err(error) => return skipped(path, "re-encode", error),
    };
    if bytes == before {
        return Ok(StaticReencode {
            rewritten: false,
            warning: None,
        });
    }
    crate::atomic::write_bytes(path, &bytes)
        .map_err(|_| CodecError::unverified("cannot replace static reencode source"))?;
    Ok(StaticReencode {
        rewritten: true,
        warning: None,
    })
}

pub fn repaint_static_typed(path: &Path, rgb: [u8; 3]) -> Result<StaticReencode, CodecError> {
    source(path, "tint")?;
    let bytes = match encoded(path, Some(rgb), None)? {
        Ok(bytes) => bytes,
        Err(error) => return skipped(path, "repaint", error),
    };
    crate::atomic::write_bytes(path, &bytes)
        .map_err(|_| CodecError::unverified("cannot replace static tint source"))?;
    Ok(StaticReencode {
        rewritten: true,
        warning: None,
    })
}

fn legacy(outcome: Result<StaticReencode, CodecError>) -> Result<bool, String> {
    let outcome = outcome.map_err(|error| error.message)?;
    if let Some(warning) = outcome.warning {
        eprintln!("WARNING: {}", crate::logging::redact(&warning, &[]));
    }
    Ok(outcome.rewritten)
}
pub fn reencode_static(path: &Path) -> Result<bool, String> {
    legacy(reencode_with(path, false))
}
pub fn repaint_static(path: &Path, rgb: [u8; 3]) -> Result<bool, String> {
    legacy(repaint_static_typed(path, rgb))
}

pub fn prepare(
    source: &Path,
    request: &Value,
    warnings: &mut Vec<String>,
) -> Result<Value, CodecError> {
    let reencode = request
        .get("reencode")
        .unwrap_or(&Value::Bool(false))
        .as_bool()
        .ok_or("reencode must be boolean")?;
    if reencode {
        let outcome = reencode_static_typed(source)?;
        warnings.extend(outcome.warning);
    }
    if let Some(tint) = request.get("tint") {
        let text = tint.as_str().ok_or("tint must be #RRGGBB")?;
        let rgb = crate::codec_tint::parse_tint(text).map_err(|message| CodecError {
            kind: "MediaError".into(),
            message,
            handled: true,
        })?;
        let outcome = repaint_static_typed(source, rgb)?;
        warnings.extend(outcome.warning);
        if !outcome.rewritten {
            return Err(CodecError::value("requested tint could not be applied"));
        }
    }
    let info = crate::codec_static_identity::fingerprint_typed(source)?;
    let extension = crate::codec_format::extension_typed(source, "static")?;
    Ok(
        json!({"content_key":info["content_key"],"phash":info["phash"],"fmt":"static","extension":extension}),
    )
}
