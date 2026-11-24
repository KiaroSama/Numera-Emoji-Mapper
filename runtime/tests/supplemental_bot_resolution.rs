//! A failed ID lookup must never become evidence that an emoji does not exist.
#[allow(dead_code)]
#[path = "baseline_cli_status_support/http.rs"]
mod http;
#[path = "support/test_admission.rs"]
mod test_admission;

use numera_runtime::{emoji_bot, logging::RunLog};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const ID: &str = "5893098741073715471";

fn message(text: &str) -> Value {
    json!({"message":{"message_id":17,"text":text,
        "from":{"id":42},"chat":{"id":42,"type":"private"}}})
}

fn handle(server: &http::Server, update: &Value) -> Result<(), String> {
    let root = tempfile::tempdir().unwrap();
    let log = RunLog::open(
        root.path(),
        "bot-resolution",
        vec!["test-only-token".into()],
    );
    let result = emoji_bot::handle(
        &server.telegram(),
        42,
        update,
        &BTreeSet::from([42]),
        &BTreeSet::new(),
        &log,
    );
    log.finish(i32::from(result.is_err()));
    result
}

#[test]
fn rejected_lookup_does_not_report_typed_ids_as_nonexistent() {
    let server = http::Server::new(|request| match request.method() {
        "getCustomEmojiStickers" => http::Reply::json(json!({
            "ok":false,"error_code":401,"description":"Unauthorized fixture"
        })),
        "sendMessage" => http::Reply::ok(json!({"message_id":1,"chat":{"id":42}})),
        method => panic!("unexpected method {method}"),
    });
    let outcome = handle(&server, &message(ID));
    assert!(outcome.is_err(), "failed lookup was presented as absence");
    assert_eq!(server.methods(), ["getCustomEmojiStickers"]);
}

#[test]
fn later_lookup_failure_does_not_send_a_partial_absence_claim() {
    let ids = (0..201)
        .map(|index| (5_893_098_741_073_715_000_u64 + index).to_string())
        .collect::<Vec<_>>();
    let mut lookups = 0;
    let server = http::Server::new(move |request| match request.method() {
        "getCustomEmojiStickers" => {
            lookups += 1;
            if lookups == 1 {
                let requested: Vec<String> =
                    serde_json::from_str(&request.form()["custom_emoji_ids"]).unwrap();
                assert_eq!(requested.len(), 200);
                http::Reply::ok(json!(
                    requested
                        .iter()
                        .map(|id| json!({
                            "custom_emoji_id":id,"emoji":"✅"
                        }))
                        .collect::<Vec<_>>()
                ))
            } else {
                http::Reply::json(json!({
                    "ok":false,"error_code":401,"description":"Unauthorized fixture"
                }))
            }
        }
        "sendMessage" => http::Reply::ok(json!({"message_id":1,"chat":{"id":42}})),
        method => panic!("unexpected method {method}"),
    });
    let outcome = handle(&server, &message(&ids.join(" ")));
    assert!(
        outcome.is_err(),
        "incomplete lookup was presented as complete"
    );
    assert_eq!(
        server.methods(),
        ["getCustomEmojiStickers", "getCustomEmojiStickers"]
    );
}

#[test]
fn malformed_lookup_results_cannot_prove_absence() {
    for result in [
        Value::Null,
        json!({}),
        json!([{}]),
        json!([{"custom_emoji_id":42}]),
        json!([{"custom_emoji_id":""}]),
        json!([{"custom_emoji_id":ID,"emoji":42}]),
        json!([{"custom_emoji_id":"5893098741073715472","emoji":"✅"}]),
        json!([{"custom_emoji_id":ID},{"custom_emoji_id":ID}]),
    ] {
        let payload = result.clone();
        let server = http::Server::new(move |request| match request.method() {
            "getCustomEmojiStickers" => http::Reply::ok(payload.clone()),
            "sendMessage" => http::Reply::ok(json!({"message_id":1,"chat":{"id":42}})),
            method => panic!("unexpected method {method}"),
        });
        let outcome = handle(&server, &message(ID));
        assert!(outcome.is_err(), "malformed lookup was accepted: {result}");
        assert_eq!(server.methods(), ["getCustomEmojiStickers"]);
    }
}

