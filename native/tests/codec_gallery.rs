//! Gallery policy differs from panel previews: 88px/Lanczos/quality65 and source animation flags.
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
fn gallery_artwork_and_animation_flags_match_source() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let committed: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/gallery-codec.json"
    ))
    .unwrap();
    let fixture: Value = std::env::var_os("NUMERA_GALLERY_CODEC_ORACLE")
        .map(PathBuf::from)
        .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap())
        .unwrap_or_else(|| committed.clone());
    assert_eq!(fixture["source_sha256"], committed["source_sha256"]);
    assert_eq!(
        fixture["cases"].as_array().unwrap().len(),
        committed["cases"].as_array().unwrap().len()
    );
    for (source, original) in fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(committed["cases"].as_array().unwrap())
    {
        for field in ["input", "input_sha256", "format", "still", "animation_info"] {
            assert_eq!(source[field], original[field]);
        }
    }
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/media_bridge.py"],
        "8d7f86244ed498b32381ba78a6321622aa28d9b72c1a1adba9a48c39565dd5b7"
    );
    let scratch = Scratch(
        root.join("logs/tmp")
            .join(format!("gallery codec {}", std::process::id())),
    );
    std::fs::create_dir(&scratch.0).unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let source = root.join(case["input"].as_str().unwrap());
        let hash = |bytes: &[u8]| {
            Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        assert_eq!(hash(&std::fs::read(&source).unwrap()), case["input_sha256"]);
        let output = scratch.0.join(format!("{index}.webp"));
        let request = json!({"operation":"gallery_thumb","source":source,"format":case["format"],"still":case["still"],"output":output});
        let result = _native::codec_worker::execute(&request).unwrap();
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(hash(&bytes), case["sha256"], "{index}");
        assert_eq!(result["bytes"], case["bytes"]);
        assert!(_native::codec_worker::execute(&request).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), bytes);
        assert_eq!(
            _native::codec_worker::execute(
                &json!({"operation":"animation_info","source":source,"format":case["format"]})
            )
            .unwrap(),
            case["animation_info"]
        );
    }
    let bad = json!({"operation":"gallery_thumb","source":root.join("tests/fixtures/native/static-codec/opaque.png"),"format":"static","still":1,"output":scratch.0.join("bad.webp")});
    assert!(_native::codec_worker::execute(&bad).is_err());
    assert!(!scratch.0.join("bad.webp").exists());
    std::fs::remove_dir_all(&scratch.0).unwrap();
}
