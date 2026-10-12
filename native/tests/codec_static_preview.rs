//! Static previews preserve pinned thumbnail pixels, bytes and exclusive publication.
use pyo3::prelude::*;
use pyo3::types::PyBytes;
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
fn static_previews_match_source_without_enlarging_or_losing_hidden_rgb() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-preview.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"],
        "8d7f86244ed498b32381ba78a6321622aa28d9b72c1a1adba9a48c39565dd5b7"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let directory = Scratch(
        root.join("logs")
            .join(format!("static preview {}", std::process::id())),
    );
    std::fs::create_dir(&directory.0).unwrap();
    let hash = |bytes: &[u8]| {
        Sha256::digest(bytes)
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>()
    };
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        assert_eq!(hash(&std::fs::read(&source).unwrap()), case["input_sha256"]);
        let output = directory
            .0
            .join(format!("{}.webp", case["name"].as_str().unwrap()));
        let request = serde_json::json!({"operation":"preview","source":source,"format":"static",
            "output":output,"key":"source-fixture","size":case["size"],"fps":case["fps"],"still":case["still"]});
        let actual = _native::codec_worker::execute(&request);
        if let Some(expected) = case.get("expected") {
            assert_eq!(
                actual.unwrap(),
                serde_json::json!({"bytes":expected["bytes"]})
            );
            let bytes = std::fs::read(&output).unwrap();
            assert_eq!(hash(&bytes), expected["sha256"], "{}", case["name"]);
            Python::attach(|py| {
                let image = py
                    .import("PIL.Image")
                    .unwrap()
                    .call_method1("open", (output.clone(),))
                    .unwrap();
                let size = image
                    .getattr("size")
                    .unwrap()
                    .extract::<(u32, u32)>()
                    .unwrap();
                assert_eq!(serde_json::json!([size.0, size.1]), expected["size"]);
                let rgba = image.call_method1("convert", ("RGBA",)).unwrap();
                assert_eq!(
                    hash(
                        rgba.call_method0("tobytes")
                            .unwrap()
                            .cast::<PyBytes>()
                            .unwrap()
                            .as_bytes()
                    ),
                    expected["rgba_sha256"]
                );
                rgba.call_method0("close").unwrap();
                image.call_method0("close").unwrap();
            });
            assert!(_native::codec_worker::execute(&request).is_err());
            assert_eq!(std::fs::read(&output).unwrap(), bytes);
            std::fs::remove_file(&output).unwrap();
        } else {
            assert_eq!(
                actual.unwrap_err(),
                "external static preview rendering failed"
            );
            assert!(!output.exists());
        }
        for (field, value) in [
            ("size", serde_json::json!(true)),
            ("fps", serde_json::json!(false)),
            ("still", serde_json::json!(1)),
            ("key", serde_json::json!("../escape")),
        ] {
            let mut invalid = request.clone();
            invalid[field] = value;
            assert_eq!(
                _native::codec_worker::execute(&invalid).unwrap_err(),
                "invalid preview parameters"
            );
            assert!(!output.exists());
        }
    }
    assert!(std::fs::read_dir(&directory.0).unwrap().next().is_none());
}
