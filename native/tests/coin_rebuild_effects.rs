//! Actual command confirms old deletions, persists intent and resumes without duplicate mutations.
use _native::process_owner::OwnedProcess;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn rebuild_effects_record_intent_before_upload_and_resume_without_a_second_mutation() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin rebuild effects {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::fs::create_dir(scratch.join("assets")).unwrap();
        std::fs::write(scratch.join("assets/panel.html"), b"fixture marker").unwrap();
        std::fs::write(scratch.join("pyproject.toml"), b"# fixture marker\n").unwrap();
        let coins = scratch.join("coins");
        let images = coins.join("logos/emoji");
        std::fs::create_dir_all(&images).unwrap();
        let art =
            std::fs::read(root.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
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
        std::fs::write(
            coins.join("currency-emoji-inventory.md"),
            b"ticker: aaa\npremium-id:\n",
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server_root = scratch.clone();
        let state_read = state.clone();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut methods = Vec::new();
            let sequence = [
                "getMe",
                "deleteStickerSet",
                "getStickerSet",
                "deleteStickerSet",
                "getStickerSet",
                "getMe",
                "deleteStickerSet",
                "getStickerSet",
                "createNewStickerSet",
                "sendMessage",
                "getMe",
                "getStickerSet",
                "getMe",
                "getStickerSet",
                "getStickerSet",
                "getFile",
                "fixture.png",
                "sendMessage",
                "getMe",
                "deleteStickerSet",
                "getStickerSet",
                "getMe",
                "createNewStickerSet",
                "getStickerSet",
                "getFile",
                "fixture.png",
                "getMe",
                "createNewStickerSet",
            ];
            for (index, method) in sequence.into_iter().enumerate() {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::yield_now()
                        }
                        Err(e) => panic!("rebuild effects fixture: {e}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buf = [0; 4096];
                let end = loop {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0 && request.len() + n < 131072);
                    request.extend_from_slice(&buf[..n]);
                    if let Some(i) = request.windows(4).position(|v| v == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let header = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                let length = header
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:").map(str::trim))
                    .unwrap_or("0")
                    .parse::<usize>()
                    .unwrap();
                assert!(length < 131072);
                while request.len() < end + length {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0 && request.len() + n < 131072);
                    request.extend_from_slice(&buf[..n]);
                }
                methods.push(
                    String::from_utf8_lossy(&request)
                        .lines()
                        .next()
                        .unwrap()
                        .to_owned(),
                );
                assert!(methods.last().unwrap().contains(&format!("/{method} ")));
                if method != "getMe" {
                    assert!(
                        _native::locks::Lease::acquire(
                            &_native::locks::pack_family_path(&server_root, "cryptoemoji"),
                            "fixture"
                        )
                        .is_err()
                    );
                }
                if index == 22 {
                    let saved: Value =
                        serde_json::from_slice(&std::fs::read(&state_read).unwrap()).unwrap();
                    assert_eq!(saved["in_flight"]["key"], "aaa");
                    assert_eq!(saved["cursor"], 1);
                    continue;
                }
                if method == "deleteStickerSet" {
                    let saved: Value =
                        serde_json::from_slice(&std::fs::read(&state_read).unwrap()).unwrap();
                    assert_eq!(saved["cursor"], 0);
                    assert!(saved["in_flight"].is_null());
                    assert_eq!(saved["deleted_old"], false);
                    let text = String::from_utf8_lossy(&request);
                    assert!(text.contains(if index == 1 || index == 19 {
                        "name=old1"
                    } else {
                        "name=old2"
                    }));
                    if index == 6 {
                        assert_eq!(saved["deleted_old_packs"], json!(["old1"]));
                    }
                }
                let body = match index {
                    0 | 5 | 10 | 12 | 18 | 21 | 26 => json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
                    15 | 24 => json!({"ok":true,"result":{"file_path":"fixture.png"}}),
                    27 => json!({"ok":true}),
                    1 | 6 | 19 => json!({"ok":true,"result":true}),
                    20 => json!({"ok":true,"result":[]}),
                    2 | 7 => json!({"ok":false,"description":"Bad Request: STICKERSET_INVALID"}),
                    3 => json!({"ok":false,"description":"controlled delete refusal"}),
                    4 => json!({"ok":true,"result":{"stickers":[]}}),
                    8 => {
                        let saved: Value = serde_json::from_slice(&std::fs::read(&state_read).unwrap()).unwrap();
                        assert_eq!(saved["in_flight"]["key"], "aaa");
                        assert_eq!(saved["in_flight"]["operation"], "create");
                        assert_eq!(saved["cursor"], 1);
                        assert_eq!(saved["order"], json!([]));
                        assert_eq!(saved["deleted_old_packs"], json!(["old1","old2"]));
                        assert_eq!(saved["deleted_old"], true);
                        assert!(request.windows(art.len()).any(|v| v == art));
                        json!({"ok":true,"result":true})
                    }
                    9 | 17 => {
                        assert!(String::from_utf8_lossy(&request).contains("disable_web_page_preview=true"));
                        json!({"ok":true,"result":{"message_id":1}})
                    }
                    _ => json!({"ok":true,"result":{"stickers":[{"file_id":"f1","file_unique_id":"u1","custom_emoji_id":"111111111"}]}}),
                }.to_string().into_bytes();
                let body = if index == 16 || index == 25 {
                    art.clone()
                } else {
                    body
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
            methods
        });
        let invocation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let executable = scratch.join(if cfg!(windows) {
                "numera-emoji.exe"
            } else {
                "numera-emoji"
            });
            std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            for iteration in 0..7 {
                if iteration >= 4 {
                    _native::atomic::write_json(&state, &json!({"sets":[],"sent":[],"order":[],"cursor":0,"in_flight":null,"deleted_old":iteration >= 5})).unwrap();
                }
                if iteration == 5 {
                    // A blocked proof workspace must propagate its cause, not erase the pending upload.
                    std::fs::write(scratch.join("logs/tmp"), b"occupied proof workspace").unwrap();
                }
                let before = std::fs::read(&state).unwrap();
                let work = scratch.join(format!("owner{iteration}"));
                std::fs::create_dir(&work).unwrap();
                let output = scratch.join(format!("out{iteration}.txt"));
                let errors = scratch.join(format!("err{iteration}.txt"));
                let mut cmd = Command::new(&executable);
                cmd.args(["coin-rebuild", if iteration == 3 { "all" } else { "build" }])
                    .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
                    .env("NUMERA_TEST_API_BASE", &base)
                    .env("COIN_PACK_BASE", "cryptoemoji")
                    .env("COIN_PACK_TITLE", "Fixture")
                    .env("PACK_OWNER_USER_ID", "111111111")
                    .env("TELEGRAM_BOT_TOKEN", "synthetic-rebuild-effects-token")
                    .env_remove("WORKER_PUBLISH_URL")
                    .env_remove("WORKER_PUBLISH_SECRET")
                    .env_remove("PACK_LINKS_CHAT_ID")
                    .stdin(Stdio::null())
                    .stdout(Stdio::from(std::fs::File::create(&output).unwrap()))
                    .stderr(Stdio::from(std::fs::File::create(&errors).unwrap()));
                let mut owner = OwnedProcess::spawn(&mut cmd, work).unwrap();
                let status = runtime
                    .block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)))
                    .unwrap();
                owner.finish().unwrap();
                assert_eq!(
                    status.code(),
                    Some(if iteration == 0 || iteration >= 4 {
                        3
                    } else {
                        0
                    }),
                    "{}",
                    std::fs::read_to_string(&errors).unwrap()
                );
                assert_eq!(std::fs::read(&old_state).unwrap(), old_before);
                assert_eq!(std::fs::read(&plan).unwrap(), plan_before);
                let saved: Value = serde_json::from_slice(&std::fs::read(&state).unwrap()).unwrap();
                if iteration >= 5 {
                    assert_eq!(saved["cursor"], 1);
                    assert_eq!(saved["order"], json!([]));
                    assert_eq!(saved["sets"], json!([]));
                    assert_eq!(saved["in_flight"]["key"], "aaa");
                    assert_eq!(saved["in_flight"]["operation"], "create");
                    let error = std::fs::read_to_string(&errors).unwrap();
                    assert!(
                        error.contains(if iteration == 5 {
                            "cannot create codec output directory"
                        } else {
                            "Bot API success has no result"
                        }),
                        "{error}"
                    );
                    assert!(
                        !std::fs::read_to_string(&output)
                            .unwrap()
                            .contains("buildonly checkpoint")
                    );
                    continue;
                }
                if iteration == 4 {
                    assert_eq!(std::fs::read(&state).unwrap(), before);
                    assert!(
                        std::fs::read_to_string(&errors)
                            .unwrap()
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
                    assert!(
                        std::fs::read_to_string(&errors)
                            .unwrap()
                            .contains("some old packs still exist")
                    );
                    continue;
                }
                assert_eq!(saved["deleted_old_packs"], json!(["old1", "old2"]));
                assert_eq!(saved["deleted_old"], true);
                assert_eq!(saved["cursor"], 1);
                assert_eq!(saved["order"], json!(["aaa"]));
                assert!(saved["in_flight"].is_null());
                assert_eq!(saved["sent"], json!(["cryptoemoji1_by_YourEmojiBot"]));
                if iteration == 2 {
                    assert_eq!(saved["sets"][0]["live"], 1);
                }
                let text = std::fs::read_to_string(&output).unwrap();
                if iteration == 3 {
                    assert_eq!(saved["final_sent"], true);
                    let mapping: Value = serde_json::from_slice(
                        &std::fs::read(coins.join("ticker_to_id.json")).unwrap(),
                    )
                    .unwrap();
                    assert_eq!(mapping, json!({"aaa":"111111111"}));
                    assert!(
                        std::fs::read_to_string(coins.join("currency-emoji-inventory.filled.md"))
                            .unwrap()
                            .contains("premium-id: 111111111")
                    );
                    assert!(text.contains("sent final links for 1 packs"));
                    assert!(!text.contains("buildonly checkpoint"));
                } else {
                    assert!(text.contains("buildonly checkpoint: plan position 1/1"));
                }
            }
        }));
        let joined = server.join();
        if let Err(error) = invocation {
            std::panic::resume_unwind(error);
        }
        let methods = joined.unwrap();
        assert_eq!(methods.len(), 28);
        assert_eq!(
            methods
                .iter()
                .filter(|method| method.contains("/deleteStickerSet "))
                .count(),
            4
        );
        assert_eq!(
            methods
                .iter()
                .filter(|method| method.contains("/sendMessage "))
                .count(),
            2
        );
        assert_eq!(
            methods
                .iter()
                .filter(|m| m.contains("createNewStickerSet"))
                .count(),
            3
        );
        assert!(methods.iter().all(|m| !m.contains("addStickerToSet")));
    }));
    if let Err(error) = result {
        // Preserve failure diagnostics and ownership markers until cleanup can be verified.
        std::panic::resume_unwind(error);
    }
    for iteration in 0..7 {
        assert!(!scratch.join(format!("owner{iteration}")).exists());
    }
    let released = _native::locks::Lease::acquire(
        &_native::locks::pack_family_path(&scratch, "cryptoemoji"),
        "fixture release",
    )
    .unwrap();
    drop(released);
    std::fs::remove_dir_all(&scratch).unwrap();
}
