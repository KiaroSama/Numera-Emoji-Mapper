//! Supplemental complete eight-state read-only archive matrix.
use crate::root::Root;
use _native::{archive_check, archive_rows, config::Config};
use numera_runtime as _native;
use rusqlite::{Connection, params};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
// Native supplemental: native/tests/archive_state.rs::original_archive_capacity_and_freshness_use_actual_files_without_changing_them
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
}
