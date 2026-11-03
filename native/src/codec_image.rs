//! Rust-owned static identity policy using unchanged external Pillow codec APIs.
use crate::codec_raster::{OpenImage, blank_rgba, dhash, digest_hex, pixels};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "codec_image_fit.rs"]
mod fitting;
use fitting::FitPolicy;

static NEXT_OUTPUT: AtomicU64 = AtomicU64::new(0);

pub use crate::codec_raster::{
    frame_phash, frame_phash_typed, same_frame, same_frame_typed, same_image, same_image_typed,
};

pub use crate::codec_static_identity::metadata;

fn fitted_png(
    py: Python<'_>,
    path: &Path,
    source_errors: bool,
) -> Result<(Vec<u8>, Vec<u8>, bool), crate::codec_error::CodecError> {
    use crate::codec_error::CodecError;
    let external = |error| CodecError::from_python(py, error);
    let module = py.import("PIL.Image").map_err(external)?;
    let source = OpenImage(
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
        {
            if source_errors {
                crate::codec_svg::load_rgba_typed(py, path)?
            } else {
                crate::codec_svg::load_rgba(py, path).map_err(external)?
            }
        } else {
            module
                .call_method1("open", (path.to_path_buf(),))
                .map_err(external)?
        },
    );
    let rgba = OpenImage(
        source
            .0
            .call_method1("convert", ("RGBA",))
            .map_err(external)?,
    );
    let policy = if source_errors {
        FitPolicy::Source
    } else {
        FitPolicy::Legacy
    };
    let canvas = fitting::fit(py, &rgba.0, policy).map_err(external)?;
    if source_errors && crate::codec_raster::blank_image_typed(py, &canvas.0)? {
        return Ok((Vec::new(), Vec::new(), true));
    }
    let rgba = pixels(&canvas.0).map_err(external)?;
    if source_errors && rgba.len() != 100 * 100 * 4 {
        return Err(CodecError::unverified("native fitted raster is incomplete"));
    }
    if !source_errors && blank_rgba(&rgba) {
        return Ok((Vec::new(), rgba, true));
    }
    Ok((png_bytes(py, &canvas.0).map_err(external)?, rgba, false))
}

fn png_bytes(py: Python<'_>, image: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let buffer = OpenImage(py.import("io")?.call_method0("BytesIO")?);
    let options = PyDict::new(py);
    options.set_item("format", "PNG")?;
    options.set_item("optimize", true)?;
    image.call_method("save", (&buffer.0,), Some(&options))?;
    Ok(buffer
        .0
        .call_method0("getvalue")?
        .cast_into::<PyBytes>()?
        .as_bytes()
        .to_vec())
}

fn on_white<'py>(py: Python<'py>, rgba: &Bound<'py, PyAny>) -> PyResult<OpenImage<'py>> {
    let size = rgba.getattr("size")?.extract::<(u32, u32)>()?;
    let white = OpenImage(
        py.import("PIL.Image")?
            .call_method1("new", ("RGB", size, (255, 255, 255)))?,
    );
    let alpha = OpenImage(rgba.call_method1("getchannel", ("A",))?);
    let options = PyDict::new(py);
    options.set_item("mask", &alpha.0)?;
    white.0.call_method("paste", (rgba,), Some(&options))?;
    Ok(white)
}

