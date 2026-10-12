//! Exact external SVG renderer call; the caller owns policy and process lifetime.
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::Path;

pub fn load_rgba<'py>(py: Python<'py>, path: &Path) -> PyResult<Bound<'py, PyAny>> {
    let options = PyDict::new(py);
    options.set_item(
        "svg_path",
        path.to_str()
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("SVG path must be UTF-8"))?,
    )?;
    options.set_item("width", 256)?;
    let bytes = py
        .import("resvg_py")?
        .call_method("svg_to_bytes", (), Some(&options))?;
    let stream = py.import("io")?.call_method1("BytesIO", (bytes,))?;
    let image = py.import("PIL.Image")?.call_method1("open", (&stream,))?;
    let result = image.call_method1("convert", ("RGBA",));
    let closed = image
        .call_method0("close")
        .and_then(|_| stream.call_method0("close"));
    match result {
        Err(error) => Err(error),
        Ok(value) => {
            closed?;
            Ok(value)
        }
    }
}
