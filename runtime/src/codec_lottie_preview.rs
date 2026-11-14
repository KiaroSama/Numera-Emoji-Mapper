//! Native rlottie sampling and WebP previews with historical frame positions and timing.
use crate::{codec_error::CodecError, native_lottie::Animation, native_raster as raster};
use serde_json::{Value, json};
use std::path::Path;

fn encoded(
    source: &Path,
    size: u32,
    fps: u32,
    still: bool,
    quality: u32,
) -> Result<Vec<u8>, CodecError> {
    let mut animation = Animation::from_path(source)?;
    let options = raster::WebpOptions {
        lossless: false,
        quality: quality as f32,
        method: 4,
    };
    if still {
        let frame = animation.render_rgba(0., size, size)?;
        // Destroy the engine before encoding the independently owned raster.
        drop(animation);
        return raster::encode_pillow_animated_webp(
            &raster::RasterAnimation {
                frames: vec![raster::RasterFrame {
                    rgba: frame,
                    duration_ms: 0,
                }],
                loop_count: 0,
            },
            options,
        )
        .map_err(Into::into);
    }
    let duration = animation.duration();
    let fps = if duration * f64::from(fps) >= 1. {
        f64::from(fps)
    } else {
        animation.frame_rate()
    };
    let count = (duration * fps).floor();
    let limits = raster::DecodeLimits::default();
    let frame_bytes = u64::from(size) * u64::from(size) * 4;
    if !count.is_finite()
        || count < 1.
        || count > limits.max_frames as f64
        || count * frame_bytes as f64 > limits.max_decoded_bytes as f64
    {
        return Err(CodecError::media(
            "Lottie preview exceeds frame allocation limits or has no sampled frames",
        ));
    }
    let count = count as usize;
    let duration_ms = (1000. / fps) as u32;
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        let frame = animation.frame_at_pos(index as f64 / count as f64)?;
        frames.push(raster::RasterFrame {
            rgba: animation.render_rgba(frame as f64, size, size)?,
            duration_ms,
        });
    }
    drop(animation);
    raster::encode_pillow_animated_webp(
        &raster::RasterAnimation {
            frames,
            loop_count: 0,
        },
        options,
    )
    .map_err(Into::into)
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
    let bytes = encoded(source, size, fps, still, quality)?;
    let length = crate::codec_image::publish_bytes(output, "webp", &bytes)?;
    Ok(json!({"bytes":length}))
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
pub fn preview(
    source: &Path,
    output: &Path,
    size: u32,
    fps: u32,
    still: bool,
) -> Result<Value, String> {
    preview_typed(source, output, size, fps, still).map_err(|error| error.message)
}
pub fn gallery_typed(source: &Path, output: &Path, still: bool) -> Result<Value, CodecError> {
    preview_encoded(source, output, 88, 9, still, 65)
}
pub fn gallery(source: &Path, output: &Path, still: bool) -> Result<Value, String> {
    gallery_typed(source, output, still).map_err(|error| error.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> std::path::PathBuf {
        crate::native_lottie::fixture_root()
    }

    #[test]
    fn retained_preview_frames_pixels_and_durations_match() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/lottie-preview.json"
        ))
        .unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let bytes = encoded(
                &root().join(case["input"].as_str().unwrap()),
                52,
                15,
                case["still"].as_bool().unwrap(),
                60,
            )
            .unwrap();
            let expected = &case["expected"];
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WEBP");
            assert_eq!(
                u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
                bytes.len()
            );
            let decoded =
                raster::decode_animation(&bytes, raster::DecodeLimits::default()).unwrap();
            let expected_frames = expected["frames"].as_array().unwrap();
            assert_eq!(
                decoded.frames.len(),
                expected_frames.len(),
                "{}",
                case["name"]
            );
            for (frame, expected) in decoded.frames.iter().zip(expected_frames) {
                assert_eq!(frame.rgba.dimensions(), (52, 52));
                assert_eq!(
                    raster::digest_hex(frame.rgba.as_raw()),
                    expected["rgba_sha256"],
                    "{}",
                    case["name"]
                );
                assert_eq!(
                    frame.duration_ms,
                    expected["duration"].as_u64().unwrap() as u32,
                    "{}",
                    case["name"]
                );
            }
        }
    }

    #[test]
    fn retained_animated_gallery_cases_have_stable_pixels_and_timing() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/gallery-codec.json"
        ))
        .unwrap();
        for case in fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["format"] == "animated")
        {
            let bytes = encoded(
                &root().join(case["input"].as_str().unwrap()),
                88,
                9,
                case["still"].as_bool().unwrap(),
                65,
            )
            .unwrap();
            // Compressed bytes in this historical fixture are platform-specific. The
            // original compatibility suite retains those observations unchanged.
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WEBP");
            assert_eq!(
                u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
                bytes.len()
            );
            let repeated = encoded(
                &root().join(case["input"].as_str().unwrap()),
                88,
                9,
                case["still"].as_bool().unwrap(),
                65,
            )
            .unwrap();
            assert_eq!(bytes, repeated);
            let decoded =
                raster::decode_animation(&bytes, raster::DecodeLimits::default()).unwrap();
            let still = case["still"].as_bool().unwrap();
            assert_eq!(decoded.frames.len(), if still { 1 } else { 9 });
            for frame in &decoded.frames {
                assert_eq!(frame.rgba.dimensions(), (88, 88));
                assert!(raster::alpha_bounds(&frame.rgba).is_some());
                assert_eq!(frame.duration_ms, if still { 0 } else { 111 });
            }
        }
    }
}
