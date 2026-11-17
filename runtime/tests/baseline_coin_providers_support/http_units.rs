//! Real pooled provider wrappers with only their wait clock substituted.
use super::{ProviderHttp, Retry};
use crate as rt;
use crate::baseline_coin_provider_support::server;
use serde_json::json;
use server::{Reply, Server, fault, reply};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
fn response(status: u16, payload: serde_json::Value) -> Reply {
    let mut r = reply("GET /x", payload);
    r.status = status;
    r
}
fn client() -> (ProviderHttp, Arc<Mutex<Vec<Duration>>>) {
    let waits = Arc::new(Mutex::new(vec![]));
    let observer = waits.clone();
    let mut c = ProviderHttp::new(Duration::from_secs(3)).unwrap();
    c.retry_clock = Some(Arc::new(move |d| observer.lock().unwrap().push(d)));
    (c, waits)
}
// tests.test_coin_http.SharedHttpClient.test_no_coin_script_hand_rolls_urlopen_again
#[test]
fn test_no_coin_script_hand_rolls_urlopen_again() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let offenders = std::fs::read_dir(root.join("runtime/src"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("coin_")
                && p.file_name().unwrap() != "coin_http.rs"
                && p.extension().is_some_and(|x| x == "rs")
        })
        .filter(|p| {
            std::fs::read_to_string(p)
                .unwrap()
                .contains("Client::builder(")
        })
        .collect::<Vec<_>>();
    assert!(
        offenders.is_empty(),
        "coin calls must stay on pooled ProviderHttp: {offenders:?}"
    );
}
// tests.test_coin_http.SharedHttpClient.test_402_is_a_quota_answer_and_is_never_retried
#[test]
fn test_402_is_a_quota_answer_and_is_never_retried() {
    let server = Server::new(vec![response(402, json!(null))]);
    let (c, w) = client();
    assert!(matches!(
        rt::coin_paprika::http_json(&c, &format!("{}/x", server.base)).unwrap(),
        rt::coin_paprika::Response::Quota
    ));
    assert_eq!(server.finish().len(), 1);
    assert!(w.lock().unwrap().is_empty());
}
// tests.test_coin_http.SharedHttpClient.test_400_and_404_are_no_match_and_are_never_retried
#[test]
fn test_400_and_404_are_no_match_and_are_never_retried() {
    for status in [400, 404] {
        let server = Server::new(vec![response(status, json!(null))]);
        let (c, w) = client();
        assert!(matches!(
            rt::coin_cmc::http_json(
                &c,
                &format!("{}/x", server.base),
                &reqwest::header::HeaderMap::new()
            )
            .unwrap(),
            rt::coin_cmc::Response::Missing
        ));
        assert_eq!(server.finish().len(), 1);
        assert!(w.lock().unwrap().is_empty());
    }
}
// tests.test_coin_http.SharedHttpClient.test_a_rejected_cmc_key_stops_the_run_and_names_the_key
#[test]
fn test_a_rejected_cmc_key_stops_the_run_and_names_the_key() {
    for status in [401, 403] {
        let server = Server::new(vec![response(status, json!(null))]);
        let (c, w) = client();
        let error = rt::coin_cmc::http_json(
            &c,
            &format!("{}/x", server.base),
            &reqwest::header::HeaderMap::new(),
        )
        .err()
        .expect("rejected key is an error");
        assert!(error.contains("CMC_API_KEY"));
        assert_eq!(server.finish().len(), 1);
        assert!(w.lock().unwrap().is_empty());
    }
}
// tests.test_coin_http.SharedHttpClient.test_an_unanswered_cmc_lookup_is_unknown_not_no_match
#[test]
fn test_an_unanswered_cmc_lookup_is_unknown_not_no_match() {
    let server = Server::new((0..4).map(|_| response(500, json!(null))).collect());
    let (c, w) = client();
    assert!(matches!(
        rt::coin_cmc::http_json(
            &c,
            &format!("{}/x", server.base),
            &reqwest::header::HeaderMap::new()
        )
        .unwrap(),
        rt::coin_cmc::Response::Transient
    ));
    assert_eq!(server.finish().len(), 4);
    assert_eq!(w.lock().unwrap().len(), 3);
}
// tests.test_coin_http.SharedHttpClient.test_a_transient_failure_is_retried_and_then_reported_as_TRANSIENT
#[test]
#[allow(non_snake_case)]
fn test_a_transient_failure_is_retried_and_then_reported_as_TRANSIENT() {
    let server = Server::new((0..4).map(|_| response(500, json!(null))).collect());
    let (c, w) = client();
    assert!(matches!(
        rt::coin_paprika::http_json(&c, &format!("{}/x", server.base)).unwrap(),
        rt::coin_paprika::Response::Transient
    ));
    assert_eq!(server.finish().len(), 4);
    assert_eq!(
        *w.lock().unwrap(),
        vec![
            Duration::from_secs(4),
            Duration::from_secs(8),
            Duration::from_secs(12)
        ],
        "no sleep after final attempt"
    );
}
// tests.test_coin_http.SharedHttpClient.test_a_success_is_decoded_and_costs_one_call
#[test]
fn test_a_success_is_decoded_and_costs_one_call() {
    let want = json!({"currencies":[{"id":"btc"}]});
    let server = Server::new(vec![response(200, want.clone())]);
    let (c, w) = client();
    let rt::coin_paprika::Response::Payload(got) =
        rt::coin_paprika::http_json(&c, &format!("{}/x", server.base)).unwrap()
    else {
        panic!("success not decoded")
    };
    assert_eq!(got, want);
    assert_eq!(server.finish().len(), 1);
    assert!(w.lock().unwrap().is_empty());
}
// tests.test_coin_http.SharedHttpClient.test_retry_after_beats_the_backoff_ladder
#[test]
fn test_retry_after_beats_the_backoff_ladder() {
    let mut busy = response(429, json!(null));
    busy.retry_after = Some("7".into());
    let server = Server::new(vec![busy, response(200, json!([]))]);
    let (c, w) = client();
    let result = c
        .get(
            &format!("{}/x", server.base),
            &reqwest::header::HeaderMap::new(),
            &Retry {
                attempts: 3,
                ..Retry::default()
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.bytes).unwrap(),
        json!([])
    );
    assert_eq!(*w.lock().unwrap(), vec![Duration::from_secs(7)]);
    assert_eq!(server.finish().len(), 2);
}
// tests.test_coin_http.SharedHttpClient.test_an_absurd_retry_after_is_capped
#[test]
fn test_an_absurd_retry_after_is_capped() {
    let mut busy = response(429, json!(null));
    busy.retry_after = Some("3600".into());
    let server = Server::new(vec![busy, response(200, json!([]))]);
    let (c, w) = client();
    assert!(
        c.get(
            &format!("{}/x", server.base),
            &reqwest::header::HeaderMap::new(),
            &Retry {
                attempts: 3,
                ..Retry::default()
            }
        )
        .unwrap()
        .is_some()
    );
    assert_eq!(*w.lock().unwrap(), vec![Duration::from_secs(60)]);
    server.finish();
}
// tests.test_coin_http.SharedHttpClient.test_a_transport_exception_is_retried_like_a_bad_status
#[test]
fn test_a_transport_exception_is_retried_like_a_bad_status() {
    let server = Server::new(vec![fault("GET /x"), fault("GET /x")]);
    let (c, w) = client();
    assert!(
        c.get(
            &format!("{}/x", server.base),
            &reqwest::header::HeaderMap::new(),
            &Retry {
                attempts: 2,
                ..Retry::default()
            }
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(server.finish().len(), 2);
    assert_eq!(*w.lock().unwrap(), vec![Duration::from_secs(4)]);
}
// tests.test_coin_http.SharedHttpClient.test_fetch_logos_still_raises_after_its_retries
#[test]
fn test_fetch_logos_still_raises_after_its_retries() {
    let server = Server::new((0..6).map(|_| response(503, json!(null))).collect());
    let (c, _) = client();
    assert!(
        rt::coin_gecko::http_json(&c, &format!("{}/x", server.base)).is_err(),
        "paging must receive an error after exhaustion"
    );
    assert_eq!(server.finish().len(), 6);
    let server = Server::new((0..2).map(|_| response(503, json!(null))).collect());
    let (c, w) = client();
    assert!(
        c.get(
            &format!("{}/x", server.base),
            &reqwest::header::HeaderMap::new(),
            &Retry {
                attempts: 2,
                ..Retry::default()
            }
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(server.finish().len(), 2);
    assert_eq!(w.lock().unwrap().len(), 1);
}
// tests.test_coin_http.PagingDelay.test_the_default_is_short
#[test]
fn test_the_default_is_short() {
    assert!(super::page_delay("").unwrap() <= Duration::from_secs(3));
}
// tests.test_coin_http.PagingDelay.test_the_env_var_overrides_it
#[test]
fn test_the_env_var_overrides_it() {
    assert_eq!(super::page_delay("15").unwrap(), Duration::from_secs(15));
}
// tests.test_coin_http.PagingDelay.test_a_typo_falls_back_instead_of_crashing_at_import
#[test]
fn test_a_typo_falls_back_instead_of_crashing_at_import() {
    assert_eq!(
        super::page_delay("12 seconds").unwrap(),
        Duration::from_secs(2)
    );
}
