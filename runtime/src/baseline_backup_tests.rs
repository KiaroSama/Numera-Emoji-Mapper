//! Original direct backup cases observe the production private unique-name allocator.
use crate::baseline_identity_support as support;
use rusqlite::Connection;
use support::*;

// tests.test_sqlite_snapshot.BoundedSnapshot.test_committed_wal_rows_are_in_the_reopened_snapshot
#[test]
fn sqlite_snapshot_committed_wal_rows_are_in_the_reopened_snapshot() {
    let f = Fixture::new();
    let writer = f.db();
    writer.pragma_update(None, "journal_mode", "WAL").unwrap();
    writer
        .execute(
            "INSERT INTO items VALUES('v:one','clip.webm','video',5)",
            [],
        )
        .unwrap();
    assert!(f.data.join("catalog.db-wal").is_file());

    let first = super::backup(&f.data).unwrap();
    let reopened = Connection::open(&first).unwrap();
    let rows = reopened
        .prepare("SELECT content_key FROM items")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(rows, ["v:one"]);
    assert_eq!(
        reopened
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    reopened.close().unwrap();

    let second = super::backup(&f.data).unwrap();
    assert_ne!(first, second);
    assert!(first.is_file() && second.is_file());
    drop(writer);
}

// tests.test_identity_migration.TheBackupIsWalSafe.test_a_snapshot_taken_with_an_open_wal_holds_the_committed_rows
#[test]
fn identity_a_snapshot_taken_with_an_open_wal_holds_the_committed_rows() {
    let f = Fixture::new();
    let writer = f.db();
    writer.pragma_update(None, "journal_mode", "WAL").unwrap();
    writer
        .execute("INSERT INTO items VALUES('v:wal','/x','video',NULL)", [])
        .unwrap();
    assert!(f.data.join("catalog.db-wal").is_file());
    let destination = super::backup(&f.data).unwrap();
    assert!(col(&destination, "items", "content_key").contains(&"v:wal".into()));
    drop(writer);
}
// tests.test_identity_migration.TheBackupIsWalSafe.test_a_rollback_journal_catalog_backs_up_too
#[test]
fn identity_a_rollback_journal_catalog_backs_up_too() {
    let f = Fixture::new();
    let db = f.db();
    db.pragma_update(None, "journal_mode", "DELETE").unwrap();
    db.execute("INSERT INTO items VALUES('v:one','/x','video',NULL)", [])
        .unwrap();
    db.close().unwrap();
    let destination = super::backup(&f.data).unwrap();
    let reopened = Connection::open(destination).unwrap();
    assert_eq!(
        reopened
            .query_row("SELECT content_key FROM items", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "v:one"
    );
}
// tests.test_identity_migration.TheBackupIsWalSafe.test_two_attempts_in_the_same_second_do_not_share_a_file
#[test]
fn identity_two_attempts_in_the_same_second_do_not_share_a_file() {
    let f = Fixture::new();
    let first = super::backup(&f.data).unwrap();
    let second = super::backup(&f.data).unwrap();
    assert_ne!(first, second);
    assert!(first.is_file() && second.is_file());
}
