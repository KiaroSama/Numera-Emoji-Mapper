//! A pre-spawn recovery marker retains the same identity but cannot be reclaimed as a stale clone.
use _native::sandbox_lease::validate_marker;

#[test]
fn exactly_one_physical_bounded_owner_or_recovery_marker_describes_the_clone() {
    let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let root = project
        .join("logs/tmp")
        .join(format!("sandbox markers {}", std::process::id()));
    std::fs::create_dir_all(root.parent().unwrap()).unwrap();
    std::fs::create_dir(&root).unwrap();
    let directory = _native::migration_bundle::resolved(&root).unwrap();
    let owner = directory.join(".sandbox-owner.json");
    let recovery = directory.join(".sandbox-recovery.json");
    let bytes =
        serde_json::to_vec(&serde_json::json!({"version":2,"directory":directory})).unwrap();
    let checked = std::panic::catch_unwind(|| {
        assert!(validate_marker(&directory).is_err());
        std::fs::write(&owner, &bytes).unwrap();
        validate_marker(&directory).unwrap();
        std::fs::rename(&owner, &recovery).unwrap();
        validate_marker(&directory).unwrap();
        assert_eq!(std::fs::read(&recovery).unwrap(), bytes);
        std::fs::write(&owner, &bytes).unwrap();
        assert!(
            validate_marker(&directory).is_err(),
            "dual markers must never silently select one"
        );
        std::fs::remove_file(&owner).unwrap();
        for value in [b"not JSON".to_vec(), b"{}".to_vec(), vec![b' '; 65537]] {
            std::fs::write(&recovery, value).unwrap();
            assert!(validate_marker(&directory).is_err());
        }
        #[cfg(unix)]
        {
            std::fs::write(&owner, &bytes).unwrap();
            std::fs::remove_file(&recovery).unwrap();
            std::os::unix::fs::symlink(&owner, &recovery).unwrap();
            assert!(validate_marker(&directory).is_err());
            std::fs::remove_file(&recovery).unwrap();
        }
    });
    std::fs::remove_dir_all(root).unwrap();
    if let Err(error) = checked {
        std::panic::resume_unwind(error);
    }
}
