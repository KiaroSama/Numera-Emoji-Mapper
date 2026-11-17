// Ported from native/tests/media_store.rs at a32dbab6e3e694f1fac9656997fa2edb6e3b2ede.
// Original source SHA256: 469beb6d506b5043fff2271681cc5bf3a850499d69c76236ba63a63c4508a792.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// Completed media publication reuses the archive transfer's non-clobbering staging contract.
use _native::archive_move;
use std::path::PathBuf;

#[test]
fn incomplete_existing_destination_is_never_adopted_or_overwritten() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("logs")
        .join(format!("media-transfer-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("source.png");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("assets/numera-emoji-mapper-logo.png"),
        &source,
    )
    .unwrap();
    let original = std::fs::read(&source).unwrap();
    let destination = root.join("destination.png");
    std::fs::write(&destination, &original[..20]).unwrap();
    assert!(archive_move::transfer(&source, &destination, true).is_err());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(std::fs::read(&destination).unwrap(), original[..20]);
    std::fs::remove_file(&destination).unwrap();
    archive_move::transfer(&source, &destination, true).unwrap();
    assert!(!source.exists());
    assert_eq!(std::fs::read(&destination).unwrap(), original);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
