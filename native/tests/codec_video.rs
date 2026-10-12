//! Real VP8/VP9 fixtures exercise alpha-preserving decode and canonical sampling.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

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
            let key = decoder.content_key(&path).await.unwrap();
            assert_eq!(key, case["expected"]["content_key"]);
            keys.push(key);
        }
        assert_ne!(keys[0], keys[1], "opacity is part of video identity");
        assert_eq!(keys[1], keys[2], "extensionless VP9 must keep alpha");
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
