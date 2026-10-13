//! Source fixture proves inversion tolerance without accepting a different logo.
use serde_json::Value;
use sha2::{Digest, Sha256};
#[test]
fn restyling_is_not_wrong_art_but_different_content_remains_far() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/logo-distance.json"
    ))
    .unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let x = std::fs::read(root.join(case["ours"].as_str().unwrap())).unwrap();
        let y = std::fs::read(root.join(case["theirs"].as_str().unwrap())).unwrap();
        let hash = |data: &[u8]| {
            Sha256::digest(data)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        assert_eq!(hash(&x), case["ours_sha256"].as_str().unwrap());
        assert_eq!(hash(&y), case["theirs_sha256"].as_str().unwrap());
        let distance = _native::codec_image::logo_distance(&x, &y).unwrap();
        assert_eq!(u64::from(distance), case["expected"].as_u64().unwrap());
        if index == 0 {
            assert!(case["plain"].as_u64().unwrap() > 20);
            assert!(distance <= 8);
        }
        if index == 1 {
            assert!(distance > 20);
        }
    }
    assert!(_native::codec_image::logo_distance(b"bad", b"bad").is_err());
}
