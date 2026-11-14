//! Native static-image pipeline; historical pixel identities remain stable across encoders.
use crate::{codec_error::CodecError, native_raster as raster};
use image::RgbaImage;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::Path,
};

pub use crate::native_raster::{frame_phash, same_frame};

fn source_bytes(path: &Path) -> Result<Vec<u8>, CodecError> {
    let limits = raster::DecodeLimits::default();
    let file = std::fs::File::open(path)?;
    if file.metadata()?.len() > limits.max_encoded_bytes {
        return Err(CodecError::media("encoded image exceeds byte limit"));
    }
    let mut bytes = Vec::new();
    file.take(limits.max_encoded_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limits.max_encoded_bytes {
        return Err(CodecError::media("encoded image exceeds byte limit"));
    }
    Ok(bytes)
}

pub fn decoded(path: &Path) -> Result<raster::DecodedRaster, CodecError> {
    let bytes = source_bytes(path)?;
    if image::guess_format(&bytes).is_err() {
        return Err(CodecError {
            kind: "UnidentifiedImageError".into(),
            message: format!(
                "cannot identify image file {}",
                crate::python_repr::quoted(&path.to_string_lossy())
            ),
            handled: true,
        });
    }
    raster::decode(&bytes).map_err(Into::into)
}

pub fn metadata(path: &Path, include_blank: bool) -> Result<Value, CodecError> {
    let source = decoded(path)?;
    let mut result = json!({"size":[source.rgba.width(),source.rgba.height()],"mode":source.mode});
    if include_blank {
        result["blank"] = json!(raster::is_blank(&source.rgba));
    }
    Ok(result)
}

pub fn metadata_operation(path: &Path, operation: &str) -> Result<Value, CodecError> {
    let source = decoded(path)?;
    if operation == "usable" {
        return Ok(json!(!raster::is_blank(&source.rgba)));
    }
    if operation == "dimensions" {
        return Ok(json!([source.rgba.width(), source.rgba.height()]));
    }
    let mut result = json!({"size":[source.rgba.width(),source.rgba.height()],"mode":source.mode});
    if operation == "image_info" {
        result["blank"] = json!(raster::is_blank(&source.rgba));
    }
    Ok(result)
}

fn regular_source(path: &Path) -> bool {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if !std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.file_attributes() & 0x400 == 0)
        {
            return false;
        }
    }
    true
}
fn identity_source(path: &Path) -> Result<(), CodecError> {
    if !regular_source(path) {
        return Err(CodecError::unverified(
            "static codec source must be an absolute regular non-symlink file",
        ));
    }
    Ok(())
}

pub fn fingerprint_typed(path: &Path) -> Result<Value, CodecError> {
    identity_source(path)?;
    let source = decoded(path)?;
    Ok(
        json!({"content_key":raster::content_key(&source.rgba)?,"phash":raster::source_dhash(&source).ok()}),
    )
}
pub fn same_image_typed(left: &Path, right: &Path) -> Result<Option<bool>, CodecError> {
    identity_source(left)?;
    let left = match decoded(left) {
        Ok(source) => source,
        Err(_) => return Ok(None),
    };
    identity_source(right)?;
    let right = match decoded(right) {
        Ok(source) => source,
        Err(_) => return Ok(None),
    };
    raster::same_decoded(&left, &right)
        .map(Some)
        .map_err(Into::into)
}
pub fn same_image(left: &Path, right: &Path) -> Option<bool> {
    same_image_typed(left, right).ok().flatten()
}
pub fn logo_distance(left: &[u8], right: &[u8]) -> Result<u32, String> {
    let result = (|| {
        let left = raster::decode(left)?;
        let right = raster::decode(right)?;
        raster::logo_distance(&left.rgba, &right.rgba)
    })();
    result.map_err(|_| "cannot decode logo review images".into())
}
pub fn remap_signature(bytes: &[u8]) -> Result<Vec<u8>, String> {
    raster::decode(bytes)
        .and_then(|r| raster::remap_signature(&r.rgba))
        .map_err(|_| "cannot decode remap source image".into())
}
pub fn audit_image(bytes: &[u8]) -> Result<(String, bool), String> {
    let source = raster::decode(bytes).map_err(|_| "cannot decode coin audit image")?;
    Ok((
        raster::audit_signature(&source.rgba).map_err(|_| "cannot decode coin audit image")?,
        raster::is_blank(&source.rgba),
    ))
}
pub fn provider_dhash(bytes: &[u8]) -> Result<u64, String> {
    raster::decode(bytes)
        .and_then(|r| raster::dhash(&r.rgba))
        .map_err(|_| "cannot decode provider identity source".into())
}
pub fn source_dhash(path: &Path) -> Result<u64, String> {
    let bytes = raster::read_bounded(path, Default::default())
        .map_err(|_| "cannot read provider identity source")?;
    provider_dhash(&bytes)
}
pub fn valid_logo(path: &Path) -> bool {
    decoded(path).is_ok_and(|r| raster::alpha_bounds(&r.rgba).is_some())
}
pub fn animation_info_typed(path: &Path) -> Result<bool, CodecError> {
    Ok(decoded(path).map(|r| r.animated).unwrap_or(false))
}
pub fn animation_info(path: &Path) -> bool {
    animation_info_typed(path).unwrap_or(false)
}

