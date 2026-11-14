//! Video admission consumes native lossless metadata in the original refusal order.
use crate::codec_error::CodecError;
use crate::codec_video::{VideoDecoder, VideoInfo};
use crate::codec_video_probe::{MetadataInteger, MetadataString};
use serde_json::Value;
use std::path::Path;

pub fn validate_info(info: &Value, bytes: u64, filename: &str) -> Result<(), String> {
    validate_info_typed(info, bytes, filename).map_err(|error| error.message)
}
pub fn validate_info_typed(info: &Value, bytes: u64, filename: &str) -> Result<(), CodecError> {
    let width = info["width"]
        .as_u64()
        .ok_or_else(|| CodecError::unverified("video width is unknown"))?;
    let height = info["height"]
        .as_u64()
        .ok_or_else(|| CodecError::unverified("video height is unknown"))?;
    let duration = info["duration"]
        .as_f64()
        .ok_or_else(|| CodecError::unverified("video duration is unknown"))?;
    let codec = info["codec"]
        .as_str()
        .ok_or_else(|| CodecError::unverified("video codec is unknown"))?;
    let fps = info["fps"]
        .as_f64()
        .ok_or_else(|| CodecError::unverified("video frame rate is unknown"))?;
    let has_audio = info["has_audio"]
        .as_bool()
        .ok_or_else(|| CodecError::unverified("video audio state is unknown"))?;
    let container = info["container"]
        .as_str()
        .ok_or_else(|| CodecError::unverified("video container is unknown"))?;
    if !duration.is_finite() || !fps.is_finite() {
        return Err(CodecError::unverified("nonfinite video metadata"));
    }
    validate_source_info(
        &VideoInfo {
            width: width.into(),
            height: height.into(),
            duration,
            codec: MetadataString::from(codec),
            fps,
            has_audio,
            container: MetadataString::from(container),
        },
        bytes,
        filename,
    )
}
pub fn validate_source_info(
    info: &VideoInfo,
    bytes: u64,
    filename: &str,
) -> Result<(), CodecError> {
    validate_source_info_size(info, &bytes.into(), filename)
}
pub fn validate_source_info_size(
    info: &VideoInfo,
    bytes: &MetadataInteger,
    filename: &str,
) -> Result<(), CodecError> {
    let mut problems = Vec::new();
    if !info.width.equals(100) || !info.height.equals(100) {
        problems.push(format!(
            "dimensions {}x{} != 100x100",
            info.width, info.height
        ));
    }
    if info.duration <= 0.0 {
        problems.push("duration is zero or unknown".into());
    } else if info.duration > 3.05 {
        problems.push(format!("duration {}s > 3.0s", fixed(info.duration, 2)));
    }
    if !info.codec.equals("vp9") {
        problems.push(format!("codec {} != 'vp9'", info.codec.repr()));
    }
    if info.has_audio {
        problems.push("contains an audio stream (video emoji must have none)".into());
    }
    if info.fps > 30.01 {
        problems.push(format!("{} fps > 30 fps", fixed(info.fps, 2)));
    }
    if !info.container.contains_component("webm") {
        problems.push(format!("container {} is not webm", info.container.repr()));
    }
    if bytes.greater_than_u64(256 * 1024) {
        problems.push(format!("size {bytes} > 262144 bytes"));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(CodecError::media(format!(
            "{filename}: {}",
            problems.join("; ")
        )))
    }
}
pub(crate) fn fixed(value: f64, precision: usize) -> String {
    if value.is_nan() {
        "nan".into()
    } else if value == f64::INFINITY {
        "inf".into()
    } else if value == f64::NEG_INFINITY {
        "-inf".into()
    } else {
        format!("{value:.precision$}")
    }
}
pub async fn validate(decoder: &VideoDecoder, path: &Path) -> Result<(), String> {
    let info = decoder
        .probe_core(path, crate::codec_video_error::CommandContext::Direct)
        .await?
        .map_err(crate::codec_video_error::VideoFailure::legacy)?;
    validate_probed(path, &info).map_err(|error| error.message)
}
pub async fn validate_typed(decoder: &VideoDecoder, path: &Path) -> Result<(), CodecError> {
    let info = decoder.probe_info_typed(path).await?;
    validate_probed(path, &info)
}
fn validate_probed(path: &Path, info: &VideoInfo) -> Result<(), CodecError> {
    let bytes = std::fs::metadata(path)
        .map_err(|error| CodecError {
            kind: "OSError".into(),
            message: error.to_string(),
            handled: true,
        })?
        .len();
    let filename = path
        .file_name()
        .ok_or_else(|| CodecError::unverified("video has no filename"))?
        .to_string_lossy();
    validate_source_info(info, bytes, &filename)
}
pub fn usable_at(root: Option<&Path>, path: &Path) -> Result<Value, CodecError> {
    let root =
        root.ok_or_else(|| CodecError::value("video worker requires an explicit project root"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| CodecError::unverified("cannot initialize video worker runtime"))?;
    let decoder = VideoDecoder::new(root).map_err(CodecError::unverified)?;
    runtime.block_on(async {
        validate_typed(&decoder, path).await?;
        Ok(Value::Bool(!is_blank_typed(&decoder, path).await?))
    })
}
pub async fn is_blank(decoder: &VideoDecoder, path: &Path) -> Result<bool, String> {
    Ok(blank_samples(&decoder.frames_rgba(path, 4).await?))
}
pub async fn is_blank_typed(decoder: &VideoDecoder, path: &Path) -> Result<bool, CodecError> {
    Ok(blank_samples(&decoder.frames_rgba_typed(path, 4).await?))
}
fn blank_samples(raw: &[u8]) -> bool {
    let frame_bytes = crate::codec_video_timeline::FRAME_BYTES;
    !raw.is_empty()
        && raw.len() >= frame_bytes
        && raw
            .chunks_exact(frame_bytes)
            .all(crate::native_raster::blank_rgba)
}
