//! Individually traceable historical migration bookkeeping tests against real durable stores.
use numera_runtime as production;
#[allow(dead_code)]
#[path = "baseline_catalog_support/mod.rs"]
mod baseline_catalog_support;
#[path = "baseline_identity_migration_support/mod.rs"]
#[allow(dead_code)]
mod support;
use production::{locks, migration_apply, migration_provenance};
use serde_json::json;
use std::collections::BTreeMap;
use support::*;

// tests.test_identity_migration.EveryDurableReferenceMoves.test_the_state_file_is_rewritten_not_just_the_tables
#[test]
fn identity_the_state_file_is_rewritten_not_just_the_tables() {
    let f = Fixture::new();
    let m = f.media("clip.webm");
    f.add("v:old", &m, "video", None, Some("cid-1"), Some("F-1"));
    let state=f.state("pk",&json!({"base":"pk","sets":[{"fmt":"mixed","index":1,"name":"pk1","title":"Pack 1","live":1,"keys":["v:old"]}],"sent":[],"skipped":["v:old"],"sent_full":[]}));
    let plan = f.plan("pk", &json!({"mixed":["v:old"]}));
    f.apply(&f.fake(&[("clip.webm", "v:new", Some(5))]))
        .unwrap();
    for table in ["items", "publications", "seen_files"] {
        assert_eq!(f.col(table, "content_key"), ["v:new"]);
    }
    let doc = read_json(&state);
    assert_eq!(doc["sets"][0]["keys"], json!(["v:new"]));
    assert_eq!(doc["skipped"], json!(["v:new"]));
    assert_eq!(read_json(&plan), json!({"mixed":["v:new"]}));
}
// tests.test_identity_migration.EveryDurableReferenceMoves.test_multiple_bases_all_move
#[test]
fn identity_multiple_bases_all_move() {
    let f = Fixture::new();
    f.add("v:old", &f.media("clip.webm"), "video", None, None, None);
    for base in ["alpha", "beta"] {
        f.plan(base, &json!({"mixed":["v:old"]}));
    }
    f.apply(&f.fake(&[("clip.webm", "v:new", Some(1))]))
        .unwrap();
    for base in ["alpha", "beta"] {
        assert_eq!(
            read_json(&f.data.join(format!("publish_plan_{base}.json"))),
            json!({"mixed":["v:new"]})
        );
    }
}
// tests.test_identity_migration.EveryDurableReferenceMoves.test_the_telegram_identifiers_are_preserved
#[test]
fn identity_the_telegram_identifiers_are_preserved() {
    let f = Fixture::new();
    f.add(
        "v:old",
        &f.media("clip.webm"),
        "video",
        None,
        Some("cid-keepme"),
        Some("F-1"),
    );
    f.apply(&f.fake(&[("clip.webm", "v:new", Some(1))]))
        .unwrap();
    let row: (String, String) = f
        .db()
        .query_row(
            "SELECT set_name,custom_emoji_id FROM publications",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(row, ("set1".into(), "cid-keepme".into()));
}
// tests.test_identity_migration.EveryDurableReferenceMoves.test_a_hash_only_correction_is_applied_even_when_the_key_holds
#[test]
fn identity_a_hash_only_correction_is_applied_even_when_the_key_holds() {
    let f = Fixture::new();
    f.add(
        "v:same",
        &f.media("clip.webm"),
        "video",
        Some(123),
        None,
        None,
    );
    f.apply(&f.fake(&[("clip.webm", "v:same", Some(0))]))
        .unwrap();
    assert_eq!(
        f.db()
            .query_row("SELECT phash FROM items", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}
// tests.test_identity_migration.EveryDurableReferenceMoves.test_an_archived_file_is_renamed_to_match_its_new_key
#[test]
fn identity_an_archived_file_is_renamed_to_match_its_new_key() {
    let f = Fixture::new();
    let archived = f.data.join("007_video_0123456789ab.webm");
    std::fs::write(&archived, b"\x1a\x45\xdf\xa3archived").unwrap();
    f.add("v:0123456789abcdef", &archived, "video", None, None, None);
    f.apply(&f.fake(&[("007_video_0123456789ab.webm", "v:fedcba9876543210", Some(1))]))
        .unwrap();
    assert!(f.data.join("007_video_fedcba987654.webm").is_file());
    assert!(!archived.is_file());
    assert_eq!(
        f.col("items", "file_path"),
        ["./007_video_fedcba987654.webm"]
    );
}
// tests.test_identity_migration.EveryDurableReferenceMoves.test_a_second_migration_is_a_verified_no_op
#[test]
fn identity_a_second_migration_is_a_verified_no_op() {
    let f = Fixture::new();
    f.add("v:old", &f.media("clip.webm"), "video", None, None, None);
    f.plan("pk", &json!({"mixed":["v:old"]}));
    let fake = f.fake(&[("clip.webm", "v:new", Some(1))]);
    f.apply(&fake).unwrap();
    let result = f.apply(&fake).unwrap();
    assert_eq!(result["noop"], true);
    assert_eq!(f.col("items", "content_key"), ["v:new"]);
}
// tests.test_identity_migration.EveryDurableReferenceMoves.test_the_migration_takes_the_pack_family_lock
#[test]
fn identity_the_migration_takes_the_pack_family_lock() {
    let f = Fixture::new();
    f.add("v:old", &f.media("clip.webm"), "video", None, None, None);
    f.plan("pk", &json!({"mixed":["v:old"]}));
    let path = locks::pack_family_path(&f.root.0, "pk");
    let held = locks::Lease::acquire(&path, UTC).unwrap();
    let fake = f.fake(&[("clip.webm", "v:new", Some(1))]);
    std::thread::scope(|scope| {
        let (done, finished) = std::sync::mpsc::channel();
        let fixture = &f;
        let fingerprint = &fake;
        let worker = scope.spawn(move || {
            let result = fixture.apply(fingerprint);
            done.send(()).unwrap();
            result
        });
        let finished = finished
            .recv_timeout(std::time::Duration::from_secs(8))
            .is_ok();
        drop(held);
        let result = worker.join().unwrap();
        assert!(
            finished || result.is_err(),
            "migration ignored the family lock"
        );
        assert!(
            !(finished && result.is_ok()),
            "migration ran while the family lock was held"
        );
    });
}
// tests.test_identity_migration.MigratedPathsStayDataRelative.test_unchanged_rows_stay_byte_identical_and_renames_stay_relative
#[test]
fn identity_unchanged_rows_stay_byte_identical_and_renames_stay_relative() {
    let (f, fake) = relative_rows();
    f.apply(&fake).unwrap();
    assert_eq!(
        paths(&f),
        BTreeMap::from([
            ("v:keep".into(), "./keep.webm".into()),
            (
                "v:fedcba9876543210".into(),
                "./007_video_fedcba987654.webm".into()
            )
        ])
    );
}
// tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_a_missing_file_is_not_reported_as_unchanged
#[test]
fn identity_a_missing_file_is_not_reported_as_unchanged() {
    let f = Fixture::new();
    f.add(
        "v:gone",
        &f.data.join("absent.webm"),
        "video",
        None,
        None,
        None,
    );
    assert!(f.apply(&Fake::default()).is_err());
    assert_eq!(f.col("items", "content_key"), ["v:gone"]);
}
// tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_some_missing_still_blocks_the_whole_migration
#[test]
fn identity_some_missing_still_blocks_the_whole_migration() {
    let f = Fixture::new();
    f.add("v:ok", &f.media("clip.webm"), "video", None, None, None);
    f.add(
        "v:gone",
        &f.data.join("absent.webm"),
        "video",
        None,
        None,
        None,
    );
    assert!(
        f.apply(&f.fake(&[("clip.webm", "v:new", Some(1))]))
            .is_err()
    );
    assert_eq!(f.col("items", "content_key"), ["v:gone", "v:ok"]);
}
// tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_all_undecodable_blocks_it_too
#[test]
fn identity_all_undecodable_blocks_it_too() {
    let f = Fixture::new();
    for key in ["a", "b"] {
        f.add(
            &format!("v:{key}"),
            &f.media(&format!("{key}.webm")),
            "video",
            None,
            None,
            None,
        );
    }
    assert!(f.apply(&Fake::default()).is_err());
    assert_eq!(f.col("items", "content_key"), ["v:a", "v:b"]);
}
// tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_a_collision_refuses_instead_of_merging
#[test]
fn identity_a_collision_refuses_instead_of_merging() {
    let f = Fixture::new();
    for key in ["a", "b"] {
        f.add(
            &format!("v:{key}"),
            &f.media(&format!("{key}.webm")),
            "video",
            None,
            Some(&format!("cid-{key}")),
            None,
        );
    }
    assert!(
        f.apply(&f.fake(&[("a.webm", "v:same", Some(1)), ("b.webm", "v:same", Some(1))]))
            .is_err()
    );
    assert_eq!(f.col("items", "content_key"), ["v:a", "v:b"]);
}
// tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_colliding_with_a_row_that_is_not_moving_also_refuses
#[test]
fn identity_colliding_with_a_row_that_is_not_moving_also_refuses() {
    let f = Fixture::new();
    f.add("v:taken", &f.media("b.webm"), "static", None, None, None);
    f.add("v:old", &f.media("a.webm"), "video", None, None, None);
    assert!(f.apply(&f.fake(&[("a.webm", "v:taken", Some(1))])).is_err());
    assert_eq!(f.col("items", "content_key"), ["v:old", "v:taken"]);
}
// tests.test_identity_migration.APreexistingPartialMigrationIsRecoverable.test_the_map_is_recovered_from_the_backup
#[test]
fn identity_the_map_is_recovered_from_the_backup() {
    let (f, backup, fake) = preexisting();
    let got =
        migration_provenance::recover(&f.data, &f.root.0, &backup, &fake, &runtime()).unwrap();
    assert_eq!(
        got,
        BTreeMap::from([
            ("v:aaaaaaaaaaaaaaaa".into(), "v:bbbbbbbbbbbbbbbb".into()),
            ("v:cccccccccccccccc".into(), "v:dddddddddddddddd".into())
        ])
    );
}
// tests.test_identity_migration.APreexistingPartialMigrationIsRecoverable.test_applying_it_repairs_the_state_file
#[test]
fn identity_applying_it_repairs_the_state_file() {
    let (f, backup, fake) = preexisting();
    f.recover_apply(&fake, &backup).unwrap();
    assert_eq!(
        read_json(&f.data.join("publish_plan_pk.json")),
        json!({"mixed":["v:bbbbbbbbbbbbbbbb","v:dddddddddddddddd"]})
    );
}
// tests.test_identity_migration.APreexistingPartialMigrationIsRecoverable.test_a_key_absent_from_the_backup_is_left_alone
#[test]
fn identity_a_key_absent_from_the_backup_is_left_alone() {
    let (f, backup, fake) = preexisting();
    let path = f.plan("old", &json!({"mixed":["v:ghostghostghost"]}));
    f.recover_apply(&fake, &backup).unwrap();
    assert_eq!(read_json(&path), json!({"mixed":["v:ghostghostghost"]}));
}
// tests.test_identity_migration.APreexistingPartialMigrationIsRecoverable.test_an_ambiguous_pairing_is_not_guessed
#[test]
fn identity_an_ambiguous_pairing_is_not_guessed() {
    let f = Fixture::new();
    let backup = f.data.join("catalog.before-video-identity-1.db");
    let old = schema(&backup, true);
    let shared = f.media("same.webm");
    add_to(
        &old,
        "v:oldoldoldoldold1",
        shared.to_str().unwrap(),
        "video",
        None,
        Some("ambiguous-CID"),
        None,
    );
    drop(old);
    for key in ["v:new1", "v:new2"] {
        f.add(key, &shared, "video", None, Some("ambiguous-CID"), None);
    }
    assert!(
        migration_provenance::recover(&f.data, &f.root.0, &backup, &Fake::default(), &runtime())
            .unwrap()
            .is_empty()
    );
}
// tests.test_identity_migration.TheBackupIsWalSafe.test_the_restored_copy_equals_the_pre_migration_catalog
#[test]
fn identity_the_restored_copy_equals_the_pre_migration_catalog() {
    let f = Fixture::new();
    f.add(
        "v:old",
        &f.media("clip.webm"),
        "video",
        None,
        Some("cid-1"),
        Some("F-1"),
    );
    let before = ["items", "publications", "seen_files"].map(|table| f.col(table, "content_key"));
    f.apply(&f.fake(&[("clip.webm", "v:new", Some(1))]))
        .unwrap();
    let backups = f.backups();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        ["items", "publications", "seen_files"].map(|table| col(&backups[0], table, "content_key")),
        before
    );
}
// tests.test_identity_migration.AnInterruptedMigrationIsResumable.test_an_unreadable_journal_is_not_deleted_blindly
#[test]
fn identity_an_unreadable_journal_is_not_deleted_blindly() {
    let f = Fixture::new();
    std::fs::write(f.journal(), "{not json").unwrap();
    assert!(migration_apply::journal(&f.data).is_err());
    assert!(f.journal().is_file());
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
