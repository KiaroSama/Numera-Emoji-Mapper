// Ported from native/tests/coin_paprika.rs at a39a049c02f8dcc90d18fe8771869f82dd655221.
// Original source SHA256: edc9eeef3646faecf3a9022abf944c276d8ac0bb1ab25820cc77a6b484f38fd9.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// Metered search order/waits are observed in virtual time, never paid/live calls.
use _native::coin_paprika::{Response, search_with};
use _native::coin_provider_resolve::Search;
use serde_json::{Value, json};

#[test]
fn paprika_name_ticker_fallback_quota_and_unknowns_match_source() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/paprika-search.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "34742583025adaad3c7513ce0031c13d32759e29cfa8076609150a0badeb1acc"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let mut replies = case["responses"].as_array().unwrap().iter();
        let mut queries = Vec::new();
        let mut waits = Vec::new();
        let result = search_with(
            case["name"].as_str().unwrap(),
            case["ticker"].as_str().unwrap(),
            |query| {
                queries.push(query.to_owned());
                let value = replies.next().unwrap();
                Ok(match value.as_str() {
                    Some("quota") => Response::Quota,
                    Some("transient") => Response::Transient,
                    _ => Response::Payload(value.clone()),
                })
            },
            |duration| {
                waits.push(duration.as_secs_f64());
                Ok(())
            },
        )
        .unwrap();
        let result = match result {
            Search::Quota => json!({"kind":"quota"}),
            Search::Transient => json!({"kind":"transient"}),
            Search::Match { id, confidence } => {
                json!({"kind":"match","id":id,"confidence":confidence})
            }
        };
        assert_eq!(result, case["expected"]);
        assert_eq!(json!(queries), case["queries"]);
        assert_eq!(json!(waits), case["waits"]);
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
