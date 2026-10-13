//! Telegram video validation preserves source refusal order and complete alpha-sample blank checks.
use crate::codec_video::VideoDecoder;
use serde_json::Value;
use std::path::Path;

pub fn validate_info(info: &Value, bytes: u64, filename: &str) -> Result<(), String> {
    let width = info["width"].as_u64().ok_or("video width is unknown")?;
    let height = info["height"].as_u64().ok_or("video height is unknown")?;
    let duration = info["duration"]
        .as_f64()
        .ok_or("video duration is unknown")?;
    let codec = info["codec"].as_str().ok_or("video codec is unknown")?;
    let fps = info["fps"].as_f64().ok_or("video frame rate is unknown")?;
    let audio = info["has_audio"]
        .as_bool()
        .ok_or("video audio state is unknown")?;
    let container = info["container"]
        .as_str()
        .ok_or("video container is unknown")?;
    if !duration.is_finite() || !fps.is_finite() {
        return Err("nonfinite video metadata".into());
    }
    let mut problems = Vec::new();
    if (width, height) != (100, 100) {
        problems.push(format!("dimensions {width}x{height} != 100x100"));
    }
    if duration <= 0.0 {
        problems.push("duration is zero or unknown".into());
    } else if duration > 3.05 {
        problems.push(format!("duration {duration:.2}s > 3.0s"));
    }
    if codec != "vp9" {
        problems.push(format!(
            "codec {} != 'vp9'",
            crate::python_repr::quoted(codec)
        ));
    }
    if audio {
        problems.push("contains an audio stream (video emoji must have none)".into());
    }
    if fps > 30.01 {
        problems.push(format!("{fps:.2} fps > 30 fps"));
    }
    if !container.split(',').any(|part| part == "webm") {
        problems.push(format!(
            "container {} is not webm",
            crate::python_repr::quoted(container)
        ));
    }
    if bytes > 256 * 1024 {
        problems.push(format!("size {bytes} > 262144 bytes"));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{filename}: {}", problems.join("; ")))
    }
}

pub async fn validate(decoder: &VideoDecoder, path: &Path) -> Result<(), String> {
    let info = decoder.probe(path).await?;
    let bytes = std::fs::metadata(path)
        .map_err(|_| "cannot inspect video size")?
        .len();
    let filename = path
        .file_name()
        .ok_or("video has no filename")?
        .to_string_lossy();
    validate_info(&info, bytes, &filename)
}

pub async fn is_blank(decoder: &VideoDecoder, path: &Path) -> Result<bool, String> {
    let raw = decoder.frames_rgba(path, 4).await?;
    let blank = raw
        .as_chunks::<{ crate::codec_video_timeline::FRAME_BYTES }>()
        .0
        .iter()
        .all(|frame| {
            frame
                .iter()
                .skip(3)
                .step_by(4)
                .filter(|&&alpha| alpha > 10)
                .count()
                <= 8
        });
    Ok(!raw.is_empty() && raw.len() >= crate::codec_video_timeline::FRAME_BYTES && blank)
}
