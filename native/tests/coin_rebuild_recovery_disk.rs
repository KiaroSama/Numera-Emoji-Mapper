//! Actual durable recovery refuses unknown/foreign/duplicate content before adopting any state.
use _native::{coin_rebuild_map_disk::Files, coin_rebuild_recovery_disk::run};
use serde_json::{Value, json};
use std::path::Path;

#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server};
const SET: &str = "cryptoemoji1_by_YourEmojiBot";

fn document(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn member(number: usize) -> Value {
    json!({"file_id":format!("f{number}"),"file_unique_id":format!("u{number}"),"custom_emoji_id":format!("cid{number}")})
}
fn listing(stickers: Vec<Value>) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", SET)],
        json!({"ok":true,"result":{"stickers":stickers}}),
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
fn recover(files: &Files, replies: Vec<Reply>) -> Result<(Value, u64), String> {
    let mut server = Server::new(replies);
    let tg = _native::telegram::Telegram::new(bot_api::TOKEN.into(), server.base.clone()).unwrap();
    let outcome =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(files, "Fixture", &tg)));
    let requests = server.finish();
    let result = match outcome {
        Ok(value) => value,
        Err(error) => std::panic::resume_unwind(error),
    };
    assert!(requests.unwrap().iter().all(|request| matches!(
        request.method.as_str(),
        "getStickerSet" | "getFile" | "GET fixture.png"
    )));
    let family = _native::locks::pack_family_path(&files.root, "cryptoemoji");
    drop(_native::locks::Lease::acquire(&family, "recovery released").unwrap());
    assert!(family.is_file());
    assert!(!_native::process_owner::cleanup_pending(&files.root));
    let scratch = files.root.join("logs/tmp");
    if scratch.exists() {
        assert!(std::fs::read_dir(scratch).unwrap().next().is_none());
    }
    result
}

