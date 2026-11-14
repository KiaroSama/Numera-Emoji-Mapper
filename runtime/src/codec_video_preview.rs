//! Rust-owned frame sampling and native libwebp preview encoding.
use crate::codec_error::CodecError;
use crate::codec_video::VideoDecoder;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::Path;

use crate::native_raster::{RasterAnimation, RasterFrame, WebpOptions};
use image::RgbaImage;

#[derive(Clone, Copy)]
enum PreviewErrors {
    Legacy,
    Source,
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
    let mut input = std::fs::File::open(source).map_err(CodecError::io)?;
    let staged =
        crate::codec_video_encode::Staged::new(output, "webm").map_err(CodecError::unverified)?;
    let result = (|| {
        let mut output_file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&staged.0)
            .map_err(|_| CodecError::unverified("cannot stage gallery video"))?;
        // Source and staging failures retain their distinct worker error boundaries.
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = match input.read(&mut buffer) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                result => result.map_err(CodecError::io)?,
            };
            if count == 0 {
                break;
            }
            output_file
                .write_all(&buffer[..count])
                .map_err(|_| CodecError::unverified("cannot stage gallery video"))?;
        }
        drop(output_file);
        staged
            .publish(output)
            .map_err(CodecError::unverified)
            .map(|bytes| json!({"bytes":bytes}))
    })();
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
        let frames = raw
            .chunks_exact(step)
            .map(|frame| {
                RgbaImage::from_raw(size, size, frame.to_vec())
                    .ok_or_else(|| CodecError::unverified("video preview raster is incomplete"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let encode_error =
            |error: crate::native_raster::RasterError| CodecError::media(error.to_string());
        let bytes = if frames.len() == 1 {
            crate::native_raster::encode_webp(
                &frames[0],
                WebpOptions {
                    lossless: true,
                    quality: 80.0,
                    method: 4,
                },
            )
            .map_err(encode_error)?
        } else {
            let duration_ms = (1000.0 / f64::from(fps)).round_ties_even() as u32;
            let animation = RasterAnimation {
                frames: frames
                    .into_iter()
                    .map(|rgba| RasterFrame { rgba, duration_ms })
                    .collect(),
                loop_count: 0,
            };
            crate::native_raster::encode_pillow_animated_webp(
                &animation,
                WebpOptions {
                    lossless: false,
                    quality: 60.0,
                    method: 4,
                },
            )
            .map_err(encode_error)?
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom};

    #[test]
    fn gallery_and_empty_sample_identity_preserve_large_source_bytes() {
        let marker = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("logs/tmp/streamed-video-test.webm");
        let source = crate::codec_video_encode::Staged::new(&marker, "webm").unwrap();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&source.0)
            .unwrap();
        let size = 2 * 1024 * 1024 + 17;
        file.set_len(size).unwrap();
        file.seek(SeekFrom::End(-4)).unwrap();
        file.write_all(b"tail").unwrap();
        drop(file);
        let digest = crate::codec_format::file_sha256(&source.0).unwrap();
        let fingerprint = crate::codec_video::fingerprint_frames_typed(&source.0, &[]).unwrap();
        assert_eq!(fingerprint["content_key"], format!("v:{}", &digest[..32]));
        assert_eq!(fingerprint["phash"], Value::Null);
        let output = source.0.with_extension("gallery");
        assert_eq!(gallery_typed(&source.0, &output).unwrap()["bytes"], size);
        assert!(crate::codec_compare::same_bytes(&source.0, &output).unwrap());
        std::fs::remove_file(output).unwrap();
        source.clean().unwrap();
    }
}