pub fn publish_bytes(output: &Path, extension: &str, bytes: &[u8]) -> Result<u64, CodecError> {
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err(CodecError::value(
            "output must be absolute and must not already exist",
        ));
    }
    let staged = crate::codec_video_encode::Staged::new(output, extension)
        .map_err(CodecError::unverified)?;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&staged.0)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        staged.publish(output).map_err(CodecError::unverified)
    })();
    let cleaned = staged.clean().map_err(CodecError::unverified);
    match result {
        Err(error) => Err(error),
        Ok(size) => {
            cleaned?;
            Ok(size)
        }
    }
}
pub fn provider_png(bytes: &[u8], output: &Path) -> Result<bool, String> {
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err("provider image output must be absolute and must not already exist".into());
    }
    let Ok(source) = raster::decode(bytes) else {
        return Ok(false);
    };
    // Admission precedes fitting: eight meaningful pixels cannot become an emoji
    // merely because the crop is enlarged to the target canvas.
    if raster::is_blank(&source.rgba) {
        return Ok(false);
    }
    let canvas = raster::fit_canvas(&source.rgba, 100).map_err(|e| e.to_string())?;
    let bytes = raster::encode_png(&canvas).map_err(|e| e.to_string())?;
    publish_bytes(output, "png", &bytes)
        .map(|_| true)
        .map_err(|e| e.message)
}
pub fn to_static_png_typed(source: &Path, output: &Path) -> Result<Value, CodecError> {
    if !regular_source(source) {
        return Err(CodecError::value(
            "static codec source must be an absolute regular non-symlink file",
        ));
    }
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err(CodecError::value(
            "conversion output must be absolute and must not already exist",
        ));
    }
    let rgba = if source
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
    {
        crate::native_svg::load_rgba(source)?
    } else {
        decoded(source)?.rgba
    };
    let canvas = raster::fit_canvas(&rgba, 100)?;
    if raster::is_blank(&canvas) {
        return Err(CodecError::media(format!(
            "{}: image is blank (no visible pixels)",
            source
                .file_name()
                .unwrap_or(source.as_os_str())
                .to_string_lossy()
        )));
    }
    let bytes = raster::encode_png(&canvas)?;
    publish_bytes(output, "png", &bytes)?;
    Ok(
        json!({"size":[100,100],"mode":"RGBA","bytes":bytes.len(),"png_sha256":raster::digest_hex(&bytes),"rgba_sha256":raster::digest_hex(canvas.as_raw())}),
    )
}
pub fn to_static_png(source: &Path, output: &Path) -> Result<Value, String> {
    to_static_png_typed(source, output).map_err(|e| e.message)
}
fn preview(source: &Path, output: &Path, size: u32, gallery: bool) -> Result<Value, CodecError> {
    if !regular_source(source) {
        return Err(CodecError::value(
            "static preview source must be an absolute regular non-symlink file",
        ));
    }
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err(CodecError::value(
            "preview output must be absolute and must not already exist",
        ));
    }
    let source = decoded(source)?;
    let image = raster::thumbnail(
        &source.rgba,
        size,
        if gallery {
            raster::ResampleFilter::Lanczos
        } else {
            raster::ResampleFilter::Bicubic
        },
    )?;
    let bytes = raster::encode_webp(
        &image,
        raster::WebpOptions {
            lossless: !gallery,
            quality: if gallery { 65.0 } else { 80.0 },
            method: 4,
        },
    )?;
    let length = publish_bytes(output, "webp", &bytes)?;
    Ok(json!({"bytes":length}))
}
pub fn preview_static_typed(source: &Path, output: &Path, size: u32) -> Result<Value, CodecError> {
    if !matches!(size, 52 | 72 | 104) {
        return Err(CodecError::value("invalid preview parameters"));
    }
    preview(source, output, size, false)
}
pub fn preview_static(source: &Path, output: &Path, size: u32) -> Result<Value, String> {
    preview_static_typed(source, output, size).map_err(|e| e.message)
}
pub fn gallery_static_typed(source: &Path, output: &Path) -> Result<Value, CodecError> {
    preview(source, output, 88, true)
}
pub fn gallery_static(source: &Path, output: &Path) -> Result<Value, String> {
    gallery_static_typed(source, output).map_err(|e| e.message)
}

