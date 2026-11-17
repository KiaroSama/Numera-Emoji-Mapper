use crate::support::{
    http::{Reply, Server},
    *,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
fn polling(mode: &'static str) {
    let f = Fixture::new();
    let replies = Arc::new(Mutex::new(Vec::new()));
    let sent = replies.clone();
    let mut polls = 0;
    let bot = Server::new(move |r| {
        let form = r.form();
        match r.method() {
            "getMe" => Reply::ok(json!({"username":"YourEmojiBot"})),
            "setMyCommands" => Reply::ok(json!(true)),
            "getUpdates" => {
                polls += 1;
                if form["offset"] == "0" && (mode == "fallback" || polls == 1) {
                    Reply::ok(
                        json!([{"update_id":7,"message":{"message_id":1,"chat":{"id":111111111,"type":"private"},"from":{"id":111111111},"entities":[{"type":"custom_emoji","custom_emoji_id":"222222222"}]}}]),
                    )
                } else {
                    Reply::json(json!({"ok":false,"description":"Conflict: another poller"}))
                }
            }
            "getCustomEmojiStickers" => {
                Reply::ok(json!([{"custom_emoji_id":"222222222","emoji":"⭐"}]))
            }
            "sendMessage" => {
                let mut replies = sent.lock().unwrap();
                replies.push(form["text"].clone());
                if mode == "dead-letter" {
                    Reply::json(json!({"ok":false,"description":"fixture reply refused"}))
                } else if replies.len() == 1 {
                    if mode == "ambiguous" {
                        Reply::Disconnect
                    } else {
                        Reply::json(json!({"ok":false,"description":"invalid custom emoji"}))
                    }
                } else {
                    Reply::ok(json!({"message_id":2}))
                }
            }
            _ => panic!("unexpected {}", r.method()),
        }
    });
    let mut env = bot.environment();
    env.extend([
        ("BOT_ALLOWED_USER_IDS", "111111111".into()),
        ("BOT_ALLOWED_CHANNEL_IDS", "".into()),
    ]);
    let r = f.run(&["emoji-bot"], &env);
    r.code(4);
    assert_eq!(
        read_json(&f.root.join("state_emoji_bot.json")),
        json!({"offset":8})
    );
    let texts = replies.lock().unwrap().clone();
    assert!(texts[0].contains("<tg-emoji"));
    assert_eq!(texts.len(), if mode == "ambiguous" { 1 } else { 2 });
    if mode == "fallback" {
        assert!(!texts[1].contains("<tg-emoji"));
    } else if mode == "ambiguous" {
        assert!(r.err.contains("reply outcome unresolved; no fallback"));
    } else {
        assert!(r.err.contains("dead-lettering update"));
    }
    let calls = bot.observed.lock().unwrap();
    let poll = calls
        .iter()
        .filter(|r| r.method() == "getUpdates")
        .map(|r| r.form())
        .collect::<Vec<_>>();
    assert_eq!(
        poll.iter()
            .map(|r| r["offset"].as_str())
            .collect::<Vec<_>>(),
        ["0", "8"]
    );
    assert_eq!(
        serde_json::from_str::<Value>(&poll[0]["allowed_updates"]).unwrap(),
        json!(["message", "channel_post", "my_chat_member"])
    );
    drop(calls);
    if mode != "fallback" {
        bot.observed.lock().unwrap().clear();
        let again = f.run(&["emoji-bot"], &env);
        again.code(4);
        assert_eq!(replies.lock().unwrap().len(), texts.len());
        let calls = bot.observed.lock().unwrap();
        assert_eq!(
            calls
                .iter()
                .filter(|r| r.method() == "getUpdates")
                .map(|r| r.form()["offset"].clone())
                .collect::<Vec<_>>(),
            ["8"]
        );
        assert!(!calls.iter().any(|r| r.method() == "getCustomEmojiStickers"));
    }
}
// tests.test_native_bot_poll.NativeBotPolling.test_rejected_rich_fallback_cursor_and_conflict_exit
#[test]
fn rejected_rich_reply_falls_back_then_persists_cursor() {
    polling("fallback");
}
// tests.test_native_bot_poll.NativeBotPolling.test_ambiguous_rich_send_never_sends_a_plain_duplicate
#[test]
fn ambiguous_rich_reply_never_sends_plain_duplicate() {
    polling("ambiguous");
}
// tests.test_native_bot_poll.NativeBotPolling.test_handler_dead_letter_persists_cursor_and_restart_does_not_resend
#[test]
fn dead_letter_persists_cursor_and_restart_does_not_resend() {
    polling("dead-letter");
}
