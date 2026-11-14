//! Native preview policy and publication around the unchanged external Lottie renderer.
use crate::codec_error::CodecError;
use crate::codec_raster::OpenImage;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Animation<'py>(Option<Bound<'py, PyAny>>);
impl Animation<'_> {
    fn destroy(mut self) -> PyResult<()> {
        if let Some(animation) = self.0.take() {
            animation.call_method0("lottie_animation_destroy")?;
        }
        Ok(())
    }
}
impl Drop for Animation<'_> {
    fn drop(&mut self) {
        if let Some(animation) = self.0.take() {
            let _ = animation.call_method0("lottie_animation_destroy");
        }
    }
}

fn render(
    source: &Path,
    staged: &Path,
    size: u32,
    fps: u32,
    still: bool,
    quality: u32,
) -> Result<(), CodecError> {
    Python::initialize();
    Python::attach(|py| {
        let document =
            crate::codec_lottie_load::load_object(py, source).map_err(|error| CodecError {
                kind: error.kind,
                message: error.message,
                handled: error.handled,
            })?;
        let result = (|| -> PyResult<()> {
            let text = py.import("json")?.call_method1("dumps", (document,))?;
            let animation = Animation(Some(
                py.import("rlottie_python")?
                    .getattr("LottieAnimation")?
                    .call_method1("from_data", (text,))?,
            ));
            let options = PyDict::new(py);
            options.set_item("width", size)?;
            options.set_item("height", size)?;
            if still {
                options.set_item("frame_num", 0)?;
                let frame = animation
                    .0
                    .as_ref()
                    .expect("live renderer")
                    .call_method("render_pillow_frame", (), Some(&options))
                    .map(OpenImage);
                // Source destroys in finally before saving the independent still.
                // A destroy error replaces a render error, but never skips frame cleanup.
                animation.destroy()?;
                let frame = frame?;
                let encoding = PyDict::new(py);
                encoding.set_item("lossless", false)?;
                encoding.set_item("quality", quality)?;
                encoding.set_item("method", 4)?;
                let saved = frame
                    .0
                    .call_method("save", (staged.to_path_buf(),), Some(&encoding));
                let closed = frame.close();
                saved?;
                closed?;
            } else {
                let result = (|| -> PyResult<()> {
                    options.set_item("lossless", false)?;
                    options.set_item("quality", quality)?;
                    options.set_item("method", 4)?;
                    let duration = animation
                        .0
                        .as_ref()
                        .expect("live renderer")
                        .call_method0("lottie_animation_get_duration")
                        .and_then(|value| py.import("builtins")?.getattr("float")?.call1((value,)))
                        .and_then(|value| value.extract::<f64>());
                    let duration = match duration {
                        Ok(value) => value,
                        Err(error) if error.is_instance_of::<pyo3::exceptions::PyException>(py) => {
                            0.0
                        }
                        Err(error) => return Err(error),
                    };
                    // Short clips keep their native rate, never an empty sampled frame list.
                    if duration * f64::from(fps) >= 1.0 {
                        options.set_item("fps", fps)?;
                    }
                    animation.0.as_ref().expect("live renderer").call_method(
                        "save_animation",
                        (staged.to_string_lossy().as_ref(),),
                        Some(&options),
                    )?;
                    Ok(())
                })();
                animation.destroy()?;
                result?;
            }
            Ok(())
        })();
        result.map_err(|error| CodecError::from_python(py, error))
    })
}

fn stage(parent: &Path) -> Result<PathBuf, String> {
    loop {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".lottie-preview-{}-{id}.webp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("cannot stage Lottie preview".into()),
        }
    }
}

pub fn preview(
    source: &Path,
    output: &Path,
    size: u32,
    fps: u32,
    still: bool,
) -> Result<Value, String> {
    preview_typed(source, output, size, fps, still).map_err(|error| error.message)
}

pub fn preview_typed(
    source: &Path,
    output: &Path,
    size: u32,
    fps: u32,
    still: bool,
) -> Result<Value, CodecError> {
    if !matches!(size, 52 | 72 | 104) || !(1..=30).contains(&fps) {
        return Err(CodecError::value("invalid preview parameters"));
    }
    preview_encoded(source, output, size, fps, still, 60)
}

pub fn gallery(source: &Path, output: &Path, still: bool) -> Result<Value, String> {
    gallery_typed(source, output, still).map_err(|error| error.message)
}

pub fn gallery_typed(source: &Path, output: &Path, still: bool) -> Result<Value, CodecError> {
    preview_encoded(source, output, 88, 9, still, 65)
}

fn preview_encoded(
    source: &Path,
    output: &Path,
    size: u32,
    fps: u32,
    still: bool,
    quality: u32,
) -> Result<Value, CodecError> {
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err(CodecError::value(
            "preview output must be absolute and must not already exist",
        ));
    }
    let parent = output
        .parent()
        .ok_or_else(|| CodecError::value("preview output has no parent"))?;
    std::fs::create_dir_all(parent)
        .map_err(|_| CodecError::unverified("cannot create preview output directory"))?;
    let staged = stage(parent).map_err(CodecError::unverified)?;
    let result = (|| {
        render(source, &staged, size, fps, still, quality)?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&staged)
            .map_err(|_| CodecError::unverified("cannot read rendered preview"))?;
        file.sync_all()
            .map_err(|_| CodecError::unverified("cannot flush rendered preview"))?;
        let bytes = file
            .metadata()
            .map_err(|_| CodecError::unverified("cannot inspect rendered preview"))?
            .len();
        drop(file);
        crate::atomic::publish_new(&staged, output)
            .map_err(|_| CodecError::unverified("cannot publish exclusive preview"))?;
        Ok(json!({"bytes":bytes}))
    })();
    match std::fs::remove_file(&staged) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(CodecError::unverified("cannot clean staged Lottie preview")),
    }
    result
}
