//! Expected identity bytes from the frozen source sampler, captured separately for Linux ARM64.
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Return the expected content key and full sampled RGBA digest for these exact input bytes.
pub fn expected(case: &Value, bytes: &[u8]) -> (String, String) {
    let input_hash: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(case["input_sha256"], input_hash);
    let original = &case["expected"];
    if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        let references: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/native/video-identity-linux-arm64.json"
        ))
        .unwrap();
        let reference = &references["cases"][case["input"].as_str().unwrap()];
        assert_eq!(reference["input_sha256"], input_hash);
        assert_eq!(reference["frame_bytes"], original["frame_bytes"]);
        let digest = reference["rgba_sha256"].as_str().unwrap();
        return (format!("v:{}", &digest[..32]), digest.to_owned());
    }
    (
        original["content_key"].as_str().unwrap().to_owned(),
        original["rgba_sha256"].as_str().unwrap().to_owned(),
    )
}
