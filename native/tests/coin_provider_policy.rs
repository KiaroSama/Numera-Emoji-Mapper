//! Pure independent source provider policies; no remote requests or secret-bearing environment.
use serde_json::{Value, json};

#[test]
fn provider_candidate_precedence_and_status_unknowns_match_source() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/provider-policy.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["coins/_paprika_api.py"],
        "34742583025adaad3c7513ce0031c13d32759e29cfa8076609150a0badeb1acc"
    );
    for case in fixture["classifications"].as_array().unwrap() {
        assert_eq!(
            _native::coin_provider_policy::classify(
                case["candidates"].as_array().unwrap(),
                case["name"].as_str().unwrap(),
                case["ticker"].as_str().unwrap()
            )
            .unwrap(),
            case["expected"]
        );
    }
    for case in fixture["statuses"].as_array().unwrap() {
        let status = case["status"].as_u64().map(|value| value as u16);
        assert_eq!(
            _native::coin_provider_policy::response(
                case["provider"].as_str().unwrap(),
                status,
                json!({"ok":"payload"})
            )
            .unwrap(),
            case["expected"]
        );
    }
    assert!(_native::coin_provider_policy::response("unknown", Some(200), Value::Null).is_err());
}
