//! Deterministic external gzip encoding with native validation and exclusive publication.
use crate::codec_error::CodecError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyTuple};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Closed<'py>(Option<Bound<'py, PyAny>>);
impl<'py> Closed<'py> {
    fn value(&self) -> &Bound<'py, PyAny> {
        self.0.as_ref().expect("open encoding stream")
    }
    fn close(mut self) -> PyResult<()> {
        if let Some(stream) = self.0.take() {
            stream.call_method0("close")?;
        }
        Ok(())
    }
}
impl Drop for Closed<'_> {
    fn drop(&mut self) {
        if let Some(stream) = self.0.take() {
            let _ = stream.call_method0("close");
        }
    }
}

pub(crate) fn encode_object(
    py: Python<'_>,
    document: &Bound<'_, PyAny>,
    ensure_ascii: bool,
) -> PyResult<Vec<u8>> {
    let options = PyDict::new(py);
    options.set_item("separators", PyTuple::new(py, [",", ":"])?)?;
    options.set_item("ensure_ascii", ensure_ascii)?;
    let raw = py
        .import("json")?
        .call_method("dumps", (document,), Some(&options))?
        .call_method1("encode", ("utf-8",))?;
    let buffer = Closed(Some(py.import("io")?.call_method0("BytesIO")?));
    let options = PyDict::new(py);
    options.set_item("filename", "")?;
    options.set_item("fileobj", buffer.value())?;
    options.set_item("mode", "wb")?;
    options.set_item("mtime", 0)?;
    let gzip = py
        .import("gzip")?
        .call_method("GzipFile", (), Some(&options))?;
    // The actual context exit must finish its trailer before bytes are inspected or published.
    crate::codec_lottie_load::with_gzip(gzip, |entered| entered.call_method1("write", (raw,)))?;
    let bytes = buffer
        .value()
        .call_method0("getvalue")?
        .cast_into::<PyBytes>()?
        .as_bytes()
        .to_vec();
    buffer.close()?;
    Ok(bytes)
}

pub fn package(source: &Path, output: &Path) -> Result<(), String> {
    package_typed(source, output).map_err(|error| error.message)
}

pub fn package_typed(source: &Path, output: &Path) -> Result<(), CodecError> {
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err(CodecError::value(
            "TGS output must be absolute and must not already exist",
        ));
    }
    Python::initialize();
    let bytes = Python::attach(|py| -> Result<Vec<u8>, CodecError> {
        let document =
            crate::codec_lottie_load::load_object(py, source).map_err(|error| CodecError {
                kind: error.kind,
                message: error.message,
                handled: error.handled,
            })?;
        let (w, h) = (|| -> PyResult<_> {
            let int = py.import("builtins")?.getattr("int")?;
            let w = int.call1((document.call_method1("get", ("w", 0))?,))?;
            let h = int.call1((document.call_method1("get", ("h", 0))?,))?;
            Ok((w, h))
        })()
        .map_err(|error| CodecError::from_python(py, error))?;
        let valid = w
            .eq(512)
            .and_then(|width| if width { h.eq(512) } else { Ok(false) })
            .map_err(|error| CodecError::from_python(py, error))?;
        if !valid {
            let name = source
                .file_name()
                .ok_or_else(|| CodecError::value("source has no filename"))?
                .to_string_lossy();
            let w = w
                .str()
                .map_err(|error| CodecError::from_python(py, error))?;
            let h = h
                .str()
                .map_err(|error| CodecError::from_python(py, error))?;
            return Err(CodecError::media(format!(
                "{name}: animated emoji require a 512x512 Lottie canvas, got {w}x{h}. Re-export the animation at 512x512."
            )));
        }
        let bytes = encode_object(py, &document, false)
            .map_err(|error| CodecError::from_python(py, error))?;
        // Packaging plain JSON must still enforce the generated gzip's decoded-size cap.
        crate::codec_lottie_validate::validate_encoded(py, &bytes, output, bytes.len() as u64)?;
        Ok(bytes)
    })?;
    let parent = output
        .parent()
        .ok_or_else(|| CodecError::value("TGS output has no parent"))?;
    std::fs::create_dir_all(parent)
        .map_err(|_| CodecError::unverified("cannot create TGS output directory"))?;
    let (staged, mut file) = loop {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".tgs-{}-{id}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => break (path, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(CodecError::unverified("cannot stage TGS output")),
        }
    };
    let result = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = result.and_then(|_| crate::atomic::publish_new(&staged, output));
    match std::fs::remove_file(staged) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(CodecError::unverified("cannot clean staged TGS output")),
    }
    result.map_err(|_| CodecError::unverified("cannot publish exclusive TGS output"))
}
