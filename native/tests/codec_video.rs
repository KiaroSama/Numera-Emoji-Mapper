//! Real VP8/VP9 fixtures exercise alpha-preserving decode and canonical sampling.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn video_probe_and_frozen_stream_match_pinned_source() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/video-codec.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/video_decode.py"],
        "bab9579240a23bb8ee59c1a66e496586ca881398bb4d82e956e2fb6c2b3ff13a"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let decoder = _native::codec_video::VideoDecoder::new(&root).unwrap();
        let mut keys = Vec::new();
        for case in cases {
            let path = root.join(case["input"].as_str().unwrap());
            let hash = |bytes: &[u8]| {
                Sha256::digest(bytes)
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<String>()
            };
            assert_eq!(hash(&std::fs::read(&path).unwrap()), case["input_sha256"]);
            let info = decoder.probe(&path).await.unwrap();
            for field in [
                "width",
                "height",
                "duration",
                "codec",
                "fps",
                "has_audio",
                "container",
            ] {
                assert_eq!(
                    info[field], case["expected"][field],
                    "{} {field}",
                    case["input"]
                );
            }
            assert_eq!(
                json!(decoder.decoder_args(&path).await.unwrap()),
                case["expected"]["decoder_args"]
            );
            let raw = decoder.frames_rgba(&path, 10).await.unwrap();
            assert_eq!(
                raw.len() as u64,
                case["expected"]["frame_bytes"].as_u64().unwrap()
            );
            assert_eq!(
                hash(&raw),
                case["expected"]["rgba_sha256"],
                "{}",
                case["input"]
            );
            let fingerprint = decoder.fingerprint(&path).await.unwrap();
            assert_eq!(fingerprint["content_key"], case["expected"]["content_key"]);
            assert_eq!(fingerprint["phash"], case["expected"]["phash"]);
            keys.push(fingerprint["content_key"].as_str().unwrap().to_owned());
        }
        assert_ne!(keys[0], keys[1], "opacity is part of video identity");
        assert_eq!(keys[1], keys[2], "extensionless VP9 must keep alpha");
        let structured: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/video-fingerprint.json"
        ))
        .unwrap();
        assert_eq!(structured["source_sha256"], fixture["source_sha256"]);
        let variants = structured["cases"].as_array().unwrap();
        assert_eq!(variants.len(), 3);
        assert_ne!(
            variants[0]["expected"]["phash"],
            variants[0]["expected"]["native_first_phash"]
        );
        assert_eq!(variants[2]["expected"]["empty_sample_retried"], true);
        for case in variants {
            let path = root.join(case["input"].as_str().unwrap());
            let hash = |bytes: &[u8]| {
                Sha256::digest(bytes)
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<String>()
            };
            assert_eq!(hash(&std::fs::read(&path).unwrap()), case["input_sha256"]);
            let raw = decoder.frames_rgba(&path, 10).await.unwrap();
            assert_eq!(hash(&raw), case["expected"]["rgba_sha256"]);
            assert_eq!(raw.len() as u64, case["expected"]["frame_bytes"]);
            let info = decoder.fingerprint(&path).await.unwrap();
            assert_eq!(info["content_key"], case["expected"]["content_key"]);
            assert_eq!(info["phash"], case["expected"]["phash"]);
            assert_eq!(
                decoder.content_key(&path).await.unwrap(),
                info["content_key"]
            );
            assert_eq!(
                json!(decoder.perceptual_hash(&path).await.unwrap()),
                info["phash"]
            );
        }
        let refusal: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/video-probe-error.json"
        ))
        .unwrap();
        assert_eq!(refusal["source_revision"], fixture["source_revision"]);
        assert_eq!(
            refusal["source_sha256"],
            fixture["source_sha256"]["emojikit/media.py"]
        );
        assert_eq!(refusal["expected_failure"], true);
        let corrupt = root.join(refusal["input"].as_str().unwrap());
        let digest = Sha256::digest(std::fs::read(&corrupt).unwrap())
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>();
        assert_eq!(digest, refusal["input_sha256"]);
        assert!(decoder.probe(&corrupt).await.is_err());
        assert!(decoder.decoder_args(&corrupt).await.is_err());
        assert!(decoder.fingerprint(&corrupt).await.is_err());
        assert_eq!(
            decoder
                .same_image(&corrupt, &root.join(cases[0]["input"].as_str().unwrap()))
                .await,
            None
        );
        assert_eq!(_native::codec_image::frame_phash(&[0; 16]), None);
        let timeline_fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/video-timeline.json"
        ))
        .unwrap();
        assert_eq!(timeline_fixture["source_sha256"], fixture["source_sha256"]);
        let mut clips = std::collections::BTreeMap::new();
        for case in timeline_fixture["cases"].as_array().unwrap() {
            let path = root.join(case["input"].as_str().unwrap());
            let hash = |bytes: &[u8]| {
                Sha256::digest(bytes)
                    .iter()
                    .map(|value| format!("{value:02x}"))
                    .collect::<String>()
            };
            assert_eq!(hash(&std::fs::read(&path).unwrap()), case["input_sha256"]);
            let timeline = decoder.timeline(&path).await.unwrap();
            assert_eq!(hash(&timeline.rgba), case["expected"]["rgba_sha256"]);
            assert_eq!(json!(timeline.times), case["expected"]["times"]);
            assert_eq!(json!(timeline.duration), case["expected"]["duration"]);
            assert_eq!(timeline.rgba.len() as u64, case["expected"]["frame_bytes"]);
            let cached = decoder.timeline(&path).await.unwrap();
            assert!(std::sync::Arc::ptr_eq(&timeline.rgba, &cached.rgba));
            assert!(
                _native::codec_video_timeline::parse(
                    (*timeline.rgba).clone(),
                    b"",
                    timeline.duration
                )
                .is_err()
            );
            clips.insert(case["name"].as_str().unwrap(), path);
        }
        for pair in timeline_fixture["pairs"].as_array().unwrap() {
            let left = &clips[pair["left"].as_str().unwrap()];
            let right = &clips[pair["right"].as_str().unwrap()];
            assert_eq!(
                json!(decoder.same_image(left, right).await),
                pair["expected"]
            );
            assert_eq!(
                json!(decoder.same_image(right, left).await),
                pair["expected"]
            );
        }
        let timing: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/video-timing.json"
        ))
        .unwrap();
        assert_eq!(
            timing["source_sha256"],
            fixture["source_sha256"]["emojikit/video_decode.py"]
        );
        for case in timing["cases"].as_array().unwrap() {
            use base64::Engine;
            let stderr = base64::engine::general_purpose::STANDARD
                .decode(case["stderr_base64"].as_str().unwrap())
                .unwrap();
            let result = _native::codec_video_timeline::parse(
                vec![0; 64 * 64 * 4],
                &stderr,
                timing["duration"].as_f64().unwrap(),
            );
            assert_eq!(json!(result.is_ok()), case["expected"]["ok"]);
            if let Ok(value) = result {
                assert_eq!(json!(value.times), case["expected"]["times"]);
                assert_eq!(json!(value.duration), case["expected"]["duration"]);
            }
        }
        let validation: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/video-validation.json"
        ))
        .unwrap();
        assert_eq!(
            validation["source_sha256"],
            fixture["source_sha256"]["emojikit/media.py"]
        );
        for case in validation["cases"].as_array().unwrap() {
            let result = _native::codec_video_validate::validate_info(
                &case["info"],
                case["bytes"].as_u64().unwrap(),
                case["filename"].as_str().unwrap(),
            );
            assert_eq!(json!(result.err()), case["error"]);
        }
        for case in validation["blank"].as_array().unwrap() {
            let path = root.join(case["input"].as_str().unwrap());
            assert_eq!(
                json!(
                    _native::codec_video_validate::is_blank(&decoder, &path)
                        .await
                        .unwrap()
                ),
                case["blank"]
            );
        }
        let scratch = Scratch(
            root.join("logs/tmp")
                .join(format!("video swap {}", std::process::id())),
        );
        std::fs::create_dir(&scratch.0).unwrap();
        let swapped = scratch.0.join("swapped.webm");
        std::fs::write(&swapped, std::fs::read(&corrupt).unwrap()).unwrap();
        assert!(decoder.decoder_args(&swapped).await.is_err());
        for case in [&cases[3], &cases[0]] {
            std::fs::write(
                &swapped,
                std::fs::read(root.join(case["input"].as_str().unwrap())).unwrap(),
            )
            .unwrap();
            assert_eq!(
                json!(decoder.decoder_args(&swapped).await.unwrap()),
                case["expected"]["decoder_args"]
            );
            assert_eq!(
                decoder.fingerprint(&swapped).await.unwrap()["content_key"],
                case["expected"]["content_key"]
            );
        }
        std::fs::remove_dir_all(&scratch.0).unwrap();
        assert!(!scratch.0.exists());
        assert!(decoder.frames_rgba(&corrupt, 0).await.is_err());
        assert!(
            decoder
                .decoder_args(&root.join("tests/fixtures/native/static-codec/opaque.png"))
                .await
                .unwrap()
                .is_empty()
        );
    });
}
