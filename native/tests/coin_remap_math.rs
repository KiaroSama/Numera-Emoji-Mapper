//! Independent source signature/BLAS results at image and bounded numerical seams.
use serde_json::Value;
use sha2::{Digest, Sha256};
#[test]
fn source_signatures_and_nearest_preserve_alpha_first_ties_and_chunk_boundary() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-remap-math.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "b66136d42adc3448d44e67c36c9f8c827a4e63d6ce259af5d6b437299b043ed7"
    );
    for case in fixture["images"].as_array().unwrap() {
        let data = std::fs::read(root.join(case["input"].as_str().unwrap())).unwrap();
        assert_eq!(
            Sha256::digest(&data)
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect::<String>(),
            case["input_sha256"].as_str().unwrap()
        );
        assert_eq!(
            serde_json::json!(_native::codec_image::remap_signature(&data).unwrap()),
            case["expected"]
        );
    }
    for case in fixture["nearest"].as_array().unwrap() {
        let rows = |key: &str| {
            case[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    row.as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_f64().unwrap() as f32)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let result = _native::coin_remap_math::nearest(&rows("local"), &rows("live")).unwrap();
        assert_eq!(serde_json::json!(result.indices), case["expected"]["index"]);
        assert_eq!(serde_json::json!(result.squared), case["expected"]["d2"]);
        assert_eq!(
            result.second.len(),
            case["expected"]["second"].as_array().unwrap().len()
        );
        for (actual, expected) in result
            .second
            .iter()
            .zip(case["expected"]["second"].as_array().unwrap())
        {
            if expected.is_null() {
                assert_eq!(*actual, f64::INFINITY);
            } else {
                assert_eq!(*actual, expected.as_f64().unwrap());
            }
        }
    }
    assert!(_native::codec_image::remap_signature(b"not image").is_err());
    assert!(_native::coin_remap_math::nearest(&[vec![1.0]], &[]).is_err());
    assert!(_native::coin_remap_math::nearest(&[vec![f32::NAN]], &[vec![1.0]]).is_err());
    assert!(_native::coin_remap_math::nearest(&[vec![1.0]], &[vec![1.0, 2.0]]).is_err());
}