pub fn logo_distance(ours: &[u8], theirs: &[u8]) -> Result<u32, String> {
    Python::initialize();
    Python::attach(|py| -> PyResult<u32> {
        let module = py.import("PIL.Image")?;
        let open = |data: &[u8]| -> PyResult<OpenImage<'_>> {
            let buffer = OpenImage(
                py.import("io")?
                    .call_method1("BytesIO", (PyBytes::new(py, data),))?,
            );
            let image = OpenImage(module.call_method1("open", (&buffer.0,))?);
            image.0.call_method0("load")?;
            Ok(image)
        };
        let x = open(ours)?;
        let y = open(theirs)?;
        let want_rgba = OpenImage(y.0.call_method1("convert", ("RGBA",))?);
        let want = dhash(py, &want_rgba.0)?;
        let direct = OpenImage(x.0.call_method1("convert", ("RGBA",))?);
        let mut closest = (dhash(py, &direct.0)? ^ want).count_ones();
        let inverted = (|| -> PyResult<u64> {
            let rgb = OpenImage(x.0.call_method1("convert", ("RGB",))?);
            let inverse = OpenImage(
                py.import("PIL.ImageOps")?
                    .call_method1("invert", (&rgb.0,))?,
            );
            let rgba = OpenImage(inverse.0.call_method1("convert", ("RGBA",))?);
            dhash(py, &rgba.0)
        })();
        if let Ok(hash) = inverted {
            closest = closest.min((hash ^ want).count_ones());
        }
        let flattened = (|| -> PyResult<u64> {
            let rgba = OpenImage(x.0.call_method1("convert", ("RGBA",))?);
            let white = on_white(py, &rgba.0)?;
            let flat = OpenImage(white.0.call_method1("convert", ("RGBA",))?);
            dhash(py, &flat.0)
        })();
        if let Ok(hash) = flattened {
            closest = closest.min((hash ^ want).count_ones());
        }
        Ok(closest)
    })
    .map_err(|_| "cannot decode logo review images".into())
}

pub fn remap_signature(data: &[u8]) -> Result<Vec<u8>, String> {
    Python::initialize();
    Python::attach(|py| -> PyResult<Vec<u8>> {
        let buffer = OpenImage(
            py.import("io")?
                .call_method1("BytesIO", (PyBytes::new(py, data),))?,
        );
        let module = py.import("PIL.Image")?;
        let source = OpenImage(module.call_method1("open", (&buffer.0,))?);
        let rgba = OpenImage(source.0.call_method1("convert", ("RGBA",))?);
        let size = rgba.0.getattr("size")?.extract::<(u32, u32)>()?;
        let background = OpenImage(module.call_method1("new", ("RGBA", size, (0, 0, 0, 255)))?);
        let composite =
            OpenImage(module.call_method1("alpha_composite", (&background.0, &rgba.0))?);
        let rgb = OpenImage(composite.0.call_method1("convert", ("RGB",))?);
        let lanczos = module.getattr("Resampling")?.getattr("LANCZOS")?;
        let flat = OpenImage(rgb.0.call_method1("resize", ((16, 16), &lanczos))?);
        let alpha = OpenImage(rgba.0.call_method1("getchannel", ("A",))?);
        let silhouette = OpenImage(alpha.0.call_method1("resize", ((16, 16), lanczos))?);
        let mut signature = pixels(&flat.0)?;
        signature.extend(pixels(&silhouette.0)?);
        Ok(signature)
    })
    .map_err(|_| "cannot decode remap source image".into())
}

pub fn audit_image(data: &[u8]) -> Result<(String, bool), String> {
    Python::initialize();
    Python::attach(
        |py| -> Result<(String, bool), crate::codec_error::CodecError> {
            let external = |error| crate::codec_error::CodecError::from_python(py, error);
            let buffer = OpenImage(
                py.import("io")
                    .and_then(|module| module.call_method1("BytesIO", (PyBytes::new(py, data),)))
                    .map_err(external)?,
            );
            let module = py.import("PIL.Image").map_err(external)?;
            let source = OpenImage(
                module
                    .call_method1("open", (&buffer.0,))
                    .map_err(external)?,
            );
            let rgba = OpenImage(
                source
                    .0
                    .call_method1("convert", ("RGBA",))
                    .map_err(external)?,
            );
            let normalized = crate::codec_raster::normalized_rgba(py, &rgba.0).map_err(external)?;
            let raw = pixels(&normalized.0).map_err(external)?;
            if raw.len() != 64 * 64 * 4 {
                return Err(crate::codec_error::CodecError::unverified(
                    "native audit raster is incomplete",
                ));
            }
            let hash = digest_hex(&raw)[..24].into();
            let blank = crate::codec_raster::blank_image_typed(py, &rgba.0)?;
            Ok((hash, blank))
        },
    )
    .map_err(|_| "cannot decode coin audit image".into())
}

