//! Recorded provider identity survives changed artwork, unknown reads and restarted run staging.
use _native::{
    coin_provider_recover::{Files, run},
    coin_provider_stage::Stage,
    logging::RunLog,
    telegram::Telegram,
};
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
fn listing(stickers: &[Value]) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", SET)],
        json!({"ok":true,"result":{"stickers":stickers}}),
    )
}
fn download(replies: &mut Vec<Reply>, bytes: &[u8]) {
    replies.push(Reply::new(
        "getFile",
        &[("file_id", "f3")],
        json!({"ok":true,"result":{"file_path":"original.png"}}),
    ));
    let mut reply = Reply::new("getFile", &[], Value::Null);
    reply.method = "GET original.png".into();
    reply.download = Some(bytes.to_vec());
    replies.push(reply);
}

#[test]
fn interrupted_provider_recovers_original_art_map_and_tally_without_mutation() {
    let root = tooling_fixture::Root::new("provider restart original identity");
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let mut art = Vec::new();
    for name in ["alpha-edge", "opaque"] {
        let row = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == name)
            .unwrap();
        let path = project.join(row["input"].as_str().unwrap());
        assert_eq!(
            _native::tooling_manifest::digest(&path).unwrap(),
            row["input_sha256"]
        );
        let bytes = std::fs::read(path).unwrap();
        let hash = row["expected"]["phash"].as_u64().unwrap();
        assert_eq!(_native::codec_image::provider_dhash(&bytes).unwrap(), hash);
        art.push((bytes, hash));
    }
    let (original, want) = &art[0];
    let (changed, different) = &art[1];
    assert_ne!(want, different);
    for scenario in [
        "recorded",
        "changed-published",
        "new-download",
        "missing-original",
        "not-landed",
        "unknown",
    ] {
        let directory = root.0.join(scenario);
        std::fs::create_dir(&directory).unwrap();
        let published = directory.join("logos/emoji");
        let mut old = Stage::new(&published).unwrap();
        let source = old.source("aaa").unwrap();
        std::fs::write(&source, original).unwrap();
        old.clean(Some(&source)).unwrap();
        let old_directory = old.directory().to_path_buf();
        drop(old);
        std::fs::create_dir(&published).unwrap();
        let destination = published.join("aaa.png");
        std::fs::write(
            &destination,
            if scenario == "changed-published" {
                changed
            } else {
                original
            },
        )
        .unwrap();
        let published_before = std::fs::read(&destination).unwrap();
        let files = Files {
            root: directory.clone(),
            state: directory.join("state.json"),
            mapping: directory.join("map.json"),
            published: published.clone(),
            base: "cryptoemoji".into(),
        };
        _native::atomic::write_json(&files.state,&json!({"sets":[{"index":1,"name":SET,"live":2}],
            "in_flight":null,"provider_in_flight":{"key":"aaa","operation":"add","set_name":SET,"set_index":1,
            "expected_before":2,"source_dhash":want,"source_path":source},"provider_added":[]})).unwrap();
        _native::atomic::write_json(&files.mapping, &json!({"btc":"c-btc","other":"preserved"}))
            .unwrap();
        let state_before = std::fs::read(&files.state).unwrap();
        let map_before = std::fs::read(&files.mapping).unwrap();
        let mut fresh = if matches!(scenario, "new-download" | "missing-original") {
            let stage = Stage::new(&published).unwrap();
            std::fs::write(stage.source("aaa").unwrap(), changed).unwrap();
            Some(stage)
        } else {
            None
        };
        if scenario == "missing-original" {
            std::fs::remove_file(&source).unwrap();
        }
        let mut replies = if scenario == "unknown" {
            vec![Reply::new(
                "getStickerSet",
                &[("name", SET)],
                json!({"ok":false,"description":"controlled unavailable"}),
            )]
        } else {
            let mut live = vec![
                json!({"custom_emoji_id":"old1"}),
                json!({"custom_emoji_id":"old2"}),
            ];
            if scenario != "not-landed" {
                live.push(json!({"file_id":"f3","file_unique_id":"u3","custom_emoji_id":"new"}));
            }
            vec![listing(&live)]
        };
        if !matches!(scenario, "unknown" | "not-landed") {
            download(&mut replies, original);
        }
        if scenario == "missing-original" {
            replies.insert(
                0,
                Reply::new(
                    "getMe",
                    &[],
                    json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
                ),
            );
        }
        let mut server = Server::new(replies);
        let log = RunLog::open(&directory, "provider-recovery", vec![bot_api::TOKEN.into()]);
        let tg = Telegram::new(bot_api::TOKEN.into(), server.base.clone())
            .unwrap()
            .with_log(&log);
        let result = if scenario == "missing-original" {
            // The original losing fallback belongs to the publisher's current run, not recovery's empty Stage.
            _native::coin_provider_effects::run(
                &files,
                &tg,
                fresh.as_mut().unwrap(),
                &["aaa".into()],
                "Title",
                1,
                &serde_json::Map::new(),
            )
            .map(|(added, failed, mapping)| {
                assert_eq!((added, failed), (1, 0));
                assert_eq!(document(&files.mapping), Value::Object(mapping));
                Some("aaa".into())
            })
        } else {
            run(&files, &tg)
        };
        if scenario == "unknown" {
            assert!(result.is_err());
            assert_eq!(std::fs::read(&files.state).unwrap(), state_before);
            assert_eq!(std::fs::read(&files.mapping).unwrap(), map_before);
            assert_eq!(
                std::fs::read(&source).unwrap().as_slice(),
                original.as_slice()
            );
            assert_eq!(std::fs::read(&destination).unwrap(), published_before);
        } else {
            assert_eq!(
                result.unwrap(),
                if scenario == "not-landed" {
                    None
                } else {
                    Some("aaa".into())
                }
            );
            let state = document(&files.state);
            assert!(state["provider_in_flight"].is_null());
            assert!(state["in_flight"].is_null());
            if scenario == "not-landed" {
                assert_eq!(std::fs::read(&files.mapping).unwrap(), map_before);
                assert_eq!(state["provider_added"], json!([]));
                assert_eq!(
                    std::fs::read(&source).unwrap().as_slice(),
                    original.as_slice()
                );
            } else {
                assert_eq!(
                    document(&files.mapping),
                    json!({"btc":"c-btc","other":"preserved","aaa":"new"})
                );
                assert_eq!(state["provider_added"], json!(["aaa"]));
                assert!(!source.exists());
                assert_eq!(
                    std::fs::read(&destination).unwrap().as_slice(),
                    original.as_slice()
                );
            }
            let state_bytes = std::fs::read(&files.state).unwrap();
            let map_bytes = std::fs::read(&files.mapping).unwrap();
            assert_eq!(run(&files, &tg).unwrap(), None);
            assert_eq!(std::fs::read(&files.state).unwrap(), state_bytes);
            assert_eq!(std::fs::read(&files.mapping).unwrap(), map_bytes);
        }
        let requests = server.finish().unwrap();
        assert_eq!(
            requests.len(),
            if matches!(scenario, "unknown" | "not-landed") {
                1
            } else if scenario == "missing-original" {
                4
            } else {
                3
            }
        );
        assert!(
            requests
                .iter()
                .all(|r| !matches!(r.method.as_str(), "addStickerToSet" | "createNewStickerSet"))
        );
        if let Some(stage) = fresh.as_mut() {
            if scenario == "missing-original" {
                assert!(!stage.directory().exists());
                assert_eq!(std::fs::read(&destination).unwrap(), published_before);
            } else {
                assert_eq!(
                    std::fs::read(stage.source("aaa").unwrap())
                        .unwrap()
                        .as_slice(),
                    changed.as_slice()
                );
                stage.clean(None).unwrap();
            }
        }
        log.finish(if scenario == "unknown" { 3 } else { 0 });
        let path = log.path.clone().unwrap();
        drop(tg);
        drop(log);
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            text.contains("was NOT updated"),
            scenario == "missing-original"
        );
        if scenario == "missing-original" {
            assert!(text.contains("[WARNING] [coins]"));
            assert!(text.contains("Re-fetch this ticker"));
        }
        assert!(!text.contains(bot_api::TOKEN));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        drop(file);
        let incoming = published.parent().unwrap().join(".incoming");
        let directories = std::fs::read_dir(&incoming)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(directories, [old_directory.clone()]);
        assert_eq!(
            std::fs::read_dir(&old_directory).unwrap().count(),
            usize::from(matches!(scenario, "unknown" | "not-landed"))
        );
        for path in [
            _native::locks::pack_family_path(&directory, "cryptoemoji"),
            directory.join(".locks/canonical_map.lock"),
        ] {
            assert!(path.is_file());
            drop(_native::locks::Lease::acquire(&path, "recovery release").unwrap());
        }
        assert!(!_native::process_owner::cleanup_pending(&directory));
    }
    root.clean().unwrap();
}