pub struct StaticReencode {
    pub rewritten: bool,
    pub warning: Option<String>,
}
fn reencode(path: &Path, rgb: Option<[u8; 3]>) -> Result<StaticReencode, CodecError> {
    let operation = if rgb.is_some() { "tint" } else { "reencode" };
    if !regular_source(path) {
        return Err(CodecError::value(format!(
            "static {operation} source must be an absolute regular non-symlink file"
        )));
    }
    // Retain a single snapshot through decoding and optional encoding failure.
    let before = source_bytes(path)?;
    let encoded = (|| {
        let source = raster::decode(&before)?;
        let image = if let Some(rgb) = rgb {
            raster::tint(&source.rgba, rgb)
        } else {
            source.rgba
        };
        raster::encode_webp(
            &image,
            raster::WebpOptions {
                lossless: true,
                quality: 100.0,
                method: 6,
            },
        )
    })();
    let bytes = match encoded {
        Ok(bytes) => bytes,
        Err(error) => {
            let action = if rgb.is_some() {
                "repaint"
            } else {
                "re-encode"
            };
            let name = path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy();
            return Ok(StaticReencode {
                rewritten: false,
                warning: Some(crate::logging::redact(
                    &format!("{action} skipped for {name} (static): {error}"),
                    &[],
                )),
            });
        }
    };
    if rgb.is_none() && bytes == before {
        return Ok(StaticReencode {
            rewritten: false,
            warning: None,
        });
    }
    crate::atomic::write_bytes(path, &bytes).map_err(CodecError::io)?;
    Ok(StaticReencode {
        rewritten: true,
        warning: None,
    })
}
pub fn reencode_static_typed(path: &Path) -> Result<StaticReencode, CodecError> {
    reencode(path, None)
}
pub fn repaint_static_typed(path: &Path, rgb: [u8; 3]) -> Result<StaticReencode, CodecError> {
    reencode(path, Some(rgb))
}
fn legacy_reencode(result: Result<StaticReencode, CodecError>) -> Result<bool, String> {
    let result = result.map_err(|e| e.message)?;
    if let Some(warning) = result.warning {
        eprintln!("WARNING: {}", crate::logging::redact(&warning, &[]));
    }
    Ok(result.rewritten)
}
pub fn reencode_static(path: &Path) -> Result<bool, String> {
    legacy_reencode(reencode(path, None))
}
pub fn repaint_static(path: &Path, rgb: [u8; 3]) -> Result<bool, String> {
    legacy_reencode(reencode(path, Some(rgb)))
}
pub fn prepare(
    source: &Path,
    request: &Value,
    warnings: &mut Vec<String>,
) -> Result<Value, CodecError> {
    if request
        .get("reencode")
        .unwrap_or(&Value::Bool(false))
        .as_bool()
        .ok_or("reencode must be boolean")?
    {
        warnings.extend(reencode_static_typed(source)?.warning);
    }
    if let Some(tint) = request.get("tint") {
        let rgb = crate::codec_tint::parse_tint(tint.as_str().ok_or("tint must be #RRGGBB")?)
            .map_err(CodecError::media)?;
        let result = repaint_static_typed(source, rgb)?;
        warnings.extend(result.warning);
        if !result.rewritten {
            return Err(CodecError::value("requested tint could not be applied"));
        }
    }
    let info = fingerprint_typed(source)?;
    let extension = crate::codec_format::extension_typed(source, "static")?;
    Ok(
        json!({"content_key":info["content_key"],"phash":info["phash"],"fmt":"static","extension":extension}),
    )
}

pub fn rgba_from_frame(raw: &[u8], side: u32) -> Result<RgbaImage, CodecError> {
    RgbaImage::from_raw(side, side, raw.to_vec())
        .ok_or_else(|| CodecError::value("invalid RGBA frame dimensions"))
}
