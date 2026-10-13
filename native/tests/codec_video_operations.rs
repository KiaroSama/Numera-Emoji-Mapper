//! Real video conversion/preview/remux observations retain independent source provenance.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn video_outputs_match_source_pixels_and_preserve_existing_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/video-operations.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/media.py"],
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53"
    );
    let scratch = Scratch(
        root.join("logs/tmp")
            .join(format!("video operations {}", std::process::id())),
    );
    std::fs::create_dir(&scratch.0).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let decoder = _native::codec_video::VideoDecoder::new(&root).unwrap();
        for (index, case) in fixture["conversions"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let source = root.join(case["input"].as_str().unwrap());
            assert_eq!(hash(&std::fs::read(&source).unwrap()), case["input_sha256"]);
            let output = scratch.0.join(format!("convert-{index}.webm"));
            _native::codec_video_encode::convert(&decoder, &source, &output, 262144)
                .await
                .unwrap();
            let info = decoder.probe(&output).await.unwrap();
            for field in [
                "width",
                "height",
                "duration",
                "fps",
                "codec",
                "has_audio",
                "container",
            ] {
                assert_eq!(info[field], case["expected"][field], "{index} {field}");
            }
            let raw = decoder.frames_rgba(&output, 10).await.unwrap();
            assert_eq!(hash(&raw), case["expected"]["rgba_sha256"], "{index}");
            assert_eq!(json!(raw.len()), case["expected"]["frame_bytes"]);
            _native::codec_video_validate::validate(&decoder, &output)
                .await
                .unwrap();
            let before = std::fs::read(&output).unwrap();
            assert!(
                _native::codec_video_encode::convert(&decoder, &source, &output, 262144)
                    .await
                    .is_err()
            );
            assert_eq!(std::fs::read(&output).unwrap(), before);
        }
        for (index, case) in fixture["previews"].as_array().unwrap().iter().enumerate() {
            let source = root.join(case["input"].as_str().unwrap());
            let output = scratch.0.join(format!("preview-{index}.webp"));
            _native::codec_video_preview::preview(
                &decoder,
                &source,
                &output,
                case["size"].as_u64().unwrap() as u32,
                case["fps"].as_u64().unwrap() as u32,
                case["still"].as_bool().unwrap(),
            )
            .await
            .unwrap();
            let bytes = std::fs::read(&output).unwrap();
            assert_eq!(hash(&bytes), case["webp_sha256"], "{index}");
            assert_eq!(json!(bytes.len()), case["bytes"]);
            assert!(
                _native::codec_video_preview::preview(&decoder, &source, &output, 104, 15, false)
                    .await
                    .is_err()
            );
            assert_eq!(std::fs::read(&output).unwrap(), bytes);
        }
        for (index, case) in fixture["remuxes"].as_array().unwrap().iter().enumerate() {
            let output = scratch.0.join(format!("remux-{index}.webm"));
            std::fs::copy(root.join(case["input"].as_str().unwrap()), &output).unwrap();
            assert_eq!(
                json!(
                    _native::codec_video_encode::reencode(&decoder, &output)
                        .await
                        .unwrap()
                ),
                case["changed"]
            );
            assert_eq!(
                hash(&decoder.frames_rgba(&output, 10).await.unwrap()),
                case["rgba_sha256"]
            );
        }
        let refused = scratch.0.join("refused.webm");
        let source = root.join("tests/fixtures/native/video-codec/alpha.webm");
        assert!(
            _native::codec_video_encode::convert(&decoder, &source, &refused, 1)
                .await
                .is_err()
        );
        assert!(!refused.exists());
        let corrupt = scratch.0.join("corrupt.webm");
        std::fs::write(&corrupt, b"not a video").unwrap();
        assert!(
            !_native::codec_video_encode::reencode(&decoder, &corrupt)
                .await
                .unwrap()
        );
        assert_eq!(std::fs::read(&corrupt).unwrap(), b"not a video");
        assert!(!std::fs::read_dir(&scratch.0).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".video-output")
        }));
    });
    std::fs::remove_dir_all(&scratch.0).unwrap();
    assert!(!scratch.0.exists());
}
