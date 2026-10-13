//! Rust-owned static identity policy using unchanged external Pillow codec APIs.
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_OUTPUT: AtomicU64 = AtomicU64::new(0);

struct OpenImage<'py>(Bound<'py, PyAny>);

impl Drop for OpenImage<'_> {
    fn drop(&mut self) {
        let _ = self.0.call_method0("close");
    }
}

fn pixels(image: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    Ok(image
        .call_method0("tobytes")?
        .cast_into::<PyBytes>()?
        .as_bytes()
        .to_vec())
}

fn digest_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn premultiplied<'py>(
    py: Python<'py>,
    image_module: &Bound<'py, PyModule>,
    image: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    if image.getattr("mode")?.extract::<String>()? != "RGBA" {
        return Ok(image.clone());
    }
    // The source applies this only to an originally RGBA image. Converting an
    // RGB/palette input first changes the frozen perceptual hash contract.
    let channels = image.call_method0("split")?;
    let channels = (0..4)
        .map(|index| channels.get_item(index).map(OpenImage))
        .collect::<PyResult<Vec<_>>>()?;
    let alpha = &channels[3].0;
    let multiply = py.import("PIL.ImageChops")?.getattr("multiply")?;
    let red = OpenImage(multiply.call1((&channels[0].0, alpha))?);
    let green = OpenImage(multiply.call1((&channels[1].0, alpha))?);
    let blue = OpenImage(multiply.call1((&channels[2].0, alpha))?);
    image_module.call_method1("merge", ("RGBA", (&red.0, &green.0, &blue.0, alpha)))
}

fn dhash<'py>(py: Python<'py>, image: &Bound<'py, PyAny>) -> PyResult<u64> {
    let module = py.import("PIL.Image")?;
    let multiplied = if image.getattr("mode")?.extract::<String>()? == "RGBA" {
        Some(OpenImage(premultiplied(py, &module, image)?))
    } else {
        None
    };
    let raster = multiplied.as_ref().map(|value| &value.0).unwrap_or(image);
    let gray = OpenImage(raster.call_method1("convert", ("L",))?);
    let lanczos = module.getattr("Resampling")?.getattr("LANCZOS")?;
    let resized = OpenImage(gray.0.call_method1("resize", ((9, 8), lanczos))?);
    let bytes = pixels(&resized.0)?;
    if bytes.len() != 72 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "invalid dHash raster",
        ));
    }
    let mut bits = 0_u64;
    for row in bytes.as_chunks::<9>().0 {
        for pair in row.windows(2) {
            bits = (bits << 1) | u64::from(pair[0] > pair[1]);
        }
    }
    Ok(bits)
}

pub fn frame_phash(raw: &[u8]) -> Option<u64> {
    let frame = raw.get(..64 * 64 * 4)?;
    Python::initialize();
    Python::attach(|py| -> PyResult<u64> {
        let image = OpenImage(
            py.import("PIL.Image")?
                .call_method1("frombytes", ("RGBA", (64, 64), PyBytes::new(py, frame)))?,
        );
        dhash(py, &image.0)
    })
    .ok()
}

fn inspect(py: Python<'_>, path: &Path) -> PyResult<Value> {
    let image_module = py.import("PIL.Image")?;
    let image = OpenImage(image_module.call_method1("open", (path.to_path_buf(),))?);
    let mode = image.0.getattr("mode")?.extract::<String>()?;
    let size = image.0.getattr("size")?.extract::<(u32, u32)>()?;
    let rgba = OpenImage(image.0.call_method1("convert", ("RGBA",))?);
    let raw = pixels(&rgba.0)?;
    let blank = blank_rgba(&raw);
    let lanczos = image_module.getattr("Resampling")?.getattr("LANCZOS")?;
    let normalized = OpenImage(rgba.0.call_method1("resize", ((64, 64), &lanczos))?);
    let key = format!("s:{}", &digest_hex(&pixels(&normalized.0)?)[..32]);
    let phash = dhash(py, &image.0).ok();
    Ok(json!({"size":[size.0,size.1],"mode":mode,"blank":blank,"content_key":key,"phash":phash}))
}

