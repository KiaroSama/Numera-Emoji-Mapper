//! One unmasked fitting body, with explicit source versus legacy external-call ordering.
use crate::codec_raster::OpenImage;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum FitPolicy {
    Legacy,
    Source,
}

pub(super) fn fit<'py>(
    py: Python<'py>,
    image: &Bound<'py, PyAny>,
    policy: FitPolicy,
) -> PyResult<OpenImage<'py>> {
    let module = py.import("PIL.Image")?;
    let converted = if policy == FitPolicy::Source && image.getattr("mode")?.ne("RGBA")? {
        Some(OpenImage(image.call_method1("convert", ("RGBA",))?))
    } else {
        None
    };
    let rgba = converted.as_ref().map(|image| &image.0).unwrap_or(image);
    let legacy_alpha = if policy == FitPolicy::Legacy {
        Some(OpenImage(rgba.call_method1("getchannel", ("A",))?))
    } else {
        None
    };
    let bounds = if let Some(alpha) = &legacy_alpha {
        alpha.0.call_method0("getbbox")?
    } else {
        let bands = rgba.call_method0("split")?;
        // Pillow owns every split band independently, including when index 3 fails.
        let owned = bands
            .cast::<PyTuple>()
            .ok()
            .map(|bands| bands.iter().map(OpenImage).collect::<Vec<_>>());
        let alpha = bands.get_item(3)?;
        let _alpha = owned.is_none().then(|| OpenImage(alpha.clone()));
        alpha.call_method0("getbbox")?
    };
    let crop = if policy == FitPolicy::Source {
        bounds.is_truthy()?
    } else {
        !bounds.is_none()
    };
    let trimmed = if crop {
        Some(OpenImage(rgba.call_method1("crop", (bounds,))?))
    } else {
        None
    };
    let raster = trimmed.as_ref().map(|image| &image.0).unwrap_or(rgba);
    let (width, height) = raster.getattr("size")?.extract::<(u32, u32)>()?;
    let new_canvas = || {
        module
            .call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))
            .map(OpenImage)
    };
    let eager_canvas = if policy == FitPolicy::Legacy {
        Some(new_canvas()?)
    } else {
        None
    };
    if width == 0 || height == 0 {
        return match eager_canvas {
            Some(canvas) => Ok(canvas),
            None => new_canvas(),
        };
    }
    let scale = (100.0 / f64::from(width)).min(100.0 / f64::from(height));
    let w = (f64::from(width) * scale).round_ties_even().max(1.0) as u32;
    let h = (f64::from(height) * scale).round_ties_even().max(1.0) as u32;
    // Python resolves the bound method before evaluating its Image.LANCZOS argument.
    let resize = if policy == FitPolicy::Source {
        Some(raster.getattr("resize")?)
    } else {
        None
    };
    let lanczos = if policy == FitPolicy::Source {
        module.getattr("LANCZOS")?
    } else {
        module.getattr("Resampling")?.getattr("LANCZOS")?
    };
    let resized = OpenImage(match resize {
        Some(resize) => resize.call1(((w, h), lanczos))?,
        None => raster.call_method1("resize", ((w, h), lanczos))?,
    });
    let canvas = match eager_canvas {
        Some(canvas) => canvas,
        None => new_canvas()?,
    };
    // No mask: preserve straight RGB and alpha rather than compositing twice.
    canvas
        .0
        .call_method1("paste", (&resized.0, ((100 - w) / 2, (100 - h) / 2)))?;
    Ok(canvas)
}
