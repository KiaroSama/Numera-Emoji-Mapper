//! Actual source bytes verify deterministic rewrite, tint and cap/corrupt preservation.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn animated_preparation_matches_source_rewrite_and_refusal_bytes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/tgs-prepare.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/repaint.py"],
        "845d1443956499a93413316342f06d819e9ef89a9a8bb789dcaa53638cdbfc59"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    let directory = Scratch(
        root.join("logs")
            .join(format!("tgs prepare {}", std::process::id())),
    );
    std::fs::create_dir(&directory.0).unwrap();
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        let bytes = std::fs::read(&source).unwrap();
        let hash = |bytes: &[u8]| {
            Sha256::digest(bytes)
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>()
        };
        assert_eq!(hash(&bytes), case["input_sha256"]);
        let copy = directory
            .0
            .join(format!("{}.tgs", case["name"].as_str().unwrap()));
        std::fs::write(&copy, bytes).unwrap();
        let tint = case["tint"]
            .as_str()
            .map(|text| _native::codec_tint::parse_tint(text).unwrap());
        assert_eq!(
            _native::codec_lottie_prepare::rewrite(&copy, tint).unwrap(),
            case["expected"]["changed"].as_bool().unwrap()
        );
        let output = std::fs::read(&copy).unwrap();
        assert_eq!(
            hash(&output),
            case["expected"]["sha256"],
            "{}",
            case["name"]
        );
        assert_eq!(
            output.len() as u64,
            case["expected"]["bytes"].as_u64().unwrap()
        );
        assert_eq!(
            _native::codec_lottie_prepare::rewrite(&copy, tint).unwrap(),
            case["expected"]["repeat"].as_bool().unwrap()
        );
        assert_eq!(std::fs::read(&copy).unwrap(), output);
        std::fs::remove_file(copy).unwrap();
    }
    let source = root.join("tests/fixtures/lottie/red_circle_512.json");
    let copy = directory.0.join("worker.tgs");
    std::fs::copy(&source, &copy).unwrap();
    let request = serde_json::json!({"operation":"prepare","source":copy,"format":"animated",
        "reencode":true,"tint":"#1A80FF"});
    let expected = _native::codec_worker::execute(&request).unwrap();
    assert_eq!(expected["fmt"], "animated");
    let bytes = std::fs::read(&copy).unwrap();
    assert_eq!(_native::codec_worker::execute(&request).unwrap(), expected);
    assert_eq!(std::fs::read(&copy).unwrap(), bytes);
    let mut invalid = request;
    invalid["reencode"] = serde_json::json!("yes");
    assert_eq!(
        _native::codec_worker::execute(&invalid).unwrap_err(),
        "reencode must be boolean"
    );
    assert_eq!(std::fs::read(&copy).unwrap(), bytes);
    std::fs::remove_file(copy).unwrap();
    assert!(std::fs::read_dir(&directory.0).unwrap().next().is_none());
}
