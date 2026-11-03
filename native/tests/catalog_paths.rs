//! Original catalog path contracts, using real SQLite and isolated synthetic media storage.
use _native::{catalog::Catalog, paths};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

const UTC: &str = "2026-10-07 00:00:00 UTC";

fn source_identity() {
    let inventory: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    assert_eq!(
        inventory["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let records = inventory["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|record| record["source"] == "tests/test_media_paths.py")
        .collect::<Vec<_>>();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0]["sha256"],
        "dda79d4365c8864be58bf82900ad20b78e292502d71896fd44e26c0203f17bd0"
    );
}

fn raw_paths(data: &Path) -> BTreeMap<String, String> {
    let observer =
        Connection::open_with_flags(data.join("catalog.db"), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let rows = {
        let mut statement = observer
            .prepare("SELECT content_key,file_path FROM items ORDER BY content_key")
            .unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<BTreeMap<_, _>>>()
            .unwrap()
    };
    observer.close().unwrap();
    rows
}

fn file_path(row: &Value) -> PathBuf {
    PathBuf::from(row["file_path"].as_str().unwrap())
}

#[test]
fn original_inside_archive_raw_reader_and_moved_catalog_keep_their_media() {
    source_identity();
    let root = Root::new("catalog paths café");
    let data = root.0.join("collection");
    std::fs::create_dir_all(data.join("media/static")).unwrap();
    let archive = root.0.join("archive");
    std::fs::create_dir(&archive).unwrap();
    let inside = data.join("media/static/a.webp");
    let outside = archive.join("b.webp");
    std::fs::write(&inside, b"inside").unwrap();
    std::fs::write(&outside, b"archive").unwrap();
    let mut catalog = Catalog::open(&data.join("catalog.db"), UTC, &root.0).unwrap();
    for (key, path) in [("s:a", &inside), ("s:b", &outside)] {
        assert!(
            catalog
                .insert(
                    &json!({"content_key":key,"fmt":"static","file_path":path}),
                    UTC
                )
                .unwrap()
        );
        assert_eq!(file_path(&catalog.get(key).unwrap().unwrap()), *path);
    }
    drop(catalog);
    let stored = raw_paths(&data);
    assert_eq!(stored["s:a"], "./media/static/a.webp");
    assert_eq!(Path::new(&stored["s:b"]), outside);
    assert_eq!(paths::resolve(&data, &stored["s:a"], &root.0), inside);
    assert_eq!(paths::resolve(&data, &stored["s:b"], &root.0), outside);

    let moved = root.0.join("renamed project/collection");
    std::fs::create_dir(moved.parent().unwrap()).unwrap();
    assert!(!moved.exists());
    std::fs::rename(&data, &moved).unwrap();
    let catalog = Catalog::open(&moved.join("catalog.db"), UTC, &root.0).unwrap();
    let items = catalog
        .all(None)
        .unwrap()
        .into_iter()
        .map(|row| {
            (
                row["content_key"].as_str().unwrap().to_owned(),
                file_path(&row),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(items.len(), 2);
    assert_eq!(std::fs::read(&items["s:a"]).unwrap(), b"inside");
    assert!(items["s:a"].starts_with(&moved));
    assert_eq!(items["s:b"], outside);
    assert_eq!(std::fs::read(&outside).unwrap(), b"archive");
    assert_eq!(raw_paths(&moved), stored);
    assert_eq!(
        paths::resolve(&moved, &stored["s:a"], &root.0),
        items["s:a"]
    );
    assert!(!data.exists());
    drop(catalog);
    root.clean().unwrap();
}

fn legacy(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let data = root.join("collection");
    std::fs::create_dir_all(data.join("media")).unwrap();
    let inside = data.join("media/a.webp");
    std::fs::write(&inside, b"a").unwrap();
    let project_file = root.join("tests/__media_paths_rel__/p.webp");
    std::fs::create_dir_all(project_file.parent().unwrap()).unwrap();
    std::fs::write(&project_file, b"p").unwrap();
    drop(Catalog::open(&data.join("catalog.db"), UTC, root).unwrap());
    let connection = Connection::open(data.join("catalog.db")).unwrap();
    connection
        .execute("DELETE FROM meta WHERE key='media_paths'", [])
        .unwrap();
    for (key, stored) in [
        ("s:a", inside.to_str().unwrap()),
        ("s:p", "tests/__media_paths_rel__/p.webp"),
    ] {
        connection
            .execute(
                "INSERT INTO items(content_key,format,file_path,created_utc,position)
                 VALUES(?,'static',?,'2026-01-01',1)",
                params![key, stored],
            )
            .unwrap();
    }
    // Count actual path writes, not a target-generated converted_media_paths approximation.
    connection
        .execute_batch(
            "CREATE TABLE path_updates(content_key TEXT NOT NULL);
             CREATE TRIGGER observe_path_update AFTER UPDATE OF file_path ON items
             BEGIN INSERT INTO path_updates VALUES(NEW.content_key); END;",
        )
        .unwrap();
    connection.close().unwrap();
    (data, inside, project_file)
}

fn updates(data: &Path) -> Vec<String> {
    let observer =
        Connection::open_with_flags(data.join("catalog.db"), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let rows = {
        let mut statement = observer
            .prepare("SELECT content_key FROM path_updates ORDER BY content_key")
            .unwrap();
        statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    observer.close().unwrap();
    rows
}

#[test]
fn original_legacy_conversion_is_portable_and_does_not_repeat_path_writes() {
    source_identity();
    let root = Root::new("catalog legacy paths");
    let (data, inside, project_file) = legacy(&root.0);
    let catalog = Catalog::open(&data.join("catalog.db"), UTC, &root.0).unwrap();
    assert_eq!(file_path(&catalog.get("s:a").unwrap().unwrap()), inside);
    assert_eq!(
        file_path(&catalog.get("s:p").unwrap().unwrap()),
        project_file
    );
    assert_eq!(
        catalog.meta("media_paths").unwrap().as_deref(),
        Some("data-relative-2")
    );
    drop(catalog);
    let stored = raw_paths(&data);
    assert_eq!(stored["s:a"], "./media/a.webp");
    assert!(Path::new(&stored["s:p"]).is_absolute());
    assert_eq!(Path::new(&stored["s:p"]), project_file);
    assert_eq!(updates(&data), ["s:a", "s:p"]);
    let catalog = Catalog::open(&data.join("catalog.db"), UTC, &root.0).unwrap();
    assert_eq!(raw_paths(&data), stored);
    assert_eq!(
        updates(&data),
        ["s:a", "s:p"],
        "reopen must not rewrite converted paths"
    );
    drop(catalog);
    assert_eq!(std::fs::read(inside).unwrap(), b"a");
    assert_eq!(std::fs::read(project_file).unwrap(), b"p");
    root.clean().unwrap();
}

#[test]
fn original_old_conversion_flag_heals_absolute_rows_without_changing_file_bytes() {
    source_identity();
    let root = Root::new("catalog old path flag");
    let (data, inside, project_file) = legacy(&root.0);
    let connection = Connection::open(data.join("catalog.db")).unwrap();
    connection
        .execute(
            "INSERT OR REPLACE INTO meta(key,value) VALUES('media_paths','data-relative')",
            [],
        )
        .unwrap();
    connection.close().unwrap();
    let catalog = Catalog::open(&data.join("catalog.db"), UTC, &root.0).unwrap();
    assert_eq!(
        catalog.meta("media_paths").unwrap().as_deref(),
        Some("data-relative-2")
    );
    assert_eq!(file_path(&catalog.get("s:a").unwrap().unwrap()), inside);
    drop(catalog);
    assert_eq!(raw_paths(&data)["s:a"], "./media/a.webp");
    assert!(updates(&data).iter().any(|key| key == "s:a"));
    assert_eq!(std::fs::read(inside).unwrap(), b"a");
    assert_eq!(std::fs::read(project_file).unwrap(), b"p");
    root.clean().unwrap();
}

#[test]
fn original_row_marker_not_meta_selects_data_or_project_relative_paths() {
    source_identity();
    let root = Root::new("catalog missing path meta");
    let (data, inside, project_file) = legacy(&root.0);
    let connection = Connection::open(data.join("catalog.db")).unwrap();
    connection.execute_batch("DROP TABLE meta").unwrap();
    connection.close().unwrap();
    assert_eq!(
        paths::resolve(&data, "tests/__media_paths_rel__/p.webp", &root.0),
        project_file
    );
    assert_eq!(paths::resolve(&data, "./media/a.webp", &root.0), inside);
    assert_eq!(std::fs::read(project_file).unwrap(), b"p");
    assert_eq!(std::fs::read(inside).unwrap(), b"a");
    root.clean().unwrap();
}

#[cfg(windows)]
#[test]
fn original_windows_case_does_not_turn_an_inside_path_into_an_archive() {
    source_identity();
    let root = Root::new("catalog Windows path case");
    let data = root.0.join("collection");
    let inside = data.join("media/static/a.webp");
    let other_case = PathBuf::from(data.to_str().unwrap().to_uppercase());
    assert_eq!(
        paths::store(&other_case, &inside).unwrap(),
        "./media/static/a.webp"
    );
    root.clean().unwrap();
}
