//! Online snapshots preserve WAL rows, bound lock contention and never overwrite recovery files.
use rusqlite::Connection;
use std::time::{Duration, Instant};

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

fn timeout(connection: &Connection) -> u32 {
    connection
        .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn snapshot_contains_wal_rows_and_restores_timeouts() {
    let root = tooling_fixture::Root::new("snapshot committed WAL");
    let source = Connection::open(root.0.join("catalog.db")).unwrap();
    source.pragma_update(None, "journal_mode", "WAL").unwrap();
    source.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    source.busy_timeout(Duration::from_millis(4321)).unwrap();
    source
        .execute_batch("CREATE TABLE items(key TEXT);INSERT INTO items VALUES('v:committed');")
        .unwrap();
    assert!(root.0.join("catalog.db-wal").metadata().unwrap().len() > 0);
    for name in ["backup.db", "second.db"] {
        let backup = root.0.join(name);
        _native::sqlite_snapshot::backup(&source, &backup).unwrap();
        let destination = Connection::open(&backup).unwrap();
        assert_eq!(
            destination
                .query_row("SELECT key FROM items", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "v:committed"
        );
        assert_eq!(
            destination
                .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        destination.close().unwrap();
        assert!(!root.0.join(format!("{name}-wal")).exists());
        assert!(!root.0.join(format!("{name}-shm")).exists());
        let before = std::fs::read(&backup).unwrap();
        assert!(_native::sqlite_snapshot::backup(&source, &backup).is_err());
        assert_eq!(std::fs::read(&backup).unwrap(), before);
        assert_eq!(timeout(&source), 4321);
    }
    source.close().unwrap();
    root.clean().unwrap();
}

#[test]
fn snapshot_contention_preserves_connections_and_rows_until_retry() {
    let root = tooling_fixture::Root::new("snapshot bounded contention");
    let path = root.0.join("catalog.db");
    let holder = Connection::open(&path).unwrap();
    holder
        .pragma_update(None, "journal_mode", "DELETE")
        .unwrap();
    holder
        .execute_batch("CREATE TABLE items(key TEXT);INSERT INTO items VALUES('v:original');")
        .unwrap();
    let source = Connection::open(&path).unwrap();
    source.busy_timeout(Duration::from_millis(4321)).unwrap();
    let partial = root.0.join("partial.db");
    holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let started = Instant::now();
    let refused =
        _native::sqlite_snapshot::backup_with_budget(&source, &partial, Duration::from_millis(150));
    assert!(refused.unwrap_err().contains("did not finish"));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(timeout(&source), 4321);
    for suffix in ["", "-wal", "-shm"] {
        assert!(!root.0.join(format!("partial.db{suffix}")).exists());
    }
    holder.execute_batch("ROLLBACK").unwrap();
    _native::sqlite_snapshot::backup_with_budget(&source, &partial, Duration::from_secs(1))
        .unwrap();
    let reopened = Connection::open(&partial).unwrap();
    assert_eq!(
        reopened
            .query_row("SELECT key FROM items", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "v:original"
    );
    reopened.close().unwrap();

    let restore = Connection::open_in_memory().unwrap();
    restore
        .execute_batch("CREATE TABLE sentinel(value TEXT);INSERT INTO sentinel VALUES('snapshot');")
        .unwrap();
    restore.busy_timeout(Duration::from_millis(4321)).unwrap();
    let mut destination = Connection::open(&path).unwrap();
    destination
        .busy_timeout(Duration::from_millis(731))
        .unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let started = Instant::now();
    let refused =
        _native::sqlite_snapshot::copy(&restore, &mut destination, Duration::from_millis(150));
    assert!(refused.unwrap_err().contains("did not finish"));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(timeout(&restore), 4321);
    assert_eq!(timeout(&destination), 731);
    holder.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        destination
            .query_row("SELECT key FROM items", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "v:original"
    );
    _native::sqlite_snapshot::copy(&restore, &mut destination, Duration::from_secs(1)).unwrap();
    assert_eq!(
        destination
            .query_row("SELECT value FROM sentinel", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "snapshot"
    );
    assert_eq!(timeout(&restore), 4321);
    assert_eq!(timeout(&destination), 731);

    let absent = root.0.join("invalid-budget.db");
    let before = _native::migration_signature::database(&destination, &[]).unwrap();
    assert!(
        _native::sqlite_snapshot::copy(&restore, &mut destination, Duration::ZERO)
            .unwrap_err()
            .contains("positive")
    );
    assert!(
        _native::sqlite_snapshot::backup_with_budget(&restore, &absent, Duration::ZERO)
            .unwrap_err()
            .contains("positive")
    );
    assert!(!absent.exists());
    assert_eq!(
        _native::migration_signature::database(&destination, &[]).unwrap(),
        before
    );
    assert_eq!(timeout(&restore), 4321);
    assert_eq!(timeout(&destination), 731);
    destination.close().unwrap();
    restore.close().unwrap();
    source.close().unwrap();
    holder.close().unwrap();
    root.clean().unwrap();
}
