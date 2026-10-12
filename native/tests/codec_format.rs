//! Independent source header/extension and collision-key observations.
use serde_json::{Value, json};
use std::path::PathBuf;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn source_detection_and_collision_lookup_contract_is_preserved() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/media-format.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/media.py"],
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/identity.py"],
        "8cbefd550e306e671df1e52af2dd9fe6b4e3647c3aa1b13dc98bf35080d36e89"
    );
    let decode = |value: &Value| {
        value
            .as_str()
            .unwrap()
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>()
    };
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 22);
    let directory = Scratch(
        root.join("logs")
            .join(format!("codec format {}", std::process::id())),
    );
    std::fs::create_dir(&directory.0).unwrap();
    for case in cases {
        let bytes = decode(&case["header_hex"]);
        let path = directory.0.join(case["name"].as_str().unwrap());
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            _native::codec_format::detect_bytes(&bytes),
            case["expected"]["header_format"]
        );
        assert_eq!(
            _native::codec_format::detect(&path).unwrap(),
            case["expected"]["format"]
        );
        assert_eq!(
            _native::codec_format::extension(&path, "static").unwrap(),
            case["expected"]["extension"]
        );
        assert_eq!(
            _native::codec_format::extension(&path, "video").unwrap(),
            ".webm"
        );
        assert_eq!(
            _native::codec_format::extension(&path, "animated").unwrap(),
            ".tgs"
        );
        std::fs::remove_file(path).unwrap();
    }
    let path = directory.0.join("collision.bin");
    std::fs::write(&path, decode(&fixture["collision_input_hex"])).unwrap();
    let collisions = fixture["collisions"].as_array().unwrap();
    assert_eq!(collisions.len(), 8);
    for case in collisions {
        let key = case["key"].as_str().unwrap();
        assert_eq!(
            _native::codec_format::collision_key(&path, key).unwrap(),
            case["expected"]
        );
        if !key.is_empty() {
            assert_eq!(
                _native::codec_worker::execute(&json!({"operation":"collision","source":path,
                "format":"video","key":key}))
                .unwrap(),
                case["expected"]
            );
        }
    }
    std::fs::remove_file(path).unwrap();
    let source = root.join("tests/fixtures/native/static-codec/opaque.png");
    let expected = _native::codec_worker::execute(
        &json!({"operation":"fingerprint","source":source,"format":"static"}),
    )
    .unwrap();
    for format in [Value::Null, json!(false), json!("")] {
        assert_eq!(
            _native::codec_worker::execute(&json!({"operation":"fingerprint","source":source,
            "format":format}))
            .unwrap(),
            expected
        );
    }
    assert_eq!(
        _native::codec_worker::execute(&json!({"operation":"fingerprint","source":source}))
            .unwrap(),
        expected
    );
    assert!(
        _native::codec_worker::execute(&json!({"operation":"collision","source":source,"key":""}))
            .is_err()
    );
    assert!(std::fs::read_dir(&directory.0).unwrap().next().is_none());
}