#[test]
fn proven_create_is_adopted_once_and_malformed_readback_keeps_the_intent() {
    let fixture = tooling_fixture::Root::new("coin rebuild recovery originals");
    let root = &fixture.0;
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
        let hash = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(hash, case["input_sha256"]);
        (bytes, case["expected"]["phash"].as_u64().unwrap())
    };
    let (ours, our_hash) = read("alpha-edge");
    let (foreign, foreign_hash) = read("public-logo");
    assert!((our_hash ^ foreign_hash).count_ones() > 8);
    for scenario in [
        "create-landed",
        "create-missing",
        "create-unknown",
        "create-foreign",
        "create-corrupt",
        "create-empty",
        "create-two",
        "create-no-source",
        "create-malformed",
        "add-landed",
        "add-foreign",
        "add-no-land",
        "add-corrupt",
        "add-duplicate",
        "add-unknown",
        "add-no-source",
    ] {
        let directory = root.join(scenario);
        let images = directory.join("logos/emoji");
        std::fs::create_dir_all(&images).unwrap();
        std::fs::write(images.join("aaa.png"), &ours).unwrap();
        std::fs::write(images.join("seed.png"), &foreign).unwrap();
        let files = Files {
            root: directory.clone(),
            directory: directory.clone(),
            images: images.clone(),
            base: "cryptoemoji".into(),
        };
        let state_file = directory.join("rebuild_dedup_state.json");
        let plan_file = directory.join("rebuild_dedup_plan.json");
        let adding = scenario.starts_with("add-");
        let plan = if adding {
            json!([{"rep":"seed","tickers":["seed"],"kw":"seed"},{"rep":"aaa","tickers":["aaa"],"kw":"aaa"}])
        } else {
            json!([{"rep":"aaa","tickers":["aaa"],"kw":"aaa"}])
        };
        let original = json!({"sets":if adding { json!([{"index":1,"name":SET,"title":"Fixture 1"}]) } else { json!([]) },"order":if adding { json!(["seed"]) } else { json!([]) },"cursor":if adding { 2 } else { 1 },"in_flight":{"key":"aaa","operation":if adding { "add" } else { "create" },"set_name":SET,"set_index":1,"expected_before":usize::from(adding),"phase":"upload"}});
        _native::atomic::write_json(&plan_file, &plan).unwrap();
        _native::atomic::write_json(&state_file, &original).unwrap();
        if scenario.ends_with("no-source") {
            std::fs::remove_file(images.join("aaa.png")).unwrap();
        }
        let before = std::fs::read(&state_file).unwrap();
        let plan_before = std::fs::read(&plan_file).unwrap();
        let mut replies = Vec::new();
        if adding {
            replies.push(if scenario == "add-unknown" {
                Reply::new(
                    "getStickerSet",
                    &[("name", SET)],
                    json!({"ok":true,"result":[]}),
                )
            } else {
                let count = if scenario == "add-no-land" || scenario == "add-no-source" {
                    1
                } else if scenario == "add-duplicate" {
                    3
                } else {
                    2
                };
                listing((0..count).map(member).collect())
            });
            if !matches!(scenario, "add-unknown" | "add-no-source") {
                let count = if scenario == "add-no-land" {
                    1
                } else if scenario == "add-duplicate" {
                    3
                } else {
                    2
                };
                replies.push(listing((0..count).map(member).collect()));
                if scenario != "add-no-land" {
                    download(
                        &mut replies,
                        1,
                        if scenario == "add-foreign" {
                            &foreign
                        } else if scenario == "add-corrupt" {
                            b"not an image at all"
                        } else {
                            &ours
                        },
                    );
                    if scenario == "add-duplicate" {
                        download(&mut replies, 2, &ours);
                    }
                }
            }
        } else {
            replies.push(match scenario {
                "create-missing" => Reply::new(
                    "getStickerSet",
                    &[("name", SET)],
                    json!({"ok":false,"description":"Bad Request: STICKERSET_INVALID"}),
                ),
                "create-unknown" => Reply::new(
                    "getStickerSet",
                    &[("name", SET)],
                    json!({"ok":true,"result":[]}),
                ),
                "create-malformed" => Reply::new(
                    "getStickerSet",
                    &[("name", SET)],
                    json!({"ok":true,"result":{"title":"missing sticker list"}}),
                ),
                "create-empty" => listing(vec![]),
                "create-two" => listing(vec![member(0), member(1)]),
                _ => listing(vec![member(0)]),
            });
            if matches!(
                scenario,
                "create-landed" | "create-foreign" | "create-corrupt"
            ) {
                download(
                    &mut replies,
                    0,
                    if scenario == "create-foreign" {
                        &foreign
                    } else if scenario == "create-corrupt" {
                        b"not an image at all"
                    } else {
                        &ours
                    },
                );
            }
        }
        let result = recover(&files, replies);
        if matches!(
            scenario,
            "create-landed" | "add-landed" | "create-missing" | "add-no-land" | "add-foreign"
        ) {
            let (saved, total) = result.unwrap();
            assert!(saved["in_flight"].is_null());
            let landed = scenario.ends_with("landed");
            assert_eq!(
                saved["cursor"],
                if landed {
                    if adding { 2 } else { 1 }
                } else if adding {
                    1
                } else {
                    0
                }
            );
            assert_eq!(
                saved["order"],
                if landed {
                    if adding {
                        json!(["seed", "aaa"])
                    } else {
                        json!(["aaa"])
                    }
                } else if adding {
                    json!(["seed"])
                } else {
                    json!([])
                }
            );
            let expected_total = if adding {
                if scenario == "add-no-land" { 1 } else { 2 }
            } else {
                u64::from(landed)
            };
            assert_eq!(total, expected_total);
            assert_eq!(document(&state_file), saved);
            if scenario == "create-landed" {
                assert_eq!(
                    saved["sets"],
                    json!([{"index":1,"name":SET,"title":"Fixture 1","live":1}])
                );
                let bytes = std::fs::read(&state_file).unwrap();
                let (again, total) = recover(&files, vec![listing(vec![member(0)])]).unwrap();
                assert_eq!((again, total), (saved, 1));
                assert_eq!(std::fs::read(&state_file).unwrap(), bytes);
            }
        } else {
            assert!(result.is_err(), "{scenario}");
            assert_eq!(std::fs::read(&state_file).unwrap(), before, "{scenario}");
            assert_eq!(document(&state_file), original);
        }
        assert_eq!(std::fs::read(&plan_file).unwrap(), plan_before);
        assert_eq!(std::fs::read(images.join("seed.png")).unwrap(), foreign);
        if !scenario.ends_with("no-source") {
            assert_eq!(std::fs::read(images.join("aaa.png")).unwrap(), ours);
        }
    }
    fixture.clean().unwrap();
}
