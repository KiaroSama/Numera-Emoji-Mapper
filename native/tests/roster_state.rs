//! Original roster assertions use public identity, provenance, page and freshness seams.
use _native::{config::Config, roster};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
#[path = "support/bounded_media.rs"]
mod bounded_media;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

fn pack(id: &str, prior: &Value) -> Value {
    roster::pack(
        &json!({"name":"s","index":1,"logo":true}),
        &json!({"stickers":[{"custom_emoji_id":"logo","emoji":"✅"},{"custom_emoji_id":id,"emoji":"🔒"}]}),
        "general", &json!({(id):{"content_key":"k","name":"lock","source_emoji_ids":["999"]}}),
        prior, "2026-10-07 00:00:00 UTC",
    ).unwrap()
}
fn config(root: &Path) -> Config {
    Config::load_with_environment(
        root.into(),
        &BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("COLLECTION_PACK_BASE".into(), "mypacks".into()),
        ]),
    )
    .unwrap()
}

#[test]
fn original_live_ids_brand_exemption_and_retired_history_follow_content_not_slots() {
    let first = pack("111", &json!({}));
    let logo = &first["emoji"][0];
    assert_eq!(logo["custom_emoji_id"], "logo");
    assert_eq!(logo["index"], 0);
    assert_eq!(logo["slot"], 1);
    assert_eq!(logo["role"], "brand-logo");
    assert_eq!(logo["history_key"], "logo:s");
    assert_eq!(first["emoji"][1]["history_key"], "ck:k");
    let prior = json!(_native::roster_provenance::history(&[first]));
    let second = pack("222", &prior);
    let row = &second["emoji"][1];
    assert_eq!(row["source_emoji_ids"], json!(["999"]));
    assert_eq!(row["previous_custom_emoji_ids"], json!(["111"]));
    assert_eq!(row["custom_emoji_id"], "222");
    let prior = json!(_native::roster_provenance::history(std::slice::from_ref(
        &second
    )));
    let third = pack("333", &prior);
    assert_eq!(
        third["emoji"][1]["previous_custom_emoji_ids"],
        json!(["111", "222"])
    );
    let repeated = pack(
        "333",
        &json!(_native::roster_provenance::history(std::slice::from_ref(
            &third
        ))),
    );
    assert_eq!(
        repeated["emoji"][1]["previous_custom_emoji_ids"],
        json!(["111", "222"])
    );
    let unchanged = pack("111", &json!({"ck:k":{"id":"111","history":[]}}));
    assert_eq!(
        unchanged["emoji"][1]["previous_custom_emoji_ids"],
        json!([])
    );
    let coins = roster::pack(
        &json!({"name":"c","index":1}),
        &json!({"stickers":[{"custom_emoji_id":"111","emoji":"₿"}]}),
        "coins",
        &json!({"111":["btc","xbt"]}),
        &json!({}),
        "2026-10-07 00:00:00 UTC",
    )
    .unwrap();
    assert_eq!(coins["emoji"][0]["role"], "emoji");
    assert_eq!(coins["emoji"][0]["name"], "btc, xbt");
    assert_eq!(coins["emoji"][0]["history_key"], "coin:btc,xbt");
    let single = roster::pack(
        &json!({"name":"c","index":1}),
        &json!({"stickers":[{"custom_emoji_id":"111"}]}),
        "coins",
        &json!({"111":["btc"]}),
        &json!({}),
        "fixture UTC",
    )
    .unwrap();
    assert_eq!(single["emoji"][0]["role"], "emoji");
    assert_eq!(single["emoji"][0]["name"], "btc");
    let reordered = roster::pack(&json!({"name":"s","logo":true}),
        &json!({"stickers":[{"custom_emoji_id":"logo"},{"custom_emoji_id":"444"},{"custom_emoji_id":"333"}]}),
        "general", &json!({"444":{"content_key":"other"},"333":{"content_key":"k","source_emoji_ids":["999"]}}),
        &json!(_native::roster_provenance::history(&[third])), "2026-10-07 00:00:00 UTC").unwrap();
    assert_eq!(reordered["emoji"][2]["history_key"], "ck:k");
    assert_eq!(
        reordered["emoji"][2]["previous_custom_emoji_ids"],
        json!(["111", "222"])
    );
    assert_eq!(reordered["emoji"][2]["custom_emoji_id"], "333");
    assert_eq!(second["emoji"][1]["custom_emoji_id"], "222");
}

