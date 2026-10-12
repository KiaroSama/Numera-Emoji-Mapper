//! Deterministic external gzip encoding with native validation and exclusive publication.
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyTuple};
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Closed<'py>(Bound<'py, PyAny>);
impl Drop for Closed<'_> {
    fn drop(&mut self) {
        let _ = self.0.call_method0("close");
    }
}

pub fn encode(document: &Value, ensure_ascii: bool) -> Result<Vec<u8>, String> {
    let text = serde_json::to_string(document).map_err(|_| "cannot serialize Lottie document")?;
    Python::initialize();
    Python::attach(|py| -> PyResult<Vec<u8>> {
        let json = py.import("json")?;
        let doc = json.call_method1("loads", (text,))?;
        let options = PyDict::new(py);
        options.set_item("separators", PyTuple::new(py, [",", ":"])?)?;
        options.set_item("ensure_ascii", ensure_ascii)?;
        let raw = json
            .call_method("dumps", (doc,), Some(&options))?
            .call_method1("encode", ("utf-8",))?;
        let buffer = Closed(py.import("io")?.call_method0("BytesIO")?);
        let options = PyDict::new(py);
        options.set_item("filename", "")?;
        options.set_item("fileobj", &buffer.0)?;
        options.set_item("mode", "wb")?;
        options.set_item("mtime", 0)?;
        let gzip = Closed(
            py.import("gzip")?
                .call_method("GzipFile", (), Some(&options))?,
        );
        gzip.0.call_method1("write", (raw,))?;
        gzip.0.call_method0("close")?;
        Ok(buffer
            .0
            .call_method0("getvalue")?
            .cast_into::<PyBytes>()?
            .as_bytes()
            .to_vec())
    })
    .map_err(|_| "external deterministic TGS encoding failed".into())
}

fn canvas_dimension(value: Option<&Value>) -> Result<i64, String> {
    match value {
        None => Ok(0),
        Some(Value::Bool(value)) => Ok(i64::from(*value)),
        Some(Value::Number(value)) => value
            .as_i64()
            .or_else(|| value.as_f64().map(|v| v.trunc() as i64))
            .ok_or_else(|| "canvas dimension cannot be converted to integer".into()),
        Some(Value::String(value)) => value
            .trim()
            .parse::<i64>()
            .map_err(|_| "canvas dimension cannot be converted to integer".into()),
        _ => Err("canvas dimension cannot be converted to integer".into()),
    }
}

pub fn package(source: &Path, output: &Path) -> Result<(), String> {
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err("TGS output must be absolute and must not already exist".into());
    }
    let document = crate::codec_lottie_load::load(source).map_err(|error| error.message)?;
    let w = canvas_dimension(document.get("w"))?;
    let h = canvas_dimension(document.get("h"))?;
    if (w, h) != (512, 512) {
        let name = source
            .file_name()
            .ok_or("source has no filename")?
            .to_string_lossy();
        return Err(format!(
            "{name}: animated emoji require a 512x512 Lottie canvas, got {w}x{h}. Re-export the animation at 512x512."
        ));
    }
    let bytes = encode(&document, false)?;
    let name = output
        .file_name()
        .ok_or("output has no filename")?
        .to_string_lossy();
    crate::codec_lottie_validate::validate_document(&document, &name, bytes.len() as u64)?;
    let parent = output.parent().ok_or("TGS output has no parent")?;
    std::fs::create_dir_all(parent).map_err(|_| "cannot create TGS output directory")?;
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
            Err(_) => return Err("cannot stage TGS output".into()),
        }
    };
    let result = file.write_all(&bytes).and_then(|_| file.sync_all());
    drop(file);
    let result = result.and_then(|_| crate::atomic::publish_new(&staged, output));
    let _ = std::fs::remove_file(staged);
    result.map_err(|_| "cannot publish exclusive TGS output".into())
}
