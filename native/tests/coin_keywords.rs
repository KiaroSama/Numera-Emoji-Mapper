//! Exact pinned UTF-8/CRLF CSV, name retention and distinct image admission policy.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[test]
fn keyword_writer_preserves_source_csv_names_order_and_verified_image_admission() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/keyword-writer.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "7c157ebff8fd5dbb8ec28c87ce8e85b70559601a8671b01c2510fe4e9f70f75f"
    );
    for case in fixture["valid"].as_array().unwrap() {
        let path = root.join(case["input"].as_str().unwrap());
        let hash: String = Sha256::digest(std::fs::read(&path).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(hash, case["input_sha256"]);
        assert_eq!(
            _native::codec_image::valid_logo(&path),
            case["expected"].as_bool().unwrap()
        );
    }
    let scratch = root
        .join("logs/tmp")
        .join(format!("keyword writer {}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let path = scratch.join(format!("keywords-{index}.csv"));
        std::fs::write(&path, case["old"].as_str().unwrap().as_bytes()).unwrap();
        let mut rows = case["rows"].as_object().unwrap().clone();
        for row in rows.values_mut() {
            if let Some(input) = row.get("path").and_then(Value::as_str) {
                row["path"] = Value::String(root.join(input).to_string_lossy().into_owned());
            }
        }
        _native::coin_keywords::write(&rows, &path).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            case["expected"].as_str().unwrap().as_bytes()
        );
        std::fs::remove_file(&path).unwrap();
    }
    let corrupt = scratch.join("corrupt.png");
    std::fs::write(&corrupt, b"not PNG").unwrap();
    assert!(!_native::codec_image::valid_logo(&corrupt));
    std::fs::remove_file(corrupt).unwrap();
    let broken = scratch.join("broken.csv");
    std::fs::write(&broken, b"other,name\r\nx,old\r\n").unwrap();
    assert!(_native::coin_keywords::write(&serde_json::Map::new(), &broken).is_err());
    assert_eq!(std::fs::read(&broken).unwrap(), b"other,name\r\nx,old\r\n");
    std::fs::remove_file(broken).unwrap();
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
    std::fs::remove_dir(scratch).unwrap();
}
