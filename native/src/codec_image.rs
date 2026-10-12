//! Rust-owned static identity policy using unchanged external Pillow codec APIs.
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

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
        for row in bytes.chunks_exact(9) {
            for pair in row.windows(2) {
                bits = (bits << 1) | u64::from(pair[0] > pair[1]);
            }
        }
        Ok(bits)
    })()
    .ok();
    Ok(json!({"size":[size.0,size.1],"mode":mode,"blank":blank,"content_key":key,"phash":phash}))
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
