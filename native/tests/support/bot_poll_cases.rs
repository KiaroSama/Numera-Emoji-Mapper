//! Original polling failure outcomes share the existing copied command, cursor and owner.
use _native::logging::RunLog;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

pub fn check(
    root: &Path,
    executable: &Path,
    runtime: &tokio::runtime::Runtime,
    environment: &BTreeMap<String, String>,
    log: &RunLog,
) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let cursor = root.join("state_emoji_bot.json");
    let saved = std::fs::read(&cursor).unwrap();
    let mut environment = environment.clone();
    environment.insert("PACK_OWNER_USER_ID".into(), super::OWNER.to_string());
    environment.insert("BOT_ALLOWED_USER_IDS".into(), String::new());
    environment.insert("BOT_ALLOWED_CHANNEL_IDS".into(), String::new());
    for outcome in ["rejected", "unknown", "dead-letter"] {
        std::fs::write(&cursor, b"{\"offset\":0}").unwrap();
        let mut update = super::message(super::OWNER, "x", "private");
        update["update_id"] = json!(7);
        update["message"]["entities"] =
            json!([{"type":"custom_emoji","custom_emoji_id":"222222222"}]);
        let rejected = json!({"ok":false,"description":"fixture rich reply refused"});
        let mut effects = vec![super::lookup(
            &["222222222"],
            json!([{"custom_emoji_id":"222222222","emoji":"⭐"}]),
        )];
        effects.push(super::Reply::new(
            "sendMessage",
            &[("chat_id", "42")],
            if outcome == "unknown" {
                json!([])
            } else {
                rejected.clone()
            },
        ));
        if outcome != "unknown" {
            effects.push(super::Reply::new(
                "sendMessage",
                &[("chat_id", "42")],
                if outcome == "dead-letter" {
                    rejected
                } else {
                    super::sent()
                },
            ));
        }
        let mut server = super::Server::new(super::script(0, vec![update], effects, 8));
        environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            super::command(runtime, root, executable, &environment, log)
        }));
        let requests = server.finish();
        let (completed, text) = result.unwrap();
        let requests = requests.unwrap();
        assert_eq!(completed.code, 4, "{outcome}: {}", completed.stderr);
        let polls = requests
            .iter()
            .filter(|request| request.method == "getUpdates")
            .collect::<Vec<_>>();
        assert_eq!(polls.len(), 2);
        assert_eq!(polls[0].form["offset"], "0");
        assert_eq!(polls[1].form["offset"], "8");
        assert_eq!(
            serde_json::from_str::<Value>(&polls[0].form["allowed_updates"]).unwrap(),
            json!(["message", "channel_post", "my_chat_member"])
        );
        let sends = requests
            .iter()
            .filter(|request| request.method == "sendMessage")
            .collect::<Vec<_>>();
        assert_eq!(sends.len(), if outcome == "unknown" { 1 } else { 2 });
        assert!(sends[0].form["text"].contains("<tg-emoji"));
        if outcome == "unknown" {
            assert!(text.contains("reply outcome unresolved; no fallback"));
            assert!(!text.contains("dead-lettering update"));
        } else {
            assert!(!sends[1].form["text"].contains("<tg-emoji"));
            assert!(text.contains("rich reply rejected"));
            assert_eq!(
                text.contains("dead-lettering update"),
                outcome == "dead-letter"
            );
        }
        let bytes = std::fs::read(&cursor).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            json!({"offset":8})
        );
        if outcome != "rejected" {
            let mut server = super::Server::new(vec![
                super::Reply::new("getMe", &[], super::ok(json!({"username":"YourEmojiBot"}))),
                super::Reply::new("setMyCommands", &[], super::ok(json!(true))),
                super::poll(8, super::conflict()),
            ]);
            environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                super::command(runtime, root, executable, &environment, log)
            }));
            let requests = server.finish();
            let (completed, _) = result.unwrap();
            assert_eq!(completed.code, 4, "{}", completed.stderr);
            let requests = requests.unwrap();
            assert_eq!(requests.len(), 3);
            assert_eq!(requests[2].method, "getUpdates");
            assert_eq!(requests[2].form["offset"], "8");
            assert_eq!(std::fs::read(&cursor).unwrap(), bytes);
        }
        assert!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
        assert!(!_native::process_owner::cleanup_pending(root));
        assert!(
            Instant::now() < deadline,
            "bot polling deadline elapsed; evidence retained"
        );
    }
    std::fs::write(&cursor, &saved).unwrap();
    assert_eq!(std::fs::read(cursor).unwrap(), saved);
}
