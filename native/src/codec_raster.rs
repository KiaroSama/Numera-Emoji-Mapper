//! Shared Pillow object ownership and frozen raster identity primitives, independent of file workflows.
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use sha2::{Digest, Sha256};

pub(crate) struct OpenImage<'py>(pub(crate) Bound<'py, PyAny>);
impl OpenImage<'_> {
    pub(crate) fn close(mut self) -> PyResult<()> {
        let py = self.0.py();
        // Preserve the first close error; cleanup must never turn it into success.
        let image = std::mem::replace(&mut self.0, py.None().into_bound(py));
        image.call_method0("close").map(|_| ())
    }
}
impl Drop for OpenImage<'_> {
    fn drop(&mut self) {
        if !self.0.is_none() {
            let _ = self.0.call_method0("close");
        }
    }
}

pub(crate) fn with_image_context<'py, T>(
    image: Bound<'py, PyAny>,
    body: impl FnOnce(&Bound<'py, PyAny>) -> PyResult<T>,
) -> PyResult<T> {
    let manager = OpenImage(image);
    let py = manager.0.py();
    let kind = manager.0.get_type();
    let enter = kind.getattr("__enter__")?;
    let exit = kind.getattr("__exit__")?;
    let image = enter.call1((&manager.0,))?;
    let result = body(&image);
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
    // Pillow exits never suppress errors; RAII additionally closes the manager on unwind.
    result
}

fn frame_phash_external(raw: &[u8]) -> PyResult<Option<u64>> {
    let Some(frame) = raw.get(..64 * 64 * 4) else {
        return Ok(None);
    };
    Python::initialize();
    Python::attach(|py| {
        let image = OpenImage(
            py.import("PIL.Image")?
                .call_method1("frombytes", ("RGBA", (64, 64), PyBytes::new(py, frame)))?,
        );
        dhash(py, &image.0).map(Some)
    })
}

pub fn frame_phash(raw: &[u8]) -> Option<u64> {
    frame_phash_external(raw).ok().flatten()
}

pub fn frame_phash_typed(raw: &[u8]) -> Result<Option<u64>, crate::codec_error::CodecError> {
    match frame_phash_external(raw) {
        Ok(hash) => Ok(hash),
        Err(error) => Python::attach(|py| {
            if error.is_instance_of::<pyo3::exceptions::PyException>(py) {
                Ok(None)
            } else {
                Err(crate::codec_error::CodecError::from_python(py, error))
            }
        }),
    }
}

fn comparison_pixels(py: Python<'_>, path: &std::path::Path) -> PyResult<Vec<u8>> {
    let module = py.import("PIL.Image")?;
    let source = OpenImage(module.call_method1("open", (path.to_path_buf(),))?);
    let rgba = OpenImage(source.0.call_method1("convert", ("RGBA",))?);
    comparison_image_pixels(py, &rgba.0)
}

fn comparison_image_pixels<'py>(py: Python<'py>, image: &Bound<'py, PyAny>) -> PyResult<Vec<u8>> {
    let module = py.import("PIL.Image")?;
    let rgba = OpenImage(image.call_method1("convert", ("RGBA",))?);
    let normalized = normalized_rgba(py, &rgba.0)?;
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

pub(crate) fn same_frame_external(x: &[u8], y: &[u8]) -> PyResult<Option<bool>> {
    if x.len() != 64 * 64 * 4 || y.len() != x.len() {
        return Ok(None);
    }
    Python::initialize();
    Python::attach(|py| {
        let module = py.import("PIL.Image")?;
        let left =
            OpenImage(module.call_method1("frombytes", ("RGBA", (64, 64), PyBytes::new(py, x)))?);
        let right =
            OpenImage(module.call_method1("frombytes", ("RGBA", (64, 64), PyBytes::new(py, y)))?);
        let left_hash = dhash_checked(py, &left.0)?;
        let right_hash = dhash_checked(py, &right.0)?;
        let (Some(left_hash), Some(right_hash)) = (left_hash, right_hash) else {
            return Ok(None);
        };
        if (left_hash ^ right_hash).count_ones() > 6 {
            return Ok(Some(false));
        }
        Ok(close_pixels(
            &comparison_image_pixels(py, &left.0)?,
            &comparison_image_pixels(py, &right.0)?,
        ))
    })
}

