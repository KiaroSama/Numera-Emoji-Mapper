//! Supplemental actual durable delete settlement variants.
use crate::{
    root::Root,
    scripted::{Reply, Server, TOKEN},
};
use _native::catalog::Catalog;
use numera_runtime as _native;
use serde_json::{Value, json};
use std::path::Path;
// Native supplemental: native/tests/plan_removal.rs::original_removal_restart_settles_applied_deletes_and_retains_unknown_without_resending
#[test]
fn original_removal_restart_settles_applied_deletes_and_retains_unknown_without_resending() {
    use _native::{plan_remove, telegram::Telegram};
    let root = Root::new("removal settlement café");
    for variant in ["applied", "not applied", "unknown", "flood"] {
        let directory = root.0.join(variant);
        let (mut state, plan) = fixture(&directory, true);
        let data = directory.join("collection");
        let journal_path = data.join("plan_apply_fixture.json");
        let mut journal = read(&journal_path);
        let before_state = std::fs::read(data.join("publish_fixture.json")).unwrap();
        let before_journal = std::fs::read(&journal_path).unwrap();
        let mut cat = Catalog::open(&data.join("catalog.db"), "fixture UTC", &directory).unwrap();
        let live = json!([{"custom_emoji_id":"111111111","file_id":"fixture-file","file_unique_id":"fixture-unique"}]);
        let mut deletion = Reply::new(
            "deleteStickerFromSet",
            &[("sticker", "fixture-file")],
            if variant == "flood" {
                json!({"ok":false,"error_code":429,
                "description":"Too Many Requests: retry after 900","parameters":{"retry_after":900}})
            } else {
                json!({"ok":true,"result":true})
            },
        );
        deletion.observation = Some((
            journal_path.clone(),
            "/intent/cid".into(),
            json!("111111111"),
        ));
        let replies = match variant {
            "applied" => vec![listing(json!([]))],
            "not applied" => vec![
                listing(live.clone()),
                listing(live),
                deletion,
                listing(json!([])),
            ],
            "unknown" => vec![Reply::new(
                "getStickerSet",
                &[("name", "fixture1_by_YourEmojiBot")],
                json!({"ok":true,"result":[]}),
            )],
            "flood" => vec![listing(live.clone()), listing(live), deletion],
            _ => unreachable!(),
        };
        let mut server = Server::new(replies);
        let mut tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        tg.max_flood_wait = Some(300);
        let settled =
            plan_remove::settle(&tg, &mut cat, &mut state, &mut journal, &data, "fixture");
        if variant == "unknown" {
            assert!(settled.unwrap_err().contains("live state unknown"));
            assert_eq!(std::fs::read(&journal_path).unwrap(), before_journal);
            assert_eq!(
                std::fs::read(data.join("publish_fixture.json")).unwrap(),
                before_state
            );
        } else {
            settled.unwrap();
            if variant != "applied" {
                assert!(journal["intent"].is_null());
                assert_eq!(journal["retired"], json!([]));
                let removed = plan_remove::remove(
                    &tg,
                    &mut cat,
                    &mut state,
                    &mut journal,
                    &data,
                    "fixture",
                    ("a", 1),
                );
                if variant == "flood" {
                    assert!(removed.unwrap_err().starts_with("FLOOD_WAIT "));
                    assert!(journal["intent"].is_null());
                    assert_eq!(journal["retired"], json!([]));
                    assert_eq!(
                        std::fs::read(data.join("publish_fixture.json")).unwrap(),
                        before_state
                    );
                } else {
                    removed.unwrap();
                }
            }
        }
        if variant == "flood" {
            assert!(read(&journal_path)["intent"].is_null());
            assert_eq!(read(&journal_path)["retired"], json!([]));
        }
        let applied = matches!(variant, "applied" | "not applied");
        if applied {
            assert_eq!(state["sets"][0]["keys"], json!([]));
            assert_eq!(state["sets"][0]["live"], 0);
            assert_eq!(journal["retired"].as_array().unwrap().len(), 1);
            assert_eq!(journal["retired"][0]["cid"], "111111111");
            assert_eq!(journal["retired"][0]["key"], "a");
            let before = std::fs::read(&journal_path).unwrap();
            plan_remove::settle(&tg, &mut cat, &mut state, &mut journal, &data, "fixture").unwrap();
            assert_eq!(std::fs::read(&journal_path).unwrap(), before);
        }
        assert_eq!(
            cat.dispatch(json!({"operation":"published","base":"fixture","content_key":"a"}))
                .unwrap(),
            !applied
        );
        assert_eq!(read(&data.join("pack_plan.json")), plan);
        let requests = server.finish().unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.method == "deleteStickerFromSet")
                .count(),
            usize::from(matches!(variant, "not applied" | "flood"))
        );
        assert!(requests.iter().all(|request| matches!(
            request.method.as_str(),
            "getStickerSet" | "deleteStickerFromSet"
        )));
        drop(tg);
        drop(cat);
        let mut reopened =
            Catalog::open(&data.join("catalog.db"), "fixture UTC", &directory).unwrap();
        assert_eq!(
            reopened
                .dispatch(json!({"operation":"published","base":"fixture","content_key":"a"}))
                .unwrap(),
            !applied
        );
        drop(reopened);
    }
    root.clean().unwrap();
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn fixture(directory: &Path, unknown: bool) -> (Value, Value) {
    std::fs::create_dir_all(directory.join("assets")).unwrap();
    std::fs::write(directory.join("assets/panel.html"), b"isolated fixture").unwrap();
    let data = directory.join("collection");
    let mut catalog = Catalog::open(
        &data.join("catalog.db"),
        "2026-10-07 00:00:00 UTC",
        directory,
    )
    .unwrap();
    assert!(
        catalog
            .insert(
                &json!({"content_key":"a","fmt":"static","file_path":directory.join("unused.png")}),
                "2026-10-07 00:00:00 UTC"
            )
            .unwrap()
    );
    catalog.uploaded(&json!({"content_key":"a","custom_emoji_id":"111111111","base":"fixture","set_name":"fixture1_by_YourEmojiBot"}), "2026-10-07 00:00:00 UTC").unwrap();
    drop(catalog);
    let state = json!({"base":"fixture","sets":[
        {"name":"fixture1_by_YourEmojiBot","title":"Fixture 1","fmt":"mixed","index":1,"keys":["a"],"live":1,"logo":false},
        {"name":"fixture2_by_YourEmojiBot","title":"Fixture 2","fmt":"mixed","index":2,"keys":[],"live":0,"logo":false}],
        "sent":[],"sent_full":[],"skipped":[]});
    let plan = json!({"version":1,"targets":[["a",2]],"known":["a"],"excluded":[],"moves":[],"held":[],"per_set":200});
    _native::atomic::write_json(&data.join("publish_fixture.json"), &state).unwrap();
    _native::atomic::write_json(&data.join("pack_plan.json"), &plan).unwrap();
    if unknown {
        _native::atomic::write_json(&data.join("plan_apply_fixture.json"), &json!({"version":1,"base":"fixture",
            "intent":{"op":"remove","key":"a","set":"fixture1_by_YourEmojiBot","cid":"111111111"},"retired":[]})).unwrap();
    }
    (state, plan)
}
fn listing(stickers: Value) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", "fixture1_by_YourEmojiBot")],
        json!({"ok":true,"result":{"stickers":stickers}}),
    )
}
