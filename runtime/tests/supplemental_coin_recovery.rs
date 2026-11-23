//! A shrunken live pack cannot prove that an interrupted append never landed.
#[allow(dead_code)]
#[path = "baseline_packs_support/http.rs"]
mod http;
#[path = "support/test_admission.rs"]
mod test_admission;

use numera_runtime::{atomic, coin_provider_recover, coin_provider_stage::Stage};
use serde_json::json;

const SET: &str = "fixture1_by_bot";

fn live_pack(count: usize) -> http::Bot {
    http::Bot::new(move |request| {
        assert_eq!(request.method, "getStickerSet");
        assert_eq!(request.form["name"], SET);
        http::ok(json!({"stickers":(0..count).map(|i| json!({
            "file_id":format!("file-{i}"),"file_unique_id":format!("unique-{i}"),
            "custom_emoji_id":format!("cid-{i}")
        })).collect::<Vec<_>>()}))
    })
}

fn art(path: &std::path::Path) {
    image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 30, 30, 255]))
        .save(path)
        .unwrap();
}

#[test]
fn provider_recovery_retains_intent_and_source_when_live_pack_shrank() {
    for count in [0, 1, 2] {
        let root = tempfile::tempdir().unwrap();
        let published = root.path().join("logos/emoji");
        let mut original_stage = Stage::new(&published).unwrap();
        let source = original_stage.source("aaa").unwrap();
        art(&source);
        original_stage.clean(Some(&source)).unwrap();
        let original_bytes = std::fs::read(&source).unwrap();
        let files = coin_provider_recover::Files {
            root: root.path().into(),
            state: root.path().join("state.json"),
            mapping: root.path().join("map.json"),
            published,
            base: "fixture".into(),
        };
        let state = json!({"sets":[{"index":1,"name":SET,"live":2}],
            "provider_added":[],"provider_in_flight":{"key":"aaa","operation":"add",
                "set_name":SET,"set_index":1,"expected_before":2,"source_dhash":0,
                "source_path":source}});
        atomic::write_json(&files.state, &state).unwrap();
        atomic::write_json(&files.mapping, &json!({"seed":"existing-id"})).unwrap();
        let state_before = std::fs::read(&files.state).unwrap();
        let map_before = std::fs::read(&files.mapping).unwrap();
        let bot = live_pack(count);
        let result = coin_provider_recover::run(&files, &bot.telegram());
        if count < 2 {
            assert!(result.is_err(), "live count {count}: {result:?}");
            assert_eq!(std::fs::read(&files.state).unwrap(), state_before);
        } else {
            assert_eq!(result.unwrap(), None);
            let saved: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&files.state).unwrap()).unwrap();
            assert!(saved["provider_in_flight"].is_null());
        }
        assert_eq!(std::fs::read(&files.mapping).unwrap(), map_before);
        assert_eq!(std::fs::read(&source).unwrap(), original_bytes);
        assert_eq!(bot.methods(), ["getStickerSet"]);
    }
}

#[test]
fn rebuild_recovery_keeps_cursor_and_intent_when_live_pack_shrank() {
    for count in [0, 1, 2] {
        let root = tempfile::tempdir().unwrap();
        let images = root.path().join("images");
        std::fs::create_dir(&images).unwrap();
        art(&images.join("aaa.png"));
        let files = numera_runtime::coin_rebuild_map_disk::Files {
            root: root.path().into(),
            directory: root.path().into(),
            images,
            base: "fixture".into(),
        };
        let path = root.path().join("rebuild_dedup_state.json");
        let state = json!({"sets":[{"index":1,"name":SET,"title":"Fixture 1","live":2}],
            "order":["seed-a","seed-b"],"cursor":3,"in_flight":{"key":"aaa",
                "operation":"add","set_name":SET,"set_index":1,"expected_before":2,
                "phase":"upload"}});
        let plan = json!([
            {"rep":"seed-a","tickers":["seed-a"],"kw":"seed-a"},
            {"rep":"seed-b","tickers":["seed-b"],"kw":"seed-b"},
            {"rep":"aaa","tickers":["aaa"],"kw":"aaa"}
        ]);
        atomic::write_json(&path, &state).unwrap();
        atomic::write_json(&root.path().join("rebuild_dedup_plan.json"), &plan).unwrap();
        let before = std::fs::read(&path).unwrap();
        let bot = live_pack(count);
        let result =
            numera_runtime::coin_rebuild_recovery_disk::run(&files, "Fixture", &bot.telegram());
        if count < 2 {
            assert!(result.is_err(), "live count {count}: {result:?}");
            assert_eq!(std::fs::read(&path).unwrap(), before);
        } else {
            let (saved, total) = result.unwrap();
            assert_eq!(total, 2);
            assert_eq!(saved["cursor"], 2);
            assert_eq!(saved["order"], json!(["seed-a", "seed-b"]));
            assert!(saved["in_flight"].is_null());
        }
        assert_eq!(bot.methods(), ["getStickerSet", "getStickerSet"]);
    }
}
