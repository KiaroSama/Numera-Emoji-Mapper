//! Worker announcements need a validated acknowledgment before recording a sent milestone.
#[allow(dead_code)]
#[path = "baseline_cli_status_support/http.rs"]
mod http;
#[path = "support/test_admission.rs"]
mod test_admission;

use http::{Reply, Server};
use numera_runtime::{announce, atomic, coin_rebuild_links, config::Config};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};

const NAME: &str = "fixture1_by_bot";
const MARKER: &str = "private-acknowledgment-fixture-detail";

fn packs() -> Vec<Value> {
    vec![json!({"name":NAME,"title":"Fixture 1","count":1})]
}

fn config(root: &Path, server: &Server) -> Config {
    Config::load_with_environment(
        root.into(),
        &BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            (
                "WORKER_PUBLISH_URL".into(),
                format!("{}/publish", server.base),
            ),
            (
                "WORKER_PUBLISH_SECRET".into(),
                "fixture-worker-secret".into(),
            ),
        ]),
    )
    .unwrap()
}

fn fixed(status: u16, body: Vec<u8>) -> Server {
    Server::new(move |request| {
        assert_eq!(request.verb, "POST");
        assert_eq!(request.path, "/publish");
        Reply::Bytes(status, body.clone())
    })
}

fn send(config: &Config) -> Result<(), String> {
    announce::worker(config, &packs(), "coin", "Fixture note", "cards", 2.0)
}

fn one_request(server: &Server) {
    let calls = server.observed.lock().unwrap();
    assert_eq!(
        calls.len(),
        1,
        "an uncertain announcement must not be retried"
    );
    assert_eq!(calls[0].verb, "POST");
    assert_eq!(calls[0].path, "/publish");
    assert_eq!(
        calls[0].headers["authorization"],
        "Bearer fixture-worker-secret"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&calls[0].body).unwrap(),
        json!({"bot":"coin","packs":packs(),"style":"cards","note":"Fixture note"})
    );
}