pub fn provider_png(data: &[u8], output: &Path) -> Result<bool, String> {
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err("provider image output must be absolute and must not already exist".into());
    }
    Python::initialize();
    let bytes = Python::attach(
        |py| -> Result<Option<Vec<u8>>, crate::codec_error::CodecError> {
            let external = |error| crate::codec_error::CodecError::from_python(py, error);
            let decoded = (|| -> PyResult<_> {
                let buffer = OpenImage(
                    py.import("io")?
                        .call_method1("BytesIO", (PyBytes::new(py, data),))?,
                );
                let image = OpenImage(py.import("PIL.Image")?.call_method1("open", (&buffer.0,))?);
                image.0.call_method1("convert", ("RGBA",)).map(OpenImage)
            })();
            let rgba = match decoded {
                Ok(rgba) => rgba,
                Err(error) if error.is_instance_of::<pyo3::exceptions::PyException>(py) => {
                    return Ok(None);
                }
                Err(error) => return Err(external(error)),
            };
            // A sparse placeholder must not become meaningful just because fitting enlarges it.
            if crate::codec_raster::blank_image_typed(py, &rgba.0)? {
                return Ok(None);
            }
            let canvas = fitting::fit(py, &rgba.0, FitPolicy::Source).map_err(external)?;
            png_bytes(py, &canvas.0).map(Some).map_err(external)
        },
    )
    .map_err(|_| "external provider image conversion failed")?;
    let Some(bytes) = bytes else {
        return Ok(false);
    };
    let staged = crate::codec_video_encode::Staged::new(output, "png")?;
    let result = std::fs::write(&staged.0, &bytes)
        .map_err(|_| "cannot write staged provider image".to_owned())
        .and_then(|_| staged.publish(output).map(|_| true));
    staged.clean()?;
    result
}

pub fn to_static_png(source: &Path, output: &Path) -> Result<Value, String> {
    to_static_png_with(source, output, false).map_err(|error| match error.kind.as_str() {
        "MediaError" | "ValueError" | "UnverifiedCodecError" => error.message,
        _ => "external static conversion failed".into(),
    })
}

pub fn to_static_png_typed(
    source: &Path,
    output: &Path,
) -> Result<Value, crate::codec_error::CodecError> {
    to_static_png_with(source, output, true)
}

fn to_static_png_with(
    source: &Path,
    output: &Path,
    source_errors: bool,
) -> Result<Value, crate::codec_error::CodecError> {
    use crate::codec_error::CodecError;
    if !source.is_absolute()
        || !std::fs::symlink_metadata(source)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return Err("static codec source must be an absolute regular non-symlink file".into());
    }
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err("conversion output must be absolute and must not already exist".into());
    }
    Python::initialize();
    let (bytes, rgba, blank) = Python::attach(|py| fitted_png(py, source, source_errors))?;
    if blank {
        return Err(CodecError {
            kind: "MediaError".into(),
            handled: true,
            message: format!(
                "{}: image is blank (no visible pixels)",
                source
                    .file_name()
                    .ok_or("source has no filename")?
                    .to_string_lossy()
            ),
        });
    }
    let parent = output.parent().ok_or("conversion output has no parent")?;
    std::fs::create_dir_all(parent)
        .map_err(|_| CodecError::unverified("cannot create conversion output directory"))?;
    let (staged, mut file) = loop {
        let id = NEXT_OUTPUT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".static-png-{}-{id}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => break (path, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(CodecError::unverified("cannot stage static conversion")),
        }
    };
    let persisted = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = persisted.and_then(|_| crate::atomic::publish_new(&staged, output));
    let _ = std::fs::remove_file(staged);
    result.map_err(|_| CodecError::unverified("cannot publish exclusive static conversion"))?;
    Ok(
        json!({"size":[100,100],"mode":"RGBA","bytes":bytes.len(),"png_sha256":digest_hex(&bytes),"rgba_sha256":digest_hex(&rgba)}),
    )
}

pub use crate::codec_static_prepare::{
    StaticReencode, reencode_static, reencode_static_typed, repaint_static,
};

pub fn preview_static(source: &Path, output: &Path, size: u32) -> Result<Value, String> {
    if !matches!(size, 52 | 72 | 104) {
        return Err("invalid preview parameters".into());
    }
    preview_paths(source, output)?;
    preview_encoded(source, output, size, false).map_err(preview_error)
}

