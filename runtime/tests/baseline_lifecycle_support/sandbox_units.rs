//! Original injected-copy and lease-race cases at actual production I/O seams.
#![allow(non_snake_case)]
use crate::tooling_sandbox::{Session, copy_media, copy_media_with, sweep_stale, sweep_stale_with};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap();
        let base =
            crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let root = base.join(format!(
            "lifecycle-session-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir_all(root.join("source/media")).unwrap();
        let image = root.join("source/media/a.png");
        std::fs::write(&image, b"\x89PNG-original").unwrap();
        let mut catalog =
            crate::catalog::Catalog::open(&root.join("source/catalog.db"), "fixture", &root)
                .unwrap();
        catalog
            .insert(
                &serde_json::json!({"content_key":"s:a","fmt":"static","file_path":image}),
                "fixture",
            )
            .unwrap();
        drop(catalog);
        Self(root)
    }
    fn destination(&self) -> PathBuf {
        self.0.join("panel-sandbox-dest")
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
mod CloneBoundaries {
    use super::*;
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_same_length_copy_corruption_is_refused_and_removed
    #[test]
    fn test_same_length_copy_corruption_is_refused_and_removed() {
        use std::io::{Seek, Write};
        let root = Fixture::new();
        let original = std::fs::read(root.0.join("source/media/a.png")).unwrap();
        let result = Session::create_with(
            &root.0,
            &root.0.join("source"),
            &root.destination(),
            |source, target| {
                copy_media_with(source, target, |input, output| {
                    let length = std::io::copy(input, output)?;
                    output.rewind()?;
                    output.write_all(&vec![b'X'; length as usize])?;
                    Ok(length)
                })
            },
            |_| Ok(()),
        );
        let error = result.err().expect("same-length corruption was accepted");
        assert!(error.contains("not copied faithfully"), "{error}");
        assert!(!root.destination().exists());
        assert_eq!(
            std::fs::read(root.0.join("source/media/a.png")).unwrap(),
            original
        );
    }
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_changed_source_during_copy_is_refused_even_when_length_is_unchanged
    #[test]
    fn test_changed_source_during_copy_is_refused_even_when_length_is_unchanged() {
        let root = Fixture::new();
        let result = Session::create_with(
            &root.0,
            &root.0.join("source"),
            &root.destination(),
            |source, target| {
                copy_media_with(source, target, |input, output| {
                    let length = std::io::copy(input, output)?;
                    std::fs::write(source, vec![b'Z'; length as usize])?;
                    Ok(length)
                })
            },
            |_| Ok(()),
        );
        let error = result
            .err()
            .expect("same-length source mutation was accepted");
        assert!(error.contains("changed"), "{error}");
        assert!(!root.destination().exists());
    }
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_sweep_cannot_claim_a_published_marker_before_clone_returns
    #[test]
    fn test_sweep_cannot_claim_a_published_marker_before_clone_returns() {
        let root = Fixture::new();
        let mut observations = vec![];
        let session = Session::create_with(
            &root.0,
            &root.0.join("source"),
            &root.destination(),
            copy_media,
            |directory| {
                observations.push(sweep_stale(&root.0)?);
                assert!(directory.join("catalog.db").is_file());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(session.count, 1);
        assert_eq!(observations, [0]);
        session.finish().unwrap();
    }
    // Original: tests.test_sandbox_lifecycle.CloneBoundaries.test_marker_is_rechecked_after_acquiring_the_sweep_lease
    #[test]
    fn test_marker_is_rechecked_after_acquiring_the_sweep_lease() {
        let root = Fixture::new();
        let session =
            Session::create(&root.0, &root.0.join("source"), &root.destination()).unwrap();
        drop(session);
        let mut callbacks = 0;
        let count = sweep_stale_with(&root.0, |directory| {
            callbacks += 1;
            std::fs::remove_file(directory.join(".sandbox-owner.json"))
                .map_err(|error| error.to_string())
        })
        .unwrap();
        assert_eq!(callbacks, 1);
        assert_eq!(count, 0);
        assert!(root.destination().join("catalog.db").is_file());
    }
}

// Technical publication variant supplements the single original wrapper ID in
// the integration target; it is never counted as another original source ID.
#[test]
fn wrapper_marker_publication_is_owned_before_return() {
    let root = Fixture::new();
    let mut counts = vec![];
    let session = Session::create_with(
        &root.0,
        &root.0.join("source"),
        &root.destination(),
        copy_media,
        |directory| {
            assert!(crate::tooling_sandbox::marker_is_self_describing(directory));
            counts.push(sweep_stale(&root.0)?);
            Ok(())
        },
    )
    .unwrap();
    counts.push(sweep_stale(&root.0).unwrap());
    assert_eq!(counts, [0, 0]);
    assert_eq!(session.count, 1);
    let directory = session.directory.clone();
    let lifetime = crate::tooling_sandbox::lifetime_lock_path(&directory).unwrap();
    session.finish().unwrap();
    assert!(!directory.exists());
    assert!(lifetime.is_file());
}
