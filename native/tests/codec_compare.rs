//! Static identity remains tri-state and requires both structure and colour agreement.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[test]
fn source_static_comparison_preserves_colour_alpha_and_unknown() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-compare.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let a = root.join(case["a"].as_str().unwrap());
        let b = root.join(case["b"].as_str().unwrap());
        for (path, key) in [(&a, "a_sha256"), (&b, "b_sha256")] {
            assert_eq!(
                Sha256::digest(std::fs::read(path).unwrap())
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<String>(),
                case[key]
            );
        }
        let request =
            serde_json::json!({"operation":"same_image","source":a,"other":b,"format":"static"});
        assert_eq!(
            _native::codec_worker::execute(&request).unwrap(),
            case["expected"]
        );
        assert_eq!(
            serde_json::json!(_native::codec_image::same_image(&a, &b)),
            case["expected"],
            "{}",
            case["name"]
        );
    }
}
