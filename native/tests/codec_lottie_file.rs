//! File validation and vector comparison keep refusal distinct from unknown.
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
fn source_tgs_file_refusals_and_vector_unknowns_are_preserved() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/lottie-file.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let hash = |bytes: &[u8]| {
        Sha256::digest(bytes)
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>()
    };
    let validations = fixture["validations"].as_array().unwrap();
    assert_eq!(validations.len(), 7);
    let directory = Scratch(
        root.join("logs")
            .join(format!("lottie file {}", std::process::id())),
    );
    std::fs::create_dir(&directory.0).unwrap();
    for case in validations {
        let path = if let Some(encoded) = case["input_base64"].as_str() {
            use base64::Engine;
            let text = std::fs::read_to_string(root.join(encoded)).unwrap();
            let path = directory.0.join(case["name"].as_str().unwrap());
            std::fs::write(
                &path,
                base64::engine::general_purpose::STANDARD
                    .decode(text.trim())
                    .unwrap(),
            )
            .unwrap();
            path
        } else {
            root.join(case["input"].as_str().unwrap())
        };
        assert_eq!(hash(&std::fs::read(&path).unwrap()), case["input_sha256"]);
        let result = _native::codec_lottie_validate::validate_file(&path);
        let worker = _native::codec_worker::execute(
            &json!({"operation":"usable","source":path,"format":"animated"}),
        );
        if case.get("expected").is_some() {
            result.unwrap();
            assert_eq!(worker.unwrap(), true);
        } else {
            assert_eq!(result.unwrap_err(), case["error"].as_str().unwrap());
            assert_eq!(worker.unwrap_err(), case["error"].as_str().unwrap());
        }
    }
    let comparisons = fixture["comparisons"].as_array().unwrap();
    assert_eq!(comparisons.len(), 6);
    for case in comparisons {
        let a = root.join(case["a"].as_str().unwrap());
        let b = root.join(case["b"].as_str().unwrap());
        assert_eq!(hash(&std::fs::read(&a).unwrap()), case["a_sha256"]);
        assert_eq!(hash(&std::fs::read(&b).unwrap()), case["b_sha256"]);
        assert_eq!(
            json!(_native::codec_lottie_identity::same_image(&a, &b)),
            case["same_image"]
        );
        for operation in ["same_image", "compare"] {
            assert_eq!(
                _native::codec_worker::execute(&json!({"operation":operation,
                "source":a,"other":b,"format":"animated"}))
                .unwrap(),
                case[operation]
            );
        }
    }
    for (name, expected) in [("opaque.png", true), ("transparent-rgb.png", false)] {
        assert_eq!(
            _native::codec_worker::execute(&json!({"operation":"usable","format":"static",
            "source":root.join("tests/fixtures/native/static-codec").join(name)}))
            .unwrap(),
            expected
        );
    }
}
