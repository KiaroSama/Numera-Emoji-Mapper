//! Per-coin durable snapshots and unanswered searches match the pinned original resolver.
use _native::coin_provider_resolve::{Search, resolve};
use serde_json::{Value, json};

#[test]
fn quota_unknown_cached_miss_and_retry_order_match_source() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/paprika-resolve.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "360809a22b57c19808b146016fe388769046f7337986684fd4d9c4db6f268b30"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let missing: Vec<(String, String)> =
            serde_json::from_value(case["missing"].clone()).unwrap();
        let mut cache = case["initial"].as_object().unwrap().clone();
        let mut replies = case["responses"].as_array().unwrap().iter();
        let mut calls = Vec::new();
        let mut saved = Vec::new();
        let result = resolve(
            &missing,
            &mut cache,
            case["retry_unmatched"].as_bool().unwrap(),
            |_, ticker| {
                calls.push(ticker.to_owned());
                let value = replies.next().unwrap();
                Ok(match value.as_str() {
                    Some("quota") => Search::Quota,
                    Some("transient") => Search::Transient,
                    _ => Search::Match {
                        id: value[0].as_str().map(str::to_owned),
                        confidence: value[1].as_str().unwrap().into(),
                    },
                })
            },
            |cache| {
                saved.push(json!(cache));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            json!({"cache":cache,"calls":calls,"saved":saved,"quota":result.quota,"unknown":result.unknown}),
            case["expected"]
        );
    }
}
