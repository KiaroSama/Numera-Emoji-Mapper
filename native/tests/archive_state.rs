//! Original archive capacity, slot identity and partial-move persistence at public native seams.
use _native::{archive_check, archive_rows, archive_sync, config::Config, telegram::Telegram};
use rusqlite::{Connection, params};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server, TOKEN};
use tooling_fixture::Root;

fn config(root: &Path) -> Config {
    Config::load_with_environment(
        root.to_path_buf(),
        &BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("COLLECTION_PACK_BASE".into(), "pk".into()),
            ("EMOJI_ARCHIVE_DIR".into(), "archive".into()),
            ("BRAND_LOGO_PATH".into(), "logo.png".into()),
        ]),
    )
    .unwrap()
}
fn database(root: &Path, rows: &[(&str, &Path, &str)]) -> PathBuf {
    let data = root.join("collection");
    std::fs::create_dir_all(&data).unwrap();
    let path = data.join("catalog.db");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT, file_path TEXT, format TEXT, keywords TEXT);
        CREATE TABLE publications(base TEXT, content_key TEXT, set_name TEXT, custom_emoji_id TEXT);").unwrap();
    for (key, media, cid) in rows {
        db.execute(
            "INSERT INTO items VALUES(?,?,?,?)",
            params![key, media.to_str().unwrap(), "static", "[\"lock\"]"],
        )
        .unwrap();
        db.execute(
            "INSERT INTO publications VALUES(?,?,?,?)",
            params!["pk", key, "set1", cid],
        )
        .unwrap();
    }
    assert!(db.is_autocommit());
    drop(db);
    path
}
fn state(root: &Path, live: u64) -> PathBuf {
    let path = root.join("collection/publish_pk.json");
    _native::atomic::write_json(
        &path,
        &json!({"sets":[{"name":"set1","title":"Pack One","index":1,"live":live,"fmt":"mixed"}]}),
    )
    .unwrap();
    path
}
fn snapshot(directory: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![directory.to_path_buf()];
    let mut left = 64;
    while let Some(path) = pending.pop() {
        assert!(left > 0, "archive fixture inventory exceeded its bound");
        left -= 1;
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        _native::tooling_environment::reject_links(&path, "archive fixture cannot follow links")
            .unwrap();
        if metadata.is_dir() {
            found.insert(path.strip_prefix(directory).unwrap().to_path_buf(), None);
            pending.extend(
                std::fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else {
            assert!(metadata.is_file() && metadata.len() <= 65536);
            found.insert(
                path.strip_prefix(directory).unwrap().to_path_buf(),
                Some(std::fs::read(path).unwrap()),
            );
        }
    }
    found
}

#[test]
fn original_archive_capacity_and_freshness_use_actual_files_without_changing_them() {
    let root = Root::new("archive freshness café");
    let key = format!("s:{}", "a".repeat(32));
    let archived_name = format!("002_static_{}.webp", "a".repeat(12));
    for (label, live, folder, metadata, archived, cid, recorded, reason) in [
        (
            "unfinished-folder",
            146,
            true,
            true,
            true,
            "1",
            "1",
            Some("not full"),
        ),
        (
            "never-archived",
            200,
            false,
            false,
            false,
            "1",
            "1",
            Some("never archived"),
        ),
        ("unfinished", 146, false, false, false, "1", "1", None),
        ("fresh", 200, true, true, true, "1", "1", None),
        (
            "replaced",
            200,
            true,
            true,
            true,
            "NEW",
            "OLD",
            Some("id(s) added/changed"),
        ),
        (
            "inside-project",
            200,
            true,
            true,
            false,
            "1",
            "1",
            Some("still in the project"),
        ),
        (
            "missing-metadata",
            200,
            true,
            false,
            true,
            "1",
            "1",
            Some("001_logo.png"),
        ),
        (
            "unreadable-history",
            200,
            true,
            true,
            true,
            "1",
            "1",
            Some("_history.json is unreadable"),
        ),
    ] {
        let project = root.0.join(label);
        let archive = project.join("archive/Pack One");
        let media = if archived {
            archive.join(&archived_name)
        } else {
            project.join("collection/media/x.webp")
        };
        std::fs::create_dir_all(media.parent().unwrap()).unwrap();
        std::fs::write(&media, b"x").unwrap();
        if folder {
            std::fs::create_dir_all(&archive).unwrap();
        }
        if metadata {
            for (name, bytes) in [
                (archive_rows::LOGO_NAME, b"logo".as_slice()),
                ("_history.md", b"x"),
                ("_manifest.md", b"x"),
            ] {
                std::fs::write(archive.join(name), bytes).unwrap();
            }
            _native::atomic::write_json(
                &archive.join("_history.json"),
                &json!({"emoji":[{"premium_id":recorded}]}),
            )
            .unwrap();
            if label == "unreadable-history" {
                std::fs::write(archive.join("_history.json"), b"{broken").unwrap();
            }
        }
        database(&project, &[(&key, &media, cid)]);
        state(&project, live);
        let before = snapshot(&project);
        let (stale, why) = archive_check::check(&config(&project)).unwrap();
        assert_eq!(stale, reason.is_some(), "{label}: {why:?}");
        if let Some(reason) = reason {
            assert!(
                why.iter().any(|line| line.contains(reason)),
                "{label}: {why:?}"
            );
        } else {
            assert!(why.is_empty(), "{label}: {why:?}");
        }
        if label == "replaced" {
            assert_eq!(why.len(), 1);
        }
        assert_eq!(
            snapshot(&project),
            before,
            "{label}: offline check wrote fixture data"
        );
    }
    root.clean().unwrap();
}

#[test]
fn original_archive_slot_names_logo_and_reports_keep_unknown_stickers_unidentified() {
    let key = format!("s:{}", "a".repeat(32));
    assert_eq!(
        archive_rows::name(7, "animated", "a:169f06002d30a194aa2a4d6ab7cabd87", ".tgs").unwrap(),
        "007_animated_169f06002d30.tgs"
    );
    assert_eq!(archive_rows::folder("A: B?"), "A_ B_");
    assert_eq!(archive_rows::folder("Pack One"), "Pack One");
    let items = json!({(key.clone()):{"path":"x.webp","fmt":"static","cid":"1","set":"set1","keywords":["lock"]}});
    let keys = json!({"1":key});
    let record = json!({"name":"set1","title":"Pack One","fmt":"mixed"});
    for unknowns in [false, true] {
        let mut live = vec![
            json!({"custom_emoji_id":"logo-cid","emoji":"✅"}),
            json!({"custom_emoji_id":"1","emoji":"🔒"}),
        ];
        if unknowns {
            live.extend(["x2", "x3", "stray"].map(|id| json!({"custom_emoji_id":id})));
        }
        let rows = archive_rows::rows(&record, &live, &items, &keys).unwrap();
        assert_eq!(rows[0]["file"], "001_logo.png");
        assert!(rows[0]["premium_id"].is_null());
        assert_eq!(rows[1]["position"], 2);
        assert_eq!(
            rows[1]["file"],
            format!("002_static_{}.webp", "a".repeat(12))
        );
        let history = archive_rows::history(&record, &rows).unwrap();
        let manifest = archive_rows::manifest(&record, &rows, &items).unwrap();
        assert!(history.contains(&format!("Emoji in the pack: **{}**", live.len())));
        assert_eq!(history.matches(archive_rows::LOGO_NAME).count(), 1);
        assert!(manifest.contains("|  1 emoji"));
        assert!(!manifest.contains(archive_rows::LOGO_NAME));
        assert!(manifest.contains("lock"));
        if unknowns {
            assert!(rows[2..].iter().all(|row| row["file"].is_null()));
            assert_eq!(rows[4]["note"], "not in the catalog (unidentified)");
            assert!(history.contains("`unidentified`"));
        }
    }
}

#[test]
fn original_partial_archive_keeps_the_first_committed_move_when_the_next_source_is_missing() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("archive partial café");
    let data = root.0.join("collection");
    std::fs::create_dir_all(data.join("media")).unwrap();
    let first = data.join("media/a.webp");
    let missing = data.join("media/gone.webp");
    std::fs::write(&first, b"a").unwrap();
    let a = format!("s:{}", "a".repeat(32));
    let b = format!("s:{}", "b".repeat(32));
    let db = database(&root.0, &[(&a, &first, "1"), (&b, &missing, "2")]);
    let state = state(&root.0, 200);
    let state_before = std::fs::read(&state).unwrap();
    let logo = root.0.join("logo.png");
    std::fs::write(&logo, b"logo").unwrap();
    let mut server = Server::new(vec![Reply::new(
        "getStickerSet",
        &[("name", "set1")],
        json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"logo"},{"custom_emoji_id":"1"},{"custom_emoji_id":"2"}]}}),
    )]);
    let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
    let log = _native::logging::RunLog::open(&root.0, "archive-test", vec![TOKEN.into()]);
    assert_eq!(archive_sync::run(&config(&root.0), &tg, &log).unwrap(), 1);
    assert_eq!(server.finish().unwrap().len(), 1);
    let moved = root
        .0
        .join("archive/Pack One")
        .join(format!("002_static_{}.webp", "a".repeat(12)));
    assert_eq!(std::fs::read(&moved).unwrap(), b"a");
    assert!(!first.exists());
    assert!(!missing.exists());
    assert_eq!(std::fs::read(&logo).unwrap(), b"logo");
    assert_eq!(std::fs::read(&state).unwrap(), state_before);
    let connection =
        Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let stored: BTreeMap<String, String> = connection
        .prepare("SELECT content_key,file_path FROM items")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(_native::paths::resolve(&data, &stored[&a], &root.0), moved);
    assert_eq!(stored[&b], missing.to_str().unwrap());
    let identities: Vec<(String, String)> = connection
        .prepare("SELECT content_key,custom_emoji_id FROM publications ORDER BY content_key")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(identities, vec![(a, "1".into()), (b, "2".into())]);
    drop(connection);
    assert_eq!(
        std::fs::read_dir(moved.parent().unwrap()).unwrap().count(),
        1
    );
    drop(tg);
    log.finish(1);
    drop(log);
    root.clean().unwrap();
}

#[test]
fn original_archive_sources_refuse_before_runtime_or_workspace_access() {
    use _native::source_git_inventory::Source;
    let absent = PathBuf::from("must-not-be-inspected");
    let source = Source {
        baseline_revision: "0".repeat(40),
        path: "unverified.py".into(),
        sha256: "0".repeat(64),
        text: "raise AssertionError('must not execute')".into(),
    };
    assert!(
        _native::source_archive_tests::observe(&absent, &absent, &[&source; 7])
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(_native::source_archive_tests::source_identity(&[]).is_err());
}
