//! Original recovery outcomes through the copied command, real pixels and durable state.
use super::{Harness, Reply, SET, Server, document, me, message};
use serde_json::{Value, json};
use std::time::Duration;

fn member(number: usize) -> Value {
    json!({"file_id":format!("f{number}"),"file_unique_id":format!("u{number}"),"custom_emoji_id":format!("cid{number}")})
}
fn listing(name: &str, count: usize) -> Reply {
    super::listing(
        name,
        json!({"stickers":(0..count).map(member).collect::<Vec<_>>()}),
    )
}
fn download(replies: &mut Vec<Reply>, number: usize, art: &[u8]) {
    let id = format!("f{number}");
    replies.push(Reply::new(
        "getFile",
        &[("file_id", &id)],
        json!({"ok":true,"result":{"file_path":"fixture.png"}}),
    ));
    let mut bytes = Reply::new("getFile", &[], Value::Null);
    bytes.method = "GET fixture.png".into();
    bytes.download = Some(art.to_vec());
    replies.push(bytes);
}
fn mutation(creating: bool, art: &[u8]) -> Reply {
    let sticker = json!({"sticker":"attach://file0","format":"static","emoji_list":["🪙"],"keywords":[if creating {"aaa"} else {"bbb"}]});
    let body = if creating { json!([sticker]) } else { sticker }.to_string();
    let mut fields = vec![
        ("user_id", "111111111"),
        ("name", SET),
        (if creating { "stickers" } else { "sticker" }, body.as_str()),
    ];
    if creating {
        fields.extend([("title", "Fixture 1"), ("sticker_type", "custom_emoji")]);
    }
    let mut reply = Reply::new(
        if creating {
            "createNewStickerSet"
        } else {
            "addStickerToSet"
        },
        &fields,
        json!({"ok":true,"result":true}),
    );
    reply.files.insert("file0".into(), art.to_vec());
    reply
}