pub fn same_frame_typed(
    x: &[u8],
    y: &[u8],
) -> Result<Option<bool>, crate::codec_error::CodecError> {
    if x.len() != 64 * 64 * 4 || y.len() != x.len() {
        return Err(crate::codec_error::CodecError::unverified(
            "native comparison frame is incomplete",
        ));
    }
    match same_frame_external(x, y) {
        Ok(Some(same)) => Ok(Some(same)),
        Ok(None) => Err(crate::codec_error::CodecError::unverified(
            "native comparison raster is incomplete",
        )),
        Err(error) => Err(Python::attach(|py| {
            crate::codec_error::CodecError::from_python(py, error)
        })),
    }
}

pub fn same_frame(x: &[u8], y: &[u8]) -> Option<bool> {
    same_frame_external(x, y).ok().flatten()
}

pub fn same_image_typed(
    a: &std::path::Path,
    b: &std::path::Path,
) -> Result<Option<bool>, crate::codec_error::CodecError> {
    use crate::codec_error::CodecError;
    use crate::codec_static_identity::{
        content_key_classified, perceptual_hash_classified, validate_identity_source,
    };
    Python::initialize();
    Python::attach(|py| {
        let external = |error: PyErr| {
            let caught = error.is_instance_of::<pyo3::exceptions::PyException>(py);
            (CodecError::from_python(py, error), caught)
        };
        let result = (|| -> Result<_, (CodecError, bool)> {
            validate_identity_source(a).map_err(|error| (error, false))?;
            let key_a = content_key_classified(py, a)?;
            validate_identity_source(b).map_err(|error| (error, false))?;
            let key_b = content_key_classified(py, b)?;
            if key_a == key_b {
                return Ok(Some(true));
            }
            let left = perceptual_hash_classified(py, a)?;
            let right = perceptual_hash_classified(py, b)?;
            let (Some(left), Some(right)) = (left, right) else {
                return Ok(None);
            };
            if (left ^ right).count_ones() > 6 {
                return Ok(Some(false));
            }
            let left = comparison_pixels(py, a).map_err(external)?;
            let right = comparison_pixels(py, b).map_err(external)?;
            close_pixels(&left, &right).map(Some).ok_or_else(|| {
                (
                    CodecError::unverified("native comparison raster is incomplete"),
                    false,
                )
            })
        })();
        match result {
            Ok(value) => Ok(value),
            Err((_, true)) => Ok(None),
            Err((error, false)) => Err(error),
        }
    })
}

pub fn same_image(a: &std::path::Path, b: &std::path::Path) -> Option<bool> {
    same_image_typed(a, b).ok().flatten()
}

pub(crate) fn normalized_pixels<'py>(
    py: Python<'py>,
    image: &Bound<'py, PyAny>,
) -> PyResult<Vec<u8>> {
    let rgba = OpenImage(image.call_method1("convert", ("RGBA",))?);
    let normalized = normalized_rgba(py, &rgba.0)?;
    pixels(&normalized.0)
}

pub(crate) fn normalized_rgba<'py>(
    py: Python<'py>,
    rgba: &Bound<'py, PyAny>,
) -> PyResult<OpenImage<'py>> {
    resize_lanczos(py, rgba, (64, 64))
}

fn resize_lanczos<'py>(
    py: Python<'py>,
    image: &Bound<'py, PyAny>,
    size: (u32, u32),
) -> PyResult<OpenImage<'py>> {
    // Source expressions resolve the resize descriptor before their direct Image.LANCZOS argument.
    let resize = image.getattr("resize")?;
    let lanczos = py.import("PIL.Image")?.getattr("LANCZOS")?;
    resize.call1((size, lanczos)).map(OpenImage)
}

