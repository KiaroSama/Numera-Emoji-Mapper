//! One independently discoverable Rust case per historical storage/catalog test.
use numera_runtime as production;
#[path = "baseline_catalog_support/mod.rs"]
mod support;
use production::{
    atomic, identity_invariants, identity_references, identity_survey, migration_restore,
    sqlite_snapshot, state_artifacts,
};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};
use support::*;

// tests.test_catalog.TestCatalogIntegrity.test_fuid_is_not_silently_reassigned
#[test]
fn catalog_integrity_fuid_is_not_silently_reassigned() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    for (key, name) in [("s:one", "a.png"), ("s:two", "b.png")] {
        let mut item = root.item(key, name);
        item["file_unique_id"] = json!("FUID1");
        add(&mut cat, item, -1);
    }
    assert_eq!(seen(&mut cat, "FUID1"), "s:one");
}
// tests.test_catalog.TestCatalogIntegrity.test_same_fuid_same_key_is_idempotent
#[test]
fn catalog_integrity_same_fuid_same_key_is_idempotent() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    let mut item = root.item("s:one", "a.png");
    item["file_unique_id"] = json!("FUID1");
    add(&mut cat, item, -1);
    cat.record_seen("FUID1", "s:one").unwrap();
    assert_eq!(seen(&mut cat, "FUID1"), "s:one");
}
// tests.test_catalog.TestCatalogIntegrity.test_merged_duplicate_leaves_no_orphan_file
#[test]
fn catalog_integrity_merged_duplicate_leaves_no_orphan_file() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    add(&mut cat, root.item("s:same", "first.png"), -1);
    assert_eq!(
        add(&mut cat, root.item("s:same", "second.png"), -1),
        ("s:same".into(), false)
    );
    assert!(root.0.join("first.png").is_file());
    assert!(!root.0.join("second.png").exists());
}
// tests.test_catalog.TestCatalogIntegrity.test_readding_the_canonical_file_does_not_delete_it
#[test]
fn catalog_integrity_readding_the_canonical_file_does_not_delete_it() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    let item = root.item("s:same", "only.png");
    add(&mut cat, item.clone(), -1);
    add(&mut cat, item, -1);
    assert!(root.0.join("only.png").is_file());
}
// tests.test_catalog.TestCatalogIntegrity.test_same_catalog_publishes_to_two_bases
#[test]
fn catalog_integrity_same_catalog_publishes_to_two_bases() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    for i in 0..3 {
        let path = root.media(&format!("m{i}.png"), &[i]);
        cat.insert(
            &json!({"content_key":format!("s:k{i}"),"fmt":"static","file_path":path}),
            UTC,
        )
        .unwrap();
    }
    assert_eq!(cat.pending(None, Some("one")).unwrap().len(), 3);
    for row in cat.pending(None, Some("one")).unwrap() {
        let key = row["content_key"].as_str().unwrap();
        upload(&mut cat, key, Some("one"), Some(&format!("cid_{key}")));
    }
    assert_eq!(cat.pending(None, Some("one")).unwrap().len(), 0);
    assert_eq!(cat.pending(None, Some("two")).unwrap().len(), 3);
}
// tests.test_catalog.TestCatalogIntegrity.test_publication_records_are_per_base
#[test]
fn catalog_integrity_publication_records_are_per_base() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    cat.insert(&root.item("s:k", "m.png"), UTC).unwrap();
    upload(&mut cat, "s:k", Some("one"), Some("cid_one"));
    for (operation, base, expected) in [
        ("published", "one", json!(true)),
        ("published", "two", json!(false)),
        ("custom_id", "one", json!("cid_one")),
        ("custom_id", "two", Value::Null),
    ] {
        assert_eq!(
            cat.dispatch(json!({"operation":operation,"base":base,"content_key":"s:k"}))
                .unwrap(),
            expected
        );
    }
}
// tests.test_catalog.TestCatalogIntegrity.test_forgetting_a_deleted_pack_allows_republishing
#[test]
fn catalog_integrity_forgetting_a_deleted_pack_allows_republishing() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    cat.insert(&root.item("s:k", "m.png"), UTC).unwrap();
    upload(&mut cat, "s:k", Some("one"), Some("cid"));
    assert_eq!(cat.pending(None, Some("one")).unwrap().len(), 0);
    assert_eq!(
        cat.dispatch(json!({"operation":"forget","base":"one"}))
            .unwrap(),
        1
    );
    assert_eq!(cat.pending(None, Some("one")).unwrap().len(), 1);
}
// tests.test_catalog.TestCatalogIntegrity.test_legacy_upload_state_is_adopted_once
#[test]
fn catalog_integrity_legacy_upload_state_is_adopted_once() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    cat.insert(&root.item("s:k", "m.png"), UTC).unwrap();
    drop(cat);
    let db = Connection::open(root.database()).unwrap();
    db.execute_batch("UPDATE items SET uploaded=1 WHERE content_key='s:k';DELETE FROM publications;UPDATE meta SET value='0' WHERE key='publications_migrated';").unwrap();
    drop(db);
    let mut cat = root.catalog();
    assert_eq!(
        cat.dispatch(json!({"operation":"bases"})).unwrap(),
        json!(["__legacy__"])
    );
    assert_eq!(
        cat.dispatch(json!({"operation":"adopt","base":"one"}))
            .unwrap(),
        1
    );
    assert_eq!(
        cat.dispatch(json!({"operation":"published","base":"one","content_key":"s:k"}))
            .unwrap(),
        true
    );
    assert_eq!(
        cat.dispatch(json!({"operation":"adopt","base":"two"}))
            .unwrap(),
        0
    );
    assert_eq!(
        cat.dispatch(json!({"operation":"published","base":"two","content_key":"s:k"}))
            .unwrap(),
        false
    );
}
// tests.test_catalog.TestCatalogIntegrity.test_excluded_items_are_not_counted_as_pending
#[test]
fn catalog_integrity_excluded_items_are_not_counted_as_pending() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    for i in 0..3 {
        let path = root.media(&format!("m{i}.png"), &[i]);
        cat.insert(
            &json!({"content_key":format!("s:k{i}"),"fmt":"static","file_path":path}),
            UTC,
        )
        .unwrap();
    }
    cat.inclusion(&["s:k0".into()]).unwrap();
    let stats = cat.stats().unwrap();
    let pending = cat
        .pending(None, None)
        .unwrap()
        .iter()
        .filter(|row| row["fmt"] == "static")
        .count();
    assert_eq!(stats["static"]["total"], 3);
    assert_eq!(stats["static"]["excluded"], 1);
    assert_eq!(stats["static"]["pending"], pending);
    assert_eq!(stats["static"]["pending"], 2);
}
// tests.test_catalog.TestCatalogDurabilityAndScale.test_set_order_survives_a_key_list_past_the_variable_limit
#[test]
fn catalog_durability_set_order_survives_a_key_list_past_the_variable_limit() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    cat.insert(&root.item("s:real", "r.png"), UTC).unwrap();
    let mut order = vec!["s:real".into()];
    order.extend((0..40_000).map(|i| format!("s:ghost{i}")));
    assert_eq!(cat.order(&order).unwrap(), 1);
    assert_eq!(cat.all(None).unwrap()[0]["content_key"], "s:real");
}
// tests.test_catalog.TestCatalogDurabilityAndScale.test_set_order_pushes_unlisted_items_after
#[test]
fn catalog_durability_set_order_pushes_unlisted_items_after() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    for i in 0..3 {
        cat.insert(&root.item(&format!("s:k{i}"), &format!("m{i}.png")), UTC)
            .unwrap();
    }
    assert_eq!(cat.order(&["s:k2".into()]).unwrap(), 1);
    assert_eq!(keys(&cat), ["s:k2", "s:k0", "s:k1"]);
}
// tests.test_catalog.TestCatalog.test_add_new_and_exact_dedup
#[test]
fn catalog_add_new_and_exact_dedup() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    let file = root.media("f.png", b"x");
    let first = add(
        &mut cat,
        json!({"content_key":"s:abc","fmt":"static","file_path":file,"emojis":["😀"],"keywords":["a"],"sources":["p1"]}),
        4,
    );
    assert!(first.1);
    let second = add(
        &mut cat,
        json!({"content_key":"s:abc","fmt":"static","file_path":file,"emojis":["🔥"],"keywords":["b"],"sources":["p2"]}),
        4,
    );
    assert!(!second.1);
    assert_eq!(first.0, second.0);
    let row = cat.get("s:abc").unwrap().unwrap();
    for (field, want) in [
        ("emojis", ["😀", "🔥"]),
        ("keywords", ["a", "b"]),
        ("sources", ["p1", "p2"]),
    ] {
        assert_eq!(
            labels(&row, field),
            want.map(str::to_owned).into_iter().collect()
        );
    }
}
// tests.test_catalog.TestCatalog.test_near_duplicate_merge_by_phash
#[test]
fn catalog_near_duplicate_merge_by_phash() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    let file = root.media("f.png", b"x");
    add(
        &mut cat,
        json!({"content_key":"s:k1","fmt":"static","file_path":file,"emojis":["A"],"phash":0}),
        4,
    );
    assert_eq!(
        add(
            &mut cat,
            json!({"content_key":"s:k2","fmt":"static","file_path":file,"emojis":["B"],"phash":3}),
            4
        ),
        ("s:k1".into(), false)
    );
    assert!(cat.get("s:k2").unwrap().is_none());
    assert_eq!(
        labels(&cat.get("s:k1").unwrap().unwrap(), "emojis"),
        BTreeSet::from(["A".into(), "B".into()])
    );
}
// tests.test_catalog.TestCatalog.test_phash_beyond_threshold_is_new
#[test]
fn catalog_phash_beyond_threshold_is_new() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    let file = root.media("f.png", b"x");
    add(
        &mut cat,
        json!({"content_key":"s:k1","fmt":"static","file_path":file,"phash":0}),
        4,
    );
    assert_eq!(
        add(
            &mut cat,
            json!({"content_key":"s:k2","fmt":"static","file_path":file,"phash":255}),
            4
        ),
        ("s:k2".into(), true)
    );
}
// tests.test_catalog.TestCatalog.test_seen_file_unique_id_skips
#[test]
fn catalog_seen_file_unique_id_skips() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    let mut item = root.item("s:k1", "f.png");
    item["file_unique_id"] = json!("FUID1");
    add(&mut cat, item, 4);
    assert_eq!(seen(&mut cat, "FUID1"), "s:k1");
    assert_eq!(seen(&mut cat, "OTHER"), Value::Null);
}
// tests.test_catalog.TestCatalog.test_pending_and_mark_uploaded
#[test]
fn catalog_pending_and_mark_uploaded() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    let file = root.media("f.png", b"x");
    for (key, fmt) in [("s:a", "static"), ("v:b", "video")] {
        cat.insert(&json!({"content_key":key,"fmt":fmt,"file_path":file}), UTC)
            .unwrap();
    }
    assert_eq!(cat.pending(None, None).unwrap().len(), 2);
    assert_eq!(cat.pending(Some("static"), None).unwrap().len(), 1);
    upload(&mut cat, "s:a", None, Some("cid123"));
    assert_eq!(cat.pending(Some("static"), None).unwrap().len(), 0);
    let row = cat.get("s:a").unwrap().unwrap();
    assert_eq!(row["custom_emoji_id"], "cid123");
    assert_eq!(row["uploaded"], true);
}
// tests.test_catalog.TestCatalog.test_stats
#[test]
fn catalog_stats() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    let file = root.media("f.png", b"x");
    for key in ["s:a", "s:b"] {
        cat.insert(
            &json!({"content_key":key,"fmt":"static","file_path":file}),
            UTC,
        )
        .unwrap();
    }
    upload(&mut cat, "s:a", None, None);
    assert_eq!(
        cat.stats().unwrap()["static"],
        json!({"total":2,"uploaded":1,"excluded":0,"pending":1})
    );
}
// tests.test_catalog.TestCatalog.test_large_phash_64bit
#[test]
fn catalog_large_phash_64bit() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    let file = root.media("f.png", b"x");
    add(
        &mut cat,
        json!({"content_key":"s:big","fmt":"static","file_path":file,"phash":u64::MAX}),
        4,
    );
    assert_eq!(cat.get("s:big").unwrap().unwrap()["phash"], u64::MAX);
    assert_eq!(
        add(
            &mut cat,
            json!({"content_key":"s:big2","fmt":"static","file_path":file,"phash":u64::MAX}),
            4
        ),
        ("s:big".into(), false)
    );
}
// tests.test_catalog.TestCatalog.test_persistence_across_reopen
#[test]
fn catalog_persistence_across_reopen() {
    let root = Scratch::new();
    let mut cat = root.catalog();
    let mut item = root.item("s:a", "f.png");
    item["file_unique_id"] = json!("FX");
    add(&mut cat, item, 4);
    drop(cat);
    let mut cat = root.catalog();
    assert!(cat.get("s:a").unwrap().is_some());
    assert_eq!(seen(&mut cat, "FX"), "s:a");
}