fn fitted_png(py: Python<'_>, path: &Path) -> PyResult<(Vec<u8>, Vec<u8>, bool)> {
    let module = py.import("PIL.Image")?;
    let source = OpenImage(
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
        {
            crate::codec_svg::load_rgba(py, path)?
        } else {
            module.call_method1("open", (path.to_path_buf(),))?
        },
    );
    let rgba = OpenImage(source.0.call_method1("convert", ("RGBA",))?);
    let canvas = fit_100(py, &rgba.0)?;
    let rgba = pixels(&canvas.0)?;
    if blank_rgba(&rgba) {
        return Ok((Vec::new(), rgba, true));
    }
    Ok((png_bytes(py, &canvas.0)?, rgba, false))
}

fn blank_rgba(raw: &[u8]) -> bool {
    raw.iter()
        .skip(3)
        .step_by(4)
        .filter(|&&alpha| alpha > 10)
        .count()
        <= 8
}

fn fit_100<'py>(py: Python<'py>, rgba: &Bound<'py, PyAny>) -> PyResult<OpenImage<'py>> {
    let module = py.import("PIL.Image")?;
    let alpha = OpenImage(rgba.call_method1("getchannel", ("A",))?);
    let bounds = alpha.0.call_method0("getbbox")?;
    let trimmed = if bounds.is_none() {
        None
    } else {
        Some(OpenImage(rgba.call_method1("crop", (bounds,))?))
    };
    let raster = trimmed.as_ref().map(|image| &image.0).unwrap_or(rgba);
    let (width, height) = raster.getattr("size")?.extract::<(u32, u32)>()?;
    let canvas = OpenImage(module.call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))?);
    if width != 0 && height != 0 {
        let scale = (100.0 / f64::from(width)).min(100.0 / f64::from(height));
        let w = (f64::from(width) * scale).round_ties_even().max(1.0) as u32;
        let h = (f64::from(height) * scale).round_ties_even().max(1.0) as u32;
        let lanczos = module.getattr("Resampling")?.getattr("LANCZOS")?;
        let resized = OpenImage(raster.call_method1("resize", ((w, h), lanczos))?);
        // No mask: preserve straight RGB and alpha rather than compositing twice.
        canvas
            .0
            .call_method1("paste", (&resized.0, ((100 - w) / 2, (100 - h) / 2)))?;
    }
    Ok(canvas)
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
    Python::attach(|py| -> PyResult<(String, bool)> {
        let buffer = OpenImage(
            py.import("io")?
                .call_method1("BytesIO", (PyBytes::new(py, data),))?,
        );
        let module = py.import("PIL.Image")?;
        let source = OpenImage(module.call_method1("open", (&buffer.0,))?);
        let rgba = OpenImage(source.0.call_method1("convert", ("RGBA",))?);
        let blank = blank_rgba(&pixels(&rgba.0)?);
        let lanczos = module.getattr("Resampling")?.getattr("LANCZOS")?;
        let normalized = OpenImage(rgba.0.call_method1("resize", ((64, 64), lanczos))?);
        Ok((digest_hex(&pixels(&normalized.0)?)[..24].into(), blank))
    })
    .map_err(|_| "cannot decode coin audit image".into())
}

pub fn provider_png(data: &[u8], output: &Path) -> Result<bool, String> {
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err("provider image output must be absolute and must not already exist".into());
    }
    Python::initialize();
    let bytes = Python::attach(|py| -> PyResult<Option<Vec<u8>>> {
        let buffer = OpenImage(
            py.import("io")?
                .call_method1("BytesIO", (PyBytes::new(py, data),))?,
        );
        let decoded = py
            .import("PIL.Image")?
            .call_method1("open", (&buffer.0,))
            .map(OpenImage)
            .and_then(|image| image.0.call_method1("convert", ("RGBA",)).map(OpenImage));
        let Ok(rgba) = decoded else {
            return Ok(None);
        };
        // A sparse placeholder must not become meaningful just because fitting enlarges it.
        if blank_rgba(&pixels(&rgba.0)?) {
            return Ok(None);
        }
        let canvas = fit_100(py, &rgba.0)?;
        png_bytes(py, &canvas.0).map(Some)
    })
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
    let (bytes, rgba, blank) = Python::attach(|py| {
        fitted_png(py, source).map_err(|_| "external static conversion failed".to_owned())
    })?;
    if blank {
        return Err(format!(
            "{}: image is blank (no visible pixels)",
            source
                .file_name()
                .ok_or("source has no filename")?
                .to_string_lossy()
        ));
    }
    let parent = output.parent().ok_or("conversion output has no parent")?;
    std::fs::create_dir_all(parent).map_err(|_| "cannot create conversion output directory")?;
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
            Err(_) => return Err("cannot stage static conversion".into()),
        }
    };
    let persisted = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = persisted.and_then(|_| crate::atomic::publish_new(&staged, output));
    let _ = std::fs::remove_file(staged);
    result.map_err(|_| "cannot publish exclusive static conversion")?;
    Ok(
        json!({"size":[100,100],"mode":"RGBA","bytes":bytes.len(),"png_sha256":digest_hex(&bytes),"rgba_sha256":digest_hex(&rgba)}),
    )
}

