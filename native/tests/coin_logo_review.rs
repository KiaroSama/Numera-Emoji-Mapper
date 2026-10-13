//! Source report-only filter/threshold/reverse tuple order with virtual provider calls.
use _native::coin_logo_review::review_with;
use serde_json::{Value, json};
#[test]
fn source_review_skips_missing_unreadable_art_and_preserves_flag_order() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-logo-review.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "86f17862ebe5019e05965fa7aa30380a20226b3632f192f051525290ae7f7031"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let mut calls = Vec::new();
        let mut images = Vec::new();
        let mut waits = Vec::new();
        let result = review_with(
            input["top"].as_i64().unwrap(),
            input["threshold"].as_i64().unwrap(),
            |url| {
                calls.push(url.to_owned());
                Ok(input["coins"].clone())
            },
            |symbol| {
                input["local"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|value| value == symbol)
            },
            |url| {
                images.push(url.to_owned());
                Ok(Some(b"fixture".to_vec()))
            },
            |symbol, _bytes| {
                if let Some(error) = input["distances"][symbol].as_str() {
                    Err(error.into())
                } else {
                    Ok(input["distances"][symbol].as_u64().unwrap() as u32)
                }
            },
            |duration| waits.push(duration.as_secs_f64()),
        )
        .unwrap();
        assert_eq!(json!(result), case["expected"]["flagged"]);
        assert_eq!(json!(calls), case["expected"]["calls"]);
        assert_eq!(json!(images), case["expected"]["images"]);
        assert_eq!(
            waits,
            case["expected"]["waits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_f64().unwrap())
                .collect::<Vec<_>>()
        );
    }
}