// tests.test_catalog_order.OrderTests.test_default_order_is_insertion
#[test]
fn catalog_order_default_order_is_insertion() {
    let (root, want) = order_fixture();
    let cat = root.catalog();
    assert_eq!(keys(&cat), want);
}
// tests.test_catalog_order.OrderTests.test_set_order_reorders
#[test]
fn catalog_order_set_order_reorders() {
    let (root, mut want) = order_fixture();
    want.reverse();
    let mut cat = root.catalog();
    cat.order(&want).unwrap();
    drop(cat);
    let cat = root.catalog();
    assert_eq!(keys(&cat), want);
}
// tests.test_catalog_order.OrderTests.test_order_persists_across_reopen_and_new_items_go_last
#[test]
fn catalog_order_order_persists_across_reopen_and_new_items_go_last() {
    let (root, original) = order_fixture();
    let want = [2, 0, 1, 3, 4].map(|i| original[i].clone());
    let mut cat = root.catalog();
    cat.order(&want).unwrap();
    let path = root.0.join("m5.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]))
        .save(&path)
        .unwrap();
    cat.insert(
        &json!({"content_key":"s:k99","fmt":"static","file_path":path,"keywords":["k99"]}),
        UTC,
    )
    .unwrap();
    drop(cat);
    let cat = root.catalog();
    let got = keys(&cat);
    assert_eq!(&got[..5], &want);
    assert_eq!(got.last().unwrap(), "s:k99");
}
// tests.test_catalog_order.OrderTests.test_meta_roundtrip
#[test]
fn catalog_order_meta_roundtrip() {
    let (root, _) = order_fixture();
    let mut cat = root.catalog();
    assert_eq!(cat.meta("order_seeded").unwrap(), None);
    cat.dispatch(json!({"operation":"set_meta","key":"order_seeded","value":"1"}))
        .unwrap();
    assert_eq!(cat.meta("order_seeded").unwrap().as_deref(), Some("1"));
}

// tests.test_atomic_state_files.AtomicStateFiles.test_existing_temporary_filename_is_never_claimed_or_removed
#[test]
fn atomic_existing_temporary_filename_is_never_claimed_or_removed() {
    let (root, path) = atomic_fixture();
    atomic::write_json(&path, &json!({"new":"سلام"})).unwrap();
    assert_eq!(read_json(&path), json!({"new":"سلام"}));
    atomic_members(&root);
}
// tests.test_atomic_state_files.AtomicStateFiles.test_serialization_failure_retains_old_state_and_removes_only_owned_temp
#[test]
fn atomic_serialization_failure_retains_old_state_and_removes_only_owned_temp() {
    struct Invalid;
    impl Serialize for Invalid {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("invalid object"))
        }
    }
    let (root, path) = atomic_fixture();
    let value = BTreeMap::from([("invalid", Invalid)]);
    assert!(atomic::write_json(&path, &value).is_err());
    assert_eq!(read_json(&path), json!({"old":true}));
    atomic_members(&root);
}
// tests.test_atomic_state_files.AtomicStateFiles.test_rewriting_an_existing_file_keeps_its_permissions
#[cfg_attr(not(unix), ignore = "permission bits are a POSIX contract")]
#[test]
fn atomic_rewriting_an_existing_file_keeps_its_permissions() {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let (_root, path) = atomic_fixture();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        atomic::write_json(&path, &json!({"new":true})).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
            0o644
        );
        assert_eq!(read_json(&path), json!({"new":true}));
    }
}

