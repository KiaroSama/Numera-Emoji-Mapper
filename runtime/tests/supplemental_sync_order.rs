//! A family-specific publication remains reorderable after publishing another family.
#[allow(dead_code)]
#[path = "baseline_packs_support/http.rs"]
mod http;
#[path = "support/test_admission.rs"]
mod test_admission;

use numera_runtime::{catalog::Catalog, sync_order};
use serde_json::{Value, json};
use std::path::Path;

fn catalog(root: &Path) -> Catalog {
    let mut cat = Catalog::open(&root.join("catalog.db"), "fixture UTC", root).unwrap();
    for key in ["a", "b"] {
        let path = root.join(format!("{key}.png"));
        std::fs::write(&path, b"reorder uses recorded identities, not media bytes").unwrap();
        cat.insert(
            &json!({"content_key":format!("s:{key}"),"fmt":"static","file_path":path}),
            "fixture UTC",
        )
        .unwrap();
    }
    for base in ["first", "second"] {
        for key in ["a", "b"] {
            cat.uploaded(
                &json!({"base":base,"content_key":format!("s:{key}"),
                    "set_name":format!("{base}1_by_bot"),"custom_emoji_id":format!("{base}-{key}")}),
                "fixture UTC",
            )
            .unwrap();
        }
    }
    cat
}
fn bot(base: &str, foreign: bool) -> http::Bot {
    let name = format!("{base}1_by_bot");
    let first = format!("{base}-a");
    let mut ids = vec!["logo".into(), format!("{base}-b"), first];
    if foreign {
        ids[2] = "unrecorded".into();
    }
    http::Bot::new(move |request| match request.method.as_str() {
        "getStickerSet" => {
            assert_eq!(request.form["name"], name);
            http::ok(json!({"stickers":ids.iter().map(|id| json!({
                "custom_emoji_id":id,"file_id":format!("file-{id}"),"file_unique_id":format!("unique-{id}")
            })).collect::<Vec<_>>()}))
        }
        "setStickerPositionInSet" => {
            let id = request.form["sticker"].strip_prefix("file-").unwrap();
            let old = ids.iter().position(|entry| entry == id).unwrap();
            let moved = ids.remove(old);
            ids.insert(request.form["position"].parse().unwrap(), moved);
            http::ok(json!(true))
        }
        method => panic!("unexpected mutation: {method}"),
    })
}
fn record(base: &str) -> Value {
    json!({"name":format!("{base}1_by_bot"),"logo":true,"keys":["s:b","s:a"]})
}
fn ids(cat: &mut Catalog) -> Vec<Value> {
    ["first", "second"]
        .into_iter()
        .flat_map(|base| ["a", "b"].into_iter().map(move |key| (base, key)))
        .map(|(base, key)| {
            cat.dispatch(
                json!({"operation":"custom_id","base":base,"content_key":format!("s:{key}")}),
            )
            .unwrap()
        })
        .collect()
}

#[test]
fn reordering_uses_the_requested_sets_publication_ids() {
    for base in ["first", "second"] {
        let root = tempfile::tempdir().unwrap();
        let mut cat = catalog(root.path());
        let before = ids(&mut cat);
        let bot = bot(base, false);
        let mut record = record(base);
        let result = sync_order::sync(&bot.telegram(), &cat, &mut record, true);
        assert_eq!(result, Ok(1), "the {base} publication is still recorded");
        assert_eq!(record["keys"], json!(["s:a", "s:b"]));
        assert_eq!(ids(&mut cat), before);
        assert_eq!(
            cat.get("s:a").unwrap().unwrap()["custom_emoji_id"],
            "second-a"
        );
        assert_eq!(
            sync_order::sync(&bot.telegram(), &cat, &mut record, true),
            Ok(0)
        );
        let moves = bot
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.method == "setStickerPositionInSet")
            .map(|request| request.form.clone())
            .collect::<Vec<_>>();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0]["sticker"], format!("file-{base}-a"));
        assert_eq!(moves[0]["position"], "1");
    }
}

#[test]
fn reporting_an_earlier_family_keeps_state_and_live_order_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let mut cat = catalog(root.path());
    let before_ids = ids(&mut cat);
    let bot = bot("first", false);
    let mut record = record("first");
    let before_record = record.clone();
    assert_eq!(
        sync_order::sync(&bot.telegram(), &cat, &mut record, false),
        Ok(1)
    );
    assert_eq!(record, before_record);
    assert_eq!(ids(&mut cat), before_ids);
    assert_eq!(bot.methods(), ["getStickerSet"]);
}

#[test]
fn a_family_publication_does_not_authorize_reordering_unknown_live_members() {
    let root = tempfile::tempdir().unwrap();
    let cat = catalog(root.path());
    let bot = bot("first", true);
    let mut record = record("first");
    let before = record.clone();
    let result = sync_order::sync(&bot.telegram(), &cat, &mut record, true);
    assert!(
        result
            .as_ref()
            .is_err_and(|error| error.contains("does not know")),
        "{result:?}"
    );
    assert_eq!(record, before);
    assert_eq!(bot.methods(), ["getStickerSet"]);
}
