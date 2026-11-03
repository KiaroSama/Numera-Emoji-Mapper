//! An inherited POSIX lifetime is released by the last close, never Lease's explicit unlock.
#[cfg(unix)]
#[test]
fn transferred_lease_keeps_all_duplicates_locked_until_the_last_close() {
    use _native::locks::Lease;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("logs/tmp")
        .join(format!("lease transfer {}", std::process::id()));
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
    std::fs::create_dir(&root).unwrap();
    let path = root.join("lifetime.lock");
    let result = std::panic::catch_unwind(|| {
        let lease = Lease::acquire(&path, "fixture").unwrap();
        let record = std::fs::read(&path).unwrap();
        let duplicate = lease.clone_file().unwrap();
        let lifetime = lease.into_inherited_file().unwrap();
        assert!(Lease::acquire(&path, "must refuse").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), record);
        drop(lifetime);
        assert!(Lease::acquire(&path, "still must refuse").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), record);
        drop(duplicate);
        let reacquired = Lease::acquire(&path, "released").unwrap();
        drop(reacquired);
        assert!(
            path.is_file(),
            "stable lock coordinates must never be unlinked by transfer"
        );
    });
    std::fs::remove_dir_all(root).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}
