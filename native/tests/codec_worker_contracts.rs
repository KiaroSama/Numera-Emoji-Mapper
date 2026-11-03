//! Worker dispatch uses source metadata/preview/package goldens, not target-derived expectations.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("clean owned worker contract scratch");
    }
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}

#[test]
fn metadata_does_not_depend_on_selected_identity_format() {
    let root = root();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        assert_eq!(hash(&std::fs::read(&source).unwrap()), case["input_sha256"]);
        for format in [
            json!("static"),
            json!("video"),
            json!("animated"),
            Value::Null,
            json!(false),
            json!(0),
            json!(0.0),
            json!(""),
            json!([]),
            json!({}),
        ] {
            for operation in ["image_info", "dimensions"] {
                let request = json!({"operation":operation,"source":source,"format":format});
                let expected = if operation == "dimensions" {
                    case["expected"]["size"].clone()
                } else {
                    json!({"size":case["expected"]["size"],"mode":case["expected"]["mode"],"blank":case["expected"]["blank"]})
                };
                assert_eq!(
                    _native::codec_worker::execute(&request).unwrap(),
                    expected,
                    "{} {operation} {format}",
                    case["name"]
                );
            }
        }
    }
}

#[test]
fn preview_accepts_source_numeric_size_but_not_boolean_or_float_fps() {
    let root = root();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-preview.json"
    ))
    .unwrap();
    let case = &fixture["cases"][1];
    let source = root.join(case["input"].as_str().unwrap());
    assert_eq!(hash(&std::fs::read(&source).unwrap()), case["input_sha256"]);
    let path = root
        .join("logs/tmp")
        .join(format!("worker numeric preview {}", std::process::id()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::create_dir(&path).unwrap();
    let scratch = Scratch(path);
    for (index, size) in [json!(52), json!(52.0)].into_iter().enumerate() {
        let output = scratch.0.join(format!("{index}.webp"));
        let request =
            json!({"operation":"preview","source":source,"output":output,"key":"","size":size});
        assert_eq!(
            _native::codec_worker::execute(&request).unwrap()["bytes"],
            case["expected"]["bytes"]
        );
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(hash(&bytes), case["expected"]["sha256"]);
        assert!(_native::codec_worker::execute(&request).is_err());
        assert_eq!(std::fs::read(output).unwrap(), bytes);
    }
    for delta in [
        json!({"size":true}),
        json!({"size":52.5}),
        json!({"fps":15.0}),
        json!({"fps":true}),
        json!({"still":0}),
    ] {
        let output = scratch.0.join("invalid.webp");
        let mut request = json!({"operation":"preview","source":source,"output":output,"key":"ok"});
        for (key, value) in delta.as_object().unwrap() {
            request[key] = value.clone();
        }
        assert!(_native::codec_worker::execute(&request).is_err(), "{delta}");
        assert!(!output.exists());
    }
}

#[test]
fn typed_worker_set_membership_preserves_unhashable_errors_and_order() {
    let source = root().join("tests/fixtures/native/static-codec/opaque.png");
    for (value, kind) in [(json!([]), "list"), (json!({}), "dict")] {
        let error = _native::codec_worker::execute_typed_at(
            None,
            &json!({"operation":value,"source":source,"format":"static"}),
        )
        .unwrap_err();
        assert_eq!(error.kind, "TypeError");
        assert_eq!(error.message, format!("unhashable type: '{kind}'"));
        assert!(!error.handled);
        let error = _native::codec_worker::execute_typed_at(
            None,
            &json!({"operation":"preview","source":source,"format":"static",
                "key":"valid","size":value}),
        )
        .unwrap_err();
        assert_eq!(error.kind, "TypeError");
        assert_eq!(error.message, format!("unhashable type: '{kind}'"));
        let error = _native::codec_worker::execute_typed_at(
            None,
            &json!({"operation":"preview","source":source,"format":"static",
                "key":false,"size":value}),
        )
        .unwrap_err();
        assert_eq!(
            error.kind, "ValueError",
            "invalid key precedes size hashing"
        );
    }
    for (value, kind) in [(json!([1]), "list"), (json!({"x":1}), "dict")] {
        for operation in ["fingerprint", "convert"] {
            let error = _native::codec_worker::execute_typed_at(
                None,
                &json!({"operation":operation,"source":source,"format":value}),
            )
            .unwrap_err();
            assert_eq!(error.kind, "TypeError");
            assert_eq!(error.message, format!("unhashable type: '{kind}'"));
        }
    }
    for value in [json!(true), json!(1), json!(1.5), json!("unknown")] {
        let error = _native::codec_worker::execute_typed_at(
            None,
            &json!({"operation":"convert","source":source,"format":value}),
        )
        .unwrap_err();
        assert_eq!(error.kind, "ValueError");
        assert_eq!(
            error.message, "output must be a non-empty path",
            "hashable invalid format must reach target dispatch"
        );
    }
}

#[test]
fn conversion_dispatches_by_target_even_when_source_format_is_unknown() {
    let root = root();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/tgs-package.json")).unwrap();
    let case = &fixture["cases"][0];
    let source = root.join(case["input"].as_str().unwrap());
    assert_eq!(hash(&std::fs::read(&source).unwrap()), case["input_sha256"]);
    let path = root
        .join("logs/tmp")
        .join(format!("worker target conversion {}", std::process::id()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::create_dir(&path).unwrap();
    let scratch = Scratch(path);
    for (index, format) in [
        Value::Null,
        json!("unknown"),
        json!("static"),
        json!("video"),
        json!("animated"),
    ]
    .into_iter()
    .enumerate()
    {
        let output = scratch.0.join(format!("{index}.tgs"));
        let result = _native::codec_worker::execute(&json!({"operation":"convert","source":source,"output":output,"format":format,"target_format":"animated"})).unwrap();
        let bytes = std::fs::read(output).unwrap();
        assert_eq!(hash(&bytes), case["expected"]["sha256"]);
        assert_eq!(
            bytes.len() as u64,
            case["expected"]["bytes"].as_u64().unwrap()
        );
        assert_eq!(result["fmt"], "animated");
        assert_eq!(result["extension"], ".tgs");
    }
}
