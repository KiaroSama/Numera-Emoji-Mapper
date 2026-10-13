//! Native preview policy and publication around the unchanged external Lottie renderer.
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Animation<'py>(Bound<'py, PyAny>);
impl Drop for Animation<'_> {
    fn drop(&mut self) {
        let _ = self.0.call_method0("lottie_animation_destroy");
    }
}
struct Image<'py>(Bound<'py, PyAny>);
impl Drop for Image<'_> {
    fn drop(&mut self) {
        let _ = self.0.call_method0("close");
    }
}

fn render(
    source: &Path,
    staged: &Path,
    size: u32,
    fps: u32,
    still: bool,
    quality: u32,
) -> Result<(), String> {
    let document = crate::codec_lottie_load::load(source).map_err(|error| error.message)?;
    let text = serde_json::to_string(&document).map_err(|_| "cannot serialize preview document")?;
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = py.import("json")?;
        let doc = module.call_method1("loads", (text,))?;
        let text = module.call_method1("dumps", (doc,))?;
        let animation = Animation(
            py.import("rlottie_python")?
                .getattr("LottieAnimation")?
                .call_method1("from_data", (text,))?,
        );
        let options = PyDict::new(py);
        options.set_item("width", size)?;
        options.set_item("height", size)?;
        if still {
            options.set_item("frame_num", 0)?;
            let frame = Image(animation.0.call_method(
                "render_pillow_frame",
                (),
                Some(&options),
            )?);
            let encoding = PyDict::new(py);
            encoding.set_item("lossless", false)?;
            encoding.set_item("quality", quality)?;
            encoding.set_item("method", 4)?;
            frame
                .0
                .call_method("save", (staged.to_path_buf(),), Some(&encoding))?;
        } else {
            options.set_item("lossless", false)?;
            options.set_item("quality", quality)?;
            options.set_item("method", 4)?;
            let duration = animation
                .0
                .call_method0("lottie_animation_get_duration")
                .and_then(|value| value.extract::<f64>())
                .unwrap_or(0.0);
            // Source clips shorter than one sample keep their native rate, never an empty frame list.
            if duration * f64::from(fps) >= 1.0 {
                options.set_item("fps", fps)?;
            }
            animation.0.call_method(
                "save_animation",
                (staged.to_string_lossy().as_ref(),),
                Some(&options),
            )?;
        }
        Ok(())
    })
    .map_err(|_| "external Lottie preview rendering failed".into())
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
    if !matches!(size, 52 | 72 | 104) || !(1..=30).contains(&fps) {
        return Err("invalid preview parameters".into());
    }
    preview_encoded(source, output, size, fps, still, 60)
}

pub fn gallery(source: &Path, output: &Path, still: bool) -> Result<Value, String> {
    preview_encoded(source, output, 88, 9, still, 65)
}

fn preview_encoded(
    source: &Path,
    output: &Path,
    size: u32,
    fps: u32,
    still: bool,
    quality: u32,
) -> Result<Value, String> {
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err("preview output must be absolute and must not already exist".into());
    }
    let parent = output.parent().ok_or("preview output has no parent")?;
    std::fs::create_dir_all(parent).map_err(|_| "cannot create preview output directory")?;
    let staged = stage(parent)?;
    let result = (|| {
        render(source, &staged, size, fps, still, quality)?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&staged)
            .map_err(|_| "cannot read rendered preview")?;
        file.sync_all()
            .map_err(|_| "cannot flush rendered preview")?;
        let bytes = file
            .metadata()
            .map_err(|_| "cannot inspect rendered preview")?
            .len();
        drop(file);
        crate::atomic::publish_new(&staged, output)
            .map_err(|_| "cannot publish exclusive preview")?;
        Ok(json!({"bytes":bytes}))
    })();
    match std::fs::remove_file(&staged) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("cannot clean staged Lottie preview".into()),
    }
    result
}
