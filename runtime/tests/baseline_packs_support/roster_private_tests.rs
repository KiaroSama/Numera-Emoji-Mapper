use super::*;
use crate::baseline_packs_support::*;
fn native_media(
    root: &std::path::Path,
    scratch: std::path::PathBuf,
) -> Result<NativeMediaAdapter, String> {
    Ok(NativeMediaAdapter {
        executable: binary(),
        root: root.into(),
        scratch,
    })
}
fn refreshed() -> Value {
    let r = Root::new();
    let old: Value = serde_json::from_str(OLD).unwrap();
    r.write("packs/index.json", old.to_string());
    r.write(
        "coins/rebuild_dedup_state.json",
        json!({"sets":[{"name":"coin1_by_bot"}]}).to_string(),
    );
    let c = r.config(&[
        ("COLLECTION_PACK_BASE", "state"),
        ("TELEGRAM_BOT_TOKEN", http::TOKEN),
    ]);
    let b = http::Bot::new(|q| {
        assert_eq!(q.method, "getStickerSet");
        assert_eq!(q.form["name"], "coin1_by_bot");
        http::ok(json!({"stickers":[]}))
    });
    let log = RunLog::open(&r.0, "one-family", vec![]);
    let code=run_with(&c,&["--refresh".into(),"--family".into(),"coins".into()],&log,|token|{assert_eq!(token,http::TOKEN);Ok(b.telegram())},|rec,_,fam,_,_,_|Ok(json!({"set_name":rec["name"],"title":"C 1","family":fam,"pack_index":1,"link":"l","captured_utc":UTC,"count":1,"emoji":[{"custom_emoji_id":"c-new","slot":0,"index":0,"source_emoji_ids":["src-c"],"previous_custom_emoji_ids":[],"format":"static"}]})),|_|Ok(serde_json::from_str(NOW).unwrap()),native_media).unwrap();
    assert_eq!(code, 0);
    assert_eq!(b.methods(), vec!["getStickerSet"]);
    crate::baseline_packs_support::read(&r.0.join("packs/index.json"))
}
const OLD: &str = r#"{"packs": [{"set_name": "gen1_by_bot", "title": "G 1", "family": "general", "pack_index": 1, "link": "l", "count": 1}, {"set_name": "coin1_by_bot", "title": "C 1", "family": "coins", "pack_index": 1, "link": "l", "count": 1}], "by_current_id": {"g1": {"set": "gen1_by_bot", "slot": 0, "index": 0}, "c-old": {"set": "coin1_by_bot", "slot": 0, "index": 0}}, "by_source_id": {"src-g": "g1", "src-c": "c-old"}, "by_previous_id": {"g0": "g1"}, "inputs": {"catalog.db": [1, 1], "publish_state.json": [2, 2], "rebuild_dedup_state.json": [3, 3], "ticker_to_id.json": [4, 4]}}"#;
const NOW: &str = r#"{"catalog.db": [9, 9], "publish_state.json": [9, 9], "rebuild_dedup_state.json": [5, 5], "ticker_to_id.json": [6, 6]}"#;

// Original: tests.test_pack_manifest.AOneFamilyRefreshKeepsTheOther.test_the_general_family_and_its_lookups_survive
#[test]
#[allow(non_snake_case)]
fn pack_manifest__AOneFamilyRefreshKeepsTheOther__test_the_general_family_and_its_lookups_survive()
{
    let i = refreshed();
    let mut packs = i["packs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["set_name"].as_str().unwrap())
        .collect::<Vec<_>>();
    packs.sort();
    assert_eq!(packs, vec!["coin1_by_bot", "gen1_by_bot"]);
    assert!(i["by_current_id"].get("g1").is_some());
    assert!(i["by_current_id"].get("c-new").is_some());
    assert!(i["by_current_id"].get("c-old").is_none());
    assert_eq!(i["by_source_id"], json!({"src-g":"g1","src-c":"c-new"}));
    assert_eq!(i["by_previous_id"], json!({"g0":"g1"}));
    assert_eq!(i["pack_count"], 2);
}

// Original: tests.test_pack_manifest.AOneFamilyRefreshKeepsTheOther.test_the_other_familys_inputs_keep_their_old_fingerprint
#[test]
#[allow(non_snake_case)]
fn pack_manifest__AOneFamilyRefreshKeepsTheOther__test_the_other_familys_inputs_keep_their_old_fingerprint()
 {
    let i = refreshed();
    assert_eq!(i["inputs"]["catalog.db"], json!([1, 1]));
    assert_eq!(i["inputs"]["publish_state.json"], json!([2, 2]));
    assert_eq!(i["inputs"]["rebuild_dedup_state.json"], json!([5, 5]));
}

// Original: tests.test_pack_manifest.AOneFamilyRefreshKeepsTheOther.test_check_with_a_family_is_a_usage_error
#[test]
#[allow(non_snake_case)]
fn pack_manifest__AOneFamilyRefreshKeepsTheOther__test_check_with_a_family_is_a_usage_error() {
    let r = Root::new();
    let c = r.config(&[]);
    let l = RunLog::open(&r.0, "bad-family-check", vec![]);
    let e = run_with(
        &c,
        &["--check".into(), "--family".into(), "coins".into()],
        &l,
        |_| panic!("invalid family check must precede Telegram"),
        crate::roster::pack,
        fingerprint,
        native_media,
    )
    .unwrap_err();
    assert!(e.contains("--check covers all families"));
}
