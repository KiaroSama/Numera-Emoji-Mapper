//! Original bot access, typed replies and persisted cursor through real native seams, loopback only.
use _native::{
    emoji_bot, logging::RunLog, telegram::Telegram, tooling_environment, tooling_process,
    tooling_test_runner,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/bot_poll_cases.rs"]
mod bot_poll_cases;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server, TOKEN};
use tooling_fixture::Root;

const OWNER: i64 = 42;
const CHANNEL: i64 = -1001234567890;
const A: &str = "5893098741073715471";
const B: &str = "5893098741073715472";
fn ok(result: Value) -> Value {
    json!({"ok":true,"result":result})
}
fn sent() -> Value {
    ok(json!({"message_id":1}))
}
fn conflict() -> Value {
    json!({"ok":false,"description":"Conflict: deliberate end of synthetic polling"})
}
fn message(sender: i64, text: &str, kind: &str) -> Value {
    json!({"message":{"message_id":1,"text":text,"from":{"id":sender},"chat":{"id":sender,"type":kind}}})
}
fn post(channel: i64) -> Value {
    json!({"channel_post":{"message_id":1,"chat":{"id":channel,"type":"channel","title":"Somebody's channel"},
        "entities":[{"type":"custom_emoji","custom_emoji_id":"111111111"}]}})
}
fn lookup(ids: &[&str], result: Value) -> Reply {
    Reply::new(
        "getCustomEmojiStickers",
        &[("custom_emoji_ids", &json!(ids).to_string())],
        ok(result),
    )
}

#[test]
fn original_bot_access_and_typed_replies_keep_denial_routing_and_resolved_identity() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("bot access café");
    let log = RunLog::open(&root.0, "bot-access", vec![TOKEN.into()]);
    let mut server = Server::new(vec![
        Reply::new("sendMessage", &[("chat_id", "42")], sent()),
        Reply::new("sendMessage", &[("chat_id", "999")], sent()),
        Reply::new("sendMessage", &[("chat_id", "999")], sent()),
        lookup(&["111111111"], json!([])),
        Reply::new("sendMessage", &[("chat_id", "42")], sent()),
        lookup(&[A, B], json!([{"custom_emoji_id":A,"emoji":"✅"}])),
        lookup(&[A], json!([{"custom_emoji_id":A,"emoji":"✅"}])),
        Reply::new("sendMessage", &[("chat_id", "900")], sent()),
        lookup(&[A], json!([])),
        Reply::new("sendMessage", &[("chat_id", "900")], sent()),
    ]);
    let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
    let allowed = BTreeSet::from([OWNER, 900]);
    let empty = BTreeSet::new();
    let handle = |update: &Value, channels: &BTreeSet<i64>| {
        emoji_bot::handle(&tg, OWNER, update, &allowed, channels, &log).unwrap();
    };
    handle(&message(OWNER, "/start", "private"), &empty);
    handle(&message(999, "/start", "private"), &empty);
    let mut unauthorized = message(999, "hi", "private");
    unauthorized["message"]["entities"] =
        json!([{"type":"custom_emoji","custom_emoji_id":"111111111"}]);
    handle(&unauthorized, &empty);
    unauthorized["message"]["chat"]["type"] = json!("supergroup");
    handle(&unauthorized, &empty);
    handle(&post(CHANNEL), &BTreeSet::from([-100777]));
    handle(&post(CHANNEL), &empty);
    handle(&post(CHANNEL), &BTreeSet::from([CHANNEL]));
    handle(&message(900, &format!("{A}\n{B}"), "private"), &empty);
    handle(&message(900, A, "private"), &empty);
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), 10);
    assert_eq!(
        requests
            .iter()
            .map(|request| request.method.as_str())
            .collect::<Vec<_>>(),
        [
            "sendMessage",
            "sendMessage",
            "sendMessage",
            "getCustomEmojiStickers",
            "sendMessage",
            "getCustomEmojiStickers",
            "getCustomEmojiStickers",
            "sendMessage",
            "getCustomEmojiStickers",
            "sendMessage"
        ]
    );
    assert!(!requests[0].form["text"].contains("not on its access list"));
    for request in &requests[1..3] {
        assert!(request.form["text"].contains("not on its access list"));
        assert!(!request.form["text"].contains("111111111"));
    }
    assert_eq!(requests[4].form["chat_id"], "42");
    assert!(requests[4].form["text"].contains("Somebody&#x27;s channel"));
    let partial = &requests[7].form["text"];
    assert!(partial.contains("does not know") && partial.contains(A) && partial.contains(B));
    assert!(partial.contains(&format!("<tg-emoji emoji-id=\"{A}\"")));
    assert!(!partial.contains(&format!("<tg-emoji emoji-id=\"{B}\"")));
    assert!(requests[9].form["text"].contains("does not know"));
    assert!(!requests[9].form["text"].contains("<tg-emoji"));
    log.finish(0);
    drop(tg);
    drop(log);
    root.clean().unwrap();
}

