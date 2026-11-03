//! Source-derived TGS bytes, Unicode, refusal and preservation at the file boundary.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[test]
fn canvas_conversion_preserves_external_integer_errors_and_unbounded_values() {
    use pyo3::prelude::*;
    use pyo3::types::PyDict;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let directory = root
        .join("logs/tmp")
        .join(format!("tgs canvas {}", std::process::id()));
    std::fs::create_dir_all(directory.parent().unwrap()).unwrap();
    std::fs::create_dir(&directory).unwrap();
    let source = directory.join("canvas.json");
    let output = directory.join("canvas.tgs");
    Python::initialize();
    Python::attach(|py| {
        let json = py.import("json").unwrap();
        let int = py.import("builtins").unwrap().getattr("int").unwrap();
        for input in [
            "null",
            "[]",
            "\"bad\"",
            "NaN",
            "Infinity",
            "-Infinity",
            "100000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "\"５_１２\"",
            "512.9",
            "512",
        ] {
            let dimension = json.call_method1("loads", (input,)).unwrap();
            let expected = int.call1((&dimension,));
            let document = json
                .call_method1(
                    "loads",
                    (r#"{"v":"5.5","w":512,"h":512,"fr":30,"ip":0,"op":60,"layers":[]}"#,),
                )
                .unwrap()
                .cast_into::<PyDict>()
                .unwrap();
            document.set_item("w", &dimension).unwrap();
            document.set_item("metadata", f64::NAN).unwrap();
            let text: String = json
                .call_method1("dumps", (&document,))
                .unwrap()
                .extract()
                .unwrap();
            std::fs::write(&source, text.as_bytes()).unwrap();
            let result = _native::codec_lottie_package::package_typed(&source, &output);
            match expected {
                Err(expected) => {
                    let actual = result.unwrap_err();
                    assert_eq!(
                        actual.kind,
                        expected.get_type(py).name().unwrap().to_string()
                    );
                    assert_eq!(
                        actual.message,
                        expected.value(py).str().unwrap().to_string()
                    );
                }
                Ok(integer) if !integer.eq(512).unwrap() => {
                    let actual = result.unwrap_err();
                    assert_eq!(actual.kind, "MediaError");
                    assert_eq!(
                        actual.message,
                        format!(
                            "canvas.json: animated emoji require a 512x512 Lottie canvas, got {}x512. Re-export the animation at 512x512.",
                            integer.str().unwrap()
                        )
                    );
                }
                Ok(_) if !dimension.eq(512).unwrap() => {
                    let actual = result.unwrap_err();
                    assert_eq!(actual.kind, "MediaError");
                    assert_eq!(
                        actual.message,
                        format!(
                            "canvas.tgs: canvas {}x512, expected 512x512",
                            dimension.str().unwrap()
                        )
                    );
                }
                Ok(_) => {
                    result.unwrap();
                    std::fs::remove_file(&output).unwrap();
                }
            }
            assert!(!output.exists());
            assert_eq!(std::fs::read(&source).unwrap(), text.as_bytes());
            assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        }
    });
    std::fs::remove_file(source).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn generated_tgs_retains_the_decoded_size_boundary() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let directory = root
        .join("logs/tmp")
        .join(format!("tgs expansion {}", std::process::id()));
    std::fs::create_dir_all(directory.parent().unwrap()).unwrap();
    std::fs::create_dir(&directory).unwrap();
    let source = directory.join("expansion.json");
    let output = directory.join("expansion.tgs");
    const LIMIT: usize = 8 * 1024 * 1024;
    let prefix = r#"{"v":"5.5","w":512,"h":512,"fr":30,"ip":0,"op":60,"layers":[],"metadata":""#;
    let suffix = "\"}";
    for size in [LIMIT, LIMIT + 1] {
        let text = format!(
            "{prefix}{}{suffix}",
            "x".repeat(size - prefix.len() - suffix.len())
        );
        assert_eq!(text.len(), size);
        std::fs::write(&source, text.as_bytes()).unwrap();
        let result = _native::codec_lottie_package::package_typed(&source, &output);
        if size == LIMIT {
            result.unwrap();
            assert!(std::fs::metadata(&output).unwrap().len() < 65536);
            _native::codec_lottie_validate::validate_file_typed(&output).unwrap();
            std::fs::remove_file(&output).unwrap();
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.kind, "MediaError");
            assert_eq!(
                error.message,
                "expansion.tgs: Lottie expands beyond 8388608 bytes"
            );
            assert!(error.handled);
        }
        assert!(!output.exists());
        assert_eq!(std::fs::read(&source).unwrap(), text.as_bytes());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

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
    assert_eq!(
        fixture["source_sha256"],
        "5112b55f21d4d241ffe9660d5294de20487d4ea1e9772188fb6bd125f7fef63b"
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
        assert_eq!(
            _native::tooling_manifest::digest(&source).unwrap(),
            case["input_sha256"].as_str().unwrap()
        );
        let actual = _native::codec_worker::execute_typed_at(
            None,
            &serde_json::json!({
                "operation":"convert","source":source,"output":output,
                "format":"animated","target_format":"animated"
            }),
        );
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
            let error = actual.unwrap_err();
            assert_eq!(error.kind, "MediaError");
            assert_eq!(error.message, case["error"].as_str().unwrap());
            assert!(error.handled);
            assert!(!output.exists());
        }
    }
    std::fs::remove_dir(directory).unwrap();
}