pub fn preview_static_typed(
    source: &Path,
    output: &Path,
    size: u32,
) -> Result<Value, crate::codec_error::CodecError> {
    if !matches!(size, 52 | 72 | 104) {
        return Err(crate::codec_error::CodecError::value(
            "invalid preview parameters",
        ));
    }
    preview_encoded(source, output, size, false)
}

fn preview_error(error: crate::codec_error::CodecError) -> String {
    if error.kind == "UnverifiedCodecError" {
        error.message
    } else {
        "external static preview rendering failed".into()
    }
}

pub fn gallery_static(source: &Path, output: &Path) -> Result<Value, String> {
    preview_paths(source, output)?;
    gallery_static_typed(source, output).map_err(preview_error)
}

pub fn gallery_static_typed(
    source: &Path,
    output: &Path,
) -> Result<Value, crate::codec_error::CodecError> {
    preview_encoded(source, output, 88, true)
}

pub fn source_dhash(source: &Path) -> Result<u64, String> {
    let bytes = std::fs::read(source).map_err(|_| "cannot read provider identity source")?;
    provider_dhash(&bytes)
}

pub fn provider_dhash(bytes: &[u8]) -> Result<u64, String> {
    Python::initialize();
    Python::attach(|py| -> PyResult<u64> {
        let buffer = OpenImage(
            py.import("io")?
                .call_method1("BytesIO", (PyBytes::new(py, bytes),))?,
        );
        let image = OpenImage(py.import("PIL.Image")?.call_method1("open", (&buffer.0,))?);
        let rgba = OpenImage(image.0.call_method1("convert", ("RGBA",))?);
        dhash(py, &rgba.0)
    })
    .map_err(|_| "cannot decode provider identity source".into())
}

pub fn valid_logo(source: &Path) -> bool {
    Python::initialize();
    Python::attach(|py| -> PyResult<bool> {
        let module = py.import("PIL.Image")?;
        {
            let image = OpenImage(module.call_method1("open", (source.to_path_buf(),))?);
            image.0.call_method0("verify")?;
        }
        // verify() invalidates the object; reopening is required, not a redundant decode.
        let image = OpenImage(module.call_method1("open", (source.to_path_buf(),))?);
        let rgba = OpenImage(image.0.call_method1("convert", ("RGBA",))?);
        let alpha = OpenImage(rgba.0.call_method1("getchannel", ("A",))?);
        Ok(!alpha.0.call_method0("getbbox")?.is_none())
    })
    .unwrap_or(false)
}

pub fn animation_info(source: &Path) -> bool {
    animation_info_typed(source).unwrap_or(false)
}

pub(crate) fn animation_info_typed(source: &Path) -> Result<bool, crate::codec_error::CodecError> {
    use crate::codec_error::CodecError;
    Python::initialize();
    Python::attach(|py| {
        let module = py
            .import("PIL.Image")
            .map_err(|error| CodecError::from_python(py, error))?;
        let result = (|| -> PyResult<bool> {
            let manager = OpenImage(module.call_method1("open", (source.to_path_buf(),))?);
            let kind = manager.0.get_type();
            let enter = kind.getattr("__enter__")?;
            let exit = kind.getattr("__exit__")?;
            let image = enter.call1((&manager.0,))?;
            let result = (|| -> PyResult<bool> {
                let animated = match image.getattr_opt("is_animated")? {
                    Some(value) => value.is_truthy()?,
                    None => false,
                };
                Ok(animated && image.getattr("n_frames")?.gt(1)?)
            })();
            // ImageFile.__exit__ propagates file-close errors that its close() method swallows.
            if let Err(error) = &result {
                exit.call1((
                    &manager.0,
                    error.get_type(py),
                    error.value(py),
                    error.traceback(py),
                ))?;
            } else {
                exit.call1((&manager.0, py.None(), py.None(), py.None()))?;
            }
            result
        })();
        match result {
            Ok(animated) => Ok(animated),
            Err(error) => {
                let error = CodecError::from_python(py, error);
                // The source catches only these codec failures, not TypeError or BaseException.
                if error.handled { Ok(false) } else { Err(error) }
            }
        }
    })
}

