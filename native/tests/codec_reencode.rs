//! Collected static bytes change without changing invisible RGB or decoded artwork.
use pyo3::prelude::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[test]
fn source_static_reencode_preserves_every_pixel_and_is_idempotent() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-reencode.json"
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
        .join(format!("native reencode {}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for case in cases {
        let path = directory.join(format!("{}.png", case["name"].as_str().unwrap()));
        std::fs::copy(root.join(case["input"].as_str().unwrap()), &path).unwrap();
        assert_eq!(
            serde_json::json!(_native::codec_image::reencode_static(&path).unwrap()),
            case["expected"]["rewritten"]
        );
        let bytes = std::fs::read(&path).unwrap();
        let digest = |bytes: &[u8]| {
            Sha256::digest(bytes)
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>()
        };
        assert_eq!(digest(&bytes), case["expected"]["sha256"]);
        let rgba: Vec<u8> = Python::attach(|py| {
            let image = py
                .import("PIL.Image")
                .unwrap()
                .call_method1("open", (path.clone(),))
                .unwrap();
            let raw = image
                .call_method1("convert", ("RGBA",))
                .unwrap()
                .call_method0("tobytes")
                .unwrap()
                .extract()
                .unwrap();
            image.call_method0("close").unwrap();
            raw
        });
        assert_eq!(digest(&rgba), case["expected"]["rgba_sha256"]);
        assert_eq!(
            serde_json::json!(_native::codec_image::reencode_static(&path).unwrap()),
            case["expected"]["repeat"]
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::remove_file(path).unwrap();
    }
    let corrupt = directory.join("corrupt.png");
    std::fs::write(&corrupt, b"not an image").unwrap();
    assert!(!_native::codec_image::reencode_static(&corrupt).unwrap());
    assert_eq!(std::fs::read(&corrupt).unwrap(), b"not an image");
    std::fs::remove_file(corrupt).unwrap();
    std::fs::remove_dir(directory).unwrap();
}
