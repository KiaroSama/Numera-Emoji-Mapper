//! Final links exercise the copied command; refused sends never publish a success milestone.
use _native::{logging::RunLog, tooling_process};
use serde_json::{Value, json};
use std::time::Duration;

#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server};

#[test]
fn final_links_resend_explicitly_and_preserve_state_when_send_or_input_fails() {
    let fixture = tooling_fixture::Root::new("coin rebuild final link commands");
    let root = &fixture.0;
    std::fs::create_dir(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/panel.html"), b"fixture marker").unwrap();
    std::fs::write(root.join("pyproject.toml"), b"# fixture marker\n").unwrap();
    let coins = root.join("coins");
    std::fs::create_dir(&coins).unwrap();
    let state = coins.join("rebuild_dedup_state.json");
    let pinned: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-rebuild-links.json"
    ))
    .unwrap();
    let source = &pinned["cases"][1];
    let mut original = source["input"]["state"].clone();
    original["sets"]
        .as_array_mut()
        .unwrap()
        .sort_by_key(|set| set["index"].as_u64());
    _native::atomic::write_json(&state, &original).unwrap();
    let packs = source["expected"]["events"][1][1].as_array().unwrap();
    let mut lines = vec![
        source["expected"]["events"][1][2]
            .as_str()
            .unwrap()
            .to_owned(),
        String::new(),
    ];
    for pack in packs {
        lines.push(format!(
            "{}. https://t.me/addemoji/{}",
            pack["title"].as_str().unwrap(),
            pack["name"].as_str().unwrap()
        ));
    }
    let text = lines.join("\n");
    let executable = root.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let input = root.join("command-eof");
    std::fs::write(&input, b"").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(
        root,
        "rebuild-links-command-owner",
        vec![bot_api::TOKEN.into()],
    );
    let family = _native::locks::pack_family_path(root, "cryptoemoji");
    let mut sends = 0;
    for iteration in 0..6 {
        match iteration {
            3 => _native::atomic::write_json(&state, &json!({"sets":[],"sent":[]})).unwrap(),
            4 => std::fs::remove_file(&state).unwrap(),
            5 => std::fs::write(&state, b"{truncated").unwrap(),
            _ => {}
        }
        let before = std::fs::read(&state).ok();
        let mut replies = vec![Reply::new(
            "getMe",
            &[],
            json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
        )];
        let mut held = None;
        if iteration < 3 {
            let mut reply = Reply::new(
                "sendMessage",
                &[
                    ("text", &text),
                    ("chat_id", "111111111"),
                    ("disable_web_page_preview", "true"),
                ],
                if iteration == 2 {
                    json!({"ok":false,"description":"controlled link refusal"})
                } else {
                    json!({"ok":true,"result":{"message_id":iteration}})
                },
            );
            if iteration != 0 {
                reply.observation = Some((state.clone(), "/final_sent".into(), json!(true)));
            }
            let (reply, reached, release) = reply.hold();
            held = Some((reached, release));
            replies.push(reply);
        }
        let mut server = Server::new(replies);
        let mut environment =
            _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
        for (key, value) in [
            ("NUMERA_TEST_API_BASE", server.base.as_str()),
            ("COIN_PACK_BASE", "cryptoemoji"),
            ("COIN_PACK_TITLE", "Fixture"),
            ("PACK_OWNER_USER_ID", "111111111"),
            ("TELEGRAM_BOT_TOKEN", bot_api::TOKEN),
        ] {
            environment.insert(key.into(), value.into());
        }
        let argv = [
            executable.as_os_str().to_owned(),
            "coin-rebuild".into(),
            "links".into(),
        ];
        let invocation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.block_on(async {
                let observe = async {
                    if let Some((reached, release)) = held {
                        tokio::time::timeout(Duration::from_secs(5), reached)
                            .await
                            .unwrap()
                            .unwrap();
                        assert!(
                            _native::locks::Lease::acquire(&family, "links competing rebuild")
                                .is_err()
                        );
                        assert_eq!(std::fs::read(&state).ok(), before);
                        if iteration == 0 {
                            let saved: Value =
                                serde_json::from_slice(before.as_ref().unwrap()).unwrap();
                            assert!(saved.get("final_sent").is_none());
                        }
                        release.send(()).unwrap();
                    }
                };
                let (output, ()) = tokio::join!(
                    tooling_process::capture_with_input(
                        root,
                        &argv,
                        &environment,
                        &input,
                        Duration::from_secs(10),
                        Duration::from_secs(5),
                        &log
                    ),
                    observe
                );
                output.unwrap()
            })
        }));
        let requests = server.finish();
        let output = match invocation {
            Ok(value) => value,
            Err(error) => std::panic::resume_unwind(error),
        };
        let requests = requests.unwrap();
        assert_eq!(requests.len(), if iteration < 3 { 2 } else { 1 });
        sends += requests
            .iter()
            .filter(|request| request.method == "sendMessage")
            .count();
        assert!(
            requests
                .iter()
                .all(|request| matches!(request.method.as_str(), "getMe" | "sendMessage"))
        );
        assert_eq!(
            output.code,
            if iteration == 2 || iteration == 5 {
                2
            } else {
                0
            },
            "{} {}",
            output.stdout,
            output.stderr
        );
        if iteration < 2 {
            let mut expected = source["expected"]["state"].clone();
            expected["sets"]
                .as_array_mut()
                .unwrap()
                .sort_by_key(|set| set["index"].as_u64());
            assert_eq!(
                serde_json::from_slice::<Value>(&std::fs::read(&state).unwrap()).unwrap(),
                expected
            );
            assert_eq!(
                output.stdout,
                source["expected"]["output"].as_str().unwrap()
            );
        } else {
            assert_eq!(std::fs::read(&state).ok(), before);
            assert!(!output.stdout.contains("sent final links"));
            if iteration == 3 || iteration == 4 {
                assert_eq!(output.stdout, "no sets to send.\n");
            }
        }
        assert!(!output.stdout.contains(bot_api::TOKEN));
        assert!(!output.stderr.contains(bot_api::TOKEN));
        drop(_native::locks::Lease::acquire(&family, "links released").unwrap());
        assert!(family.is_file());
        assert!(!_native::process_owner::cleanup_pending(root));
        assert!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
    }
    assert_eq!(sends, 3);
    drop(log);
    fixture.clean().unwrap();
}
