//! Copied map commands prove content under both leases and preserve every refused canonical map.
use _native::{logging::RunLog, tooling_process};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server};
const SET: &str = "cryptoemoji1_by_YourEmojiBot";

fn document(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn member(id: &str, file: &str) -> Value {
    json!({"custom_emoji_id":id,"file_id":file})
}
fn download(replies: &mut Vec<Reply>, id: &str, art: &[u8]) {
    replies.push(Reply::new(
        "getFile",
        &[("file_id", id)],
        json!({"ok":true,"result":{"file_path":"fixture.png"}}),
    ));
    let mut raw = Reply::new("getFile", &[], Value::Null);
    raw.method = "GET fixture.png".into();
    raw.download = Some(art.to_vec());
    replies.push(raw);
}
#[test]
fn reordered_images_restore_aliases_and_unreadable_live_content_preserves_the_map() {
    let fixture = tooling_fixture::Root::new("rebuild map original commands");
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
        "rebuild-map-command-owner",
        vec![bot_api::TOKEN.into()],
    );
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let pinned: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let read = |name: &str| {
        use sha2::{Digest, Sha256};
        let case = pinned["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap();
        let bytes = std::fs::read(project.join(case["input"].as_str().unwrap())).unwrap();
        let digest = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(digest, case["input_sha256"]);
        (bytes, case["expected"]["phash"].as_u64().unwrap())
    };
    let (a, a_hash) = read("public-logo");
    let (b, b_hash) = read("alpha-edge");
    let distance: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/logo-distance.json"
    ))
    .unwrap();
    let third = &distance["cases"][2];
    let c_path = project.join(third["ours"].as_str().unwrap());
    let c = std::fs::read(&c_path).unwrap();
    {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(&c)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(digest, third["ours_sha256"]);
    }
    // This is an input separation precondition, not an expected mapping derived from the target.
    let c_hash = _native::codec_image::source_dhash(&c_path).unwrap();
    for (left, right) in [(a_hash, b_hash), (a_hash, c_hash), (b_hash, c_hash)] {
        assert!((left ^ right).count_ones() > 8);
    }
    let family = _native::locks::pack_family_path(root, "cryptoemoji");
    let canonical = root.join(".locks/canonical_map.lock");
    let mut accepted_map = Vec::new();
    for scenario in [
        "reordered",
        "unreadable",
        "untouched",
        "foreign",
        "missing-source",
        "extra",
        "provider",
        "provider-no-source",
        "lost-alias",
        "map-held",
        "family-held",
        "shared-unreviewed",
        "shared-reviewed",
    ] {
        let directory = root.join(scenario);
        let images = directory.join("logos/emoji");
        std::fs::create_dir_all(&images).unwrap();
        for (name, bytes) in [("aaa", &a), ("bbb", &b), ("ccc", &c)] {
            std::fs::write(images.join(format!("{name}.png")), bytes).unwrap();
        }
        let plan_path = directory.join("rebuild_dedup_plan.json");
        let state_path = directory.join("rebuild_dedup_state.json");
        let map_path = directory.join("ticker_to_id.json");
        let candidate = directory.join("ticker_to_id.candidate.json");
        let inventory = directory.join("currency-emoji-inventory.md");
        let filled = directory.join("currency-emoji-inventory.filled.md");
        let groups = directory.join("shared_logo_groups.json");
        let mut plan = json!([{"rep":"aaa","tickers":["aaa"],"kw":"A"},{"rep":"bbb","tickers":["bbb"],"kw":"B"}]);
        let shared = scenario.starts_with("shared-");
        let names = (0..21)
            .map(|index| format!("alias{index}"))
            .collect::<Vec<_>>();
        if shared {
            plan[0]["tickers"] = json!(names);
        }
        let mut state = json!({"sets":[{"index":1,"name":SET}],"order":["aaa","bbb"],"cursor":2,"in_flight":null});
        if scenario.starts_with("provider") {
            state["provider_added"] = json!(["ccc"]);
        }
        _native::atomic::write_json(&plan_path, &plan).unwrap();
        _native::atomic::write_json(&state_path, &state).unwrap();
        let previous = if scenario == "lost-alias" {
            json!({"aaa":"old-a","zzz":"old-z","zzzbsc":"old-z"})
        } else if scenario == "provider-no-source" {
            Value::Null
        } else if shared {
            json!({"keep":"prior"})
        } else {
            json!({"aaa":"old-a","aaaerc":"old-a","bbb":"old-b"})
        };
        if scenario == "unreadable" {
            std::fs::write(&map_path, &accepted_map).unwrap();
        } else if !previous.is_null() {
            _native::atomic::write_json(&map_path, &previous).unwrap();
        }
        if scenario == "shared-reviewed" {
            _native::atomic::write_json(&groups, &json!({"reviewed":names})).unwrap();
        }
        let inventory_bytes = b"## A - A\nticker: aaa\npremium-id:\n## Alias - A\nticker: aaaerc\npremium-id:\n## B - B\nticker: bbb\npremium-id:\n";
        std::fs::write(&inventory, inventory_bytes).unwrap();
        std::fs::write(&filled, b"previous filled inventory").unwrap();
        if scenario == "missing-source" {
            std::fs::remove_file(images.join("bbb.png")).unwrap();
        }
        if scenario == "provider-no-source" {
            std::fs::remove_file(images.join("ccc.png")).unwrap();
        }
        let map_before = std::fs::read(&map_path).ok();
        let state_before = std::fs::read(&state_path).unwrap();
        let plan_before = std::fs::read(&plan_path).unwrap();
        let mut replies = vec![];
        let held_lock = match scenario {
            "map-held" => {
                Some(_native::locks::Lease::acquire(&canonical, "map writer held").unwrap())
            }
            "family-held" => {
                Some(_native::locks::Lease::acquire(&family, "family writer held").unwrap())
            }
            _ => None,
        };
        if held_lock.is_none() {
            let reordered = matches!(scenario, "reordered" | "unreadable");
            let mut stickers = if reordered {
                vec![member("222222222", "fb"), member("111111111", "fa")]
            } else {
                vec![member("111111111", "fa"), member("222222222", "fb")]
            };
            if matches!(scenario, "extra" | "provider" | "provider-no-source") {
                stickers.push(member("333333333", "fc"));
            }
            replies.push(Reply::new(
                "getStickerSet",
                &[("name", SET)],
                json!({"ok":true,"result":{"stickers":stickers}}),
            ));
            if !matches!(scenario, "missing-source" | "provider-no-source") {
                if reordered {
                    download(
                        &mut replies,
                        "fb",
                        if scenario == "unreadable" {
                            b"controlled unreadable live image"
                        } else {
                            &b
                        },
                    );
                    if scenario != "unreadable" {
                        download(&mut replies, "fa", &a);
                    }
                } else {
                    download(&mut replies, "fa", &a);
                    download(
                        &mut replies,
                        "fb",
                        if scenario == "foreign" { &c } else { &b },
                    );
                    if matches!(scenario, "extra" | "provider") {
                        download(&mut replies, "fc", &c);
                    }
                }
            }
        }
        let mut signals = Vec::new();
        let replies = replies
            .into_iter()
            .map(|reply| {
                let (held, reached, release) = reply.hold();
                signals.push((reached, release));
                held
            })
            .collect();
        let mut server = Server::new(replies);
        let mut environment =
            _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
        for (key, value) in [
            ("NUMERA_TEST_API_BASE", server.base.as_str()),
            ("COIN_PACK_BASE", "cryptoemoji"),
            ("TELEGRAM_BOT_TOKEN", bot_api::TOKEN),
        ] {
            environment.insert(key.into(), value.into());
        }
        let args = [
            executable.as_os_str().to_owned(),
            "coin-rebuild-map".into(),
            "--coin-dir".into(),
            directory.as_os_str().to_owned(),
        ];
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.block_on(async {
                let observe = async {
                    for (reached, release) in signals {
                        tokio::time::timeout(Duration::from_secs(5), reached)
                            .await
                            .unwrap()
                            .unwrap();
                        for lock in [&family, &canonical] {
                            assert!(
                                _native::locks::Lease::acquire(lock, "map snapshot competitor")
                                    .is_err()
                            );
                            assert!(lock.is_file());
                        }
                        assert_eq!(std::fs::read(&map_path).ok(), map_before);
                        assert_eq!(std::fs::read(&state_path).unwrap(), state_before);
                        release.send(()).unwrap();
                    }
                };
                let (output, ()) = tokio::join!(
                    tooling_process::capture_with_input(
                        root,
                        &args,
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
        drop(held_lock);
        let output = match outcome {
            Ok(value) => value,
            Err(error) => std::panic::resume_unwind(error),
        };
        let requests = requests.unwrap();
        let success = matches!(
            scenario,
            "reordered" | "untouched" | "provider" | "shared-reviewed"
        );
        assert_eq!(
            output.code,
            if success { 0 } else { 2 },
            "{scenario}: {} {}",
            output.stdout,
            output.stderr
        );
        assert!(requests.iter().all(|request| matches!(
            request.method.as_str(),
            "getStickerSet" | "getFile" | "GET fixture.png"
        )));
        if matches!(scenario, "map-held" | "family-held") {
            assert!(requests.is_empty());
        }
        if success {
            let mut expected = json!({"aaa":"111111111","aaaerc":"111111111","bbb":"222222222"});
            if scenario == "provider" {
                expected["ccc"] = json!("333333333");
            }
            if shared {
                expected = json!({"bbb":"222222222"});
                for name in &names {
                    expected[name] = json!("111111111");
                }
            }
            assert_eq!(document(&map_path), expected);
            if scenario == "reordered" {
                accepted_map = std::fs::read(&map_path).unwrap();
            }
            assert!(!candidate.exists());
            let text = std::fs::read_to_string(&filled).unwrap();
            if !shared {
                assert_eq!(text.matches("premium-id: 111111111").count(), 2);
                assert_eq!(text.matches("premium-id: 222222222").count(), 1);
            }
        } else {
            assert_eq!(std::fs::read(&map_path).ok(), map_before);
            assert_eq!(
                std::fs::read(&filled).unwrap(),
                b"previous filled inventory"
            );
            if matches!(scenario, "map-held" | "family-held") {
                assert!(!candidate.exists());
            } else {
                let refused = document(&candidate);
                if scenario == "lost-alias" {
                    assert_eq!(refused, json!({"aaa":"111111111","bbb":"222222222"}));
                    assert!(output.stderr.contains("zzzbsc"));
                } else if scenario == "shared-unreviewed" {
                    assert_eq!(refused.as_object().unwrap().len(), 22);
                    assert!(output.stderr.contains("21 unreviewed tickers"));
                } else {
                    assert_eq!(refused["recorded_uploads"], 2);
                    assert!(!refused["error"].as_str().unwrap().is_empty());
                    if scenario == "unreadable" {
                        assert!(
                            refused["error"]
                                .as_str()
                                .unwrap()
                                .contains("222222222 could not be read")
                        );
                    }
                }
            }
        }
        assert_eq!(std::fs::read(&state_path).unwrap(), state_before);
        assert_eq!(std::fs::read(&plan_path).unwrap(), plan_before);
        assert_eq!(std::fs::read(&inventory).unwrap(), inventory_bytes);
        for (name, bytes) in [("aaa", &a), ("bbb", &b), ("ccc", &c)] {
            if (scenario == "missing-source" && name == "bbb")
                || (scenario == "provider-no-source" && name == "ccc")
            {
                assert!(!images.join(format!("{name}.png")).exists());
            } else {
                assert_eq!(
                    std::fs::read(images.join(format!("{name}.png"))).unwrap(),
                    *bytes
                );
            }
        }
        for lock in [&family, &canonical] {
            drop(_native::locks::Lease::acquire(lock, "map released").unwrap());
            assert!(lock.is_file());
        }
        assert!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
        assert!(!_native::process_owner::cleanup_pending(root));
        assert!(!output.stdout.contains(bot_api::TOKEN));
        assert!(!output.stderr.contains(bot_api::TOKEN));
    }
    drop(log);
    fixture.clean().unwrap();
}