#[test]
fn original_provenance_excludes_our_live_ids_and_preserves_every_shared_ticker() {
    let root = Root::new("roster provenance café");
    let path = root.0.join("catalog.db");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT,keywords TEXT,emojis TEXT,format TEXT);
        CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);
        INSERT INTO items VALUES('k','[\"premium-id:999\",\"premium-id:555\",\"lock\"]','[]','static');
        INSERT INTO publications VALUES('k','555');").unwrap();
    db.close().unwrap();
    let before = std::fs::read(&path).unwrap();
    let provenance =
        _native::roster_provenance::general(&path, &BTreeSet::from(["555".into()])).unwrap();
    assert_eq!(provenance["555"]["source_emoji_ids"], json!(["999"]));
    assert_eq!(provenance["555"]["name"], "lock");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let coins = _native::roster_provenance::coins(&json!({"btc":"1","xbt":"1","eth":"2"}));
    assert_eq!(coins["1"], json!(["btc", "xbt"]));
    assert_eq!(coins["2"], json!(["eth"]));
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    root.clean().unwrap();
}

#[test]
fn original_page_keeps_machine_data_copyable_ids_view_only_controls_and_frozen_animation() {
    let mut doc = pack("222", &json!({"ck:k":{"id":"111","history":[]}}));
    let original = doc.clone();
    doc["emoji"][1]["name"] = json!("</script><b>x & \"quoted\"\u{2028}\u{2029}");
    let thumbnails = BTreeMap::from([(
        "222".into(),
        (
            "anim".into(),
            "data:image/webp;base64,YW5pbQ==".into(),
            Some("data:image/webp;base64,c3RpbGw=".into()),
        ),
    )]);
    let page = _native::pack_gallery::render_page(&doc, &thumbnails).unwrap();
    let block = page
        .split("id=\"roster\">")
        .nth(1)
        .unwrap()
        .split("</script>")
        .next()
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(block).unwrap(), doc);
    assert!(!block.contains(['<', '>', '\u{2028}', '\u{2029}']));
    assert!(!page.contains("</script><b>x"));
    assert!(page.contains("&lt;/script&gt;&lt;b&gt;x &amp; &quot;quoted&quot;"));
    for id in ["222", "999", "111"] {
        assert!(page.contains(&format!("data-id=\"{id}\"")));
    }
    assert!(
        page.contains("this pack, before")
            && page.contains("class=\"glyph\"")
            && page.contains("🔒")
    );
    assert!(page.contains("data-custom-emoji-id=\"logo\"") && page.contains("data-index=\"0\""));
    let body = page.split("</style>").nth(1).unwrap();
    for mutation in [
        "draggable",
        "/api/save",
        "/api/order",
        "class=\"tick\"",
        "Save selection",
        "contenteditable",
    ] {
        assert!(!body.contains(mutation), "{mutation}");
    }
    for control in ["top", "bot", "bg", "anim"] {
        assert!(page.contains(&format!("id=\"{control}\"")));
    }
    assert!(page.contains("bg-checker"));
    assert!(page.contains(
        "src=\"data:image/webp;base64,c3RpbGw=\" data-still=\"data:image/webp;base64,c3RpbGw=\""
    ));
    assert!(page.contains("freezeAll()") && page.contains("visibilitychange"));
    assert!(page.contains("emoji 0 is the brand logo"));
    let markdown = _native::roster_markdown::pack(&original).unwrap();
    assert!(markdown.contains("emoji 0 is the brand logo"));
    let coins = roster::pack(
        &json!({"name":"c","index":1}),
        &json!({"stickers":[{"custom_emoji_id":"111"}]}),
        "coins",
        &json!({}),
        &json!({}),
        "2026-10-07 00:00:00 UTC",
    )
    .unwrap();
    assert!(
        _native::roster_markdown::pack(&coins)
            .unwrap()
            .contains("no brand logo")
    );
    assert!(
        _native::pack_gallery::render_page(&coins, &BTreeMap::new())
            .unwrap()
            .contains("no brand logo")
    );
}

