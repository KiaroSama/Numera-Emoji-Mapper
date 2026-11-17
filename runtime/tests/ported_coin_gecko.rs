// Ported from native/tests/coin_gecko.rs at a32dbab6e3e694f1fac9656997fa2edb6e3b2ede.
// Original source SHA256: 4f56bbe8822e77c32f21df9af1d6a00310a749776d175601476e7a53d860dc6c.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// Canonical provider name precedence and completeness are pinned independent source traces.
use serde_json::{Value, json};
use std::time::Duration;

#[test]
fn market_order_long_tail_failures_and_page_waits_match_source() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/gecko-names.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let mut replies = case["responses"].as_array().unwrap().iter();
        let mut calls = Vec::new();
        let mut waits = Vec::new();
        let (names, complete) = _native::coin_gecko::names_with(
            Duration::from_secs(2),
            |url| {
                calls.push(url.to_owned());
                let value = replies.next().unwrap();
                if value == "failure" {
                    Err("controlled provider unavailable".into())
                } else {
                    Ok(value.clone())
                }
            },
            |duration| {
                waits.push(duration.as_secs_f64());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(json!({"names":names,"complete":complete}), case["expected"]);
        assert_eq!(json!(calls), case["calls"]);
        assert_eq!(json!(waits), case["waits"]);
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
