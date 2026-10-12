//! Static identity is source-generated, including hidden RGB and original image mode.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[test]
fn static_conversion_preserves_source_png_pixels_and_blank_refusal() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-convert.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let directory = root
        .join("logs")
        .join(format!("static conversion {}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        let output = directory.join(format!("{}.png", case["name"].as_str().unwrap()));
        let result = _native::codec_image::to_static_png(&source, &output);
        if let Some(error) = case["error"].as_str() {
            assert_eq!(result.unwrap_err(), error);
            assert!(!output.exists());
        } else {
            assert_eq!(result.unwrap(), case["expected"], "{}", case["name"]);
            let bytes = std::fs::read(&output).unwrap();
            assert_eq!(
                Sha256::digest(&bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
                case["expected"]["png_sha256"]
            );
            assert!(_native::codec_image::to_static_png(&source, &output).is_err());
            assert_eq!(std::fs::read(&output).unwrap(), bytes);
            std::fs::remove_file(output).unwrap();
        }
    }
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn static_image_info_and_fingerprint_match_pinned_source() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/fixtures/native/static-codec.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        let bytes = std::fs::read(&source).unwrap();
        assert_eq!(
            Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            case["input_sha256"].as_str().unwrap(),
            "{} fixture bytes changed",
            case["name"]
        );
        let actual = _native::codec_image::inspect_and_fingerprint(&source).unwrap();
        assert_eq!(actual, case["expected"], "{}", case["name"]);
    }
}