#[test]
fn original_freshness_refuses_missing_changed_and_unreadable_rosters_without_writes() {
    let root = Root::new("roster freshness café");
    let config = config(&root.0);
    let (stale, reason) = _native::roster_cli::stale(&config).unwrap();
    assert!(stale && reason.contains("index.json"));
    assert!(!root.0.join("packs").exists());
    std::fs::create_dir(root.0.join("packs")).unwrap();
    std::fs::create_dir(root.0.join("collection")).unwrap();
    let catalog = root.0.join("collection/catalog.db");
    std::fs::write(&catalog, b"one").unwrap();
    let path = root.0.join("packs/index.json");
    _native::atomic::write_json(
        &path,
        &json!({"inputs":_native::roster_cli::fingerprint(&config).unwrap()}),
    )
    .unwrap();
    let recorded = std::fs::read(&path).unwrap();
    assert!(!_native::roster_cli::stale(&config).unwrap().0);
    std::fs::write(&catalog, b"two but longer").unwrap();
    let (stale, reason) = _native::roster_cli::stale(&config).unwrap();
    assert!(stale && reason.contains("catalog.db"));
    assert_eq!(std::fs::read(&path).unwrap(), recorded);
    std::fs::write(&path, b"{not json").unwrap();
    assert!(_native::roster_cli::stale(&config).unwrap().0);
    assert_eq!(std::fs::read(path).unwrap(), b"{not json");
    assert_eq!(std::fs::read(catalog).unwrap(), b"two but longer");
    root.clean().unwrap();
}

#[test]
fn original_all_format_gallery_embeds_real_bytes_and_reuses_cache_without_touching_inputs() {
    use _native::media_native_adapter::NativeMediaAdapter;
    use std::time::{Duration, Instant};
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let root = Root::new("roster thumbnails café");
    let native = NativeMediaAdapter {
        executable: std::path::PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
        root: project.into(),
        scratch: root.0.join("envelopes"),
    };
    let media = bounded_media::BoundedCodec {
        native,
        deadline: Instant::now() + Duration::from_secs(60),
    };
    let (preview, committed) = _native::source_lottie_render::fixtures().unwrap();
    let fixture: Value = std::env::var_os("NUMERA_GALLERY_CODEC_ORACLE")
        .map(std::path::PathBuf::from)
        .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap())
        .unwrap_or(committed);
    _native::source_lottie_render::fixture_identity(&preview, &fixture).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    let mut art = BTreeMap::new();
    let mut originals = Vec::new();
    for (index, id, extension) in [(0, "111", "png"), (2, "222", "json"), (4, "333", "webm")] {
        let case = &cases[index];
        let source = project.join(case["input"].as_str().unwrap());
        _native::tooling_environment::reject_links(&source, "gallery fixture cannot follow links")
            .unwrap();
        let bytes = std::fs::read(&source).unwrap();
        assert_eq!(
            _native::tooling_manifest::digest(&source).unwrap(),
            case["input_sha256"]
        );
        let local = root.0.join(format!("{id}.{extension}"));
        std::fs::write(&local, &bytes).unwrap();
        art.insert(id.into(), local.clone());
        originals.push((source, local, bytes));
    }
    let doc = roster::pack(&json!({"name":"s","index":1,"logo":false}),
        &json!({"stickers":[{"custom_emoji_id":"111"},{"custom_emoji_id":"222","is_animated":true},{"custom_emoji_id":"333","is_video":true}]}),
        "general", &json!({}), &json!({}), "2026-10-07 00:00:00 UTC").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let cache = root.0.join("thumbs");
    let page = runtime
        .block_on(_native::pack_gallery::render(&doc, &media, &art, &cache))
        .unwrap();
    assert!(page.contains("data:image/webp;base64,") && page.contains("data:video/webm;base64,"));
    assert!(!page.contains("no preview</div>"));
    let expected = [
        ("111.webp", 0),
        ("222.webp", 2),
        ("222_still.webp", 3),
        ("333.webm", 4),
    ];
    let mut before = BTreeMap::new();
    for (name, index) in expected {
        let path = cache.join(name);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len() as u64, cases[index]["bytes"].as_u64().unwrap());
        assert_eq!(
            _native::tooling_manifest::digest(&path).unwrap(),
            cases[index]["sha256"]
        );
        before.insert(name, (bytes, path.metadata().unwrap().modified().unwrap()));
    }
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 4);
    assert!(
        page.contains("src=\"data:image/webp;base64,")
            && page.contains("data-still=\"data:image/webp;base64,")
    );
    let again = runtime
        .block_on(_native::pack_gallery::render(&doc, &media, &art, &cache))
        .unwrap();
    assert_eq!(page, again);
    for (name, (bytes, modified)) in before {
        let path = cache.join(name);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(path.metadata().unwrap().modified().unwrap(), modified);
    }
    for (source, local, bytes) in originals {
        assert_eq!(std::fs::read(source).unwrap(), bytes);
        assert_eq!(std::fs::read(local).unwrap(), bytes);
    }
    let missing_cache = root.0.join("missing-cache");
    let missing = BTreeMap::from([("111".into(), root.0.join("absent.png"))]);
    let page = runtime
        .block_on(_native::pack_gallery::render(
            &doc,
            &media,
            &missing,
            &missing_cache,
        ))
        .unwrap();
    assert!(page.contains("no preview</div>"));
    assert!(!missing_cache.exists());
    assert!(!media.native.scratch.exists());
    assert_eq!(std::fs::read_dir(cache).unwrap().count(), 4);
    runtime.shutdown_timeout(Duration::from_secs(1));
    root.clean().unwrap();
}

