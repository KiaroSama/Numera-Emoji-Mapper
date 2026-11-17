//! Exact independent native Session tails not subsumed by the original Python cohorts.
use crate::tooling_fixture;
use numera_runtime::tooling_sandbox::{self, Session};
use rusqlite::Connection;
use serde_json::json;
use std::path::Path;
use std::time::{Duration, Instant};
use tooling_fixture::Root;

#[test]
fn clone_contains_wal_rows_independent_media_and_state_while_lifetime_excludes_sweep() {
    let root = Root::new("sandbox clone");
    let source = root.0.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("image.bin"), b"real independently copied bytes").unwrap();
    std::fs::write(
        source.join("pack_plan.json"),
        b"{\"version\":1,\"known\":[]}",
    )
    .unwrap();
    let db = Connection::open(source.join("catalog.db")).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT); INSERT INTO items VALUES('s:legacy/key','./image.bin');").unwrap();
    let session = Session::create(&root.0, &source, &root.0.join("panel-sandbox-copy")).unwrap();
    assert_eq!(session.count, 1);
    assert!(tooling_sandbox::marker_is_self_describing(
        &session.directory
    ));
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    assert_eq!(
        std::fs::read(session.directory.join("pack_plan.json")).unwrap(),
        b"{\"version\":1,\"known\":[]}"
    );
    let clone = Connection::open(session.directory.join("catalog.db")).unwrap();
    let path: String = clone
        .query_row("SELECT file_path FROM items", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"real independently copied bytes"
    );
    assert!(Path::new(&path).starts_with(&session.directory));
    std::fs::write(&path, b"private clone mutation").unwrap();
    assert_eq!(
        std::fs::read(source.join("image.bin")).unwrap(),
        b"real independently copied bytes"
    );
    clone.close().unwrap();
    let lock = tooling_sandbox::lifetime_lock_path(&session.directory).unwrap();
    let directory = session.directory.clone();
    session.finish().unwrap();
    assert!(!directory.exists());
    assert!(
        lock.is_file(),
        "lifetime tombstone must survive clone cleanup"
    );
    db.close().unwrap();
}

#[test]
fn recovery_marker_prevents_reclamation_after_uncertain_child_cleanup() {
    let root = Root::new("sandbox clone");
    let source = root.0.join("source");
    std::fs::create_dir(&source).unwrap();
    let db = Connection::open(source.join("catalog.db")).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT);")
        .unwrap();
    db.close().unwrap();
    let session =
        Session::create(&root.0, &source, &root.0.join("panel-sandbox-recovery")).unwrap();
    let directory = session.directory.clone();
    let owner_bytes = std::fs::read(directory.join(".sandbox-owner.json")).unwrap();
    session.retain_for_recovery().unwrap();
    assert!(
        !directory.join(".sandbox-owner.json").exists(),
        "no reclaimable marker may remain before spawn"
    );
    assert_eq!(
        std::fs::read(directory.join(".sandbox-recovery.json")).unwrap(),
        owner_bytes
    );
    session.retain_for_recovery().unwrap();
    std::fs::write(
        directory.join(".sandbox-recovery.json"),
        b"{\"version\":2,\"directory\":\"foreign\"}",
    )
    .unwrap();
    assert!(
        session.retain_for_recovery().is_err(),
        "existing malformed/foreign recovery is not readiness"
    );
    std::fs::write(directory.join(".sandbox-recovery.json"), &owner_bytes).unwrap();
    assert!(!tooling_sandbox::marker_is_self_describing(&directory));
    assert!(directory.join(".sandbox-recovery.json").is_file());
    drop(session);
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    assert!(directory.join("catalog.db").is_file());
}

#[test]
fn occupied_recovery_refuses_without_replacing_owner_or_foreign_bytes() {
    let root = Root::new("sandbox clone");
    let source = root.0.join("source");
    std::fs::create_dir(&source).unwrap();
    let db = Connection::open(source.join("catalog.db")).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT);")
        .unwrap();
    db.close().unwrap();
    let session = Session::create(&root.0, &source, &root.0.join("panel-sandbox-refused")).unwrap();
    let owner = session.directory.join(".sandbox-owner.json");
    let recovery = session.directory.join(".sandbox-recovery.json");
    let owner_bytes = std::fs::read(&owner).unwrap();
    std::fs::write(&recovery, b"foreign preserved").unwrap();
    assert!(session.retain_for_recovery().is_err());
    assert_eq!(std::fs::read(&owner).unwrap(), owner_bytes);
    assert_eq!(std::fs::read(&recovery).unwrap(), b"foreign preserved");
    session.finish().unwrap();
}

#[test]
fn original_stale_marker_variants_preserve_foreign_data_and_stable_lease_files() {
    let deadline = Instant::now() + Duration::from_secs(10);
    let root = Root::new("sandbox stale markers");
    let foreign = root.0.join("foreign");
    std::fs::create_dir(&foreign).unwrap();
    std::fs::write(foreign.join("precious"), b"not a sandbox").unwrap();
    for variant in [
        "abandoned",
        "held",
        "unmarked",
        "foreign",
        "malformed",
        "old",
    ] {
        let directory = root.0.join(format!("panel-sandbox-{variant}"));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("catalog.db"), b"owned marker fixture").unwrap();
        let marker = directory.join(".sandbox-owner.json");
        match variant {
            "unmarked" => {}
            "malformed" => std::fs::write(&marker, b"{not json").unwrap(),
            _ => numera_runtime::atomic::write_json(
                &marker,
                &json!({
                    "version":if variant == "old" {1} else {2},
                    "directory":if variant == "foreign" {&foreign} else {&directory},
                }),
            )
            .unwrap(),
        }
        let lease = tooling_sandbox::lifetime_lock_path(&directory).unwrap();
        let held = numera_runtime::locks::Lease::acquire(&lease, "fixture").unwrap();
        if variant != "held" {
            drop(held);
        } else {
            assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
            assert_eq!(
                std::fs::read(directory.join("catalog.db")).unwrap(),
                b"owned marker fixture"
            );
            drop(held);
            // Retain the previously live fixture as foreign before the next sweep.
            std::fs::remove_file(&marker).unwrap();
        }
        let expected = usize::from(variant == "abandoned");
        assert_eq!(
            tooling_sandbox::sweep_stale(&root.0).unwrap(),
            expected,
            "{variant}"
        );
        assert_eq!(directory.exists(), expected == 0, "{variant}");
        assert!(lease.is_file(), "lifetime tombstone must remain: {variant}");
    }
    #[cfg(unix)]
    {
        let link = root.0.join("panel-sandbox-link");
        std::os::unix::fs::symlink(&foreign, &link).unwrap();
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
        assert!(link.is_symlink());
        std::fs::remove_file(link).unwrap();
    }
    assert_eq!(
        std::fs::read(foreign.join("precious")).unwrap(),
        b"not a sandbox"
    );
    assert!(Instant::now() < deadline);
    root.clean().unwrap();
}

#[test]
fn foreign_or_missing_source_does_not_create_or_erase_target() {
    let root = Root::new("sandbox clone");
    let source = root.0.join("source");
    let target = root.0.join("panel-sandbox-occupied");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), b"precious").unwrap();
    assert!(Session::create(&root.0, &source, &target).is_err());
    assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"precious");
    assert!(Session::create(&root.0, &source, &root.0.join("panel-sandbox-new")).is_err());
    assert!(!root.0.join("panel-sandbox-new").exists());
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    assert_eq!(tooling_sandbox::safe_key("a:b/é"), "613a622fc3a9");
}
