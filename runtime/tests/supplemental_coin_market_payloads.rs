//! Malformed successful market replies must not claim a complete provider run.
extern crate numera_runtime as production;

use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

fn invalid_markets() -> Vec<Value> {
    vec![
        json!({"error":"temporarily unavailable"}),
        json!({}),
        json!("temporarily unavailable"),
        json!(true),
        json!(42),
    ]
}

#[test]
fn malformed_markets_keep_name_lookup_incomplete_and_still_read_the_long_tail() {
    for payload in invalid_markets() {
        let mut requests = Vec::new();
        let (names, complete) = production::coin_gecko::names_with(
            Duration::from_secs(2),
            |url| {
                requests.push(url.to_owned());
                Ok(if url.ends_with("/coins/list") {
                    json!([{"symbol":"btc","name":"Bitcoin"}])
                } else {
                    payload.clone()
                })
            },
            |_| panic!("a malformed market page must not incur a page wait"),
        )
        .unwrap();
        assert!(
            !complete,
            "malformed market payload was accepted: {payload}"
        );
        assert_eq!(json!(names), json!({"btc":"Bitcoin"}));
        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("/coins/markets?"));
        assert!(requests[1].ends_with("/coins/list"));
    }
}

#[test]
fn malformed_later_market_page_preserves_earlier_name_precedence() {
    let replies = [
        json!([{"symbol":"btc","name":"Market Bitcoin"}]),
        json!({"error":"temporarily unavailable"}),
        json!([
            {"symbol":"btc","name":"Tail Bitcoin"},
            {"symbol":"eth","name":"Ethereum"}
        ]),
    ];
    let mut requests = Vec::new();
    let mut waits = Vec::new();
    let (names, complete) = production::coin_gecko::names_with(
        Duration::from_secs(2),
        |url| {
            let reply = replies[requests.len()].clone();
            requests.push(url.to_owned());
            Ok(reply)
        },
        |duration| {
            waits.push(duration);
            Ok(())
        },
    )
    .unwrap();
    assert!(!complete);
    assert_eq!(
        json!(names),
        json!({"btc":"Market Bitcoin","eth":"Ethereum"})
    );
    assert_eq!(requests.len(), 3);
    assert!(requests[0].contains("page=1&"));
    assert!(requests[1].contains("page=2&"));
    assert!(requests[2].ends_with("/coins/list"));
    assert_eq!(waits, vec![Duration::from_secs(2)]);
}

fn directory() -> tempfile::TempDir {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent =
        production::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    tempfile::Builder::new()
        .prefix("market-payload-")
        .tempdir_in(parent)
        .unwrap()
}

#[test]
fn malformed_markets_leave_cached_art_and_names_available_but_report_incomplete() {
    const SVG: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><rect width=\"10\" height=\"10\"/></svg>";
    const CSV: &[u8] = b"ticker,name,format,file,keywords\r\nbtc,Bitcoin,svg,logos/svg/btc.svg,\"btc, Bitcoin\"\r\n";
    for payload in invalid_markets() {
        let directory = directory();
        let svg = directory.path().join("logos/svg/btc.svg");
        std::fs::create_dir_all(svg.parent().unwrap()).unwrap();
        std::fs::write(&svg, SVG).unwrap();
        let csv = directory.path().join("keywords.csv");
        std::fs::write(&csv, CSV).unwrap();
        let mut requests = 0;
        let exit = production::coin_gecko_fetch::fetch_with(
            directory.path(),
            2,
            Duration::ZERO,
            |url, binary, attempts| {
                requests += 1;
                assert!(url.contains("page=1&"));
                assert!(!binary);
                assert_eq!(attempts, 6);
                Ok(production::coin_gecko_fetch::Payload::Json(payload.clone()))
            },
            |_| panic!("a malformed market page must not incur a page wait"),
        )
        .unwrap();
        assert_eq!(exit, 3, "malformed market payload was accepted: {payload}");
        assert_eq!(requests, 1);
        assert_eq!(std::fs::read(&svg).unwrap(), SVG);
        assert_eq!(std::fs::read(&csv).unwrap(), CSV);
        assert_eq!(
            std::fs::read_dir(directory.path().join("logos/png"))
                .unwrap()
                .count(),
            0
        );
    }
}

#[test]
fn empty_and_legacy_null_market_pages_remain_compatible_completion() {
    for payload in [json!([]), Value::Null] {
        let directory = directory();
        let code = production::coin_gecko_fetch::fetch_with(
            directory.path(),
            1,
            Duration::ZERO,
            |_, binary, _| {
                assert!(!binary);
                Ok(production::coin_gecko_fetch::Payload::Json(payload.clone()))
            },
            |_| panic!("completed market paging must not incur another wait"),
        )
        .unwrap();
        assert_eq!(code, 0);
        let (names, complete) = production::coin_gecko::names_with(
            Duration::ZERO,
            |url| {
                Ok(if url.ends_with("/coins/list") {
                    json!([{"symbol":"btc","name":"Bitcoin"}])
                } else {
                    payload.clone()
                })
            },
            |_| panic!("completed market paging must not incur another wait"),
        )
        .unwrap();
        assert!(complete);
        assert_eq!(json!(names), json!({"btc":"Bitcoin"}));
    }
}

#[path = "support/test_admission.rs"]
mod test_admission;
