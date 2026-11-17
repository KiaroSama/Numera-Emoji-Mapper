// Retained supplemental native regression from native/tests/codec_worker_source_parity.rs.
// Source SHA256: 555e91167c4d7371a41ac160c0d01e7400c179faeae0fb2086b9f891780a142e; separate from the1516 baseline IDs.
//! All captured worker outcomes are independent source observations; errors retain their text.
use serde_json::{Value, json};
#[path = "conversion_oracle.rs"]
mod conversion_oracle;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::PathBuf;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("clean worker source parity scratch");
    }
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn image_error(message: &str, relative: &str) -> String {
    let path = message
        .strip_prefix("cannot identify image file ")
        .expect("source-compatible codec error prefix");
    assert!(path.starts_with('\'') && path.ends_with('\''));
    // Only the captured absolute workspace prefix/OS separator is nondeterministic.
    // The independently observed relative input and every other error byte remain checked.
    let path = path[1..path.len() - 1]
        .replace("\\\\", "/")
        .replace('\\', "/");
    assert!(
        path.ends_with(relative),
        "error named a different input: {message}"
    );
    "cannot identify image file '$corrupt'".to_owned()
}

#[test]
fn every_captured_static_worker_case_preserves_result_error_and_output() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/media-continuation/worker.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/media_bridge.py"],
        "8d7f86244ed498b32381ba78a6321622aa28d9b72c1a1adba9a48c39565dd5b7"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 46);
    let mut names = BTreeSet::new();
    let path = root
        .join("logs/tmp")
        .join(format!("worker source parity {}", std::process::id()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::create_dir(&path).unwrap();
    let scratch = Scratch(path);
    let source = root.join(fixture["inputs"]["$source"].as_str().unwrap());
    let corrupt = root.join(fixture["inputs"]["$corrupt"].as_str().unwrap());
    for (tag, path) in [("$source", &source), ("$corrupt", &corrupt)] {
        assert_eq!(
            hash(&std::fs::read(path).unwrap()),
            fixture["input_sha256"][tag]
        );
    }
    for (index, case) in cases.iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        assert!(names.insert(name));
        let directory = scratch.0.join(index.to_string());
        std::fs::create_dir(&directory).unwrap();
        let output = directory.join(if name.starts_with("convert") {
            "output.png"
        } else {
            "output.webp"
        });
        let existing = directory.join("existing.webp");
        std::fs::write(&existing, b"preserve").unwrap();
        let replacements = [
            ("$source", source.clone()),
            ("$corrupt", corrupt.clone()),
            ("$directory", directory.clone()),
            ("$missing", directory.join("missing.png")),
            ("$output", output.clone()),
            ("$existing", existing.clone()),
        ];
        let mut request = case["request"].clone();
        if let Some(object) = request.as_object_mut() {
            for value in object.values_mut() {
                if let Some(path) = value.as_str().and_then(|text| {
                    replacements
                        .iter()
                        .find(|(tag, _)| *tag == text)
                        .map(|(_, path)| path)
                }) {
                    *value = json!(path);
                }
            }
        }
        let actual = _native::codec_worker::execute_typed_at(None, &request);
        if let Some(expected) = case.get("expected") {
            assert_eq!(actual.unwrap(), *expected, "{name}");
        } else {
            let error = actual.unwrap_err();
            assert_eq!(
                error.kind,
                case["error"]["kind"].as_str().unwrap(),
                "{name}"
            );
            let expected = case["error"]["message"].as_str().unwrap();
            if name == "corrupt-metadata" {
                let input = fixture["inputs"]["$corrupt"].as_str().unwrap();
                assert_eq!(
                    image_error(&error.message, input),
                    image_error(expected, input),
                    "{name}"
                );
            } else {
                assert_eq!(error.message, expected, "{name}");
            }
        }
        if case["output"].is_null() {
            assert!(
                !output.exists(),
                "failed/readonly case published output: {name}"
            );
        } else {
            let bytes = std::fs::read(&output).unwrap();
            if name == "convert-static" {
                // Existing native representation policy, as in static_codec:
                // immutable original pixels/identity are exact; PNG deflate may differ.
                let frozen =
                    conversion_oracle::expected(fixture["inputs"]["$source"].as_str().unwrap());
                assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
                let image = image::load_from_memory(&bytes).unwrap();
                assert_eq!(image.color(), image::ColorType::Rgba8);
                let decoded = image.to_rgba8();
                assert_eq!(json!([decoded.width(), decoded.height()]), frozen["size"]);
                assert_eq!(hash(decoded.as_raw()), frozen["rgba_sha256"]);
                let direct = directory.join("producer-metadata.png");
                let metadata = _native::codec_image::to_static_png_typed(&source, &direct).unwrap();
                assert_eq!(metadata["size"], frozen["size"]);
                assert_eq!(metadata["mode"], frozen["mode"]);
                assert_eq!(metadata["rgba_sha256"], frozen["rgba_sha256"]);
                assert_eq!(metadata["bytes"], bytes.len());
                assert_eq!(metadata["png_sha256"], hash(&bytes));
                assert_eq!(std::fs::read(&direct).unwrap(), bytes);
                std::fs::remove_file(direct).unwrap();
            } else {
                assert_eq!(
                    bytes.len() as u64,
                    case["output"]["bytes"].as_u64().unwrap(),
                    "{name}"
                );
                assert_eq!(hash(&bytes), case["output"]["sha256"], "{name}");
            }
        }
        assert_eq!(std::fs::read(existing).unwrap(), b"preserve", "{name}");
    }
    assert_eq!(
        hash(&std::fs::read(source).unwrap()),
        fixture["input_sha256"]["$source"]
    );
    assert_eq!(
        hash(&std::fs::read(corrupt).unwrap()),
        fixture["input_sha256"]["$corrupt"]
    );
}
