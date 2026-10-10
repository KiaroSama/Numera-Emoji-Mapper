//! Online backup reads still-uncheckpointed WAL and never overwrites an existing recovery file.
use rusqlite::Connection;
use std::path::PathBuf;
#[test]
fn snapshot_contains_wal_rows_and_restores_timeouts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("logs")
        .join(format!("snapshot-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let source = Connection::open(root.join("catalog.db")).unwrap();
    source.pragma_update(None, "journal_mode", "WAL").unwrap();
    source.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    source.pragma_update(None, "busy_timeout", 4321).unwrap();
    source
        .execute_batch("CREATE TABLE items(key TEXT);INSERT INTO items VALUES('v:committed');")
        .unwrap();
    assert!(root.join("catalog.db-wal").metadata().unwrap().len() > 0);
    let backup = root.join("backup.db");
    _native::sqlite_snapshot::backup(&source, &backup).unwrap();
    let destination = Connection::open(&backup).unwrap();
    assert_eq!(
        destination
            .query_row("SELECT key FROM items", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "v:committed"
    );
    assert_eq!(
        source
            .query_row("PRAGMA busy_timeout", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        4321
    );
    assert!(_native::sqlite_snapshot::backup(&source, &backup).is_err());
    destination.close().unwrap();
    source.close().unwrap();
    std::fs::remove_dir_all(root).unwrap();
}