pub(super) fn check(harness: &Harness<'_>, ours: &[u8], foreign: &[u8]) {
    for scenario in [
        "create-landed",
        "create-missing",
        "create-unknown",
        "create-foreign",
        "create-corrupt",
        "create-empty",
        "create-two",
        "add-landed",
        "add-no-land",
        "add-foreign",
        "add-unknown",
    ] {
        let directory = harness.application.join(format!("resume {scenario}"));
        let images = directory.join("logos/emoji");
        std::fs::create_dir_all(&images).unwrap();
        for name in ["aaa", "bbb"] {
            std::fs::write(images.join(format!("{name}.png")), ours).unwrap();
        }
        std::fs::write(images.join("seed.png"), foreign).unwrap();
        let adding = scenario.starts_with("add-");
        let complete_after_recovery = scenario == "add-landed";
        let plan = if adding {
            json!([{"rep":"seed","tickers":["seed"],"kw":"seed"},{"rep":"bbb","tickers":["bbb"],"kw":"bbb"}])
        } else if scenario == "create-landed" {
            json!([{"rep":"aaa","tickers":["aaa"],"kw":"aaa"},{"rep":"bbb","tickers":["bbb"],"kw":"bbb"}])
        } else {
            json!([{"rep":"aaa","tickers":["aaa"],"kw":"aaa"}])
        };
        let original = json!({"sets":if adding {json!([{"index":1,"name":SET,"title":"Fixture 1"}])} else {json!([])},"sent":[],"deleted_old":true,"final_sent":false,"order":if adding {json!(["seed"])} else {json!([])},"cursor":if adding {2} else {1},"in_flight":{"key":if adding {"bbb"} else {"aaa"},"operation":if adding {"add"} else {"create"},"set_name":SET,"set_index":1,"expected_before":usize::from(adding),"phase":"upload"}});
        let state_path = directory.join("rebuild_dedup_state.json");
        let plan_path = directory.join("rebuild_dedup_plan.json");
        _native::atomic::write_json(&plan_path, &plan).unwrap();
        _native::atomic::write_json(&state_path, &original).unwrap();
        let before = std::fs::read(&state_path).unwrap();
        let plan_before = std::fs::read(&plan_path).unwrap();
        let mut replies = vec![me()];
        if adding {
            if scenario == "add-unknown" {
                replies.push(super::listing(SET, json!([])));
            } else {
                let count = if scenario == "add-no-land" { 1 } else { 2 };
                replies.extend([listing(SET, count), listing(SET, count)]);
                if count == 2 {
                    download(
                        &mut replies,
                        1,
                        if scenario == "add-foreign" {
                            foreign
                        } else {
                            ours
                        },
                    );
                }
                if scenario == "add-no-land" {
                    replies.push(listing(SET, 1));
                    replies.push(mutation(false, ours));
                }
            }
        } else {
            replies.push(match scenario {
                "create-missing" => super::missing(SET),
                "create-unknown" => super::listing(SET, json!([])),
                "create-empty" => listing(SET, 0),
                "create-two" => listing(SET, 2),
                _ => listing(SET, 1),
            });
            if matches!(
                scenario,
                "create-landed" | "create-foreign" | "create-corrupt"
            ) {
                download(
                    &mut replies,
                    0,
                    match scenario {
                        "create-foreign" => foreign,
                        "create-corrupt" => b"not an image at all",
                        _ => ours,
                    },
                );
            }
            if scenario == "create-landed" {
                replies.push(listing(SET, 1));
                replies.push(mutation(false, ours));
            } else if scenario == "create-missing" {
                replies.push(mutation(true, ours));
            }
        }
        let successful = matches!(
            scenario,
            "create-landed" | "create-missing" | "add-no-land" | "add-landed"
        );
        if successful {
            replies.push(message());
        }
        let mut signals = Vec::new();
        let replies = replies
            .into_iter()
            .map(|mut reply| {
                if reply.method != "getMe" {
                    let method = reply.method.clone();
                    if matches!(method.as_str(), "createNewStickerSet" | "addStickerToSet") {
                        reply.observation = Some((
                            state_path.clone(),
                            "/in_flight/key".into(),
                            json!(if method == "createNewStickerSet" {
                                "aaa"
                            } else {
                                "bbb"
                            }),
                        ));
                    }
                    let (held, reached, release) = reply.hold();
                    signals.push((method, reached, release));
                    reply = held;
                }
                reply
            })
            .collect();
        let mut server = Server::new(replies);
        let args = [
            "build".into(),
            "--coin-dir".into(),
            directory.as_os_str().to_owned(),
        ];
        let invocation = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            harness.runtime.block_on(async {
                let observe = async {
                    for (method, reached, release) in signals {
                        tokio::time::timeout(Duration::from_secs(5), reached)
                            .await
                            .unwrap()
                            .unwrap();
                        let family =
                            _native::locks::pack_family_path(harness.application, "cryptoemoji");
                        assert!(
                            _native::locks::Lease::acquire(&family, "resume competitor").is_err()
                        );
                        if matches!(method.as_str(), "createNewStickerSet" | "addStickerToSet") {
                            let state = document(&state_path);
                            assert_eq!(
                                state["cursor"],
                                if method == "createNewStickerSet" {
                                    1
                                } else {
                                    2
                                }
                            );
                            assert_eq!(
                                state["in_flight"]["operation"],
                                if method == "createNewStickerSet" {
                                    "create"
                                } else {
                                    "add"
                                }
                            );
                            assert_eq!(state["in_flight"]["set_name"], SET);
                            assert_eq!(
                                state["in_flight"]["expected_before"],
                                if method == "createNewStickerSet" {
                                    0
                                } else {
                                    1
                                }
                            );
                            assert_eq!(
                                state["order"],
                                if method == "createNewStickerSet" {
                                    json!([])
                                } else if adding {
                                    json!(["seed"])
                                } else {
                                    json!(["aaa"])
                                }
                            );
                        }
                        release.send(()).unwrap();
                    }
                };
                let (output, ()) = tokio::join!(harness.command(&server, &args, &[]), observe);
                output.unwrap()
            })
        }));
        let requests = server.finish();
        let output = match invocation {
            Ok(value) => value,
            Err(error) => std::panic::resume_unwind(error),
        };
        let requests = requests.unwrap();
        assert_eq!(
            output.code,
            if successful { 0 } else { 3 },
            "{scenario}: {} {}",
            output.stdout,
            output.stderr
        );
        assert!(
            !requests
                .iter()
                .any(|request| request.method == "deleteStickerSet")
        );
        let creates = requests
            .iter()
            .filter(|r| r.method == "createNewStickerSet")
            .count();
        let adds = requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .count();
        assert_eq!(creates, usize::from(scenario == "create-missing"));
        assert_eq!(
            adds,
            usize::from(matches!(scenario, "create-landed" | "add-no-land"))
        );
        if complete_after_recovery {
            assert_eq!(creates + adds, 0);
        }
        let saved = document(&state_path);
        if successful {
            assert!(saved["in_flight"].is_null());
            assert_eq!(
                saved["order"],
                if adding {
                    json!(["seed", "bbb"])
                } else if scenario == "create-landed" {
                    json!(["aaa", "bbb"])
                } else {
                    json!(["aaa"])
                }
            );
            assert_eq!(
                saved["cursor"],
                if scenario == "create-missing" { 1 } else { 2 }
            );
            assert!(output.stdout.contains("buildonly checkpoint"));
        } else if scenario == "add-foreign" {
            assert!(saved["in_flight"].is_null());
            assert_eq!(saved["cursor"], 1);
            assert_eq!(saved["order"], json!(["seed"]));
            assert!(
                output
                    .stderr
                    .contains("2 stickers are live but 1 uploads are recorded")
            );
        } else {
            assert_eq!(std::fs::read(&state_path).unwrap(), before);
            assert_eq!(saved, original);
        }
        assert_eq!(std::fs::read(&plan_path).unwrap(), plan_before);
        for name in ["aaa", "bbb"] {
            assert_eq!(
                std::fs::read(images.join(format!("{name}.png"))).unwrap(),
                ours
            );
        }
        assert_eq!(std::fs::read(images.join("seed.png")).unwrap(), foreign);
        let proof_storage = harness.application.join("logs/tmp");
        if proof_storage.exists() {
            assert!(std::fs::read_dir(proof_storage).unwrap().next().is_none());
        }
        assert!(!output.stdout.contains(super::bot_api::TOKEN));
        assert!(!output.stderr.contains(super::bot_api::TOKEN));
        drop(
            _native::locks::Lease::acquire(
                &_native::locks::pack_family_path(harness.application, "cryptoemoji"),
                "resume released",
            )
            .unwrap(),
        );
        assert!(!_native::process_owner::cleanup_pending(harness.root));
        assert!(!_native::process_owner::cleanup_pending(
            harness.application
        ));
        assert!(
            std::fs::read_dir(harness.root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
    }
}