fn preview_paths(source: &Path, output: &Path) -> Result<(), String> {
    if !source.is_absolute()
        || !std::fs::symlink_metadata(source)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return Err("static preview source must be an absolute regular non-symlink file".into());
    }
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err("preview output must be absolute and must not already exist".into());
    }
    Ok(())
}

fn preview_frame<'py>(
    py: Python<'py>,
    image: &Bound<'py, PyAny>,
    size: u32,
    gallery: bool,
) -> PyResult<OpenImage<'py>> {
    let rgba = OpenImage(image.call_method1("convert", ("RGBA",))?);
    if gallery {
        let lanczos = py
            .import("PIL.Image")?
            .getattr("Resampling")?
            .getattr("LANCZOS")?;
        rgba.0.call_method1("thumbnail", ((size, size), lanczos))?;
    } else {
        rgba.0.call_method1("thumbnail", ((size, size),))?;
    }
    Ok(rgba)
}

fn preview_encoded(
    source: &Path,
    output: &Path,
    size: u32,
    gallery: bool,
) -> Result<Value, crate::codec_error::CodecError> {
    use crate::codec_error::CodecError;
    preview_paths(source, output).map_err(CodecError::value)?;
    Python::initialize();
    let bytes = Python::attach(|py| {
        (|| -> PyResult<Vec<u8>> {
            let encode = |rgba: &Bound<'_, PyAny>| -> PyResult<Vec<u8>> {
                let buffer = OpenImage(py.import("io")?.call_method0("BytesIO")?);
                let options = PyDict::new(py);
                if gallery {
                    options.set_item("quality", 65)?;
                } else {
                    options.set_item("lossless", true)?;
                }
                options.set_item("exact", true)?;
                rgba.call_method("save", (&buffer.0, "WEBP"), Some(&options))?;
                Ok(buffer
                    .0
                    .call_method0("getvalue")?
                    .cast_into::<PyBytes>()?
                    .as_bytes()
                    .to_vec())
            };
            let image = py
                .import("PIL.Image")?
                .call_method1("open", (source.to_path_buf(),))?;
            if gallery {
                crate::codec_raster::with_image_context(image, |image| {
                    encode(&preview_frame(py, image, size, gallery)?.0)
                })
            } else {
                let rgba = crate::codec_raster::with_image_context(image, |image| {
                    preview_frame(py, image, size, gallery)
                })?;
                let encoded = encode(&rgba.0);
                // Source finally closes the frame even after encoding fails; that error may escape.
                rgba.close()?;
                encoded
            }
        })()
        .map_err(|error| CodecError::from_python(py, error))
    })?;
    let parent = output.parent().ok_or("preview output has no parent")?;
    std::fs::create_dir_all(parent)
        .map_err(|_| CodecError::unverified("cannot create preview directory"))?;
    let (staged, mut file) = loop {
        let id = NEXT_OUTPUT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".static-preview-{}-{id}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => break (path, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(CodecError::unverified("cannot stage static preview")),
        }
    };
    let persisted = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = persisted.and_then(|_| crate::atomic::publish_new(&staged, output));
    match std::fs::remove_file(&staged) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(CodecError::unverified("cannot clean staged static preview")),
    }
    result.map_err(|_| CodecError::unverified("cannot publish exclusive static preview"))?;
    Ok(json!({"bytes":bytes.len()}))
}

