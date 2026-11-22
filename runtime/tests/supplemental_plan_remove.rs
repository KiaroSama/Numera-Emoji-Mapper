//! Malformed live identity evidence cannot commit a planned removal.
#[allow(dead_code)]
#[path = "baseline_packs_support/http.rs"]
mod http;
#[path = "support/test_admission.rs"]
mod test_admission;

use numera_runtime::{atomic, catalog::Catalog, plan_remove};
use serde_json::{Value, json};

const KEY: &str = "s:owned-fixture";
const ID: &str = "123456789";
const BASE: &str = "fixture";
const NAME: &str = "fixture1_by_bot";

fn recorded() -> Value {
    json!({"base":BASE,"sets":[{"index":1,"name":NAME,"fmt":"mixed",
        "title":"Owned fixture","keys":[KEY],"logo":false,"live":1}]})
}

fn intent() -> Value {
    json!({"version":1,"base":BASE,"retired":[],
        "intent":{"op":"remove","key":KEY,"set":NAME,"cid":ID,"at":"fixture"}})
}

fn malformed() -> Vec<Value> {
    vec![
        json!([{"file_id":"unknown"}]),
        json!([{"custom_emoji_id":""}]),
        json!([{"custom_emoji_id":123456789}]),
        json!([null]),
        json!([{"custom_emoji_id":"other"},{"custom_emoji_id":"other"}]),
    ]
}

fn catalog(root: &std::path::Path) -> Catalog {
    let mut cat = Catalog::open(&root.join("catalog.db"), "fixture", root).unwrap();
    cat.insert(
        &json!({"content_key":KEY,"fmt":"static","file_path":root.join("unused.png")}),
        "fixture",
    )
    .unwrap();
    cat.uploaded(
        &json!({"content_key":KEY,"custom_emoji_id":ID,"base":BASE,"set_name":NAME}),
        "fixture",
    )
    .unwrap();
    cat
}

#[test]
fn recovery_retains_removal_intent_when_live_ids_are_incomplete() {
    for stickers in malformed() {
        let bot = http::Bot::new(move |request| {
            assert_eq!(request.method, "getStickerSet");
            http::ok(json!({"stickers":stickers}))
        });
        let root = tempfile::tempdir().unwrap();
        let mut cat = catalog(root.path());
        let mut state = recorded();
        let mut journal = intent();
        let state_path = root.path().join("publish_fixture.json");
        let journal_path = root.path().join("plan_apply_fixture.json");
        atomic::write_json(&state_path, &state).unwrap();
        atomic::write_json(&journal_path, &journal).unwrap();
        let before_state = std::fs::read(&state_path).unwrap();
        let before_journal = std::fs::read(&journal_path).unwrap();
        let result = plan_remove::settle(
            &bot.telegram(),
            &mut cat,
            &mut state,
            &mut journal,
            root.path(),
            BASE,
        );
        assert!(
            result.is_err(),
            "incomplete identity list cannot prove absence"
        );
        assert_eq!(state, recorded());
        assert_eq!(journal, intent());
        assert_eq!(std::fs::read(state_path).unwrap(), before_state);
        assert_eq!(std::fs::read(journal_path).unwrap(), before_journal);
        assert_eq!(
            cat.dispatch(json!({"operation":"custom_id","base":BASE,"content_key":KEY}))
                .unwrap(),
            ID
        );
        assert_eq!(bot.methods(), ["getStickerSet"]);
    }
}

#[test]
fn answered_delete_retains_intent_until_live_identity_absence_is_proven() {
    for stickers in malformed() {
        let mut probes = 0;
        let bot = http::Bot::new(move |request| match request.method.as_str() {
            "getStickerSet" => {
                probes += 1;
                http::ok(json!({"stickers":if probes == 1 {
                    json!([{"custom_emoji_id":ID,"file_id":"owned-file"}])
                } else { stickers.clone() }}))
            }
            "deleteStickerFromSet" => {
                assert_eq!(request.form["sticker"], "owned-file");
                http::ok(json!(true))
            }
            method => panic!("unexpected fixture method {method}"),
        });
        let root = tempfile::tempdir().unwrap();
        let mut cat = catalog(root.path());
        let mut state = recorded();
        let mut journal = intent();
        journal["intent"] = Value::Null;
        let result = plan_remove::remove(
            &bot.telegram(),
            &mut cat,
            &mut state,
            &mut journal,
            root.path(),
            BASE,
            (KEY, 1),
        );
        assert!(result.is_err(), "malformed readback cannot confirm removal");
        assert_eq!(state, recorded());
        assert_eq!(journal["intent"]["cid"], ID);
        assert_eq!(journal["retired"], json!([]));
        assert_eq!(
            cat.dispatch(json!({"operation":"custom_id","base":BASE,"content_key":KEY}))
                .unwrap(),
            ID
        );
        assert_eq!(
            bot.methods(),
            ["getStickerSet", "deleteStickerFromSet", "getStickerSet"]
        );
    }
}

#[test]
fn complete_empty_live_set_still_proves_recovered_removal() {
    let bot = http::Bot::new(|request| {
        assert_eq!(request.method, "getStickerSet");
        http::ok(json!({"stickers":[]}))
    });
    let root = tempfile::tempdir().unwrap();
    let mut cat = catalog(root.path());
    let mut state = recorded();
    let mut journal = intent();
    plan_remove::settle(
        &bot.telegram(),
        &mut cat,
        &mut state,
        &mut journal,
        root.path(),
        BASE,
    )
    .unwrap();
    assert_eq!(state["sets"][0]["keys"], json!([]));
    assert_eq!(state["sets"][0]["live"], 0);
    assert!(journal["intent"].is_null());
    assert_eq!(journal["retired"][0]["cid"], ID);
    assert_eq!(
        cat.dispatch(json!({"operation":"published","base":BASE,"content_key":KEY}))
            .unwrap(),
        false
    );
}