// tests.test_sqlite_snapshot.BoundedSnapshot.test_exclusive_source_lock_expires_and_removes_partial_backup
#[test]
fn sqlite_snapshot_exclusive_source_lock_expires_and_removes_partial_backup() {
    let root = Scratch::new();
    let path = root.database();
    let holder = minimal_database(&path);
    holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let source = Connection::open(&path).unwrap();
    let backup = path.with_file_name("catalog.before-test.db");
    let start = Instant::now();
    assert!(
        sqlite_snapshot::backup_with_budget(&source, &backup, Duration::from_millis(150))
            .unwrap_err()
            .contains("did not finish")
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(std::fs::read_dir(path.parent().unwrap()).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_str()
            .unwrap()
            .starts_with("catalog.before")
    }));
    holder.execute_batch("ROLLBACK").unwrap();
    sqlite_snapshot::backup(&source, &backup).unwrap();
    let reopened = Connection::open(&backup).unwrap();
    assert_eq!(
        reopened
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}
// tests.test_sqlite_snapshot.BoundedSnapshot.test_locked_restore_destination_keeps_original_rows_and_can_retry
#[test]
fn sqlite_snapshot_locked_restore_destination_keeps_original_rows_and_can_retry() {
    let root = Scratch::new();
    let path = root.database();
    let holder = minimal_database(&path);
    let source = Connection::open_in_memory().unwrap();
    source
        .execute_batch("CREATE TABLE sentinel(value);INSERT INTO sentinel VALUES('snapshot')")
        .unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let mut destination = Connection::open(&path).unwrap();
    destination
        .busy_timeout(Duration::from_millis(731))
        .unwrap();
    assert!(
        sqlite_snapshot::copy(&source, &mut destination, Duration::from_millis(150))
            .unwrap_err()
            .contains("did not finish")
    );
    assert_eq!(
        destination
            .query_row("PRAGMA busy_timeout", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        731
    );
    holder.execute_batch("ROLLBACK").unwrap();
    assert!(
        destination
            .query_row(
                "SELECT name FROM sqlite_master WHERE name='items'",
                [],
                |r| r.get::<_, String>(0)
            )
            .is_ok()
    );
    sqlite_snapshot::copy(&source, &mut destination, Duration::from_secs(30)).unwrap();
    assert_eq!(
        destination
            .query_row("SELECT value FROM sentinel", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "snapshot"
    );
}
// tests.test_sqlite_snapshot.BoundedSnapshot.test_bad_budgets_refuse_before_writing
#[test]
fn sqlite_snapshot_bad_budgets_refuse_before_writing() {
    let source = Connection::open_in_memory().unwrap();
    let mut destination = Connection::open_in_memory().unwrap();
    // Rust's typed duration rejects negative/nonfinite seconds before the backup API.
    for seconds in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        match Duration::try_from_secs_f64(seconds) {
            Ok(budget) => assert!(
                sqlite_snapshot::copy(&source, &mut destination, budget)
                    .unwrap_err()
                    .contains("positive")
            ),
            Err(_) => assert!(seconds < 0.0 || !seconds.is_finite()),
        }
        assert_eq!(
            destination
                .query_row("SELECT count(*) FROM sqlite_master", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

// tests.test_state_artifacts.CurationPlanMigration.test_migrate_restore_and_repeat_cover_all_plan_references_not_labels
#[test]
fn state_artifacts_migrate_restore_and_repeat_cover_all_plan_references_not_labels() {
    let fixture = PlanFixture::new();
    fixture.apply(NEW, 9).unwrap();
    let after = read_json(&fixture.path);
    assert_eq!(plan_keys(&after), BTreeSet::from([NEW.into()]));
    assert_eq!(after["moves"][0]["label"], OLD);
    assert_eq!(after["held"][0]["label"], "v:free-text");
    let media = Fingerprint {
        key: NEW.into(),
        phash: 9,
    };
    let survey =
        identity_survey::survey(&fixture.data, &fixture.root.0, &media, &runtime()).unwrap();
    assert!(survey.complete());
    assert!(!survey.pending());
    assert!(
        identity_invariants::issues(&fixture.data, &fixture.root.0, &media, &runtime())
            .unwrap()
            .is_empty()
    );
    let count = fixture.manifests().len();
    fixture.apply(NEW, 9).unwrap();
    assert_eq!(fixture.manifests().len(), count);
    let manifest = fixture.manifests().pop().unwrap();
    assert!(
        read_json(&manifest)["states"]
            .get("pack_plan.json")
            .is_some()
    );
    migration_restore::restore(&fixture.data, &fixture.root.0, &manifest, true).unwrap();
    assert_eq!(read_json(&fixture.path), fixture.before);
    assert_eq!(fixture.row_keys(), [OLD]);
    migration_restore::restore(&fixture.data, &fixture.root.0, &manifest, true).unwrap();
    assert_eq!(read_json(&fixture.path), fixture.before);
}
// tests.test_state_artifacts.CurationPlanMigration.test_later_plan_edit_blocks_rollback_without_overwriting_it
#[test]
fn state_artifacts_later_plan_edit_blocks_rollback_without_overwriting_it() {
    let fixture = PlanFixture::new();
    fixture.apply(NEW, 9).unwrap();
    let manifest = fixture.manifests().pop().unwrap();
    let mut edited = read_json(&fixture.path);
    edited["moves"][0]["to_pack"] = json!(3);
    atomic::write_json(&fixture.path, &edited).unwrap();
    assert!(
        migration_restore::restore(&fixture.data, &fixture.root.0, &manifest, true)
            .unwrap_err()
            .contains("state changed")
    );
    assert_eq!(read_json(&fixture.path), edited);
    assert_eq!(fixture.row_keys(), [NEW]);
}
// tests.test_state_artifacts.CurationPlanMigration.test_missing_plan_reference_is_incomplete_even_without_a_video_change
#[test]
fn state_artifacts_missing_plan_reference_is_incomplete_even_without_a_video_change() {
    let fixture = PlanFixture::new();
    let mut doc = fixture.before.clone();
    doc["targets"] = json!([["v:missing", 2]]);
    atomic::write_json(&fixture.path, &doc).unwrap();
    let required = identity_references::stale(&fixture.data, true).unwrap();
    assert_eq!(required["pack_plan.json"], ["v:missing"]);
    assert!(
        fixture
            .apply(OLD, 0)
            .unwrap_err()
            .contains("required state")
    );
    assert!(fixture.manifests().is_empty());
    assert!(!fixture.journal().exists());
}
// tests.test_state_artifacts.CurationPlanMigration.test_invalid_plan_is_refused_before_a_snapshot_or_mutation
#[test]
fn state_artifacts_invalid_plan_is_refused_before_a_snapshot_or_mutation() {
    let fixture = PlanFixture::new();
    for text in [
        "null",
        r#"{"version":999}"#,
        r#"{"version":1,"moves":[],"held":{},"targets":[]}"#,
    ] {
        std::fs::write(&fixture.path, text).unwrap();
        assert!(
            state_artifacts::rewrite(
                "pack_plan.json",
                &read_json(&fixture.path),
                &BTreeMap::from([(OLD.into(), NEW.into())])
            )
            .is_err()
        );
        assert!(fixture.apply(NEW, 9).is_err());
        assert_eq!(fixture.row_keys(), [OLD]);
        assert!(fixture.manifests().is_empty());
        assert!(!fixture.journal().exists());
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
