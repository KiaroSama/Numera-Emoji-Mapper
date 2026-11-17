//! Connection-private historical catalog cases: the actual production connection is observed.
use super::Catalog;
use crate::baseline_catalog_support as support;
use rusqlite::Connection;
use support::*;

// tests.test_catalog.TestCatalogDurabilityAndScale.test_wal_and_relaxed_sync
#[test]
fn catalog_durability_wal_and_relaxed_sync() {
    let root = Scratch::new();
    let cat = root.catalog();
    assert_eq!(
        cat.db
            .query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "wal"
    );
    assert_eq!(
        cat.db
            .query_row("PRAGMA synchronous", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
// tests.test_catalog.TestCatalogDurabilityAndScale.test_empty_order_writes_nothing
#[test]
fn catalog_durability_empty_order_writes_nothing() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    for i in 0..3 {
        cat.insert(&root.item(&format!("s:k{i}"), &format!("m{i}.png")), UTC)
            .unwrap();
    }
    let before = cat.db.total_changes();
    assert_eq!(cat.order(&[]).unwrap(), 0);
    assert_eq!(cat.db.total_changes(), before);
}
// tests.test_catalog.TestCatalogDurabilityAndScale.test_failed_init_does_not_leak_the_connection
#[test]
fn catalog_durability_failed_init_does_not_leak_the_connection() {
    let root = Scratch::new();
    let path = root.database();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE items(wrong_column TEXT)")
        .unwrap();
    db.close().unwrap();
    // Real schema-index failure occurs after Connection::open, avoiding an artificial error path.
    let error = Catalog::open(&path, UTC, &root.0).err().unwrap();
    assert!(error.contains("no such column: format"), "{error}");
    #[cfg(target_os = "linux")]
    {
        let handles = std::fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|entry| std::fs::read_link(entry.ok()?.path()).ok())
            .filter(|target| {
                target == &path
                    || target
                        .to_string_lossy()
                        .starts_with(&format!("{}-", path.display()))
            })
            .collect::<Vec<_>>();
        assert!(
            handles.is_empty(),
            "failed initialization retained SQLite handles: {handles:?}"
        );
    }
    // Windows denies removing an open SQLite file; POSIX additionally has the FD assertion above.
    std::fs::remove_file(&path).unwrap();
    let cat = Catalog::open(&path, UTC, &root.0).unwrap();
    assert!(cat.all(None).unwrap().is_empty());
}