#[test]
fn successful_empty_lookup_still_names_the_unresolved_id() {
    let server = http::Server::new(|request| match request.method() {
        "getCustomEmojiStickers" => http::Reply::ok(json!([])),
        "sendMessage" => http::Reply::ok(json!({"message_id":1,"chat":{"id":42}})),
        method => panic!("unexpected method {method}"),
    });
    handle(&server, &message(ID)).unwrap();
    assert_eq!(server.methods(), ["getCustomEmojiStickers", "sendMessage"]);
    let requests = server.observed.lock().unwrap();
    let text = &requests[1].form()["text"];
    assert!(text.contains("does not know 1"));
    assert!(text.contains(ID));
    assert!(!text.contains("<tg-emoji"));
}

#[test]
fn established_typed_identity_survives_an_optional_display_lookup_failure() {
    let mut lookups = 0;
    let server = http::Server::new(move |request| match request.method() {
        "getCustomEmojiStickers" => {
            lookups += 1;
            if lookups == 1 {
                http::Reply::ok(json!([{"custom_emoji_id":ID}]))
            } else {
                http::Reply::json(json!({
                    "ok":false,"error_code":401,"description":"Unauthorized fixture"
                }))
            }
        }
        "sendMessage" => http::Reply::ok(json!({"message_id":1,"chat":{"id":42}})),
        method => panic!("unexpected method {method}"),
    });
    handle(&server, &message(ID)).unwrap();
    assert_eq!(
        server.methods(),
        [
            "getCustomEmojiStickers",
            "getCustomEmojiStickers",
            "sendMessage"
        ]
    );
    let requests = server.observed.lock().unwrap();
    let form = requests[2].form();
    assert!(form["text"].contains(&format!("<tg-emoji emoji-id=\"{ID}\">⭐")));
    assert!(!form["text"].contains("does not know"));
}

#[test]
fn complete_multi_batch_lookup_preserves_every_requested_identity() {
    let ids = (0..201)
        .map(|index| (5_893_098_741_073_715_000_u64 + index).to_string())
        .collect::<Vec<_>>();
    let server = http::Server::new(|request| match request.method() {
        "getCustomEmojiStickers" => {
            let requested: Vec<String> =
                serde_json::from_str(&request.form()["custom_emoji_ids"]).unwrap();
            http::Reply::ok(json!(
                requested
                    .iter()
                    .map(|id| json!({"custom_emoji_id":id,"emoji":"✅"}))
                    .collect::<Vec<_>>()
            ))
        }
        "sendMessage" => http::Reply::ok(json!({"message_id":1,"chat":{"id":42}})),
        method => panic!("unexpected method {method}"),
    });
    handle(&server, &message(&ids.join(" "))).unwrap();
    let requests = server.observed.lock().unwrap();
    let batches = requests
        .iter()
        .filter(|request| request.method() == "getCustomEmojiStickers")
        .map(|request| {
            serde_json::from_str::<Vec<String>>(&request.form()["custom_emoji_ids"]).unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        batches.iter().map(Vec::len).collect::<Vec<_>>(),
        [200, 1, 200, 1]
    );
    assert_eq!(batches[..2].concat(), ids);
    assert_eq!(batches[2..].concat(), ids);
    let replies = requests
        .iter()
        .filter(|request| request.method() == "sendMessage")
        .map(|request| request.form()["text"].clone())
        .collect::<Vec<_>>();
    assert!(!replies.is_empty());
    assert!(
        replies
            .iter()
            .all(|text| text.encode_utf16().count() <= 4096)
    );
    let combined = replies.join("\n");
    assert!(!combined.contains("does not know"));
    for id in ids {
        assert_eq!(
            combined
                .matches(&format!("<tg-emoji emoji-id=\"{id}\">✅"))
                .count(),
            1,
            "every resolved identity must appear exactly once"
        );
    }
}

#[test]
fn extracted_entity_still_uses_fallback_when_optional_glyph_lookup_fails() {
    let server = http::Server::new(|request| match request.method() {
        "getCustomEmojiStickers" => http::Reply::json(json!({
            "ok":false,"error_code":401,"description":"Unauthorized fixture"
        })),
        "sendMessage" => http::Reply::ok(json!({"message_id":1,"chat":{"id":42}})),
        method => panic!("unexpected method {method}"),
    });
    let mut update = message("✅");
    update["message"]["entities"] = json!([{
        "type":"custom_emoji","offset":0,"length":1,"custom_emoji_id":ID
    }]);
    handle(&server, &update).unwrap();
    assert_eq!(server.methods(), ["getCustomEmojiStickers", "sendMessage"]);
    let requests = server.observed.lock().unwrap();
    let form = requests[1].form();
    assert!(form["text"].contains(&format!("<tg-emoji emoji-id=\"{ID}\">⭐")));
    assert!(!form["text"].contains("does not know"));
}
