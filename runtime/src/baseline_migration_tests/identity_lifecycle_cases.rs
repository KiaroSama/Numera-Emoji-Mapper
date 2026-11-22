use super::{identity_process as process, identity_support::*};
use crate::{catalog::Catalog, locks, migration_apply, migration_database, sqlite_snapshot};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

fn pending_identity() -> (Fixture, Fake) {
    let f = Fixture::new();
    f.add(
        "v:old",
        &f.media("clip.webm"),
        "video",
        None,
        Some("cid-1"),
        Some("F-1"),
    );
    f.plan("pk", &json!({"mixed":["v:old"]}));
    let fake = f.fake(&[("clip.webm", "v:new", Some(1))]);
    (f, fake)
}
// tests.test_identity_migration.MigratedPathsStayDataRelative.test_rows_written_absolute_by_the_old_code_still_verify
#[test]
fn identity_rows_written_absolute_by_the_old_code_still_verify() {
    let (f, fake) = relative_rows();
    assert_eq!(process::completed(&f, &fake, "absolute-paths"), 0);
    assert_eq!(process::result(&f), 0);
    assert!(Path::new(&paths(&f)["v:keep"]).is_absolute());
}
// tests.test_identity_migration.TheBackupIsWalSafe.test_an_unverifiable_backup_stops_before_any_change
#[test]
fn identity_an_unverifiable_backup_stops_before_any_change() {
    let f = Fixture::new();
    f.add("v:old", &f.media("clip.webm"), "video", None, None, None);
    let fake = f.fake(&[("clip.webm", "v:new", Some(1))]);
    assert_eq!(process::completed(&f, &fake, "refuse-backup"), 0);
    assert_eq!(process::result(&f), 4);
    assert_eq!(f.col("items", "content_key"), ["v:old"]);
}
// tests.test_identity_migration.AnInterruptedMigrationIsResumable.test_a_crash_after_the_backup_resumes
#[test]
fn identity_a_crash_after_the_backup_resumes() {
    let (f, fake) = pending_identity();
    process::kill(&f, &fake, "backup");
    assert!(migration_apply::journal(&f.data).unwrap().is_some());
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        0
    );
    assert_eq!(f.col("items", "content_key"), ["v:new"]);
    assert_eq!(
        read_json(&f.data.join("publish_plan_pk.json")),
        json!({"mixed":["v:new"]})
    );
    assert!(migration_apply::journal(&f.data).unwrap().is_none());
}
// tests.test_identity_migration.AnInterruptedMigrationIsResumable.test_a_crash_after_the_database_stage_resumes
#[test]
fn identity_a_crash_after_the_database_stage_resumes() {
    let (f, fake) = pending_identity();
    process::kill(&f, &fake, "database");
    assert_eq!(f.col("items", "content_key"), ["v:new"]);
    assert_eq!(
        read_json(&f.data.join("publish_plan_pk.json")),
        json!({"mixed":["v:old"]})
    );
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        0
    );
    assert_eq!(f.col("items", "content_key"), ["v:new"]);
    assert_eq!(
        read_json(&f.data.join("publish_plan_pk.json")),
        json!({"mixed":["v:new"]})
    );
    assert!(migration_apply::journal(&f.data).unwrap().is_none());
}
// tests.test_identity_migration.AnInterruptedMigrationIsResumable.test_a_crash_after_the_state_stage_resumes
#[test]
fn identity_a_crash_after_the_state_stage_resumes() {
    let (f, fake) = pending_identity();
    process::kill(&f, &fake, "state");
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        0
    );
    assert_eq!(f.col("items", "content_key"), ["v:new"]);
    assert_eq!(
        read_json(&f.data.join("publish_plan_pk.json")),
        json!({"mixed":["v:new"]})
    );
    assert!(migration_apply::journal(&f.data).unwrap().is_none());
}
// tests.test_identity_migration.AnInterruptedMigrationIsResumable.test_report_names_an_interrupted_migration
#[test]
fn identity_report_names_an_interrupted_migration() {
    let (f, fake) = pending_identity();
    process::kill(&f, &fake, "database");
    assert_eq!(process::cli(&f, &fake, &["report"]), 3);
    assert!(f.journal().is_file());
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_actual_publisher_lock_prevents_migration
#[test]
fn lifecycle_actual_publisher_lock_prevents_migration() {
    let f = Fixture::new();
    let (src, dest, fake) = f.pending();
    let _held = locks::Lease::acquire(&f.data.join("publish_audit.lock"), UTC).unwrap();
    assert!(f.apply(&fake).is_err());
    assert!(src.is_file());
    assert!(!dest.exists());
    assert_eq!(f.col("items", "content_key"), ["v:aaaaaaaaaaaaaaaa"]);
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_unrelated_rename_destination_is_never_overwritten
#[test]
fn lifecycle_unrelated_rename_destination_is_never_overwritten() {
    let f = Fixture::new();
    let (src, dest, fake) = f.pending();
    std::fs::write(&dest, b"unrelated sentinel").unwrap();
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        4
    );
    assert_eq!(std::fs::read(&dest).unwrap(), b"unrelated sentinel");
    assert!(src.is_file());
    assert_eq!(f.col("items", "content_key"), ["v:aaaaaaaaaaaaaaaa"]);
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_required_stale_manifest_cannot_be_called_plan_noise
#[test]
fn lifecycle_required_stale_manifest_cannot_be_called_plan_noise() {
    let f = Fixture::new();
    f.add(
        "v:correct",
        &f.media("clip.webm"),
        "video",
        Some(7),
        None,
        None,
    );
    let state = f.state("audit", &json!({"sets":[{"keys":["v:lost"]}]}));
    let fake = f.fake(&[("clip.webm", "v:correct", Some(7))]);
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        4
    );
    assert_ne!(process::cli(&f, &fake, &["report"]), 0);
    assert_eq!(read_json(&state)["sets"][0]["keys"], json!(["v:lost"]));
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_late_missing_media_retains_journal_and_refuses_success
#[test]
fn lifecycle_late_missing_media_retains_journal_and_refuses_success() {
    let f = Fixture::new();
    let (src, _, fake) = f.pending();
    assert_eq!(process::completed(&f, &fake, "late-missing"), 0);
    assert_eq!(process::result(&f), 4);
    assert!(f.journal().is_file());
    assert!(!src.exists());
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_completed_migration_restores_files_state_and_identifiers
#[test]
fn lifecycle_completed_migration_restores_files_state_and_identifiers() {
    super::identity_lazy_factory_order::help_does_not_construct_the_codec();
    super::identity_lazy_factory_order::missing_catalog_does_not_construct_the_codec();
    super::identity_lazy_factory_order::missing_restore_bundle_does_not_construct_the_codec();
    super::identity_lazy_factory_order::successful_restore_does_not_construct_the_codec();
    let f = Fixture::new();
    let (src, dest, fake) = f.pending();
    let before = f.signature();
    let original = std::fs::read(&src).unwrap();
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        0
    );
    let manifest = f.manifest();
    assert_eq!(
        process::cli(
            &f,
            &fake,
            &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"]
        ),
        0
    );
    assert_eq!(f.signature(), before);
    assert_eq!(std::fs::read(&src).unwrap(), original);
    assert!(!dest.exists());
    assert_eq!(f.col("publications", "custom_emoji_id"), ["CID"]);
    assert_eq!(f.col("seen_files", "file_unique_id"), ["FUID"]);
    assert_eq!(
        read_json(&f.data.join("publish_audit.json"))["sets"][0]["keys"],
        json!(["v:aaaaaaaaaaaaaaaa"])
    );
    assert_eq!(
        process::cli(
            &f,
            &fake,
            &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"]
        ),
        0
    );
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_restore_preserves_an_identical_preexisting_destination
#[test]
fn lifecycle_restore_preserves_an_identical_preexisting_destination() {
    let f = Fixture::new();
    let (src, dest, fake) = f.pending();
    std::fs::copy(&src, &dest).unwrap();
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        0
    );
    let manifest = f.manifest();
    assert_eq!(
        process::cli(
            &f,
            &fake,
            &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"]
        ),
        0
    );
    assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&dest).unwrap());
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_restore_refuses_later_catalog_or_file_changes
#[test]
fn lifecycle_restore_refuses_later_catalog_or_file_changes() {
    let f = Fixture::new();
    let (src, dest, fake) = f.pending();
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        0
    );
    let manifest = f.manifest();
    f.db()
        .execute("UPDATE publications SET custom_emoji_id='LATER'", [])
        .unwrap();
    assert_eq!(
        process::cli(
            &f,
            &fake,
            &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"]
        ),
        4
    );
    assert_eq!(f.col("publications", "custom_emoji_id"), ["LATER"]);
    assert!(dest.is_file());
    assert!(!src.exists());
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_journal_bound_to_another_directory_refuses_without_mutation
#[test]
fn lifecycle_journal_bound_to_another_directory_refuses_without_mutation() {
    let f = Fixture::new();
    let (src, dest, fake) = f.pending();
    process::kill(&f, &fake, "backup");
    let mut doc = read_json(&f.journal());
    doc["data_dir"] = json!(f.data.join("elsewhere"));
    crate::atomic::write_json(&f.journal(), &doc).unwrap();
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        4
    );
    assert!(src.is_file());
    assert!(!dest.exists());
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_restore_after_a_move_before_database_path_commit
#[test]
fn lifecycle_restore_after_a_move_before_database_path_commit() {
    let f = Fixture::new();
    let (src, dest, fake) = f.pending();
    process::kill(&f, &fake, "move-before-path");
    assert!(!src.exists());
    assert_eq!(f.col("items", "file_path"), [src.to_str().unwrap()]);
    let manifest = f.manifest();
    assert_eq!(
        process::cli(
            &f,
            &fake,
            &["restore", "--bundle", manifest.to_str().unwrap(), "--apply"]
        ),
        0
    );
    assert!(src.is_file());
    assert!(!dest.exists());
    assert_eq!(f.col("items", "content_key"), ["v:aaaaaaaaaaaaaaaa"]);
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_existing_link_after_interruption_is_replayed_without_losing_bytes
#[test]
fn lifecycle_existing_link_after_interruption_is_replayed_without_losing_bytes() {
    let f = Fixture::new();
    let (src, dest, fake) = f.pending();
    let original = std::fs::read(&src).unwrap();
    process::kill(&f, &fake, "linked-before-unlink");
    assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&dest).unwrap());
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        0
    );
    assert!(!src.exists());
    let paths = f.col("items", "file_path");
    assert_eq!(
        paths,
        [format!("./{}", dest.file_name().unwrap().to_str().unwrap())]
    );
    let stored = crate::paths::resolve(&f.data, &paths[0], &f.root.0);
    assert_eq!(stored.canonicalize().unwrap(), dest.canonicalize().unwrap());
    assert_eq!(std::fs::read(&stored).unwrap(), original);
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_partial_legacy_migration_requires_full_backup_coverage
#[test]
fn lifecycle_partial_legacy_migration_requires_full_backup_coverage() {
    let f = Fixture::new();
    let (src, dest, fake) = f.pending();
    let original = f.data.join("catalog.before-original.db");
    sqlite_snapshot::backup(&f.db(), &original).unwrap();
    migration_database::rewrite(
        &mut f.db(),
        &BTreeMap::from([("v:aaaaaaaaaaaaaaaa".into(), "v:bbbbbbbbbbbbbbbb".into())]),
        &BTreeMap::from([("v:bbbbbbbbbbbbbbbb".into(), Some(7))]),
    )
    .unwrap();
    let insufficient = f.data.join("catalog.before-insufficient.db");
    sqlite_snapshot::backup(&f.db(), &insufficient).unwrap();
    assert_eq!(
        process::cli(
            &f,
            &fake,
            &[
                "migrate-video-keys",
                "--apply",
                "--from-backup",
                insufficient.to_str().unwrap()
            ]
        ),
        4
    );
    assert!(src.is_file());
    assert_eq!(
        process::cli(
            &f,
            &fake,
            &[
                "migrate-video-keys",
                "--apply",
                "--from-backup",
                original.to_str().unwrap()
            ]
        ),
        0
    );
    assert_eq!(process::cli(&f, &fake, &["report"]), 0);
    assert!(dest.is_file());
    assert!(!src.exists());
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_late_decode_failure_cannot_retire_the_journal
#[test]
fn lifecycle_late_decode_failure_cannot_retire_the_journal() {
    let f = Fixture::new();
    let (_, _, fake) = f.pending();
    assert_eq!(process::completed(&f, &fake, "late-decode"), 0);
    assert_eq!(process::result(&f), 4);
    assert!(f.journal().is_file());
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_orphaned_identifiers_prevent_a_clean_report_or_noop
#[test]
fn lifecycle_orphaned_identifiers_prevent_a_clean_report_or_noop() {
    let f = Fixture::new();
    f.add(
        "v:correct",
        &f.media("clip.webm"),
        "video",
        Some(7),
        None,
        None,
    );
    f.db()
        .execute("INSERT INTO seen_files VALUES('FOREIGN','v:absent')", [])
        .unwrap();
    let fake = f.fake(&[("clip.webm", "v:correct", Some(7))]);
    assert_eq!(process::cli(&f, &fake, &["report"]), 4);
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        4
    );
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_cli_recovers_after_each_state_replacement_and_journal_retirement
#[test]
fn lifecycle_cli_recovers_after_each_state_replacement_and_journal_retirement() {
    for point in [
        "database-before",
        "database-after",
        "publish_audit.json",
        "publish_plan_audit.json",
        "verified",
    ] {
        let f = Fixture::new();
        let (src, dest, fake) = f.pending();
        process::kill(&f, &fake, point);
        assert_eq!(
            process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
            0,
            "{point}"
        );
        assert_eq!(process::cli(&f, &fake, &["report"]), 0, "{point}");
        assert!(dest.is_file(), "{point}");
        assert!(!src.exists(), "{point}");
    }
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_a_second_catalog_connection_cannot_write_during_migration
#[test]
fn lifecycle_a_second_catalog_connection_cannot_write_during_migration() {
    let f = Fixture::new();
    let (_, _, fake) = f.pending();
    let mut child = process::held(&f, &fake, "migrate", "state", false);
    let error = Catalog::open(&f.data.join("catalog.db"), UTC, &f.root.0)
        .err()
        .expect("second writer entered migration");
    assert!(error.contains("catalog ownership refused"), "{error}");
    child.finish().unwrap();
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        0
    );
    assert_eq!(f.col("publications", "custom_emoji_id"), ["CID"]);
}
// tests.test_migration_lifecycle.MigrationCannotCertifyDamage.test_native_death_after_second_move_replays_via_cli
#[test]
fn lifecycle_native_death_after_second_move_replays_via_cli() {
    let f = Fixture::new();
    let mut values = Vec::new();
    let mut names = Vec::new();
    let mut keys = Vec::new();
    for i in 0..3u8 {
        let key = format!("v:{}", format!("{i:02x}").repeat(16));
        let name = format!("{i:03}_video_{}.webm", &key[2..14]);
        let path = f.data.join(&name);
        let mut bytes = b"\x1a\x45\xdf\xa3fixture".to_vec();
        bytes.push(i);
        std::fs::write(&path, bytes).unwrap();
        f.add(
            &key,
            &path,
            "video",
            None,
            Some(&format!("CID{i}")),
            Some(&format!("FUID{i}")),
        );
        names.push(name);
        keys.push(format!("v:{}", format!("{:02x}", i + 16).repeat(16)));
    }
    for i in 0..3 {
        values.push((names[i].as_str(), keys[i].as_str(), Some(i as u64)));
    }
    let fake = f.fake(&values);
    assert_eq!(process::completed(&f, &fake, "exit73"), 73);
    assert!(f.journal().is_file());
    assert_eq!(process::completed(&f, &fake, "migrate"), 0);
    assert_eq!(process::result(&f), 0);
    assert!(!f.journal().exists());
    assert_eq!(f.col("items", "content_key"), keys);
    for path in f.col("items", "file_path") {
        assert!(crate::paths::resolve(&f.data, &path, &f.root.0).is_file());
    }
}
