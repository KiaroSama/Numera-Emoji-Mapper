use super::*;
use crate::baseline_catalog_support::Scratch;
use crate::baseline_cli_http::{Reply, Server};
use std::collections::BTreeMap;
fn fixture() -> (Scratch, Config, Arc<RunLog>, PathBuf) {
    let root = Scratch::new();
    let data = root.0.join("data");
    std::fs::create_dir(&data).unwrap();
    let db = data.join("catalog.db");
    std::fs::write(
        &db,
        b"unreadable retained database: reopening must not load or migrate this",
    )
    .unwrap();
    let config = Config::load_with_environment(
        root.0.clone(),
        &BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("GENERAL_BOT_TOKEN".into(), "test-only-token".into()),
        ]),
    )
    .unwrap();
    let log = Arc::new(RunLog::open(&root.0, "panel-reopen", vec![]));
    (root, config, log, db)
}
fn options(data: &std::path::Path, port: u16) -> Value {
    json!({"data_dir":data,"port":port,"preview_fps":15,"show_published":false,"bot":"FixtureBot","packs":[],"no_open":false})
}
// tests.test_panel_instance.ReopenPanel.test_reopening_opens_browser_without_reloading_catalog_or_bot
#[test]
fn reopening_opens_browser_without_catalog_or_bot_reload() {
    let (_root, config, log, db) = fixture();
    let identity = panel_session::identity(&db, false, &[]).unwrap();
    let bot = Server::new(move |r| {
        assert_eq!(r.verb, "GET");
        assert_eq!(r.path, "/api/session");
        Reply::json(identity.clone())
    });
    let before = std::fs::read(&db).unwrap();
    let mut opened = Vec::new();
    start_panel_with_browser(
        &config,
        options(db.parent().unwrap(), bot.port),
        log,
        |_, port, no_open| {
            opened.push(_native::browser_open::panel_url(port, no_open).unwrap());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(opened, [format!("http://127.0.0.1:{}/", bot.port)]);
    assert_eq!(std::fs::read(&db).unwrap(), before);
    assert_eq!(bot.observed.lock().unwrap().len(), 1);
}
// tests.test_panel_instance.ReopenPanel.test_another_collection_or_view_is_not_silently_adopted
#[test]
fn another_collection_or_view_is_not_adopted() {
    let (root, config, log, db) = fixture();
    let identity = panel_session::identity(&db, false, &[]).unwrap();
    let bot = Server::new(move |r| {
        assert_eq!(r.verb, "GET");
        Reply::json(identity.clone())
    });
    for mode in ["all", "pack", "data"] {
        let mut opts = options(db.parent().unwrap(), bot.port);
        match mode {
            "all" => opts["show_published"] = json!(true),
            "pack" => opts["packs"] = json!([1]),
            _ => {
                let other = root.0.join("other");
                std::fs::create_dir(&other).unwrap();
                std::fs::write(other.join("catalog.db"), b"other db").unwrap();
                opts["data_dir"] = json!(other);
            }
        }
        let mut opened = 0;
        let result = start_panel_with_browser(&config, opts, log.clone(), |_, _, _| {
            opened += 1;
            Ok(())
        });
        assert!(
            result.unwrap_err().contains("another collection or view"),
            "{mode}"
        );
        assert_eq!(opened, 0);
    }
}
// tests.test_panel_instance.ReopenPanel.test_legacy_panel_is_reopened_but_foreign_or_malformed_services_are_not
#[test]
fn legacy_panel_reopens_foreign_and_malformed_services_refuse() {
    let (_root, config, log, db) = fixture();
    let legacy = "<title>Numera Emoji Mapper — Curate</title><script id=\"items-data\">[]</script><script>const TOKEN = \"fixture\";</script><script src=\"/static/panel-grid.js\"></script>";
    for mode in [
        "legacy",
        "foreign",
        "invalid-json",
        "other-app",
        "fake-legacy",
    ] {
        let bot = Server::new(move |r| {
            assert_eq!(r.verb, "GET");
            match r.path.as_str() {
                "/api/session" => match mode {
                    "legacy" | "foreign" => Reply::Bytes(404, vec![]),
                    "invalid-json" => Reply::Bytes(200, b"invalid JSON".to_vec()),
                    "other-app" => Reply::json(json!({"application":"other"})),
                    _ => Reply::json(
                        json!({"application":"numera-emoji-mapper-panel","legacy":true}),
                    ),
                },
                "/api/ping" => Reply::Bytes(200, br#"{"ok":true}"#.to_vec()),
                "/" => Reply::Bytes(
                    200,
                    if mode == "legacy" {
                        legacy.as_bytes().to_vec()
                    } else {
                        b"Other app".to_vec()
                    },
                ),
                _ => Reply::Bytes(404, vec![]),
            }
        });
        let mut opened = Vec::new();
        let result = start_panel_with_browser(
            &config,
            options(db.parent().unwrap(), bot.port),
            log.clone(),
            |_, port, no_open| {
                opened.push(_native::browser_open::panel_url(port, no_open).unwrap());
                Ok(())
            },
        );
        if mode == "legacy" {
            assert!(result.is_ok());
            assert_eq!(opened, [format!("http://127.0.0.1:{}/", bot.port)]);
        } else {
            assert!(result.is_err(), "{mode}");
            assert!(opened.is_empty());
        }
        assert!(
            !bot.observed
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.path.contains("getMe"))
        );
    }
}
