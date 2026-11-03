//! Original rebuild add/resume/rollover assertions at the actual command and durable state seam.
use _native::{logging::RunLog, tooling_process};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server};

fn document(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn listing(landed: bool) -> Reply {
    let mut stickers =
        vec![json!({"file_id":"seed","file_unique_id":"seed-u","custom_emoji_id":"seed-c"})];
    if landed {
        stickers
            .push(json!({"file_id":"our-file","file_unique_id":"our-u","custom_emoji_id":"our-c"}));
    }
    Reply::new(
        "getStickerSet",
        &[("name", "s1")],
        json!({"ok":true,"result":{"stickers":stickers}}),
    )
}
fn mutation(creating: bool, art: &[u8], payload: Value) -> Reply {
    mutation_for(creating, "Beta", art, payload)
}
fn mutation_for(creating: bool, keyword: &str, art: &[u8], payload: Value) -> Reply {
    let sticker = json!({"sticker":"attach://file0","format":"static","emoji_list":["🪙"],"keywords":[keyword]});
    let body = if creating { json!([sticker]) } else { sticker }.to_string();
    let mut fields = vec![
        ("user_id", "111111111"),
        (
            "name",
            if creating {
                "cryptoemoji2_by_YourEmojiBot"
            } else {
                "s1"
            },
        ),
        (if creating { "stickers" } else { "sticker" }, body.as_str()),
    ];
    if creating {
        fields.extend([("title", "Fixture 2"), ("sticker_type", "custom_emoji")]);
    }
    let mut reply = Reply::new(
        if creating {
            "createNewStickerSet"
        } else {
            "addStickerToSet"
        },
        &fields,
        payload,
    );
    reply.files.insert("file0".into(), art.to_vec());
    reply
}
fn script(scenario: &str, iteration: usize, art: &[u8]) -> Vec<Reply> {
    let mut replies = vec![Reply::new(
        "getMe",
        &[],
        json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
    )];
    if scenario == "unknown" {
        replies.push(Reply::new(
            "getStickerSet",
            &[("name", "s1")],
            json!({"ok":true,"result":[]}),
        ));
        return replies;
    }
    let restarting = scenario == "restart" && iteration == 1;
    replies.push(listing(restarting));
    replies.push(listing(restarting));
    if restarting {
        replies.push(Reply::new(
            "getFile",
            &[("file_id", "our-file")],
            json!({"ok":true,"result":{"file_path":"fixture.png"}}),
        ));
        let mut download = Reply::new("getFile", &[], Value::Null);
        download.method = "GET fixture.png".into();
        download.download = Some(art.to_vec());
        replies.push(download);
    } else {
        replies.push(mutation(
            false,
            art,
            match scenario {
                "rollover" => json!({"ok":false,"description":"STICKERS_TOO_MUCH"}),
                "restart" | "ambiguous-next" => json!({"ok":true}),
                "rejected" => json!({"ok":false,"description":"controlled retryable refusal"}),
                "permanent" => json!({"ok":false,"description":"STICKER_PNG_NOPNG"}),
                _ => json!({"ok":true,"result":true}),
            },
        ));
        if scenario == "rollover" {
            replies.push(mutation(true, art, json!({"ok":true,"result":true})));
        } else if scenario == "permanent" {
            replies.push(listing(false));
            replies.push(mutation_for(
                false,
                "ccc",
                art,
                json!({"ok":false,"description":"STICKER_PNG_NOPNG"}),
            ));
        }
    }
    if matches!(scenario, "add" | "rollover" | "permanent") || restarting {
        replies.push(Reply::new(
            "sendMessage",
            &[
                ("chat_id", "111111111"),
                ("disable_web_page_preview", "true"),
            ],
            json!({"ok":true,"result":{"message_id":1}}),
        ));
    }
    replies
}

#[test]
fn rebuild_add_rollover_and_unknown_restart_preserve_original_walk_and_intent() {
    let fixture = tooling_fixture::Root::new("coin rebuild add commands");
    let root = &fixture.0;
    std::fs::create_dir(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/panel.html"), b"fixture marker").unwrap();
    std::fs::write(root.join("pyproject.toml"), b"# fixture marker\n").unwrap();
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
        "rebuild-add-command-owner",
        vec![bot_api::TOKEN.into()],
    );
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let art =
        std::fs::read(project.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
    let walk: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-rebuild-walk.json"
    ))
    .unwrap();
    let rollover = walk["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "server full rolls new set")
        .unwrap();
    assert_eq!(
        walk["source_sha256"],
        "17771c6245dedf36d6c66deba99bb6405a624505962f74251f7f5a7f7e4eb54f"
    );
    for scenario in [
        "add",
        "rollover",
        "restart",
        "rejected",
        "unknown",
        "permanent",
        "ambiguous-next",
    ] {
        let directory = root.join(scenario);
        let images = directory.join("logos/emoji");
        std::fs::create_dir_all(&images).unwrap();
        for name in ["aaa", "bbb", "ccc"] {
            std::fs::write(images.join(format!("{name}.png")), &art).unwrap();
        }
        let state = directory.join("rebuild_dedup_state.json");
        let plan = directory.join("rebuild_dedup_plan.json");
        _native::atomic::write_json(&state, &rollover["input"]["state"]).unwrap();
        let mut entries = rollover["input"]["plan"].clone();
        if scenario == "ambiguous-next" || scenario == "permanent" {
            entries
                .as_array_mut()
                .unwrap()
                .push(json!({"rep":"ccc","tickers":["ccc"],"kw":"ccc"}));
        }
        _native::atomic::write_json(&plan, &entries).unwrap();
        let original = std::fs::read(&state).unwrap();
        let plan_before = std::fs::read(&plan).unwrap();
        let mut all = Vec::new();
        for iteration in 0..if scenario == "restart" { 2 } else { 1 } {
            let mut signals = Vec::new();
            let mut mutation_index = 0;
            let replies = script(scenario, iteration, &art)
                .into_iter()
                .map(|mut reply| {
                    if reply.method == "getMe" {
                        return reply;
                    }
                    let method = reply.method.clone();
                    let key = if scenario == "permanent" && mutation_index > 0 {
                        "ccc"
                    } else {
                        "bbb"
                    };
                    if method == "addStickerToSet" || method == "createNewStickerSet" {
                        reply.observation =
                            Some((state.clone(), "/in_flight/key".into(), json!(key)));
                        mutation_index += 1;
                    }
                    let (reply, ready, release) = reply.hold();
                    signals.push((method, key, ready, release));
                    reply
                })
                .collect();
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
                "build".into(),
                "--coin-dir".into(),
                directory.as_os_str().to_owned(),
            ];
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.block_on(async {
                    let observe = async {
                        for (method, key, ready, release) in signals {
                            tokio::time::timeout(Duration::from_secs(5), ready)
                                .await
                                .unwrap()
                                .unwrap();
                            let family = _native::locks::pack_family_path(root, "cryptoemoji");
                            assert!(
                                _native::locks::Lease::acquire(&family, "add competing run")
                                    .is_err()
                            );
                            if method == "addStickerToSet" || method == "createNewStickerSet" {
                                let saved = document(&state);
                                assert_eq!(saved["cursor"], if key == "ccc" { 3 } else { 2 });
                                assert_eq!(saved["order"], json!(["aaa"]));
                                let marker = &saved["in_flight"];
                                assert_eq!(marker["key"], key);
                                assert_eq!(marker["phase"], "upload");
                                let creating = method == "createNewStickerSet";
                                assert_eq!(
                                    marker["operation"],
                                    if creating { "create" } else { "add" }
                                );
                                assert_eq!(marker["expected_before"], if creating { 0 } else { 1 });
                                assert_eq!(
                                    marker["set_name"],
                                    if creating {
                                        "cryptoemoji2_by_YourEmojiBot"
                                    } else {
                                        "s1"
                                    }
                                );
                                assert_eq!(saved["sets"].as_array().unwrap().len(), 1);
                            }
                            release.send(()).unwrap();
                        }
                    };
                    let (completed, ()) = tokio::join!(
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
                    completed.unwrap()
                })
            }));
            let requests = server.finish();
            let output = match outcome {
                Ok(value) => value,
                Err(error) => std::panic::resume_unwind(error),
            };
            all.extend(requests.unwrap());
            let partial = matches!(scenario, "unknown" | "rejected" | "ambiguous-next")
                || (scenario == "restart" && iteration == 0);
            assert_eq!(
                output.code,
                if partial { 3 } else { 0 },
                "{scenario}: {} {}",
                output.stdout,
                output.stderr
            );
            assert_eq!(std::fs::read(&plan).unwrap(), plan_before);
            for name in ["aaa", "bbb", "ccc"] {
                assert_eq!(
                    std::fs::read(images.join(format!("{name}.png"))).unwrap(),
                    art
                );
            }
            let saved = document(&state);
            if scenario == "unknown" {
                assert_eq!(std::fs::read(&state).unwrap(), original);
            } else if scenario == "ambiguous-next" || (scenario == "restart" && iteration == 0) {
                assert_eq!(saved["cursor"], 2);
                assert_eq!(saved["order"], json!(["aaa"]));
                assert_eq!(
                    saved["in_flight"],
                    json!({"key":"bbb","operation":"add","set_name":"s1","set_index":1,"expected_before":1,"phase":"upload"})
                );
                assert!(!output.stdout.contains("buildonly checkpoint"));
            } else if scenario == "rejected" || scenario == "permanent" {
                assert_eq!(saved["cursor"], if scenario == "permanent" { 3 } else { 1 });
                assert_eq!(saved["order"], json!(["aaa"]));
                assert!(saved["in_flight"].is_null());
                if scenario == "permanent" {
                    assert!(
                        output
                            .stdout
                            .contains("buildonly checkpoint: plan position 3/3")
                    );
                }
            } else {
                assert_eq!(saved["order"], rollover["expected"]["state"]["order"]);
                assert_eq!(saved["cursor"], rollover["expected"]["state"]["cursor"]);
                assert!(saved["in_flight"].is_null());
                if scenario == "rollover" {
                    assert_eq!(saved["sets"], rollover["expected"]["state"]["sets"]);
                }
                assert!(
                    output
                        .stdout
                        .contains("buildonly checkpoint: plan position 2/2")
                );
            }
            assert!(!output.stdout.contains(bot_api::TOKEN));
            assert!(!output.stderr.contains(bot_api::TOKEN));
            drop(
                _native::locks::Lease::acquire(
                    &_native::locks::pack_family_path(root, "cryptoemoji"),
                    "add released",
                )
                .unwrap(),
            );
            assert!(
                std::fs::read_dir(root.join("logs/tmp/tooling"))
                    .unwrap()
                    .next()
                    .is_none()
            );
            assert!(!_native::process_owner::cleanup_pending(root));
        }
        assert_eq!(
            all.iter().filter(|r| r.method == "addStickerToSet").count(),
            if scenario == "permanent" {
                2
            } else {
                usize::from(scenario != "unknown")
            }
        );
        assert_eq!(
            all.iter()
                .filter(|r| r.method == "createNewStickerSet")
                .count(),
            usize::from(scenario == "rollover")
        );
        assert!(!all.iter().any(|r| r.method == "deleteStickerSet"));
    }
    drop(log);
    fixture.clean().unwrap();
}
