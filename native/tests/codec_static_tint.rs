//! Source-derived colour flattening retains every alpha value and invisible RGB.
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
fn static_tint_preserves_source_bytes_alpha_and_corrupt_inputs() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/static-tint.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"],
        "845d1443956499a93413316342f06d819e9ef89a9a8bb789dcaa53638cdbfc59"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let directory = Scratch(
        root.join("logs")
            .join(format!("static tint {}", std::process::id())),
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
        let bytes = std::fs::read(&source).unwrap();
        assert_eq!(hash(&bytes), case["input_sha256"]);
        let copy = directory
            .0
            .join(format!("{}.png", case["name"].as_str().unwrap()));
        std::fs::write(&copy, bytes).unwrap();
        let rgb = _native::codec_tint::parse_tint(case["tint"].as_str().unwrap()).unwrap();
        assert_eq!(
            _native::codec_image::repaint_static(&copy, rgb).unwrap(),
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
        if case["expected"]["changed"] == true {
            Python::attach(|py| {
                let image = py
                    .import("PIL.Image")
                    .unwrap()
                    .call_method1("open", (copy.clone(),))
                    .unwrap();
                let rgba = image.call_method1("convert", ("RGBA",)).unwrap();
                assert_eq!(
                    hash(
                        rgba.call_method0("tobytes")
                            .unwrap()
                            .cast::<PyBytes>()
                            .unwrap()
                            .as_bytes()
                    ),
                    case["expected"]["rgba_sha256"]
                );
                let alpha = rgba.call_method1("getchannel", ("A",)).unwrap();
                assert_eq!(
                    hash(
                        alpha
                            .call_method0("tobytes")
                            .unwrap()
                            .cast::<PyBytes>()
                            .unwrap()
                            .as_bytes()
                    ),
                    case["expected"]["alpha_sha256"]
                );
                alpha.call_method0("close").unwrap();
                rgba.call_method0("close").unwrap();
                image.call_method0("close").unwrap();
            });
        }
        assert_eq!(
            _native::codec_image::repaint_static(&copy, rgb).unwrap(),
            case["expected"]["repeat"].as_bool().unwrap()
        );
        assert_eq!(std::fs::read(&copy).unwrap(), output);
        let request = serde_json::json!({"operation":"prepare","source":copy,"format":"static",
            "tint":case["tint"]});
        let result = _native::codec_worker::execute(&request);
        if case["expected"]["changed"] == true {
            assert_eq!(result.unwrap()["fmt"], "static");
        } else {
            assert_eq!(result.unwrap_err(), "requested tint could not be applied");
        }
        assert_eq!(std::fs::read(&copy).unwrap(), output);
        std::fs::remove_file(copy).unwrap();
    }
    assert!(std::fs::read_dir(&directory.0).unwrap().next().is_none());
}
