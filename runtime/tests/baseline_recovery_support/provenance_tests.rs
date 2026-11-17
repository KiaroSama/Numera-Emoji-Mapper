use super::run_with_codec_factory;
use crate::baseline_identity_support::{self as fixture, Fake, Fixture};
use crate::{logging::RunLog, migration_provenance};
use serde_json::json;
use std::path::{Path, PathBuf};
const OLD: &str = "v:aaaaaaaaaaaaaaaa";
const CURRENT: &str = "v:bbbbbbbbbbbbbbbb";
const RIVAL: &str = "v:cccccccccccccccc";
fn snapshots(
    same: bool,
    old_ids: (Option<&str>, Option<&str>),
    current_ids: (Option<&str>, Option<&str>),
) -> (Fixture, PathBuf, PathBuf) {
    let f = Fixture::new();
    let old = f.data.join("001_video_aaaaaaaaaaaa.webm");
    let current = if same {
        old.clone()
    } else {
        f.data.join("001_video_bbbbbbbbbbbb.webm")
    };
    std::fs::write(&current, b"\x1a\x45\xdf\xa3different current artwork").unwrap();
    let backup = f.data.join("catalog.old.db");
    let db = fixture::schema(&backup, false);
    fixture::add_to(
        &db,
        OLD,
        old.to_str().unwrap(),
        "video",
        None,
        old_ids.0,
        old_ids.1,
    );
    drop(db);
    f.add(
        CURRENT,
        &current,
        "video",
        Some(7),
        current_ids.0,
        current_ids.1,
    );
    f.state("audit", &json!({"sets":[{"keys":[OLD]}]}));
    (f, backup, current)
}
fn codec(f: &Fixture, current: &Path) -> Fake {
    f.fake(&[(
        current.file_name().unwrap().to_str().unwrap(),
        CURRENT,
        Some(7),
    )])
}
fn cli(f: &Fixture, backup: &Path, media: Fake) -> u8 {
    let log = RunLog::open(&f.config.root, "baseline-provenance", vec![]);
    run_with_codec_factory(
        &f.config,
        &[
            "migrate-video-keys".into(),
            "--apply".into(),
            "--from-backup".into(),
            backup.to_str().unwrap().into(),
            "--data-dir".into(),
            f.data.to_str().unwrap().into(),
        ],
        &log,
        |_, _| Ok(media),
    )
    .unwrap()
}
fn refused(f: &Fixture, backup: &Path, current: &Path) {
    let before = std::fs::read(current).unwrap();
    let original_state = std::fs::read(f.data.join("publish_audit.json")).unwrap();
    assert_eq!(cli(f, backup, codec(f, current)), 4);
    assert_eq!(std::fs::read(current).unwrap(), before);
    assert_eq!(
        std::fs::read(f.data.join("publish_audit.json")).unwrap(),
        original_state
    );
    assert_eq!(f.col("items", "content_key"), [CURRENT]);
    assert!(!f.journal().exists());
}

// Original: tests.test_migration_provenance.LegacyRecoveryRequiresProvenance.test_conflicting_shared_fuid_and_cid_refuse
#[test]
fn provenance_test_conflicting_shared_fuid_and_cid_refuse() {
    let (f, b, p) = snapshots(
        false,
        (Some("OLD-CID"), Some("OLD-FUID")),
        (Some("OTHER-CID"), Some("OLD-FUID")),
    );
    let other = f.media("different-rival.webm");
    f.add(
        RIVAL,
        &other,
        "video",
        Some(9),
        Some("OLD-CID"),
        Some("RIVAL-FUID"),
    );
    let fake = f.fake(&[
        (p.file_name().unwrap().to_str().unwrap(), CURRENT, Some(7)),
        ("different-rival.webm", RIVAL, Some(9)),
    ]);
    assert!(
        migration_provenance::recover(&f.data, &f.root.0, &b, &fake, &fixture::runtime())
            .unwrap()
            .is_empty()
    );
    assert_eq!(cli(&f, &b, fake), 4);
}

// Original: tests.test_migration_provenance.LegacyRecoveryRequiresProvenance.test_equal_path_with_different_immutable_ids_refuses
#[test]
fn provenance_test_equal_path_with_different_immutable_ids_refuses() {
    let (f, b, p) = snapshots(
        true,
        (Some("OLD-CID"), Some("OLD-FUID")),
        (Some("OTHER-CID"), Some("OTHER-FUID")),
    );
    refused(&f, &b, &p);
}

// Original: tests.test_migration_provenance.LegacyRecoveryRequiresProvenance.test_path_only_without_immutable_provenance_refuses
#[test]
fn provenance_test_path_only_without_immutable_provenance_refuses() {
    let (f, b, p) = snapshots(false, (None, None), (None, None));
    refused(&f, &b, &p);
}

