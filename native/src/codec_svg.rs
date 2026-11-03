//! Exact external SVG renderer call; the caller owns policy and process lifetime.
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::Path;

struct Closed<'py>(Option<Bound<'py, PyAny>>);
impl Closed<'_> {
    fn close(&mut self) -> PyResult<()> {
        if let Some(value) = self.0.take() {
            value.call_method0("close")?;
        }
        Ok(())
    }
}
impl Drop for Closed<'_> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[derive(Clone, Copy)]
enum Policy {
    Legacy,
    Source,
}

fn rendered<'py>(
    py: Python<'py>,
    path: &Path,
    policy: Policy,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    // The source import is outside its renderer-call catch, including import-time exceptions.
    let imported = if matches!(policy, Policy::Source) {
        Some(py.import("resvg_py")?)
    } else {
        None
    };
    let result = (|| -> PyResult<_> {
        let options = PyDict::new(py);
        match policy {
            Policy::Legacy => options.set_item(
                "svg_path",
                path.to_str().ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err("SVG path must be UTF-8")
                })?,
            )?,
            Policy::Source => options.set_item(
                "svg_path",
                py.import("builtins")?
                    .getattr("str")?
                    .call1((path.to_path_buf(),))?,
            )?,
        }
        options.set_item("width", 256)?;
        let renderer = match imported {
            Some(module) => module,
            None => py.import("resvg_py")?,
        };
        renderer.call_method("svg_to_bytes", (), Some(&options))
    })();
    match result {
        Err(error)
            if matches!(policy, Policy::Source)
                && error.is_instance_of::<pyo3::exceptions::PyException>(py) =>
        {
            Ok(None)
        }
        result => result.map(Some),
    }
}

pub fn load_rgba<'py>(py: Python<'py>, path: &Path) -> PyResult<Bound<'py, PyAny>> {
    let bytes = rendered(py, path, Policy::Legacy)?.ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err(
            "legacy SVG renderer unexpectedly returned no result",
        )
    })?;
    decode(py, bytes, Policy::Legacy)
}

pub fn load_rgba_typed<'py>(
    py: Python<'py>,
    path: &Path,
) -> Result<Bound<'py, PyAny>, crate::codec_error::CodecError> {
    let bytes = rendered(py, path, Policy::Source)
        .map_err(|error| crate::codec_error::CodecError::from_python(py, error))?
        .ok_or_else(|| {
            crate::codec_error::CodecError::media(format!(
                "failed to render SVG: {}",
                path.file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
            ))
        })?;
    decode(py, bytes, Policy::Source)
        .map_err(|error| crate::codec_error::CodecError::from_python(py, error))
}

fn decode<'py>(
    py: Python<'py>,
    bytes: Bound<'py, PyAny>,
    policy: Policy,
) -> PyResult<Bound<'py, PyAny>> {
    // bytes(png) is deliberately after the optional renderer call, not inside its catch.
    let bytes = match policy {
        Policy::Legacy => bytes,
        Policy::Source => py.import("builtins")?.getattr("bytes")?.call1((bytes,))?,
    };
    let stream = py.import("io")?.call_method1("BytesIO", (bytes,))?;
    let mut stream_owner = Closed(Some(stream.clone()));
    let image = py.import("PIL.Image")?.call_method1("open", (&stream,))?;
    let mut image_owner = Closed(Some(image.clone()));
    let result = image.call_method1("convert", ("RGBA",));
    let image_closed = image_owner.close();
    let stream_closed = stream_owner.close();
    let closed = image_closed.and(stream_closed);
    match result {
        Err(error) => Err(error),
        Ok(value) => {
            if let Err(error) = closed {
                let _ = value.call_method0("close");
                return Err(error);
            }
            Ok(value)
        }
    }
}
