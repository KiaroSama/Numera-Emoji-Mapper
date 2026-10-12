//! Real compressed source fixtures exercise the bound, decoder errors and root shape.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[test]
fn pinned_tgs_load_preserves_decoded_document_and_bounded_source_errors() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/tgs-load.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 7);
    let directory = root
        .join("logs")
        .join(format!("tgs load {}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for case in cases {
        let path = if let Some(encoded) = case["input_base64"].as_str() {
            use base64::Engine;
            let text = std::fs::read_to_string(root.join(encoded)).unwrap();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(text.trim())
                .unwrap();
            let path = directory.join(case["name"].as_str().unwrap());
            std::fs::write(&path, bytes).unwrap();
            path
        } else {
            root.join(case["input"].as_str().unwrap())
        };
        assert_eq!(
            Sha256::digest(std::fs::read(&path).unwrap())
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>(),
            case["input_sha256"]
        );
        let actual = _native::codec_lottie_load::load(&path);
        if let Some(expected) = case.get("expected") {
            assert_eq!(actual.unwrap(), *expected, "{}", case["name"]);
        } else {
            let error = actual.unwrap_err();
            assert_eq!(error.kind, case["error_kind"].as_str().unwrap());
            assert_eq!(error.message, case["error"].as_str().unwrap());
        }
        if case["input_base64"].is_string() {
            std::fs::remove_file(path).unwrap();
        }
    }
    std::fs::remove_dir(directory).unwrap();
}
