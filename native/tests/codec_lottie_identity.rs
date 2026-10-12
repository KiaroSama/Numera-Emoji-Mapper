//! Vector keys preserve source JSON canonicalization and exact raw-fallback boundaries.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[test]
fn pinned_animated_identity_matches_source_and_worker() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/animated-key.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/identity.py"],
        "8cbefd550e306e671df1e52af2dd9fe6b4e3647c3aa1b13dc98bf35080d36e89"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/media_lottie.py"],
        "5112b55f21d4d241ffe9660d5294de20487d4ea1e9772188fb6bd125f7fef63b"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 11);
    for case in cases {
        let path = root.join(case["input"].as_str().unwrap());
        assert_eq!(
            Sha256::digest(std::fs::read(&path).unwrap())
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>(),
            case["input_sha256"]
        );
        let direct = _native::codec_lottie_identity::fingerprint(&path);
        let worker = _native::codec_worker::execute(&serde_json::json!({
            "operation":"fingerprint","source":path,"format":"animated"
        }));
        if let Some(expected) = case.get("expected") {
            assert_eq!(direct.unwrap(), *expected, "{}", case["input"]);
            assert_eq!(worker.unwrap(), *expected, "{}", case["input"]);
        } else {
            assert_eq!(direct.unwrap_err(), case["error"].as_str().unwrap());
            assert_eq!(worker.unwrap_err(), case["error"].as_str().unwrap());
            let error = _native::codec_lottie_load::canonical_bytes(&path).unwrap_err();
            assert_eq!(error.kind, case["error_kind"].as_str().unwrap());
            assert!(!error.raw_fallback);
        }
    }
    let relative = PathBuf::from("tests/fixtures/native/tgs-load/valid.tgs");
    assert!(_native::codec_lottie_identity::fingerprint(&relative).is_err());
    let nonfinite = root.join("tests/fixtures/native/animated-key/nonfinite.json");
    let error = _native::codec_lottie_load::load(&nonfinite).unwrap_err();
    assert_eq!(error.kind, "CompatibilityError");
    assert!(!error.raw_fallback);
}