fn lossless_webp(py: Python<'_>, path: &Path, tint: Option<[u8; 3]>) -> PyResult<Vec<u8>> {
    let module = py.import("PIL.Image")?;
    let source = OpenImage(module.call_method1("open", (path.to_path_buf(),))?);
    let rgba = OpenImage(source.0.call_method1("convert", ("RGBA",))?);
    let flat = if let Some([r, g, b]) = tint {
        let size = rgba.0.getattr("size")?;
        let flat = OpenImage(module.call_method1("new", ("RGBA", size, (r, g, b, 255)))?);
        let alpha = OpenImage(rgba.0.call_method1("getchannel", ("A",))?);
        flat.0.call_method1("putalpha", (&alpha.0,))?;
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

pub fn reencode_static(path: &Path) -> Result<bool, String> {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return Err("static reencode source must be an absolute regular non-symlink file".into());
    }
    let before = std::fs::read(path).map_err(|_| "cannot read static reencode source")?;
    Python::initialize();
    let Some(bytes) = Python::attach(|py| lossless_webp(py, path, None).ok()) else {
        return Ok(false);
    };
    if bytes == before {
        return Ok(false);
    }
    crate::atomic::write_bytes(path, &bytes)
        .map_err(|_| "cannot replace static reencode source")?;
    Ok(true)
}

pub fn repaint_static(path: &Path, rgb: [u8; 3]) -> Result<bool, String> {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return Err("static tint source must be an absolute regular non-symlink file".into());
    }
    Python::initialize();
    let Some(bytes) = Python::attach(|py| lossless_webp(py, path, Some(rgb)).ok()) else {
        return Ok(false);
    };
    crate::atomic::write_bytes(path, &bytes).map_err(|_| "cannot replace static tint source")?;
    Ok(true)
}

pub fn preview_static(source: &Path, output: &Path, size: u32) -> Result<Value, String> {
    if !matches!(size, 52 | 72 | 104) {
        return Err("invalid preview parameters".into());
    }
    preview_encoded(source, output, size, false)
}

pub fn gallery_static(source: &Path, output: &Path) -> Result<Value, String> {
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
    Python::initialize();
    Python::attach(|py| -> PyResult<bool> {
        let image = OpenImage(
            py.import("PIL.Image")?
                .call_method1("open", (source.to_path_buf(),))?,
        );
        let animated = image
            .0
            .getattr("is_animated")
            .and_then(|value| value.extract::<bool>())
            .unwrap_or(false);
        Ok(animated && image.0.getattr("n_frames")?.extract::<u32>()? > 1)
    })
    .unwrap_or(false)
}

