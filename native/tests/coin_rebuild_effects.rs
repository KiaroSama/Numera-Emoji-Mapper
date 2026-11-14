//! Actual command confirms old deletions, persists intent and resumes without duplicate mutations.
use _native::{logging::RunLog, tooling_process};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/coin_rebuild_admission.rs"]
mod coin_rebuild_admission;
#[path = "support/coin_rebuild_resume.rs"]
mod coin_rebuild_resume;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Request, Server};

const SET: &str = "cryptoemoji1_by_YourEmojiBot";

fn document(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn me() -> Reply {
    Reply::new(
        "getMe",
        &[],
        json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
    )
}
fn listing(name: &str, result: Value) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", name)],
        json!({"ok":true,"result":result}),
    )
}
fn missing(name: &str) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", name)],
        json!({"ok":false,"description":"Bad Request: STICKERSET_INVALID"}),
    )
}
fn delete(name: &str, accepted: bool) -> Reply {
    Reply::new(
        "deleteStickerSet",
        &[("name", name)],
        if accepted {
            json!({"ok":true,"result":true})
        } else {
            json!({"ok":false,"description":"controlled delete refusal"})
        },
    )
}
fn live() -> Value {
    json!({"stickers":[{"file_id":"f1","file_unique_id":"u1","custom_emoji_id":"111111111"}]})
}
fn create(art: &[u8], payload: Value) -> Reply {
    let stickers = json!([{"sticker":"attach://file0","format":"static","emoji_list":["🪙"],"keywords":["Alpha"]}]).to_string();
    let mut reply = Reply::new(
        "createNewStickerSet",
        &[
            ("user_id", "111111111"),
            ("name", SET),
            ("title", "Fixture 1"),
            ("sticker_type", "custom_emoji"),
            ("stickers", &stickers),
        ],
        payload,
    );
    reply.files.insert("file0".into(), art.to_vec());
    reply
}
fn message() -> Reply {
    Reply::new(
        "sendMessage",
        &[
            ("chat_id", "111111111"),
            ("disable_web_page_preview", "true"),
        ],
        json!({"ok":true,"result":{"message_id":1}}),
    )
}
fn download(art: &[u8]) -> Vec<Reply> {
    let mut bytes = Reply::new("getFile", &[], Value::Null);
    bytes.method = "GET fixture.png".into();
    bytes.download = Some(art.to_vec());
    vec![
        Reply::new(
            "getFile",
            &[("file_id", "f1")],
            json!({"ok":true,"result":{"file_path":"fixture.png"}}),
        ),
        bytes,
    ]
}
fn replies(iteration: usize, art: &[u8]) -> Vec<Reply> {
    let mut replies = vec![me()];
    match iteration {
        0 => replies.extend([
            delete("old1", true),
            missing("old1"),
            delete("old2", false),
            listing("old2", json!({"stickers":[]})),
        ]),
        1 => replies.extend([
            delete("old2", true),
            missing("old2"),
            create(art, json!({"ok":true,"result":true})),
            message(),
        ]),
        2 => replies.push(listing(SET, live())),
        3 => {
            replies.extend([listing(SET, live()), listing(SET, live())]);
            replies.extend(download(art));
            replies.push(message());
        }
        4 => replies.extend([delete("old1", true), listing("old1", json!([]))]),
        5 => {
            // Non-object JSON reaches the same uncertain transport/proof arm as a lost response.
            replies.extend([create(art, Value::Null), listing(SET, live())]);
            replies.extend(download(art));
        }
        6 => replies.push(create(art, json!({"ok":true}))),
        7 | 8 => {
            replies.push(if iteration == 7 {
                create(art, json!({"ok":true,"result":true}))
            } else {
                listing(SET, live())
            });
            let mut failed = message();
            failed.payload = Value::Null;
            replies.push(failed);
            let mut second = message();
            if iteration == 7 {
                second.payload = Value::Null;
            }
            replies.push(second);
        }
        9 => replies.push(listing(SET, live())),
        _ => panic!("unexpected rebuild iteration"),
    }
    replies
}
struct Harness<'a> {
    root: &'a Path,
    application: &'a Path,
    executable: &'a Path,
    input: &'a Path,
    runtime: &'a tokio::runtime::Runtime,
    log: &'a RunLog,
    state: &'a Path,
}
impl Harness<'_> {
    async fn command(
        &self,
        server: &Server,
        arguments: &[std::ffi::OsString],
        overrides: &[(&str, &str)],
    ) -> Result<tooling_process::Completed, String> {
        let mut environment =
            _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
        for (key, value) in [
            ("NUMERA_TEST_API_BASE", server.base.as_str()),
            ("COIN_PACK_BASE", "cryptoemoji"),
            ("COIN_PACK_TITLE", "Fixture"),
            ("PACK_OWNER_USER_ID", "111111111"),
            ("TELEGRAM_BOT_TOKEN", bot_api::TOKEN),
        ]
        .into_iter()
        .chain(overrides.iter().copied())
        {
            environment.insert(key.into(), value.into());
        }
        let mut argv = vec![
            self.executable.as_os_str().to_owned(),
            "coin-rebuild".into(),
        ];
        argv.extend_from_slice(arguments);
        tooling_process::capture_with_input(
            self.root,
            &argv,
            &environment,
            self.input,
            Duration::from_secs(10),
            Duration::from_secs(5),
            self.log,
        )
        .await
    }
    fn run(
        &self,
        iteration: usize,
        replies: Vec<Reply>,
    ) -> (tooling_process::Completed, Vec<Request>) {
        let mut signals = Vec::new();
        let replies = replies
            .into_iter()
            .map(|mut reply| {
                if reply.method == "getMe" {
                    return reply;
                }
                let method = reply.method.clone();
                let cursor = match iteration {
                    0 | 4 => 0,
                    1 if method == "deleteStickerSet" || method == "getStickerSet" => 0,
                    _ => 1,
                };
                reply.observation = Some((self.state.to_owned(), "/cursor".into(), json!(cursor)));
                let (reply, reached, release) = reply.hold();
                signals.push((method, reached, release));
                reply
            })
            .collect();
        let mut server = Server::new(replies);
        let arguments = [if iteration == 3 { "all" } else { "build" }.into()];
        let invocation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.runtime.block_on(async {
                let observe = async {
                    for (method, reached, release) in signals {
                        tokio::time::timeout(Duration::from_secs(5), reached)
                            .await
                            .unwrap()
                            .unwrap();
                        let family =
                            _native::locks::pack_family_path(self.application, "cryptoemoji");
                        assert!(family.is_file());
                        assert!(
                            _native::locks::Lease::acquire(&family, "rebuild competing run")
                                .is_err()
                        );
                        let saved = document(self.state);
                        if iteration >= 7 && method == "sendMessage" {
                            assert_eq!(saved["sent"], json!([]));
                            assert!(saved["in_flight"].is_null());
                            assert_eq!(saved["order"], json!(["aaa"]));
                        }
                        if method == "deleteStickerSet" {
                            assert_eq!(saved["cursor"], 0);
                            assert!(saved["in_flight"].is_null());
                            assert_eq!(saved["deleted_old"], false);
                            if iteration == 1 {
                                assert_eq!(saved["deleted_old_packs"], json!(["old1"]));
                            }
                        }
                        if method == "createNewStickerSet" {
                            assert_eq!(saved["in_flight"]["key"], "aaa");
                            assert_eq!(saved["in_flight"]["operation"], "create");
                            assert_eq!(saved["in_flight"]["set_name"], SET);
                            assert_eq!(saved["in_flight"]["expected_before"], 0);
                            assert_eq!(saved["cursor"], 1);
                            assert_eq!(saved["order"], json!([]));
                            assert_eq!(saved["sets"], json!([]));
                            assert_eq!(saved["deleted_old"], true);
                            if iteration == 1 {
                                assert_eq!(saved["deleted_old_packs"], json!(["old1", "old2"]));
                            }
                        }
                        release.send(()).unwrap();
                    }
                };
                let (completed, ()) =
                    tokio::join!(self.command(&server, &arguments, &[]), observe,);
                completed.unwrap()
            })
        }));
        let requests = server.finish();
        let completed = match invocation {
            Ok(value) => value,
            Err(error) => std::panic::resume_unwind(error),
        };
        let family = _native::locks::pack_family_path(self.application, "cryptoemoji");
        drop(_native::locks::Lease::acquire(&family, "rebuild released").unwrap());
        assert!(family.is_file());
        assert!(!_native::process_owner::cleanup_pending(self.root));
        assert!(!_native::process_owner::cleanup_pending(self.application));
        assert!(
            std::fs::read_dir(self.root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
        assert!(!completed.stdout.contains(bot_api::TOKEN));
        assert!(!completed.stderr.contains(bot_api::TOKEN));
        assert!(!completed.stderr.contains("Traceback"));
        (completed, requests.unwrap())
    }
}

