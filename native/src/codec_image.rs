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
    let alpha = channels.get_item(3)?;
    let multiply = py.import("PIL.ImageChops")?.getattr("multiply")?;
    let red = multiply.call1((channels.get_item(0)?, &alpha))?;
    let green = multiply.call1((channels.get_item(1)?, &alpha))?;
    let blue = multiply.call1((channels.get_item(2)?, &alpha))?;
    image_module.call_method1("merge", ("RGBA", (red, green, blue, alpha)))
}

fn inspect(py: Python<'_>, path: &Path) -> PyResult<Value> {
    let image_module = py.import("PIL.Image")?;
    let image = OpenImage(image_module.call_method1("open", (path.to_path_buf(),))?);
    let mode = image.0.getattr("mode")?.extract::<String>()?;
    let size = image.0.getattr("size")?.extract::<(u32, u32)>()?;
    let rgba = image.0.call_method1("convert", ("RGBA",))?;
    let raw = pixels(&rgba)?;
    let blank = raw
        .iter()
        .skip(3)
        .step_by(4)
        .filter(|&&alpha| alpha > 10)
        .count()
        <= 8;
    let lanczos = image_module.getattr("Resampling")?.getattr("LANCZOS")?;
    let normalized = rgba.call_method1("resize", ((64, 64), &lanczos))?;
    let key = format!("s:{}", &digest_hex(&pixels(&normalized)?)[..32]);
    let phash = (|| -> PyResult<u64> {
        let multiplied = premultiplied(py, &image_module, &image.0)?;
        let gray = multiplied
            .call_method1("convert", ("L",))?
            .call_method1("resize", ((9, 8), &lanczos))?;
        let bytes = pixels(&gray)?;
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
    })()
    .ok();
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
    let rgba = source.0.call_method1("convert", ("RGBA",))?;
    let alpha = rgba.call_method1("getchannel", ("A",))?;
    let bounds = alpha.call_method0("getbbox")?;
    let trimmed = if bounds.is_none() {
        rgba
    } else {
        rgba.call_method1("crop", (bounds,))?
    };
    let (width, height) = trimmed.getattr("size")?.extract::<(u32, u32)>()?;
    let canvas = module.call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))?;
    if width != 0 && height != 0 {
        let scale = (100.0 / f64::from(width)).min(100.0 / f64::from(height));
        let w = (f64::from(width) * scale).round_ties_even().max(1.0) as u32;
        let h = (f64::from(height) * scale).round_ties_even().max(1.0) as u32;
        let lanczos = module.getattr("Resampling")?.getattr("LANCZOS")?;
        let resized = trimmed.call_method1("resize", ((w, h), lanczos))?;
        // No mask: the source preserves straight RGB and alpha rather than compositing twice.
        canvas.call_method1("paste", (resized, ((100 - w) / 2, (100 - h) / 2)))?;
    }
    let rgba = pixels(&canvas)?;
    let blank = rgba
        .iter()
        .skip(3)
        .step_by(4)
        .filter(|&&alpha| alpha > 10)
        .count()
        <= 8;
    if blank {
        return Ok((Vec::new(), rgba, true));
    }
    let buffer = OpenImage(py.import("io")?.call_method0("BytesIO")?);
    let options = PyDict::new(py);
    options.set_item("format", "PNG")?;
    options.set_item("optimize", true)?;
    canvas.call_method("save", (&buffer.0,), Some(&options))?;
    let bytes = buffer
        .0
        .call_method0("getvalue")?
        .cast_into::<PyBytes>()?
        .as_bytes()
        .to_vec();
    Ok((bytes, rgba, false))
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

fn comparison_pixels(py: Python<'_>, path: &Path) -> PyResult<Vec<u8>> {
    let module = py.import("PIL.Image")?;
    let source = OpenImage(module.call_method1("open", (path.to_path_buf(),))?);
    let lanczos = module.getattr("Resampling")?.getattr("LANCZOS")?;
    let normalized = source
        .0
        .call_method1("convert", ("RGBA",))?
        .call_method1("resize", ((64, 64), lanczos))?;
    pixels(&premultiplied(py, &module, &normalized)?)
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
        if x.len() != 64 * 64 * 4 || x.len() != y.len() {
            return None;
        }
        let mut totals = [0_u64; 4];
        for (index, (left, right)) in x.iter().zip(&y).enumerate() {
            totals[index % 4] += u64::from(left.abs_diff(*right));
        }
        let mean = totals
            .iter()
            .map(|sum| *sum as f64 / (64.0 * 64.0))
            .sum::<f64>()
            / 4.0;
        Some(mean <= 8.0)
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
