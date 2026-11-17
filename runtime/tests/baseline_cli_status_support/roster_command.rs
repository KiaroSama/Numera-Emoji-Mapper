use crate::support::{
    http::{Reply, Server},
    *,
};
use rusqlite::Connection;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
// tests.test_native_roster_cli.NativeRosterCLI.test_live_refresh_family_preservation_and_read_failure
#[test]
fn actual_roster_refresh_preserves_families_history_and_fail_closed_output() {
    let f = Fixture::new();
    let general = "fixture_by_YourEmojiBot";
    let coin = "coins_by_YourEmojiBot";
    document(
        &f.data().join("publish_fixture.json"),
        &json!({"sets":[{"name":general,"title":"Fixture <&>","index":1,"logo":false}]}),
    );
    let coin_state = f.root.join("coins/rebuild_dedup_state.json");
    document(
        &coin_state,
        &json!({"sets":[{"name":coin,"title":"Coins","index":1}]}),
    );
    let map = f.root.join("coins/ticker_to_id.json");
    document(&map, &json!({"bbb":"222222222","aaa":"222222222"}));
    let database = f.data().join("catalog.db");
    let db = Connection::open(&database).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT,keywords TEXT,file_path TEXT);CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);INSERT INTO items VALUES('s:fixture','[\"fixture\",\"premium-id:444444444\",\"premium-id:111111111\"]','missing.png');INSERT INTO publications VALUES('s:fixture','111111111');").unwrap();
    let live = Arc::new(Mutex::new(BTreeMap::from([
        (
            general.to_string(),
            json!([{"custom_emoji_id":"111111111","emoji":"😀"}]),
        ),
        (
            coin.to_string(),
            json!([{"custom_emoji_id":"222222222","emoji":"⭐"}]),
        ),
    ])));
    let failed = Arc::new(Mutex::new(BTreeSet::new()));
    let served = live.clone();
    let failure = failed.clone();
    let bot = Server::new(move |r| {
        assert_eq!(r.method(), "getStickerSet");
        let name = &r.form()["name"];
        if failure.lock().unwrap().contains(name) {
            Reply::json(json!({"ok":false,"description":"STICKERSET_INVALID"}))
        } else {
            Reply::ok(json!({"title":"ignored-live-title","stickers":served.lock().unwrap()[name]}))
        }
    });
    let mut env = bot.environment();
    env.extend([
        ("GENERAL_BOT_TOKEN", "test-general-token".into()),
        ("TELEGRAM_BOT_TOKEN", "test-coin-token".into()),
        ("BRAND_LOGO_PATH", "".into()),
    ]);
    let run = |args: &[&str], env: &[(&str, String)]| {
        let mut command = vec!["pack-manifest"];
        command.extend_from_slice(args);
        f.run(&command, env)
    };
    run(&["--refresh"], &env).code(0);
    let out = f.root.join("packs");
    let index = read_json(&out.join("index.json"));
    assert_eq!(index["pack_count"], 2);
    assert_eq!(index["emoji_count"], 2);
    assert_eq!(index["by_current_id"]["111111111"]["slot"], 1);
    let coin_path = out.join(format!("{coin}.json"));
    let general_path = out.join(format!("{general}.json"));
    assert_eq!(read_json(&coin_path)["emoji"][0]["name"], "aaa, bbb");
    let doc = read_json(&general_path);
    assert_eq!(doc["emoji"][0]["role"], "emoji");
    assert_eq!(doc["title"], "Fixture <&>");
    let html = std::fs::read_to_string(out.join(format!("{general}.html"))).unwrap();
    for id in ["bg", "animLabel", "roster"] {
        assert!(html.contains(&format!("id=\"{id}\"")));
    }
    assert_eq!(bot.methods(), ["getStickerSet", "getStickerSet"]);
    run(&["--check"], &env).code(0);
    let coin_bytes = std::fs::read(&coin_path).unwrap();
    let recorded = index["inputs"]["rebuild_dedup_state.json"].clone();
    let mut bytes = std::fs::read(&coin_state).unwrap();
    bytes.push(b' ');
    write(&coin_state, bytes);
    live.lock().unwrap().insert(
        general.into(),
        json!([{"custom_emoji_id":"333333333","emoji":"😀"}]),
    );
    db.execute("UPDATE publications SET custom_emoji_id='333333333'", [])
        .unwrap();
    run(&["--refresh", "--family", "general"], &env).code(0);
    let index = read_json(&out.join("index.json"));
    assert_eq!(std::fs::read(&coin_path).unwrap(), coin_bytes);
    assert_eq!(index["inputs"]["rebuild_dedup_state.json"], recorded);
    assert_eq!(index["by_current_id"]["222222222"]["set"], coin);
    assert_eq!(index["by_previous_id"]["111111111"], "333333333");
    run(&["--check"], &env).code(3);
    let before = snapshot(&out);
    failed.lock().unwrap().insert(coin.to_owned());
    let refused = run(&["--refresh"], &env);
    assert_ne!(refused.code, 0);
    assert_eq!(snapshot(&out), before);
    assert!(
        !bot.methods()
            .iter()
            .any(|m| ["addStickerToSet", "createNewStickerSet"].contains(&m.as_str()))
    );
    failed.lock().unwrap().clear();
    let mut coin_only = env.clone();
    coin_only.push(("COLLECTION_PACK_BASE", "".into()));
    write(&out.join("index.json"), b"{broken");
    document(&map, &json!({"aaa":222222222,"bbb":"222222222"}));
    run(&["--refresh", "--family", "coins"], &coin_only).code(0);
    assert_eq!(read_json(&coin_path)["emoji"][0]["name"], "aaa, bbb");
    write(&map, b"{broken");
    run(&["--refresh", "--family", "general"], &env).code(0);
    run(&["--refresh"], &env).code(0);
    let mut index = read_json(&out.join("index.json"));
    index["inputs"]["publish_old.json"] = json!([0, 0]);
    document(&out.join("index.json"), &index);
    run(&["--check"], &env).code(3);
}
