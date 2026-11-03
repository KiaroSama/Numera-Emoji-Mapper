//! Original copy and marker races use real clone rollback with deterministic private fault seams.
use crate::tooling_sandbox::{self, Session};
use rusqlite::Connection;
use std::path::Path;
use std::time::{Duration, Instant};

mod tooling_fixture {
    use crate as _native;
    include!("../tests/support/tooling_fixture.rs");
}
use tooling_fixture::Root;

fn seed(root: &Path) -> std::path::PathBuf {
    let source = root.join("source");
    std::fs::create_dir_all(source.join("media")).unwrap();
    std::fs::write(source.join("media/0.png"), b"\x89PNG-0").unwrap();
    let db = Connection::open(source.join("catalog.db")).unwrap();
    db.execute_batch("PRAGMA journal_mode=DELETE; CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT); INSERT INTO items VALUES('s:original','./media/0.png');").unwrap();
    db.close().unwrap();
    source
}

#[test]
fn same_length_copy_corruption_and_source_mutation_refuse_and_remove_the_entire_clone() {
    let deadline = Instant::now() + Duration::from_secs(10);
    for changed_source in [false, true] {
        let root = Root::new("sandbox injected copy fault");
        let source = seed(&root.0);
        let original = std::fs::read(source.join("media/0.png")).unwrap();
        let database = std::fs::read(source.join("catalog.db")).unwrap();
        let target = root.0.join("panel-sandbox-refused");
        let mut calls = 0;
        let result = Session::create_with(
            &root.0,
            &source,
            &target,
            |input, output| {
                calls += 1;
                tooling_sandbox::copy_media_with(input, output, |reader, writer| {
                    let count = std::io::copy(reader, writer)?;
                    let changed = if changed_source { input } else { output };
                    std::fs::write(
                        changed,
                        vec![if changed_source { b'Z' } else { b'X' }; count as usize],
                    )?;
                    Ok(count)
                })
            },
            |_| Ok(()),
        );
        assert!(result.err().unwrap().contains("not copied faithfully"));
        assert_eq!(calls, 1);
        assert!(
            !target.exists(),
            "failed validation cannot leave a partial clone"
        );
        assert_eq!(std::fs::read(source.join("catalog.db")).unwrap(), database);
        assert_eq!(
            std::fs::read(source.join("media/0.png")).unwrap(),
            if changed_source {
                vec![b'Z'; original.len()]
            } else {
                original
            }
        );
        assert!(
            tooling_sandbox::lifetime_lock_path(&target)
                .unwrap()
                .is_file()
        );
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
        assert!(Instant::now() < deadline);
        root.clean().unwrap();
    }
}

#[test]
fn published_marker_is_not_reclaimable_before_create_returns_and_finish_keeps_its_lease() {
    let root = Root::new("sandbox construction lease");
    let source = seed(&root.0);
    let database = std::fs::read(source.join("catalog.db")).unwrap();
    let target = root.0.join("panel-sandbox-constructing");
    let mut callbacks = 0;
    let session = Session::create_with(
        &root.0,
        &source,
        &target,
        tooling_sandbox::copy_media,
        |directory| {
            callbacks += 1;
            assert!(tooling_sandbox::marker_is_self_describing(directory));
            assert!(directory.join("catalog.db").is_file());
            assert_eq!(
                std::fs::read_dir(directory.join("media")).unwrap().count(),
                1
            );
            assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(callbacks, 1);
    assert_eq!(session.count, 1);
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    let lease = tooling_sandbox::lifetime_lock_path(&target).unwrap();
    session.finish().unwrap();
    assert!(!target.exists());
    assert!(lease.is_file());
    assert_eq!(std::fs::read(source.join("catalog.db")).unwrap(), database);
    root.clean().unwrap();
}

#[test]
fn sweeper_rechecks_marker_after_acquiring_lease_instead_of_erasing_the_changed_directory() {
    let root = Root::new("sandbox postlease marker");
    let source = seed(&root.0);
    let target = root.0.join("panel-sandbox-marker-changed");
    let session = Session::create(&root.0, &source, &target).unwrap();
    let before = std::fs::read(target.join("catalog.db")).unwrap();
    drop(session);
    let mut callbacks = 0;
    let removed = tooling_sandbox::sweep_stale_with(&root.0, |directory| {
        assert_eq!(directory, target);
        callbacks += 1;
        std::fs::remove_file(directory.join(".sandbox-owner.json")).map_err(|e| e.to_string())
    })
    .unwrap();
    assert_eq!(callbacks, 1);
    assert_eq!(removed, 0);
    assert_eq!(std::fs::read(target.join("catalog.db")).unwrap(), before);
    assert!(
        tooling_sandbox::lifetime_lock_path(&target)
            .unwrap()
            .is_file()
    );
    assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
    root.clean().unwrap();
}