fn preview_encoded(
    source: &Path,
    output: &Path,
    size: u32,
    gallery: bool,
) -> Result<Value, String> {
    if !source.is_absolute()
        || !std::fs::symlink_metadata(source)
            .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
    {
        return Err("static preview source must be an absolute regular non-symlink file".into());
    }
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err("preview output must be absolute and must not already exist".into());
    }
    Python::initialize();
    let bytes = Python::attach(|py| -> PyResult<Vec<u8>> {
        let image = OpenImage(
            py.import("PIL.Image")?
                .call_method1("open", (source.to_path_buf(),))?,
        );
        let rgba = OpenImage(image.0.call_method1("convert", ("RGBA",))?);
        if gallery {
            let lanczos = py
                .import("PIL.Image")?
                .getattr("Resampling")?
                .getattr("LANCZOS")?;
            rgba.0.call_method1("thumbnail", ((size, size), lanczos))?;
        } else {
            rgba.0.call_method1("thumbnail", ((size, size),))?;
        }
        let buffer = OpenImage(py.import("io")?.call_method0("BytesIO")?);
        let options = PyDict::new(py);
        if gallery {
            options.set_item("quality", 65)?;
        } else {
            options.set_item("lossless", true)?;
        }
        options.set_item("exact", true)?;
        rgba.0
            .call_method("save", (&buffer.0, "WEBP"), Some(&options))?;
        Ok(buffer
            .0
            .call_method0("getvalue")?
            .cast_into::<PyBytes>()?
            .as_bytes()
            .to_vec())
    })
    .map_err(|_| "external static preview rendering failed")?;
    let parent = output.parent().ok_or("preview output has no parent")?;
    std::fs::create_dir_all(parent).map_err(|_| "cannot create preview directory")?;
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
            Err(_) => return Err("cannot stage static preview".into()),
        }
    };
    let persisted = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = persisted.and_then(|_| crate::atomic::publish_new(&staged, output));
    match std::fs::remove_file(&staged) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("cannot clean staged static preview".into()),
    }
    result.map_err(|_| "cannot publish exclusive static preview")?;
    Ok(json!({"bytes":bytes.len()}))
}

fn comparison_pixels(py: Python<'_>, path: &Path) -> PyResult<Vec<u8>> {
    let module = py.import("PIL.Image")?;
    let source = OpenImage(module.call_method1("open", (path.to_path_buf(),))?);
    let lanczos = module.getattr("Resampling")?.getattr("LANCZOS")?;
    let rgba = OpenImage(source.0.call_method1("convert", ("RGBA",))?);
    let normalized = OpenImage(rgba.0.call_method1("resize", ((64, 64), lanczos))?);
    let multiplied = OpenImage(premultiplied(py, &module, &normalized.0)?);
    pixels(&multiplied.0)
}

fn close_pixels(x: &[u8], y: &[u8]) -> Option<bool> {
    if x.len() != 64 * 64 * 4 || x.len() != y.len() {
        return None;
    }
    let mut totals = [0_u64; 4];
    for (index, (left, right)) in x.iter().zip(y).enumerate() {
        totals[index % 4] += u64::from(left.abs_diff(*right));
    }
    let mean = totals
        .iter()
        .map(|sum| *sum as f64 / (64.0 * 64.0))
        .sum::<f64>()
        / 4.0;
    Some(mean <= 8.0)
}

pub fn same_frame(x: &[u8], y: &[u8]) -> Option<bool> {
    if x.len() != 64 * 64 * 4 || y.len() != x.len() {
        return None;
    }
    if (frame_phash(x)? ^ frame_phash(y)?).count_ones() > 6 {
        return Some(false);
    }
    Python::attach(|py| {
        let module = py.import("PIL.Image").ok()?;
        let normalized = |raw: &[u8]| -> PyResult<Vec<u8>> {
            let image = OpenImage(
                module.call_method1("frombytes", ("RGBA", (64, 64), PyBytes::new(py, raw)))?,
            );
            let multiplied = OpenImage(premultiplied(py, &module, &image.0)?);
            pixels(&multiplied.0)
        };
        close_pixels(&normalized(x).ok()?, &normalized(y).ok()?)
    })
}

pub fn same_image(a: &Path, b: &Path) -> Option<bool> {
    let a_info = inspect_and_fingerprint(a).ok()?;
    let b_info = inspect_and_fingerprint(b).ok()?;
    if a_info["content_key"] == b_info["content_key"] {
        return Some(true);
    }
    let distance = (a_info["phash"].as_u64()? ^ b_info["phash"].as_u64()?).count_ones();
    if distance > 6 {
        return Some(false);
    }
    Python::attach(|py| {
        let x = comparison_pixels(py, a).ok()?;
        let y = comparison_pixels(py, b).ok()?;
        close_pixels(&x, &y)
    })
}

pub fn inspect_and_fingerprint(path: &Path) -> Result<Value, String> {
    if !path.is_absolute()
        || !std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    {
        return Err("static codec source must be an absolute regular non-symlink file".into());
    }
    Python::initialize();
    Python::attach(|py| {
        inspect(py, path).map_err(|error| {
            format!(
                "external static codec failed: {}",
                error
                    .get_type(py)
                    .name()
                    .map(|name| name.to_string())
                    .unwrap_or_else(|_| "PythonError".into())
            )
        })
    })
}
