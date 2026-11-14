//! Original pre-delete validation at the existing copied rebuild command seam.
use super::{Harness, Reply, Server, delete, document, live, me, message, missing};
use serde_json::json;
use std::path::Path;

pub(super) fn check(harness: &Harness<'_>, art: &[u8]) {
    for scenario in [
        "truncated-plan",
        "missing-fields",
        "truncated-state",
        "negative-cursor",
        "oversized-cursor",
        "string-cursor",
        "boolean-cursor",
        "repeated-order",
        "repeated-set",
        "backwards-index",
        "oversized-live",
        "marker-no-target",
        "unknown-operation",
        "string-order",
        "string-deleted",
        "legacy-marker",
        "list-marker",
        "unreached-order",
        "reversed-order",
        "foreign-order",
        "orphan-order",
        "wrong-marker-key",
        "marker-no-cursor",
        "live-drift",
        "surviving-packs",
        "unknown-second-delete",
        "family-held",
        "missing-owner",
        "invalid-owner",
        "valid-owner",
        "subsequence",
        "trailing-skips",
        "sound-state",
    ] {
        let directory = harness.application.join(format!("admission-{scenario}"));
        let images = directory.join("logos/emoji");
        std::fs::create_dir_all(&images).unwrap();
        let plan_path = directory.join("rebuild_dedup_plan.json");
        let state_path = directory.join("rebuild_dedup_state.json");
        let old = directory.join("rebuild_state.json");
        let map = directory.join("ticker_to_id.json");
        let inventory = directory.join("currency-emoji-inventory.md");
        let mut plan = json!([
            {"rep":"aaa","tickers":["aaa"],"kw":"aaa"},
            {"rep":"bbb","tickers":["bbb"],"kw":"bbb"},
            {"rep":"ccc","tickers":["ccc"],"kw":"ccc"},
        ]);
        let mut state = json!({"sets":[],"sent":[],"deleted_old":false,"final_sent":false,
            "order":[],"cursor":0,"in_flight":null});
        let mut expected = "";
        match scenario {
            "negative-cursor" => {
                state["cursor"] = json!(-1);
                expected = "cursor -1";
            }
            "oversized-cursor" => {
                state["cursor"] = json!(4);
                expected = "past the end";
            }
            "string-cursor" => {
                state["cursor"] = json!("1");
                expected = "cursor";
            }
            "boolean-cursor" => {
                state["cursor"] = json!(true);
                expected = "cursor";
            }
            "repeated-order" => {
                state["order"] = json!(["aaa", "aaa"]);
                expected = "more than once";
            }
            "repeated-set" => {
                state["sets"] = json!([{"index":1,"name":"s1"},{"index":2,"name":"s1"}]);
                expected = "repeats the set name";
            }
            "backwards-index" => {
                state["sets"] = json!([{"index":2,"name":"s2"},{"index":1,"name":"s1"}]);
                expected = "does not ascend";
            }
            "oversized-live" => {
                state["sets"] = json!([{"index":1,"name":"s1","live":201}]);
                expected = "outside 0..";
            }
            "marker-no-target" => {
                state["in_flight"] = json!({"key":"aaa","operation":"add"});
                expected = "set_name";
            }
            "unknown-operation" => {
                state["in_flight"] =
                    json!({"key":"aaa","operation":"delete","set_name":"s1","set_index":1});
                expected = "unknown";
            }
            "string-order" => {
                state["order"] = json!("aaa");
                expected = "'order'";
            }
            "string-deleted" => {
                state["deleted_old"] = json!("no");
                expected = "'deleted_old'";
            }
            "legacy-marker" => {
                // The unchanged original repair starts after the old packs were deleted.
                state["deleted_old"] = json!(true);
                plan = json!([
                    {"rep":"seed","tickers":["seed"],"kw":"seed"},
                    {"rep":"aaa","tickers":["aaa"],"kw":"aaa"},
                ]);
                state["sets"] = json!([{"index":1,"name":"s1","title":"T 1","live":2}]);
                state["order"] = json!(["seed"]);
                state["cursor"] = json!(2);
                state["in_flight"] = json!("aaa");
                expected = "legacy bare key";
            }
            "list-marker" => {
                state["in_flight"] = json!(["aaa"]);
                expected = "'in_flight'";
            }
            "unreached-order" | "reversed-order" | "foreign-order" | "live-drift" => {
                state["sets"] = json!([{"index":1,"name":"s1","title":"T 1","live":1}]);
                state["order"] = match scenario {
                    "reversed-order" => json!(["ccc", "aaa"]),
                    "foreign-order" => json!(["zzz"]),
                    _ => json!(["aaa"]),
                };
                state["cursor"] = json!(if scenario == "unreached-order" {
                    0
                } else if scenario == "live-drift" {
                    1
                } else {
                    3
                });
                expected = if scenario == "live-drift" {
                    "uploads are recorded"
                } else {
                    "'order' records"
                };
            }
            "orphan-order" => {
                state["order"] = json!(["aaa"]);
                state["cursor"] = json!(1);
                expected = "no set holds them";
            }
            "wrong-marker-key" | "marker-no-cursor" => {
                state["sets"] = json!([{"index":1,"name":"s1","live":0}]);
                state["cursor"] = json!(usize::from(scenario == "wrong-marker-key"));
                state["in_flight"] = json!({"key":if scenario == "wrong-marker-key" { "ccc" } else { "aaa" },"operation":"add","set_name":"s1","set_index":1,"expected_before":0});
                expected = "is not plan entry";
            }
            "valid-owner" | "subsequence" | "trailing-skips" => {
                state["sets"] = json!([{"index":1,"name":"s1","title":"T 1","live":1}]);
                state["order"] = if scenario == "subsequence" {
                    json!(["aaa", "ccc"])
                } else {
                    json!(["aaa"])
                };
                state["cursor"] = json!(if scenario == "trailing-skips" { 1 } else { 3 });
                state["deleted_old"] = json!(true);
            }
            _ => {}
        }
        _native::atomic::write_json(&plan_path, &plan).unwrap();
        _native::atomic::write_json(&state_path, &state).unwrap();
        _native::atomic::write_json(&old, &json!({"sets":[{"name":"old1"},{"name":"old2"}]}))
            .unwrap();
        _native::atomic::write_json(&map, &json!({"existing":"preserved"})).unwrap();
        std::fs::write(&inventory, b"ticker: aaa\npremium-id:\n").unwrap();
        for name in ["aaa", "bbb", "ccc"] {
            std::fs::write(images.join(format!("{name}.png")), art).unwrap();
        }
        match scenario {
            "truncated-plan" => {
                std::fs::write(&plan_path, b"[{\"rep\": \"aaa\", \"tickers\": [\"aa").unwrap();
                expected = "invalid frozen rebuild plan JSON";
            }
            "missing-fields" => {
                _native::atomic::write_json(&plan_path, &json!([plan[0],{"rep":"bbb"}])).unwrap();
                expected = "missing rep/tickers/kw";
            }
            "truncated-state" => {
                std::fs::write(&state_path, b"{not json").unwrap();
                expected = "invalid provider durable JSON";
            }
            "trailing-skips" => {
                std::fs::remove_file(images.join("bbb.png")).unwrap();
                std::fs::write(images.join("ccc.png"), b"").unwrap();
            }
            _ => {}
        }
        let snapshot =
            [&plan_path, &state_path, &old, &map, &inventory].map(|p| std::fs::read(p).unwrap());
        let overrides: &[(&str, &str)] = match scenario {
            "missing-owner" => &[("PACK_OWNER_USER_ID", "")],
            "invalid-owner" => &[("PACK_OWNER_USER_ID", "42abc")],
            "valid-owner" => &[("PACK_OWNER_USER_ID", "12345")],
            _ => &[],
        };
        let mut replies = if scenario == "missing-owner" || scenario == "invalid-owner" {
            vec![]
        } else {
            vec![me()]
        };
        match scenario {
            "live-drift" | "subsequence" => {
                replies.push(super::listing("s1", json!({"stickers":[live()["stickers"][0],{"file_id":"f2","file_unique_id":"u2","custom_emoji_id":"222222222"}]})));
            }
            "valid-owner" | "trailing-skips" => replies.push(super::listing("s1", live())),
            "surviving-packs" => replies.extend([
                delete("old1", false),
                super::listing("old1", json!({"stickers":[]})),
                delete("old2", false),
                super::listing("old2", json!({"stickers":[]})),
            ]),
            "unknown-second-delete" => replies.extend([
                delete("old1", true),
                missing("old1"),
                delete("old2", true),
                super::listing("old2", json!([])),
            ]),
            "sound-state" => {
                replies.extend([
                    delete("old1", true),
                    missing("old1"),
                    delete("old2", true),
                    missing("old2"),
                ]);
                for (index, key) in ["aaa", "bbb", "ccc"].into_iter().enumerate() {
                    if index > 0 {
                        replies.push(super::listing(super::SET, json!({"stickers":(0..index).map(|n| json!({"file_id":format!("f{n}"),"file_unique_id":format!("u{n}")})).collect::<Vec<_>>()})));
                    }
                    let sticker = json!({"sticker":"attach://file0","format":"static","emoji_list":["🪙"],"keywords":[key]});
                    let body = if index == 0 {
                        json!([sticker])
                    } else {
                        sticker
                    }
                    .to_string();
                    let mut fields = vec![
                        ("user_id", "111111111"),
                        ("name", super::SET),
                        (
                            if index == 0 { "stickers" } else { "sticker" },
                            body.as_str(),
                        ),
                    ];
                    if index == 0 {
                        fields.extend([("title", "Fixture 1"), ("sticker_type", "custom_emoji")]);
                    }
                    let mut reply = Reply::new(
                        if index == 0 {
                            "createNewStickerSet"
                        } else {
                            "addStickerToSet"
                        },
                        &fields,
                        json!({"ok":true,"result":true}),
                    );
                    reply.files.insert("file0".into(), art.to_vec());
                    reply.observation =
                        Some((state_path.clone(), "/in_flight/key".into(), json!(key)));
                    replies.push(reply);
                }
                replies.push(message());
            }
            _ => {}
        }
        let successful = matches!(
            scenario,
            "valid-owner" | "subsequence" | "trailing-skips" | "sound-state"
        );
        if successful && scenario != "sound-state" {
            replies.push(if scenario == "valid-owner" {
                Reply::new(
                    "sendMessage",
                    &[("chat_id", "12345"), ("disable_web_page_preview", "true")],
                    json!({"ok":true,"result":{"message_id":1}}),
                )
            } else {
                message()
            });
        }
        let mut server = Server::new(replies);
        let family = _native::locks::pack_family_path(harness.application, "cryptoemoji");
        let lease = if scenario == "family-held" {
            Some(_native::locks::Lease::acquire(&family, "original held rebuild").unwrap())
        } else {
            None
        };
        let args = [
            "build".into(),
            "--coin-dir".into(),
            directory.as_os_str().to_owned(),
        ];
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            harness
                .runtime
                .block_on(harness.command(&server, &args, overrides))
        }));
        let requests = server.finish();
        drop(lease);
        let completed = match outcome {
            Ok(value) => value.unwrap(),
            Err(error) => std::panic::resume_unwind(error),
        };
        let requests = requests.unwrap();
        assert_eq!(
            completed.code,
            if successful {
                0
            } else if matches!(scenario, "missing-owner" | "invalid-owner") {
                2
            } else {
                3
            },
            "{scenario}: {} {}",
            completed.stdout,
            completed.stderr
        );
        if scenario == "sound-state" {
            assert_eq!(
                requests
                    .iter()
                    .filter(|r| r.method == "createNewStickerSet")
                    .count(),
                1
            );
            assert_eq!(
                requests
                    .iter()
                    .filter(|r| r.method == "addStickerToSet")
                    .count(),
                2
            );
        } else {
            assert!(
                !requests.iter().any(|r| matches!(
                    r.method.as_str(),
                    "addStickerToSet" | "createNewStickerSet"
                ))
            );
        }
        if !matches!(
            scenario,
            "surviving-packs" | "unknown-second-delete" | "sound-state"
        ) {
            assert!(!requests.iter().any(|r| r.method == "deleteStickerSet"));
        }
        if !successful {
            assert!(!requests.iter().any(|r| r.method == "sendMessage"));
        }
        if !expected.is_empty() {
            assert!(
                completed.stderr.contains(expected),
                "{scenario}: {}",
                completed.stderr
            );
        }
        if scenario == "legacy-marker" {
            for prescribed in ["'in_flight' to null", "append 'aaa' to 'order'", "cursor"] {
                assert!(completed.stderr.contains(prescribed));
            }
        }
        for (index, path) in [&plan_path, &state_path, &old, &map, &inventory]
            .into_iter()
            .enumerate()
        {
            if index != 1
                || (!successful && !matches!(scenario, "surviving-packs" | "unknown-second-delete"))
            {
                assert_eq!(
                    std::fs::read(path).unwrap(),
                    snapshot[index],
                    "{scenario}: {}",
                    path.display()
                );
            }
        }
        let saved = if scenario == "truncated-state" {
            serde_json::Value::Null
        } else {
            document(&state_path)
        };
        if scenario == "surviving-packs" || scenario == "unknown-second-delete" {
            assert_eq!(saved["deleted_old"], false);
            assert_eq!(
                saved
                    .get("deleted_old_packs")
                    .cloned()
                    .unwrap_or_else(|| json!([])),
                if scenario == "surviving-packs" {
                    json!([])
                } else {
                    json!(["old1"])
                }
            );
            assert_eq!(saved["cursor"], 0);
            assert!(saved["in_flight"].is_null());
        } else if successful {
            assert_eq!(saved["cursor"], 3);
            assert_eq!(
                saved["order"],
                if scenario == "sound-state" {
                    json!(["aaa", "bbb", "ccc"])
                } else {
                    state["order"].clone()
                }
            );
            if scenario == "sound-state" {
                assert_eq!(saved["deleted_old"], true);
                assert_eq!(saved["deleted_old_packs"], json!(["old1", "old2"]));
            }
            assert!(saved["in_flight"].is_null());
            assert!(
                completed
                    .stdout
                    .contains("buildonly checkpoint: plan position 3/3")
            );
            assert_eq!(
                requests
                    .iter()
                    .filter(|r| r.method == "sendMessage")
                    .count(),
                1
            );
        }
        if scenario == "legacy-marker" {
            // Apply only the two edits the actual refusal prescribed, then resume.
            let mut repaired = saved.clone();
            repaired["in_flight"] = serde_json::Value::Null;
            repaired["order"].as_array_mut().unwrap().push(json!("aaa"));
            _native::atomic::write_json(&state_path, &repaired).unwrap();
            let mut server = Server::new(vec![
                me(),
                super::listing(
                    "s1",
                    json!({"stickers":[live()["stickers"][0],{"file_id":"f2","file_unique_id":"u2","custom_emoji_id":"222222222"}]}),
                ),
                message(),
            ]);
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                harness
                    .runtime
                    .block_on(harness.command(&server, &args, &[]))
            }));
            let requests = server.finish();
            let output = match outcome {
                Ok(value) => value.unwrap(),
                Err(error) => std::panic::resume_unwind(error),
            };
            assert_eq!(output.code, 0, "{} {}", output.stdout, output.stderr);
            let requests = requests.unwrap();
            assert_eq!(requests.len(), 3);
            assert!(!requests.iter().any(|r| matches!(
                r.method.as_str(),
                "addStickerToSet" | "createNewStickerSet" | "deleteStickerSet"
            )));
            let repaired = document(&state_path);
            assert!(repaired["in_flight"].is_null());
            assert_eq!(repaired["order"], json!(["seed", "aaa"]));
            assert_eq!(repaired["cursor"], 2);
            assert_eq!(std::fs::read(&plan_path).unwrap(), snapshot[0]);
            assert_eq!(std::fs::read(&old).unwrap(), snapshot[2]);
            assert_eq!(std::fs::read(&map).unwrap(), snapshot[3]);
        }
        unchanged_images(&images, art, scenario);
        assert!(!directory.join("shared_logo_groups.json").exists());
        assert!(!directory.join("ticker_to_id.candidate.json").exists());
        assert!(
            !directory
                .join("currency-emoji-inventory.filled.md")
                .exists()
        );
        assert!(!completed.stdout.contains(super::bot_api::TOKEN));
        assert!(!completed.stderr.contains(super::bot_api::TOKEN));
        drop(_native::locks::Lease::acquire(&family, "admission release").unwrap());
        assert!(family.is_file());
        assert!(
            std::fs::read_dir(harness.root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
        assert!(!_native::process_owner::cleanup_pending(harness.root));
    }
}
fn unchanged_images(images: &Path, art: &[u8], scenario: &str) {
    assert_eq!(std::fs::read(images.join("aaa.png")).unwrap(), art);
    if scenario == "trailing-skips" {
        assert!(!images.join("bbb.png").exists());
        assert!(std::fs::read(images.join("ccc.png")).unwrap().is_empty());
    } else {
        for name in ["bbb", "ccc"] {
            assert_eq!(
                std::fs::read(images.join(format!("{name}.png"))).unwrap(),
                art
            );
        }
    }
}