fn poll(offset: i64, result: Value) -> Reply {
    Reply::new(
        "getUpdates",
        &[("offset", &offset.to_string()), ("timeout", "50")],
        result,
    )
}
fn script(offset: i64, updates: Vec<Value>, effects: Vec<Reply>, next: i64) -> Vec<Reply> {
    let mut replies = vec![
        Reply::new("getMe", &[], ok(json!({"username":"YourEmojiBot"}))),
        Reply::new("setMyCommands", &[], ok(json!(true))),
        poll(offset, ok(json!(updates))),
    ];
    replies.extend(effects);
    replies.push(poll(next, conflict()));
    replies
}
fn copied(root: &Path) -> PathBuf {
    std::fs::create_dir(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/panel.html"), b"synthetic bot root").unwrap();
    std::fs::write(root.join("pyproject.toml"), b"# synthetic root\n").unwrap();
    let executable = root.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    executable
}
fn bot_logs(root: &Path) -> BTreeSet<PathBuf> {
    std::fs::read_dir(root.join("logs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("emoji-bot_")
                && path.extension().is_some_and(|suffix| suffix == "log")
        })
        .collect()
}
fn command(
    runtime: &tokio::runtime::Runtime,
    root: &Path,
    executable: &Path,
    environment: &BTreeMap<String, String>,
    log: &RunLog,
) -> (tooling_process::Completed, String) {
    let before = bot_logs(root);
    let result = runtime
        .block_on(tooling_process::capture(
            root,
            &[
                executable.as_os_str().to_owned(),
                OsString::from("emoji-bot"),
            ],
            environment,
            Duration::from_secs(6),
            Duration::from_secs(3),
            log,
        ))
        .unwrap();
    let after = bot_logs(root);
    assert!(before.is_subset(&after), "a prior bot log was removed");
    let added = after.difference(&before).collect::<Vec<_>>();
    assert_eq!(added.len(), 1, "each command must create its own bot log");
    let text = std::fs::read_to_string(added[0]).unwrap();
    assert!(text.contains("[runtime] native backend started"));
    assert!(text.contains(&format!("[runtime] exit={} ", result.code)));
    for output in [&result.stdout, &result.stderr, &text] {
        assert!(!output.contains(TOKEN));
    }
    (result, text)
}

#[test]
fn actual_bot_command_keeps_numeric_access_wiring_and_resumes_the_atomic_cursor() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("bot command café");
    let executable = copied(&root.0);
    let inherited = tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    let mut environment = tooling_environment::environment(&root.0, &inherited, false).unwrap();
    environment.extend([
        ("GENERAL_BOT_TOKEN".into(), TOKEN.into()),
        ("PACK_OWNER_USER_ID".into(), OWNER.to_string()),
        ("BOT_ALLOWED_USER_IDS".into(), String::new()),
        ("BOT_ALLOWED_CHANNEL_IDS".into(), String::new()),
        ("EMOJI_LOG_RETENTION_DAYS".into(), "0".into()),
    ]);
    let log = RunLog::open(&root.0, "bot-command-owner", vec![TOKEN.into()]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let cursor = root.0.join("state_emoji_bot.json");
    assert!(!cursor.exists());
    let mut saved = Vec::new();
    for (index, offset, next, sender) in [
        (0, 0, 8, OWNER),
        (1, 8, 4242, 2),
        (2, 4242, 4242, 0),
        (3, 0, 8, OWNER),
    ] {
        if index == 1 {
            environment.insert("BOT_ALLOWED_USER_IDS".into(), "1, 2 ;3, oops,".into());
        }
        if index == 3 {
            std::fs::write(&cursor, b"{not json").unwrap();
            environment.insert("BOT_ALLOWED_USER_IDS".into(), "\u{1c} \u{1f}".into());
        }
        let replies = if index == 2 {
            vec![
                Reply::new("getMe", &[], ok(json!({"username":"YourEmojiBot"}))),
                Reply::new("setMyCommands", &[], ok(json!(true))),
                poll(offset, conflict()),
            ]
        } else {
            let mut update = message(sender, "/start", "private");
            update["update_id"] = json!(next - 1);
            let mut updates = vec![update];
            if index == 0 {
                let mut ignored = post(CHANNEL);
                ignored["update_id"] = json!(next - 2);
                updates.insert(0, ignored);
            }
            script(
                offset,
                updates,
                vec![Reply::new(
                    "sendMessage",
                    &[("chat_id", &sender.to_string())],
                    sent(),
                )],
                next,
            )
        };
        let mut server = Server::new(replies);
        environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        let (completed, text) = command(&runtime, &root.0, &executable, &environment, &log);
        let requests = server.finish().unwrap();
        assert_eq!(completed.code, 4, "{}", completed.stderr);
        assert!(text.contains("[ERROR] [bot] getUpdates conflict"));
        assert_eq!(requests.len(), if index == 2 { 3 } else { 5 });
        let polls = requests
            .iter()
            .filter(|request| request.method == "getUpdates")
            .collect::<Vec<_>>();
        assert_eq!(polls[0].form["offset"], offset.to_string());
        assert_eq!(
            serde_json::from_str::<Value>(&polls[0].form["allowed_updates"]).unwrap(),
            json!(["message", "channel_post", "my_chat_member"])
        );
        for request in requests
            .iter()
            .filter(|request| request.method == "sendMessage")
        {
            assert!(
                !request.form["text"].contains("not on its access list"),
                "numeric allowlist lost at polling boundary"
            );
        }
        let bytes = std::fs::read(&cursor).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            json!({"offset":next})
        );
        if index == 2 {
            assert_eq!(bytes, saved, "restart must not rewrite or lose cursor");
        }
        saved = bytes;
    }
    // Observe every member and a rejected user through polling, not an internal parser.
    for (users, channels) in [
        ("1, 2 ;3, oops,", format!("{CHANNEL}; abc,,-5")),
        (
            "\u{1c}+۱, ۲;۳_۰, oops,\u{1f}",
            "-۱۰۰۱۲۳۴۵۶۷۸۹۰;-۵;+6;7_0;abc".into(),
        ),
    ] {
        environment.insert("BOT_ALLOWED_USER_IDS".into(), users.into());
        environment.insert("BOT_ALLOWED_CHANNEL_IDS".into(), channels);
        let third = if users.contains("۳_۰") { 30 } else { 3 };
        let mut updates = [1, 2, third, 999]
            .into_iter()
            .map(|sender| message(sender, "/start", "private"))
            .collect::<Vec<_>>();
        updates.extend([post(CHANNEL), post(-5), post(-100777), post(6), post(70)]);
        for (index, update) in updates.iter_mut().enumerate() {
            update["update_id"] = json!(4242 + index);
        }
        let mut effects = [1, 2, third, 999]
            .into_iter()
            .map(|sender| Reply::new("sendMessage", &[("chat_id", &sender.to_string())], sent()))
            .collect::<Vec<_>>();
        for _ in 0..2 {
            effects.push(lookup(&["111111111"], json!([])));
            effects.push(Reply::new("sendMessage", &[("chat_id", "42")], sent()));
        }
        std::fs::write(&cursor, b"{\"offset\":4242}").unwrap();
        let mut server = Server::new(script(4242, updates, effects, 4251));
        environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        let (completed, text) = command(&runtime, &root.0, &executable, &environment, &log);
        let requests = server.finish().unwrap();
        assert_eq!(completed.code, 4, "{}", completed.stderr);
        assert!(text.contains("[ERROR] [bot] getUpdates conflict"));
        assert_eq!(requests.len(), 12);
        let sends = requests
            .iter()
            .filter(|request| request.method == "sendMessage")
            .collect::<Vec<_>>();
        assert_eq!(sends.len(), 6);
        for request in &sends[..3] {
            assert!(!request.form["text"].contains("not on its access list"));
        }
        assert!(sends[3].form["text"].contains("not on its access list"));
        assert!(
            sends[4..]
                .iter()
                .all(|request| request.form["chat_id"] == "42"
                    && request.form["text"].contains("From channel"))
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&cursor).unwrap()).unwrap(),
            json!({"offset":4251})
        );
    }
    saved = std::fs::read(&cursor).unwrap();
    for (owner, allowed, diagnostic) in [
        (
            "0",
            "",
            "PACK_OWNER_USER_ID must be a valid positive numeric ID",
        ),
        ("42", "oops", "no authorized users; refusing an open bot"),
    ] {
        environment.insert("PACK_OWNER_USER_ID".into(), owner.into());
        environment.insert("BOT_ALLOWED_USER_IDS".into(), allowed.into());
        let mut server = Server::new(vec![]);
        environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        let (completed, text) = command(&runtime, &root.0, &executable, &environment, &log);
        assert!(
            server.finish().unwrap().is_empty(),
            "invalid access configuration reached transport"
        );
        assert_eq!(completed.code, 2, "{}", completed.stderr);
        assert!(text.contains(&format!("[ERROR] [emoji-bot] {diagnostic}")));
        assert_eq!(std::fs::read(&cursor).unwrap(), saved);
    }
    assert!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(!std::fs::read_dir(&root.0).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
    assert_eq!(bot_logs(&root.0).len(), 8);
    bot_poll_cases::check(&root.0, &executable, &runtime, &environment, &log);
    assert_eq!(bot_logs(&root.0).len(), 13);
    assert_eq!(std::fs::read(&cursor).unwrap(), saved);
    log.finish(0);
    drop(log);
    runtime.shutdown_timeout(Duration::from_secs(1));
    root.clean().unwrap();
}
