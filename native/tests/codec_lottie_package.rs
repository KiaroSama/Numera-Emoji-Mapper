//! Source-derived TGS bytes, Unicode, refusal and preservation at the file boundary.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[test]
fn source_tgs_package_is_deterministic_and_failed_output_is_not_published() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/tgs-package.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 3);
    let directory = root
        .join("logs")
        .join(format!("tgs package {}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        let output = directory.join(format!("{}.tgs", case["name"].as_str().unwrap()));
        let actual = _native::codec_worker::execute(&serde_json::json!({
            "operation":"convert","source":source,"output":output,
            "format":"animated","target_format":"animated"
        }));
        if let Some(expected) = case.get("expected") {
            actual.unwrap();
            let bytes = std::fs::read(&output).unwrap();
            assert_eq!(bytes.len() as u64, expected["bytes"].as_u64().unwrap());
            assert_eq!(
                Sha256::digest(&bytes)
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<String>(),
                expected["sha256"]
            );
            assert_eq!(
                _native::codec_lottie_load::load(&output).unwrap(),
                expected["document"]
            );
            assert!(_native::codec_lottie_package::package(&source, &output).is_err());
            assert_eq!(std::fs::read(&output).unwrap(), bytes);
            std::fs::remove_file(output).unwrap();
        } else {
            assert_eq!(actual.unwrap_err(), case["error"].as_str().unwrap());
            assert!(!output.exists());
        }
    }
    std::fs::remove_dir(directory).unwrap();
}
