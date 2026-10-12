//! Actual rendered source bytes, short-clip policy and exclusive preview publication.
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
fn lottie_previews_match_pinned_source_and_preserve_existing_outputs() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let path = std::env::var_os("NUMERA_LOTTIE_PREVIEW_ORACLE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("tests/fixtures/native/lottie-preview.json"));
    let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
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
    let directory = Scratch(
        root.join("logs")
            .join(format!("lottie preview {}", std::process::id())),
    );
    std::fs::create_dir(&directory.0).unwrap();
    for case in cases {
        let source = root.join(case["input"].as_str().unwrap());
        let output = directory
            .0
            .join(format!("{}.webp", case["name"].as_str().unwrap()));
        let hash = |bytes: &[u8]| {
            Sha256::digest(bytes)
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>()
        };
        assert_eq!(hash(&std::fs::read(&source).unwrap()), case["input_sha256"]);
        let request = serde_json::json!({"operation":"preview","source":source,
            "output":output,"format":"animated","key":"source-fixture",
            "size":case["size"],"fps":case["fps"],"still":case["still"]});
        assert_eq!(
            _native::codec_worker::execute(&request).unwrap(),
            serde_json::json!({"bytes":case["expected"]["bytes"]})
        );
        let bytes = std::fs::read(&output).unwrap();
        assert_eq!(
            hash(&bytes),
            case["expected"]["webp_sha256"],
            "{}",
            case["name"]
        );
        assert!(_native::codec_worker::execute(&request).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), bytes);
        std::fs::remove_file(&output).unwrap();
        let mut invalid = request.clone();
        invalid["size"] = serde_json::json!(true);
        assert_eq!(
            _native::codec_worker::execute(&invalid).unwrap_err(),
            "invalid preview parameters"
        );
        assert!(!output.exists());
    }
    let request = serde_json::json!({"operation":"preview","source":root.join("tests/fixtures/native/tgs-load/root-list.tgs"),
        "output":directory.0.join("failed.webp"),"format":"animated","key":"source-fixture"});
    assert!(
        _native::codec_worker::execute(&request)
            .unwrap_err()
            .contains("expected an object")
    );
    assert!(std::fs::read_dir(&directory.0).unwrap().next().is_none());
}
