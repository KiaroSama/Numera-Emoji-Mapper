//! CMC metadata is synthetic; no paid provider calls or credentials.
use _native::coin_cmc::{Response, candidates_with, headers, logo_with, symbols};
use serde_json::{Value, json};

#[test]
fn metadata_logo_refusals_resize_and_metering_match_pinned_source() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/cmc-logo.json")).unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "74a5a49bc7bbe91d3718d27886ce97f55d3479c3cd78d89a8a711389d1bbe5e9"
    );
    assert!(headers("").is_err());
    assert!(headers(" \t").is_err());
    assert!(headers("key\nheader").is_err());
    let h = headers(" synthetic-test-key ").unwrap();
    assert_eq!(h["X-CMC_PRO_API_KEY"], "synthetic-test-key");
    assert!(h["X-CMC_PRO_API_KEY"].is_sensitive());
    assert_eq!(h["Accept"], "application/json");
    for case in fixture["cases"].as_array().unwrap() {
        let mut calls = Vec::new();
        let mut waits = Vec::new();
        let result = logo_with(
            case["id"].as_str().unwrap(),
            |url| {
                calls.push(url.to_owned());
                Ok(Response::Payload(case["response"].clone()))
            },
            |duration| {
                waits.push(duration.as_secs_f64());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(json!(result), case["expected"]);
        assert_eq!(json!(calls), case["calls"]);
        assert_eq!(json!(waits), case["waits"]);
    }
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/cmc-map.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let ordered: Vec<String> = serde_json::from_value(case["queries"].clone()).unwrap();
        let mut actual_symbols = symbols(case["ticker"].as_str().unwrap());
        let mut expected_symbols = ordered.clone();
        actual_symbols.sort();
        expected_symbols.sort();
        assert_eq!(actual_symbols, expected_symbols);
        let mut calls = Vec::new();
        let mut waits = Vec::new();
        let (candidates, transient) = candidates_with(
            &ordered,
            |symbol| {
                calls.push(symbol.to_owned());
                let payload = case["responses"][symbol].clone();
                Ok(if payload == "transient" {
                    Response::Transient
                } else {
                    Response::Payload(payload)
                })
            },
            |duration| {
                waits.push(duration.as_secs_f64());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            json!({"candidates":candidates,"transient":transient}),
            case["expected"]
        );
        assert_eq!(json!(calls), case["queries"]);
        assert_eq!(json!(waits), case["waits"]);
    }
}