pub(crate) fn pixels(image: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    Ok(image
        .call_method0("tobytes")?
        .cast_into::<PyBytes>()?
        .as_bytes()
        .to_vec())
}
pub(crate) fn digest_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
/// Source alpha calls retain their exceptions; None is native structural uncertainty, not blankness.
pub(crate) fn blank_image_external<'py>(
    py: Python<'py>,
    image: &Bound<'py, PyAny>,
) -> PyResult<Option<bool>> {
    let rgba = OpenImage(image.call_method1("convert", ("RGBA",))?);
    let channels = rgba.0.call_method0("split")?;
    let alpha = OpenImage(channels.get_item(3)?);
    // Other split bands are independent Pillow images too; close all of them on every path.
    let _colors = channels
        .cast::<pyo3::types::PyTuple>()
        .ok()
        .map(|bands| bands.iter().take(3).map(OpenImage).collect::<Vec<_>>());
    if alpha.0.call_method0("getbbox")?.is_none() {
        return Ok(Some(true));
    }
    let histogram = alpha.0.call_method0("histogram")?;
    let Ok(bins) = histogram.cast::<pyo3::types::PyList>() else {
        return Ok(None);
    };
    if bins.len() != 256 {
        return Ok(None);
    }
    let mut visible = pyo3::types::PyInt::new(py, 0).into_any();
    for (index, count) in bins.iter().enumerate() {
        if !count.is_instance_of::<pyo3::types::PyInt>() || count.lt(0)? {
            return Ok(None);
        }
        if index > 10 {
            visible = visible.add(count)?;
        }
    }
    Ok(Some(visible.le(8)?))
}

pub(crate) fn blank_image_typed<'py>(
    py: Python<'py>,
    image: &Bound<'py, PyAny>,
) -> Result<bool, crate::codec_error::CodecError> {
    blank_image_external(py, image)
        .map_err(|error| crate::codec_error::CodecError::from_python(py, error))?
        .ok_or_else(|| {
            crate::codec_error::CodecError::unverified("native alpha histogram is unverified")
        })
}

pub(crate) fn blank_rgba(raw: &[u8]) -> bool {
    raw.iter()
        .skip(3)
        .step_by(4)
        .filter(|&&alpha| alpha > 10)
        .count()
        <= 8
}

fn premultiplied<'py>(
    py: Python<'py>,
    module: &Bound<'py, PyModule>,
    image: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let channels = image.call_method0("split")?;
    let channels = (0..4)
        .map(|index| channels.get_item(index).map(OpenImage))
        .collect::<PyResult<Vec<_>>>()?;
    let alpha = &channels[3].0;
    // Source resolves merge first, then each multiply independently while evaluating its RGB tuple.
    let merge = module.getattr("merge")?;
    let chops = py.import("PIL.ImageChops")?;
    let red = OpenImage(chops.getattr("multiply")?.call1((&channels[0].0, alpha))?);
    let green = OpenImage(chops.getattr("multiply")?.call1((&channels[1].0, alpha))?);
    let blue = OpenImage(chops.getattr("multiply")?.call1((&channels[2].0, alpha))?);
    merge.call1(("RGBA", (&red.0, &green.0, &blue.0, alpha)))
}

pub(crate) fn dhash<'py>(py: Python<'py>, image: &Bound<'py, PyAny>) -> PyResult<u64> {
    dhash_checked(py, image)?
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid dHash raster"))
}

pub(crate) fn dhash_checked<'py>(
    py: Python<'py>,
    image: &Bound<'py, PyAny>,
) -> PyResult<Option<u64>> {
    let module = py.import("PIL.Image")?;
    // Only dHash gates on the original mode; comparison already supplies normalized RGBA.
    let multiplied = if image.getattr("mode")?.eq("RGBA")? {
        Some(OpenImage(premultiplied(py, &module, image)?))
    } else {
        None
    };
    let raster = multiplied.as_ref().map(|value| &value.0).unwrap_or(image);
    let gray = OpenImage(raster.call_method1("convert", ("L",))?);
    let resized = resize_lanczos(py, &gray.0, (9, 8))?;
    let bytes = pixels(&resized.0)?;
    if bytes.len() != 72 {
        return Ok(None);
    }
    let mut bits = 0_u64;
    for row in bytes.as_chunks::<9>().0 {
        for pair in row.windows(2) {
            bits = (bits << 1) | u64::from(pair[0] > pair[1]);
        }
    }
    Ok(Some(bits))
}
