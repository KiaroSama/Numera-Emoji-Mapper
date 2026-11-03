//! Real animated WebP and screenshot pixels through typed external Pillow, with explicit closure.
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

pub struct Animated {
    pub moving: Vec<u8>,
    pub still: Vec<u8>,
}
struct Images<'py>(Vec<Bound<'py, PyAny>>);
impl Images<'_> {
    fn close(&mut self) -> PyResult<()> {
        let mut first = None;
        while let Some(image) = self.0.pop() {
            if let Err(error) = image.call_method0("close") {
                eprintln!("ERROR: browser media close failed: {error}");
                first.get_or_insert(error);
            }
        }
        match first {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
impl Drop for Images<'_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

pub fn animated(py: Python<'_>) -> PyResult<Animated> {
    let pillow = py.import("PIL.Image")?;
    let io = py.import("io")?;
    let mut frames = Images(Vec::new());
    for i in 0..28 {
        let image = pillow.call_method1("new", ("RGBA", (104, 104), (0, 0, 0, 0)))?;
        frames.0.push(image);
        let x = 4 + i * 3;
        frames
            .0
            .last()
            .unwrap()
            .call_method1("paste", ((40, 160, 255, 255), (x, 36, x + 32, 68)))?;
    }
    let mut buffers = Images(Vec::new());
    buffers.0.push(io.call_method0("BytesIO")?);
    buffers.0.push(io.call_method0("BytesIO")?);
    let moving = PyDict::new(py);
    for (key, value) in [
        ("duration", 66),
        ("loop", 0),
        ("quality", 60),
        ("method", 4),
    ] {
        moving.set_item(key, value)?;
    }
    moving.set_item("save_all", true)?;
    moving.set_item("append_images", &frames.0[1..])?;
    frames.0[0].call_method("save", (&buffers.0[0], "WEBP"), Some(&moving))?;
    let still = PyDict::new(py);
    still.set_item("quality", 60)?;
    still.set_item("method", 4)?;
    frames.0[0].call_method("save", (&buffers.0[1], "WEBP"), Some(&still))?;
    let result = Animated {
        moving: buffers.0[0].call_method0("getvalue")?.extract()?,
        still: buffers.0[1].call_method0("getvalue")?.extract()?,
    };
    let frames_closed = frames.close();
    let buffers_closed = buffers.close();
    frames_closed?;
    buffers_closed?;
    Ok(result)
}

pub fn checker_pixels(py: Python<'_>, screenshot: &[u8]) -> PyResult<Vec<(u8, u8, u8)>> {
    let buffer = py
        .import("io")?
        .call_method1("BytesIO", (PyBytes::new(py, screenshot),))?;
    let mut images = Images(vec![buffer]);
    let image = py
        .import("PIL.Image")?
        .call_method1("open", (&images.0[0],))?;
    images.0.push(image);
    let rgb = images.0[1].call_method1("convert", ("RGB",))?;
    images.0.push(rgb);
    let pixels = [(4, 4), (12, 4), (4, 12), (12, 12)]
        .into_iter()
        .map(|point| images.0[2].call_method1("getpixel", (point,))?.extract())
        .collect::<PyResult<Vec<(u8, u8, u8)>>>();
    let closed = images.close();
    let pixels = pixels?;
    closed?;
    Ok(pixels)
}
