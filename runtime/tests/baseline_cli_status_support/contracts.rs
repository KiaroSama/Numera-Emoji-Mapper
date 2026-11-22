use crate::support::{
    http::{Reply, Server},
    *,
};
use numera_runtime::{self as native, locks::Lease, telegram::SetState};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

// tests.test_native_command_help.NativeCommandHelp.test_top_level_help_names_existing_backend_commands
#[test]
fn top_level_help_names_every_existing_command() {
    let f = Fixture::new();
    let r = f.run(&["--help"], &[]);
    r.code(0);
    for name in [
        "panel",
        "add-media",
        "fetch-pack",
        "fetch-emoji-ids",
        "build-collection",
        "build-pack",
        "emoji-bot",
        "sync-order",
        "plan-apply",
        "make-emoji-pngs",
        "plan-status",
        "pack-manifest",
        "status",
        "pack-archive",
        "identity-repair",
    ] {
        assert!(
            r.out.split_whitespace().any(|s| s == name),
            "missing {name}: {}",
            r.out
        );
    }
    assert_eq!(r.err, "");
}
// tests.test_native_command_help.NativeCommandHelp.test_extra_identity_command_is_a_usage_error_before_catalog_access
#[test]
fn extra_identity_command_refuses_before_catalog_access() {
    let f = Fixture::new();
    let r = f.run(&["identity-repair", "report", "restore"], &[]);
    r.code(2);
    assert!(r.err.contains("only one identity-repair command"));
    assert!(!r.out.contains("no catalog"));
    assert!(!f.data().exists());
}
// tests.test_native_storage_recovery.NativeStorage.test_atomic_utf8_replaces_and_leaves_no_sibling
#[test]
fn native_atomic_utf8_replaces_without_sibling() {
    let root = Scratch::new();
    let path = root.0.join("state.json");
    native::atomic::write_json(&path, &json!({"key":"😀","uploaded":true})).unwrap();
    native::atomic::write_json(&path, &json!({"key":"é","uploaded":false})).unwrap();
    assert_eq!(read_json(&path), json!({"key":"é","uploaded":false}));
    assert_eq!(
        std::fs::read_dir(&root.0)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect::<Vec<_>>(),
        vec![path]
    );
}
// tests.test_native_storage_recovery.NativeStorage.test_legacy_and_native_exclude_each_other_and_release
#[test]
fn independent_native_lock_owners_exclude_each_other_and_release() {
    let root = Scratch::new();
    let path = root.0.join("family.lock");
    {
        let first = Lease::acquire(&path, UTC).unwrap();
        assert!(Lease::acquire(&path, UTC).is_err());
        drop(first);
    }
    let native = Lease::acquire(&path, "2026-10-07 00:00:00 UTC").unwrap();
    assert_eq!(read_json(&path)["started"], "2026-10-07 00:00:00 UTC");
    assert!(Lease::acquire(&path, UTC).is_err());
    drop(native);
    let _lease = Lease::acquire(&path, UTC).unwrap();
    assert!(path.is_file());
}
// tests.test_native_catalog.NativeCatalogParity.test_order_inclusion_signed_hash_and_family_publications_match_source
#[test]
fn native_catalog_order_inclusion_signed_hash_family_contract() {
    let f = Fixture::new();
    let mut cat = f.catalog();
    for (key, phash) in [("a", json!(u64::MAX)), ("b", json!(0)), ("c", Value::Null)] {
        assert!(cat.insert(&json!({"content_key":key,"fmt":"static","file_path":f.root.join(format!("{key}.png")),"emojis":["😀"],"phash":phash}),"2026-10-07 00:00:00 UTC").unwrap());
    }
    assert_eq!(
        cat.dispatch(json!({"operation":"order","keys":["c","a","missing"]}))
            .unwrap(),
        2
    );
    assert_eq!(
        cat.dispatch(json!({"operation":"inclusion","keys":["b"]}))
            .unwrap(),
        json!([2, 1])
    );
    for (id, set) in [
        (json!("111111111"), json!("fixture1")),
        (Value::Null, Value::Null),
    ] {
        cat.uploaded(
            &json!({"content_key":"a","custom_emoji_id":id,"base":"fixture","set_name":set}),
            UTC,
        )
        .unwrap();
    }
    let rows = cat.all(None).unwrap();
    let expected:Vec<Value>=[("c",Value::Null,false,true),("a",json!(u64::MAX),true,true),("b",json!(0),false,false)].into_iter().map(|(key,phash,uploaded,included)|json!({"content_key":key,"fmt":"static","file_path":f.root.join(format!("{key}.png")),"emojis":["😀"],"keywords":[],"sources":[],"phash":phash,"custom_emoji_id":null,"uploaded":uploaded,"included":included})).collect();
    assert_eq!(rows, expected);
    assert_eq!(
        cat.pending(None, Some("fixture")).unwrap(),
        vec![expected[0].clone()]
    );
    assert_eq!(cat.pending(None, Some("other")).unwrap(), expected[..2]);
    assert_eq!(
        cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":"a"}))
            .unwrap(),
        "111111111"
    );
    assert_eq!(
        cat.stats().unwrap(),
        json!({"static":{"total":3,"uploaded":1,"excluded":1,"pending":1}})
    );
    let data = f.data();
    assert!(
        std::thread::spawn(move || native::ownership::Ownership::acquire(
            &data,
            native::ownership::Mode::Writer,
            UTC
        )
        .is_err())
        .join()
        .unwrap()
    );
    drop(cat);
    let data = f.data();
    assert!(
        std::thread::spawn(move || native::ownership::Ownership::acquire(
            &data,
            native::ownership::Mode::Writer,
            UTC
        )
        .is_ok())
        .join()
        .unwrap()
    );
}
// tests.test_native_telegram.NativeTelegramTransport.test_native_transport_cannot_bypass_suite_network_guard
#[test]
fn native_transport_cannot_bypass_numeric_loopback_guard() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    for base in [
        "https://api.telegram.org",
        "http://localhost",
        "http://example.com",
    ] {
        let error = native::telegram::Telegram::new("test-only-token".into(), base.into())
            .err()
            .unwrap();
        assert!(error.to_string().contains("numeric loopback"), "{error}");
    }
}
// tests.test_native_telegram.NativeTelegramTransport.test_probe_keeps_exists_missing_unknown_and_sends_form
#[test]
fn probes_keep_three_states_and_send_names_as_form() {
    let bot = Server::new(|r| match r.form()["name"].as_str() {
        "exists" => Reply::ok(json!({"stickers":[],"name":"exists"})),
        "missing" => Reply::json(json!({"ok":false,"description":"STICKERSET_INVALID"})),
        _ => Reply::json(json!([])),
    });
    let tg = bot.telegram();
    for (name, expected) in [
        ("exists", SetState::Exists),
        ("missing", SetState::Missing),
        ("unknown", SetState::Unknown),
    ] {
        let (state, value) = tg.probe(name);
        assert_eq!(state, expected);
        if state != SetState::Exists {
            assert!(value.is_none());
        }
    }
    let calls = bot.observed.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|r| r.form()["name"].clone())
            .collect::<Vec<_>>(),
        ["exists", "missing", "unknown"]
    );
    assert!(calls.iter().all(|r| r.path.ends_with("/getStickerSet")));
}
// tests.test_native_announce_compat.NativeAnnouncementContract.test_direct_cards_list_worker_refusal_and_nondefault_timeout
#[test]
fn direct_cards_list_worker_refusal_and_timeout_contract() {
    let f = Fixture::new();
    let mode = Arc::new(Mutex::new("success"));
    let server_mode = mode.clone();
    let bot = Server::new(move |r| {
        if r.path == "/publish" {
            match *server_mode.lock().unwrap() {
                "refuse" => Reply::Bytes(503, b"{\"ok\":true}".to_vec()),
                "disconnect" => Reply::Disconnect,
                _ => Reply::json(json!({"ok":true,"message_ids":[1]})),
            }
        } else {
            Reply::ok(json!({"message_id":1}))
        }
    });
    let tg = bot.telegram();
    let packs = vec![
        json!({"name":"fixture1_by_YourEmojiBot","title":"Fixture 1","count":1}),
        json!({"name":"fixture2_by_YourEmojiBot","title":"","count":2}),
    ];
    assert_eq!(
        native::announce::packs(
            &f.config(&[]),
            &tg,
            111111111,
            &packs,
            "coin",
            "Note",
            "cards"
        )
        .unwrap(),
        "111111111"
    );
    {
        let calls = bot.observed.lock().unwrap();
        assert_eq!(
            calls
                .iter()
                .map(|r| r.form()["text"].clone())
                .collect::<Vec<_>>(),
            [
                "Note",
                "✅ Fixture 1\nhttps://t.me/addemoji/fixture1_by_YourEmojiBot",
                "✅ fixture2_by_YourEmojiBot\nhttps://t.me/addemoji/fixture2_by_YourEmojiBot"
            ]
        );
        assert!(calls.iter().all(|r| r.form()["chat_id"] == "111111111"
            && r.form()["disable_web_page_preview"] == "true"));
    }
    bot.observed.lock().unwrap().clear();
    assert_eq!(
        native::announce::packs(
            &f.config(&[("PACK_LINKS_CHAT_ID", "@fixturechannel".into())]),
            &tg,
            111111111,
            &packs,
            "coin",
            "Note",
            "list"
        )
        .unwrap(),
        "@fixturechannel"
    );
    {
        let calls = bot.observed.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].form()["text"],
            "Note\n\nFixture 1. https://t.me/addemoji/fixture1_by_YourEmojiBot\nfixture2_by_YourEmojiBot. https://t.me/addemoji/fixture2_by_YourEmojiBot"
        );
    }
    let config = f.config(&[
        ("WORKER_PUBLISH_URL", format!("{}/publish", bot.base)),
        ("WORKER_PUBLISH_SECRET", "fixture-secret".into()),
    ]);
    bot.observed.lock().unwrap().clear();
    assert_eq!(
        native::announce::packs(&config, &tg, 111111111, &packs, "coin", "Note", "list").unwrap(),
        "the worker"
    );
    {
        let calls = bot.observed.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            serde_json::from_slice::<Value>(&calls[0].body).unwrap(),
            json!({"bot":"coin","packs":packs,"style":"list","note":"Note"})
        );
        assert_eq!(calls[0].headers["authorization"], "Bearer fixture-secret");
    }
    bot.observed.lock().unwrap().clear();
    native::announce::worker(&config, &packs, "general", "", "cards", 2.0).unwrap();
    {
        let calls = bot.observed.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(
            serde_json::from_slice::<Value>(&calls[0].body)
                .unwrap()
                .get("note")
                .is_none()
        );
    }
    for (value, error) in [("refuse", "HTTP 503"), ("disconnect", "outcome unknown")] {
        *mode.lock().unwrap() = value;
        bot.observed.lock().unwrap().clear();
        assert!(
            native::announce::packs(&config, &tg, 111111111, &packs, "coin", "", "cards")
                .unwrap_err()
                .contains(error)
        );
        assert_eq!(bot.observed.lock().unwrap().len(), 1);
    }
}