#[test]
fn original_roster_sources_refuse_before_runtime_or_workspace_access() {
    use _native::source_git_inventory::Source;
    let absent = std::path::PathBuf::from("must-not-be-inspected");
    let source = Source {
        baseline_revision: "0".repeat(40),
        path: "unverified.py".into(),
        sha256: "0".repeat(64),
        text: "raise AssertionError('must not execute')".into(),
    };
    assert!(
        _native::source_roster_tests::observe(&absent, &absent, &[&source; 18])
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(_native::source_roster_tests::source_identity(&[]).is_err());
}

#[test]
fn original_family_merge_preserves_other_lookups_and_old_input_fingerprints() {
    let old = json!({"packs":[{"set_name":"gen1_by_bot","family":"general","count":1},{"set_name":"coin1_by_bot","family":"coins","count":1}],
        "by_current_id":{"g1":{"set":"gen1_by_bot","slot":0,"index":0},"c-old":{"set":"coin1_by_bot","slot":0,"index":0}},
        "by_source_id":{"src-g":"g1","src-c":"c-old"},"by_previous_id":{"g0":"g1"},
        "inputs":{"catalog.db":[1,1],"publish_state.json":[2,2],"rebuild_dedup_state.json":[3,3],"ticker_to_id.json":[4,4]}});
    let inputs = json!({"catalog.db":[9,9],"publish_state.json":[9,9],"rebuild_dedup_state.json":[5,5],"ticker_to_id.json":[6,6]});
    let coin = json!({"set_name":"coin1_by_bot","family":"coins","count":1,"emoji":[{"custom_emoji_id":"c-new","slot":0,"index":0,"source_emoji_ids":["src-c"],"previous_custom_emoji_ids":[]}]});
    let index =
        _native::roster_index::build(&[coin], "coins", &old, &inputs, "fixture UTC").unwrap();
    let names = index["packs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["set_name"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(names, BTreeSet::from(["gen1_by_bot", "coin1_by_bot"]));
    assert!(
        index["by_current_id"].get("g1").is_some() && index["by_current_id"].get("c-new").is_some()
    );
    assert!(index["by_current_id"].get("c-old").is_none());
    assert_eq!(index["by_source_id"], json!({"src-g":"g1","src-c":"c-new"}));
    assert_eq!(index["by_previous_id"], json!({"g0":"g1"}));
    assert_eq!(index["pack_count"], 2);
    assert_eq!(index["inputs"]["catalog.db"], json!([1, 1]));
    assert_eq!(index["inputs"]["publish_state.json"], json!([2, 2]));
    assert_eq!(index["inputs"]["rebuild_dedup_state.json"], json!([5, 5]));
    assert_eq!(index["inputs"]["ticker_to_id.json"], json!([6, 6]));
    let root = Root::new("roster family refusal café");
    let log = _native::logging::RunLog::open(&root.0, "roster-test", vec![]);
    assert!(
        _native::roster_cli::run(
            &config(&root.0),
            &["--check".into(), "--family".into(), "coins".into()],
            &log
        )
        .unwrap_err()
        .contains("--check covers all families")
    );
    assert!(!root.0.join("packs").exists());
    drop(log);
    root.clean().unwrap();
}
