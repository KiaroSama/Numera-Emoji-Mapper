//! Rust-owned frame sampling and WebP options around unchanged external Pillow encoding.
use crate::codec_video::VideoDecoder;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;

struct Image<'py>(Bound<'py, PyAny>);
impl Drop for Image<'_> {
    fn drop(&mut self) {
        let _ = self.0.call_method0("close");
    }
}

pub async fn preview(
    decoder: &VideoDecoder,
    source: &Path,
    output: &Path,
    size: u32,
    fps: u32,
    still: bool,
) -> Result<Value, String> {
    if !matches!(size, 52 | 72 | 104) || !(1..=30).contains(&fps) {
        return Err("invalid preview parameters".into());
    }
    let input = decoder.decoder_args(source).await?;
    let staged = crate::codec_video_encode::Staged::new(output, "webp")?;
    let result = async {
        let mut args = vec!["-nostdin".into(), "-v".into(), "error".into()];
        args.extend(input);
        args.extend([
            "-i".into(),
            source.to_string_lossy().into_owned(),
            "-frames:v".into(),
            if still { "1".into() } else { "90".into() },
            "-an".into(),
            "-threads".into(),
            "1".into(),
            "-vf".into(),
            format!(
                "{}scale={size}:{size},format=rgba",
                if still {
                    String::new()
                } else {
                    format!("fps={fps},")
                }
            ),
            "-f".into(),
            "rawvideo".into(),
            "-".into(),
        ]);
        let raw = crate::codec_process::capture(
            &decoder.root,
            &decoder.ffmpeg,
            &args,
            u64::from(size) * u64::from(size) * 4 * 90,
        )
        .await?
        .stdout;
        let step = size as usize * size as usize * 4;
        if raw.len() < step {
            return Err("no video preview frames decoded".into());
        }
        Python::initialize();
        let bytes = Python::attach(|py| -> PyResult<Vec<u8>> {
            let module = py.import("PIL.Image")?;
            let mut frames = Vec::new();
            for frame in raw.chunks_exact(step) {
                frames.push(Image(module.call_method1(
                    "frombytes",
                    ("RGBA", (size, size), PyBytes::new(py, frame)),
                )?));
            }
            let options = PyDict::new(py);
            if frames.len() == 1 {
                options.set_item("lossless", true)?;
                options.set_item("exact", true)?;
            } else {
                options.set_item("save_all", true)?;
                options.set_item(
                    "append_images",
                    PyList::new(py, frames.iter().skip(1).map(|frame| &frame.0))?,
                )?;
                options.set_item(
                    "duration",
                    (1000.0 / f64::from(fps)).round_ties_even() as u32,
                )?;
                options.set_item("loop", 0)?;
                options.set_item("lossless", false)?;
                options.set_item("quality", 60)?;
                options.set_item("method", 4)?;
            }
            let buffer = Image(py.import("io")?.call_method0("BytesIO")?);
            frames[0]
                .0
                .call_method("save", (&buffer.0, "WEBP"), Some(&options))?;
            Ok(buffer
                .0
                .call_method0("getvalue")?
                .cast_into::<PyBytes>()?
                .as_bytes()
                .to_vec())
        })
        .map_err(|_| "external video preview encoding failed".to_owned())?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&staged.0)
            .map_err(|_| "cannot open staged video preview")?;
        file.write_all(&bytes)
            .map_err(|_| "cannot write video preview")?;
        file.sync_all().map_err(|_| "cannot flush video preview")?;
        drop(file);
        staged.publish(output).map(|bytes| json!({"bytes":bytes}))
    }
    .await;
    staged.clean()?;
    result
}