fn refused(bodies: Vec<(&str, Vec<u8>)>) {
    let mut accepted = Vec::new();
    for (name, body) in bodies {
        let root = tempfile::tempdir().unwrap();
        let server = fixed(200, body);
        let result = send(&config(root.path(), &server));
        one_request(&server);
        match result {
            Ok(()) => accepted.push(name),
            Err(error) => {
                assert!(
                    !error.contains(MARKER),
                    "response detail must not escape into logs"
                );
                assert!(!error.contains("fixture-worker-secret"));
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "invalid acknowledgments accepted: {accepted:?}"
    );
}

#[test]
fn worker_refuses_failure_and_malformed_acknowledgment_envelopes() {
    refused(
        [
            ("null", Value::Null),
            ("array", json!([])),
            ("scalar", json!(true)),
            ("empty object", json!({})),
            ("missing ok", json!({"message_ids":[1]})),
            (
                "false ok",
                json!({"ok":false,"message_ids":[1],"error":MARKER}),
            ),
            ("string ok", json!({"ok":"true","message_ids":[1]})),
            ("numeric ok", json!({"ok":1,"message_ids":[1]})),
        ]
        .into_iter()
        .map(|(name, value)| (name, serde_json::to_vec(&value).unwrap()))
        .collect(),
    );
}

#[test]
fn worker_requires_nonempty_complete_message_identifiers() {
    refused(
        [
            ("missing identifiers", json!({"ok":true})),
            ("null identifiers", json!({"ok":true,"message_ids":null})),
            ("scalar identifiers", json!({"ok":true,"message_ids":1})),
            ("empty identifiers", json!({"ok":true,"message_ids":[]})),
            ("null identifier", json!({"ok":true,"message_ids":[null]})),
            (
                "mixed identifiers",
                json!({"ok":true,"message_ids":[1,null]}),
            ),
            ("negative identifier", json!({"ok":true,"message_ids":[-1]})),
            (
                "fractional identifier",
                json!({"ok":true,"message_ids":[1.5]}),
            ),
            ("string identifier", json!({"ok":true,"message_ids":["1"]})),
            (
                "boolean identifier",
                json!({"ok":true,"message_ids":[true]}),
            ),
            (
                "unsafe identifier",
                json!({"ok":true,"message_ids":[9_007_199_254_740_992_u64]}),
            ),
        ]
        .into_iter()
        .map(|(name, value)| (name, serde_json::to_vec(&value).unwrap()))
        .collect(),
    );
}

#[test]
fn worker_rejects_object_shaped_message_identifiers() {
    refused(vec![
        (
            "object identifier",
            br#"{"ok":true,"message_ids":[{"$serde_json::private::Number":"1"}]}"#.to_vec(),
        ),
        (
            "mixed numeric and object identifiers",
            br#"{"ok":true,"message_ids":[0,{"$serde_json::private::Number":"1"}]}"#.to_vec(),
        ),
    ]);
}

#[test]
fn worker_requires_an_object_acknowledgment_envelope() {
    refused(vec![
        ("tuple envelope", br#"[true,[1]]"#.to_vec()),
        ("extended tuple envelope", br#"[true,[1],"extra"]"#.to_vec()),
    ]);
}

#[test]
fn worker_refuses_non_json_and_empty_success_responses() {
    refused(vec![
        ("empty body", vec![]),
        ("bare ok", b"ok".to_vec()),
        ("HTML", format!("<html>{MARKER}</html>").into_bytes()),
        (
            "truncated JSON",
            b"{\"ok\":true,\"message_ids\":[1]".to_vec(),
        ),
    ]);
}

#[test]
fn worker_acknowledgments_have_a_bounded_response_body() {
    refused(vec![(
        "oversized acknowledgment",
        serde_json::to_vec(&json!({
            "ok":true,"message_ids":[1],"padding":"x".repeat(64 * 1024)
        }))
        .unwrap(),
    )]);
}

#[test]
fn valid_zero_and_multiple_message_identifiers_are_accepted_once() {
    for ids in [
        json!([0]),
        json!([7, 11]),
        json!([9_007_199_254_740_991_u64]),
    ] {
        let root = tempfile::tempdir().unwrap();
        let server = fixed(
            200,
            serde_json::to_vec(&json!({"ok":true,"message_ids":ids,"extra":"compatible"})).unwrap(),
        );
        send(&config(root.path(), &server)).unwrap();
        one_request(&server);
    }
}

#[test]
fn http_and_transport_failures_are_not_retried() {
    for status in [401, 429, 502] {
        let root = tempfile::tempdir().unwrap();
        let server = fixed(
            status,
            serde_json::to_vec(&json!({"ok":true,"message_ids":[1]})).unwrap(),
        );
        let result = send(&config(root.path(), &server));
        assert!(result.unwrap_err().contains(&format!("HTTP {status}")));
        one_request(&server);
    }
    let root = tempfile::tempdir().unwrap();
    let server = Server::new(|_| Reply::Disconnect);
    let result = send(&config(root.path(), &server));
    assert!(result.unwrap_err().contains("outcome unknown"));
    one_request(&server);
}

#[test]
fn invalid_ack_does_not_persist_or_suppress_a_coin_announcement() {
    let root = tempfile::tempdir().unwrap();
    let reply = Arc::new(Mutex::new(json!({"ok":false,"error":MARKER})));
    let response = reply.clone();
    let server = Server::new(move |_| Reply::json(response.lock().unwrap().clone()));
    let config = config(root.path(), &server);
    let tg = server.telegram();
    let path = root.path().join("rebuild_dedup_state.json");
    let mut state = json!({"sent":[],"order":["seed"],"cursor":1});
    atomic::write_json(&path, &state).unwrap();
    let before_state = state.clone();
    let before_file = std::fs::read(&path).unwrap();
    let mut messages = Vec::new();
    let mut notify = |state: &mut Value| {
        coin_rebuild_links::notify(
            state,
            NAME,
            || announce::packs(&config, &tg, 7, &packs(), "coin", "", "cards"),
            |value| atomic::write_json(&path, value).map_err(|error| error.to_string()),
            |message| messages.push(message.to_owned()),
        )
        .unwrap();
    };
    notify(&mut state);
    assert_eq!(
        state, before_state,
        "a failed acknowledgment cannot mark the pack sent"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before_file);
    assert_eq!(server.methods(), ["publish"]);

    *reply.lock().unwrap() = json!({"ok":true,"message_ids":[0]});
    notify(&mut state);
    assert_eq!(state["sent"], json!([NAME]));
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(),
        state
    );
    notify(&mut state);
    assert_eq!(server.methods(), ["publish", "publish"]);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("notify failed"))
    );
    assert!(messages.iter().all(|message| !message.contains(MARKER)));
}

#[test]
fn final_coin_links_require_acknowledgment_before_saving_final_sent() {
    let root = tempfile::tempdir().unwrap();
    let reply = Arc::new(Mutex::new(json!({"ok":true,"message_ids":[null]})));
    let response = reply.clone();
    let server = Server::new(move |_| Reply::json(response.lock().unwrap().clone()));
    let config = config(root.path(), &server);
    let tg = server.telegram();
    let path = root.path().join("rebuild_dedup_state.json");
    let state = json!({"sets":[{"index":1,"name":NAME,"title":"Fixture 1","live":1}],
        "sent":[],"order":["seed"],"cursor":1,"in_flight":null,
        "deleted_old":true,"final_sent":false});
    assert!(numera_runtime::coin_state::problem(&state, None).is_empty());
    atomic::write_json(&path, &state).unwrap();
    let before = std::fs::read(&path).unwrap();
    let result = coin_rebuild_links::send(&config, root.path(), "fixture", "Fixture", 7, &tg);
    assert!(
        result.is_err(),
        "a malformed acknowledgment cannot confirm final links"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(server.methods(), ["publish"]);

    *reply.lock().unwrap() = json!({"ok":true,"message_ids":[17,19]});
    coin_rebuild_links::send(&config, root.path(), "fixture", "Fixture", 7, &tg).unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["final_sent"], true);
    assert_eq!(server.methods(), ["publish", "publish"]);
}
