//! Rust-owned frame sampling and WebP options around unchanged external Pillow encoding.
use crate::codec_error::CodecError;
use crate::codec_video::VideoDecoder;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;

use crate::codec_raster::OpenImage as Image;

#[derive(Clone, Copy)]
enum PreviewErrors {
    Legacy,
    Source,
}
impl PreviewErrors {
    fn python(self, py: Python<'_>, error: PyErr) -> CodecError {
        match self {
            Self::Legacy => CodecError::unverified("external video preview encoding failed"),
            Self::Source => CodecError::from_python(py, error),
        }
    }
}

pub fn gallery_typed(
    source: &Path,
    output: &Path,
) -> Result<Value, crate::codec_error::CodecError> {
    use crate::codec_error::CodecError;
    crate::codec_video::VideoDecoder::validate_source(source).map_err(CodecError::value)?;
    if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
        return Err(CodecError::value("gallery output already exists"));
    }
    Python::initialize();
    let bytes = Python::attach(|py| {
        crate::codec_lottie_load::source_bytes(py, source)
            .map_err(|error| CodecError::from_python(py, error))
    })?;
    let staged =
        crate::codec_video_encode::Staged::new(output, "webm").map_err(CodecError::unverified)?;
    let result = std::fs::write(&staged.0, &bytes)
        .map_err(|_| CodecError::unverified("cannot stage gallery video"))
        .and_then(|_| staged.publish(output).map_err(CodecError::unverified))
        .map(|bytes| json!({"bytes":bytes}));
    staged.clean().map_err(CodecError::unverified)?;
    result
}

pub async fn preview(
    decoder: &VideoDecoder,
    source: &Path,
    output: &Path,
    size: u32,
    fps: u32,
    still: bool,
) -> Result<Value, String> {
    preview_with(
        decoder,
        source,
        output,
        size,
        fps,
        still,
        PreviewErrors::Legacy,
    )
    .await
    .map_err(|error| error.message)
}

pub fn preview_at(
    root: Option<&Path>,
    source: &Path,
    output: &Path,
    size: u32,
    fps: u32,
    still: bool,
) -> Result<Value, CodecError> {
    let root =
        root.ok_or_else(|| CodecError::value("video worker requires an explicit project root"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| CodecError::unverified("cannot initialize video worker runtime"))?;
    let decoder = VideoDecoder::new(root).map_err(CodecError::unverified)?;
    runtime.block_on(preview_typed(&decoder, source, output, size, fps, still))
}

pub async fn preview_typed(
    decoder: &VideoDecoder,
    source: &Path,
    output: &Path,
    size: u32,
    fps: u32,
    still: bool,
) -> Result<Value, CodecError> {
    preview_with(
        decoder,
        source,
        output,
        size,
        fps,
        still,
        PreviewErrors::Source,
    )
    .await
}

async fn preview_with(
    decoder: &VideoDecoder,
    source: &Path,
    output: &Path,
    size: u32,
    fps: u32,
    still: bool,
    errors: PreviewErrors,
) -> Result<Value, CodecError> {
    if !matches!(size, 52 | 72 | 104) || !(1..=30).contains(&fps) {
        return Err(CodecError::value("invalid preview parameters"));
    }
    let input = match errors {
        PreviewErrors::Legacy => decoder
            .decoder_args(source)
            .await
            .map_err(CodecError::unverified)?,
        PreviewErrors::Source => decoder.decoder_args_typed(source).await?,
    };
    let staged =
        crate::codec_video_encode::Staged::new(output, "webp").map_err(CodecError::unverified)?;
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
        let settled = crate::codec_process::capture_outcome_input(
            &decoder.root,
            &decoder.ffmpeg,
            &args,
            u64::from(size) * u64::from(size) * 4 * 90,
            None,
        )
        .await
        .map_err(CodecError::unverified)?;
        let captured = match errors {
            PreviewErrors::Legacy => settled.into_legacy().map_err(CodecError::unverified)?,
            PreviewErrors::Source => crate::codec_video_error::capture_result(
                &decoder.ffmpeg,
                &args,
                crate::codec_video_error::CommandContext::Direct,
                settled,
            )
            .map_err(|error| error.typed(source))?,
        };
        let raw = captured.stdout;
        let step = size as usize * size as usize * 4;
        if raw.len() < step {
            return Err(CodecError::value("no video preview frames decoded"));
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
            let encoded = (|| -> PyResult<Vec<u8>> {
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
            })();
            // Source finally closes forward; its first close error replaces the encoding result.
            // Remaining-frame/buffer Drop cleanup is a stronger native ownership floor.
            for frame in frames {
                frame.close()?;
            }
            encoded
        })
        .map_err(|error| Python::attach(|py| errors.python(py, error)))?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&staged.0)
            .map_err(|_| CodecError::unverified("cannot open staged video preview"))?;
        file.write_all(&bytes)
            .map_err(|_| CodecError::unverified("cannot write video preview"))?;
        file.sync_all()
            .map_err(|_| CodecError::unverified("cannot flush video preview"))?;
        drop(file);
        staged
            .publish(output)
            .map_err(CodecError::unverified)
            .map(|bytes| json!({"bytes":bytes}))
    }
    .await;
    staged.clean().map_err(CodecError::unverified)?;
    result
}