// Original: tests.test_migration_provenance.LegacyRecoveryRequiresProvenance.test_same_archive_slot_with_different_immutable_ids_refuses
#[test]
fn provenance_test_same_archive_slot_with_different_immutable_ids_refuses() {
    let (f, b, p) = snapshots(
        false,
        (Some("OLD-CID"), Some("OLD-FUID")),
        (Some("OTHER-CID"), Some("OTHER-FUID")),
    );
    refused(&f, &b, &p);
}

// Original: tests.test_migration_provenance.LegacyRecoveryRequiresProvenance.test_shared_cid_bound_to_multiple_backup_rows_refuses
#[test]
fn provenance_test_shared_cid_bound_to_multiple_backup_rows_refuses() {
    let (f, b, p) = snapshots(
        false,
        (Some("OLD-CID"), Some("OLD-FUID")),
        (Some("OLD-CID"), Some("OLD-FUID")),
    );
    let db = rusqlite::Connection::open(&b).unwrap();
    fixture::add_to(
        &db,
        RIVAL,
        f.data.join("other-old-file.webm").to_str().unwrap(),
        "video",
        None,
        Some("OLD-CID"),
        Some("OTHER-OLD-FUID"),
    );
    drop(db);
    refused(&f, &b, &p);
}

// Original: tests.test_migration_provenance.LegacyRecoveryRequiresProvenance.test_shared_cid_bound_to_multiple_current_rows_refuses
#[test]
fn provenance_test_shared_cid_bound_to_multiple_current_rows_refuses() {
    let (f, b, p) = snapshots(
        false,
        (Some("OLD-CID"), Some("OLD-FUID")),
        (Some("OLD-CID"), Some("OLD-FUID")),
    );
    let other = f.media("same-cid-rival.webm");
    f.add(
        RIVAL,
        &other,
        "video",
        Some(9),
        Some("OLD-CID"),
        Some("RIVAL-FUID"),
    );
    let fake = f.fake(&[
        (p.file_name().unwrap().to_str().unwrap(), CURRENT, Some(7)),
        ("same-cid-rival.webm", RIVAL, Some(9)),
    ]);
    assert!(
        migration_provenance::recover(&f.data, &f.root.0, &b, &fake, &fixture::runtime())
            .unwrap()
            .is_empty()
    );
    assert_eq!(cli(&f, &b, fake), 4);
}

// Original: tests.test_migration_provenance.LegacyRecoveryRequiresProvenance.test_shared_fuid_alone_can_prove_the_mapping
#[test]
fn provenance_test_shared_fuid_alone_can_prove_the_mapping() {
    let (f, b, p) = snapshots(
        false,
        (None, Some("SHARED-FUID")),
        (None, Some("SHARED-FUID")),
    );
    assert_eq!(cli(&f, &b, codec(&f, &p)), 0);
    assert_eq!(f.col("seen_files", "file_unique_id"), ["SHARED-FUID"]);
}

// Original: tests.test_migration_provenance.LegacyRecoveryRequiresProvenance.test_shared_identifiers_recover_across_different_paths_and_positions
#[test]
fn provenance_test_shared_identifiers_recover_across_different_paths_and_positions() {
    let (f, b, p) = snapshots(
        false,
        (Some("OLD-CID"), Some("OLD-FUID")),
        (Some("OLD-CID"), Some("OLD-FUID")),
    );
    let renamed = p.with_file_name("unrelated-new-position.webm");
    std::fs::rename(&p, &renamed).unwrap();
    f.db()
        .execute("UPDATE items SET file_path=?", [renamed.to_str().unwrap()])
        .unwrap();
    assert_eq!(cli(&f, &b, codec(&f, &renamed)), 0);
    assert_eq!(
        fixture::read_json(&f.data.join("publish_audit.json"))["sets"][0]["keys"],
        json!([CURRENT])
    );
    assert_eq!(f.col("publications", "custom_emoji_id"), ["OLD-CID"]);
    assert_eq!(f.col("seen_files", "file_unique_id"), ["OLD-FUID"]);
}

// Original: tests.test_migration_provenance.LegacyRecoveryRequiresProvenance.test_shared_publication_cid_alone_can_prove_the_mapping
#[test]
fn provenance_test_shared_publication_cid_alone_can_prove_the_mapping() {
    let (f, b, p) = snapshots(
        false,
        (Some("SHARED-CID"), None),
        (Some("SHARED-CID"), None),
    );
    assert_eq!(cli(&f, &b, codec(&f, &p)), 0);
    assert_eq!(f.col("publications", "custom_emoji_id"), ["SHARED-CID"]);
}