pub use crate::codec_static_identity::inspect_legacy as inspect_and_fingerprint;

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_pixel(actual: [u8; 4], expected: [u8; 4], resampled: bool) {
        assert_eq!(actual[3], expected[3], "fitting changed opacity");
        for channel in 0..3 {
            assert!(
                actual[channel].abs_diff(expected[channel]) <= if resampled { 2 } else { 0 },
                "fitting changed colour: {actual:?} != {expected:?}"
            );
        }
    }

    #[test]
    fn original_white_backdrop_blends_without_changing_transparent_canvas_fitting() {
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let rgba = OpenImage(
                py.import("PIL.Image")?
                    .call_method1("new", ("RGBA", (10, 10), (255, 0, 0, 128)))?,
            );
            let white = on_white(py, &rgba.0)?;
            assert_eq!(white.0.getattr("mode")?.extract::<String>()?, "RGB");
            let [red, green, blue] = white
                .0
                .call_method1("getpixel", ((5, 5),))?
                .extract::<[u8; 3]>()?;
            assert_eq!(red, 255);
            assert!(green.abs_diff(127) <= 2);
            assert!(blue.abs_diff(127) <= 2);
            assert_eq!(
                rgba.0
                    .call_method1("getpixel", ((5, 5),))?
                    .extract::<[u8; 4]>()?,
                [255, 0, 0, 128]
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn fitting_preserves_original_partial_alpha_resize_cutout_and_trim_assertions() {
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let module = py.import("PIL.Image")?;
            assert_eq!(
                module.getattr("__version__")?.extract::<String>()?,
                "12.3.0"
            );
            let new = |size: (u32, u32), colour: (u8, u8, u8, u8)| {
                module
                    .call_method1("new", ("RGBA", size, colour))
                    .map(OpenImage)
            };
            for policy in [FitPolicy::Legacy, FitPolicy::Source] {
                let fitted_pixel = |source: &OpenImage<'_>, at: (u32, u32)| -> PyResult<[u8; 4]> {
                    let output = fitting::fit(py, &source.0, policy)?;
                    assert_eq!(
                        output.0.getattr("size")?.extract::<(u32, u32)>()?,
                        (100, 100)
                    );
                    output.0.call_method1("getpixel", (at,))?.extract()
                };
                for alpha in [1, 64, 128, 192, 254] {
                    let source = new((100, 100), (255, 0, 0, alpha))?;
                    assert_pixel(fitted_pixel(&source, (50, 50))?, [255, 0, 0, alpha], false);
                }
                let opaque = new((100, 100), (12, 200, 90, 255))?;
                assert_pixel(fitted_pixel(&opaque, (50, 50))?, [12, 200, 90, 255], false);
                let resized = new((40, 40), (0, 100, 255, 96))?;
                assert_pixel(fitted_pixel(&resized, (50, 50))?, [0, 100, 255, 96], true);
                let cutout = new((60, 60), (255, 128, 0, 200))?;
                for x in 20..40 {
                    for y in 20..40 {
                        cutout.0.call_method1("putpixel", ((x, y), (0, 0, 0, 0)))?;
                    }
                }
                let output = fitting::fit(py, &cutout.0, policy)?;
                let hole = output
                    .0
                    .call_method1("getpixel", ((50, 50),))?
                    .extract::<[u8; 4]>()?;
                assert_eq!(hole[3], 0, "fitting filled the cutout");
                let ring = output.0.call_method1("getpixel", ((5, 5),))?.extract()?;
                assert_pixel(ring, [255, 128, 0, 200], true);
                let bordered = new((80, 80), (0, 0, 0, 0))?;
                for x in 30..50 {
                    for y in 30..50 {
                        bordered
                            .0
                            .call_method1("putpixel", ((x, y), (10, 220, 30, 150)))?;
                    }
                }
                assert_pixel(fitted_pixel(&bordered, (50, 50))?, [10, 220, 30, 150], true);
                let empty = new((0, 10), (0, 0, 0, 0))?;
                assert_eq!(fitted_pixel(&empty, (50, 50))?, [0, 0, 0, 0]);
            }
            let rgb = OpenImage(module.call_method1("new", ("RGB", (20, 10), (12, 200, 90)))?);
            let output = fitting::fit(py, &rgb.0, FitPolicy::Source)?;
            assert_pixel(
                output.0.call_method1("getpixel", ((50, 50),))?.extract()?,
                [12, 200, 90, 255],
                true,
            );
            assert_eq!(rgb.0.getattr("mode")?.extract::<String>()?, "RGB");
            assert_eq!(
                rgb.0
                    .call_method1("getpixel", ((5, 5),))?
                    .extract::<[u8; 3]>()?,
                [12, 200, 90]
            );
            Ok(())
        })
        .unwrap();
    }
}