#[test]
fn rebuild_effects_record_intent_before_upload_and_resume_without_a_second_mutation() {
    let fixture = tooling_fixture::Root::new("coin rebuild original commands");
    let root = &fixture.0;
    // Capture custody must remain usable when the application's codec workspace is occupied.
    let application = root.join("application");
    std::fs::create_dir_all(application.join("assets")).unwrap();
    std::fs::write(application.join("assets/panel.html"), b"fixture marker").unwrap();
    std::fs::write(application.join("pyproject.toml"), b"# fixture marker\n").unwrap();
    let coins = application.join("coins");
    let images = coins.join("logos/emoji");
    std::fs::create_dir_all(&images).unwrap();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let art =
        std::fs::read(project.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
    std::fs::write(images.join("aaa.png"), &art).unwrap();
    let plan = coins.join("rebuild_dedup_plan.json");
    let state = coins.join("rebuild_dedup_state.json");
    _native::atomic::write_json(
        &plan,
        &json!([{"rep":"aaa","tickers":["aaa"],"kw":"Alpha"}]),
    )
    .unwrap();
    _native::atomic::write_json(
        &state,
        &json!({"sets":[],"sent":[],"order":[],"cursor":0,"in_flight":null,"deleted_old":false}),
    )
    .unwrap();
    let old_state = coins.join("rebuild_state.json");
    _native::atomic::write_json(
        &old_state,
        &json!({"sets":[{"name":"old1"},{"name":"old2"}]}),
    )
    .unwrap();
    let old_before = std::fs::read(&old_state).unwrap();
    let plan_before = std::fs::read(&plan).unwrap();
    let inventory = coins.join("currency-emoji-inventory.md");
    let inventory_bytes = b"ticker: aaa\npremium-id:\n";
    std::fs::write(&inventory, inventory_bytes).unwrap();
    let executable = application.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let input = root.join("command-eof");
    std::fs::write(&input, b"").unwrap();
    let log = RunLog::open(
        root,
        "coin-rebuild-command-owner",
        vec![bot_api::TOKEN.into()],
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let harness = Harness {
        root,
        application: &application,
        executable: &executable,
        input: &input,
        runtime: &runtime,
        log: &log,
        state: &state,
    };
    let mut requests = Vec::new();
    for iteration in 0..7 {
        if iteration >= 4 {
            _native::atomic::write_json(&state, &json!({"sets":[],"sent":[],"order":[],"cursor":0,"in_flight":null,"deleted_old":iteration >= 5})).unwrap();
        }
        if iteration == 5 {
            assert!(!application.join("logs/tmp").exists());
            std::fs::write(application.join("logs/tmp"), b"occupied proof workspace").unwrap();
        }
        let before = std::fs::read(&state).unwrap();
        let (output, observed) = harness.run(iteration, replies(iteration, &art));
        assert_eq!(
            output.code,
            if iteration == 0 || iteration >= 4 {
                3
            } else {
                0
            },
            "{} {}",
            output.stdout,
            output.stderr
        );
        assert_eq!(observed.len(), [5, 5, 2, 6, 3, 5, 2][iteration]);
        requests.extend(observed);
        assert_eq!(std::fs::read(&old_state).unwrap(), old_before);
        assert_eq!(std::fs::read(&plan).unwrap(), plan_before);
        assert_eq!(std::fs::read(images.join("aaa.png")).unwrap(), art);
        assert_eq!(std::fs::read(&inventory).unwrap(), inventory_bytes);
        let saved = document(&state);
        if iteration >= 5 {
            assert_eq!(saved["cursor"], 1);
            assert_eq!(saved["order"], json!([]));
            assert_eq!(saved["sets"], json!([]));
            assert_eq!(saved["in_flight"]["key"], "aaa");
            assert_eq!(saved["in_flight"]["operation"], "create");
            assert!(
                output.stderr.contains(if iteration == 5 {
                    "cannot create codec output directory"
                } else {
                    "Bot API success has no result"
                }),
                "{}",
                output.stderr
            );
            assert!(!output.stdout.contains("buildonly checkpoint"));
            if iteration == 5 {
                let occupied = application.join("logs/tmp");
                assert_eq!(
                    std::fs::read(&occupied).unwrap(),
                    b"occupied proof workspace"
                );
                std::fs::remove_file(occupied).unwrap();
            }
            continue;
        }
        if iteration == 4 {
            assert_eq!(std::fs::read(&state).unwrap(), before);
            assert!(
                output
                    .stderr
                    .contains("cannot confirm old pack old1 was deleted")
            );
            continue;
        }
        if iteration == 0 {
            assert_eq!(saved["cursor"], 0);
            assert_eq!(saved["order"], json!([]));
            assert_eq!(saved["sets"], json!([]));
            assert_eq!(saved["deleted_old_packs"], json!(["old1"]));
            assert_eq!(saved["deleted_old"], false);
            assert!(saved["in_flight"].is_null());
            assert!(output.stderr.contains("some old packs still exist"));
            continue;
        }
        assert_eq!(saved["deleted_old_packs"], json!(["old1", "old2"]));
        assert_eq!(saved["deleted_old"], true);
        assert_eq!(saved["cursor"], 1);
        assert_eq!(saved["order"], json!(["aaa"]));
        assert!(saved["in_flight"].is_null());
        assert_eq!(saved["sent"], json!([SET]));
        if iteration == 2 {
            assert_eq!(saved["sets"][0]["live"], 1);
        }
        if iteration == 3 {
            assert_eq!(saved["final_sent"], true);
            assert_eq!(
                document(&coins.join("ticker_to_id.json")),
                json!({"aaa":"111111111"})
            );
            assert!(
                std::fs::read_to_string(coins.join("currency-emoji-inventory.filled.md"))
                    .unwrap()
                    .contains("premium-id: 111111111")
            );
            assert!(output.stdout.contains("sent final links for 1 packs"));
            assert!(!output.stdout.contains("buildonly checkpoint"));
        } else {
            assert!(
                output
                    .stdout
                    .contains("buildonly checkpoint: plan position 1/1")
            );
        }
    }
    assert_eq!(requests.len(), 28);
    for (method, count) in [
        ("deleteStickerSet", 4),
        ("sendMessage", 2),
        ("createNewStickerSet", 3),
        ("addStickerToSet", 0),
    ] {
        assert_eq!(
            requests.iter().filter(|r| r.method == method).count(),
            count
        );
    }
    let family = application.join(".locks/pack_cryptoemoji.lock");
    assert_eq!(
        family,
        _native::locks::pack_family_path(&application, "cryptoemoji")
    );
    assert!(!coins.join("rebuild_dedup_state.json.lock").exists());
    drop(_native::locks::Lease::acquire(&family, "fixture release").unwrap());
    assert!(family.is_file());
    let other = application.join(".locks/pack_othercoins.lock");
    let unrelated = _native::locks::Lease::acquire(&other, "independent family").unwrap();
    _native::atomic::write_json(
        &state,
        &json!({"sets":[],"sent":[],"order":[],"cursor":0,"in_flight":null,"deleted_old":true}),
    )
    .unwrap();
    for iteration in 7..10 {
        let before = std::fs::read(&state).unwrap();
        let (output, observed) = harness.run(iteration, replies(iteration, &art));
        assert_eq!(output.code, 0, "{} {}", output.stdout, output.stderr);
        assert_eq!(observed.len(), if iteration == 9 { 2 } else { 4 });
        assert_eq!(
            observed
                .iter()
                .filter(|request| request.method == "sendMessage")
                .count(),
            if iteration == 9 { 0 } else { 2 }
        );
        let saved = document(&state);
        assert_eq!(saved["cursor"], 1);
        assert_eq!(saved["order"], json!(["aaa"]));
        assert!(saved["in_flight"].is_null());
        if iteration == 7 {
            assert_eq!(saved["sent"], json!([]));
            assert!(output.stdout.contains("notify failed"));
            assert!(
                output
                    .stdout
                    .contains("sendMessage failed after 2 attempts")
            );
        } else {
            assert_eq!(saved["sent"], json!([SET]));
            if iteration == 8 {
                assert!(output.stdout.contains("sent link for"));
                assert!(!output.stdout.contains("notify failed"));
            } else {
                assert_eq!(std::fs::read(&state).unwrap(), before);
            }
        }
        assert_eq!(
            observed
                .iter()
                .filter(|request| request.method == "createNewStickerSet")
                .count(),
            usize::from(iteration == 7)
        );
        assert!(!observed.iter().any(|request| matches!(
            request.method.as_str(),
            "deleteStickerSet" | "addStickerToSet"
        )));
        assert_eq!(std::fs::read(&old_state).unwrap(), old_before);
        assert_eq!(std::fs::read(&plan).unwrap(), plan_before);
        assert_eq!(std::fs::read(images.join("aaa.png")).unwrap(), art);
        assert_eq!(std::fs::read(&inventory).unwrap(), inventory_bytes);
    }
    drop(unrelated);
    drop(_native::locks::Lease::acquire(&other, "unrelated released").unwrap());
    coin_rebuild_admission::check(&harness, &art);
    let foreign =
        std::fs::read(project.join("tests/fixtures/native/static-codec/public-logo.png")).unwrap();
    coin_rebuild_resume::check(&harness, &art, &foreign);
    drop(log);
    fixture.clean().unwrap();
}
