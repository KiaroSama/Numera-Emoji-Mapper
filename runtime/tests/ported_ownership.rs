// Ported from native/tests/ownership.rs at a32dbab6e3e694f1fac9656997fa2edb6e3b2ede.
// Original source SHA256: 58f12c11d72e31b1dfdc3525309bf3aadd7a57e12cf5f0f8dc6459cb21f8b5a6.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// Same-thread catalog nesting never relaxes incompatible-mode or cross-thread exclusion.
use _native::{
    locks::Lease,
    ownership::{Mode, Ownership},
};
use std::path::PathBuf;
#[test]
fn nested_same_mode_preserves_os_lock_until_last_owner() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("logs")
        .join(format!("ownership-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let first = Ownership::acquire(&root, Mode::Writer, "fixture").unwrap();
    let nested = Ownership::acquire(&root, Mode::Writer, "fixture").unwrap();
    assert!(Ownership::acquire(&root, Mode::Maintenance, "fixture").is_err());
    let other = root.clone();
    assert!(
        std::thread::spawn(move || Ownership::acquire(&other, Mode::Writer, "fixture").is_err())
            .join()
            .unwrap()
    );
    drop(first);
    assert!(Lease::acquire(&root.join(".maintenance.lock"), "fixture").is_err());
    drop(nested);
    let maintenance = Ownership::acquire(&root, Mode::Maintenance, "fixture").unwrap();
    assert!(Ownership::acquire(&root, Mode::Writer, "fixture").is_err());
    std::fs::write(root.join("identity-migration.journal.json"), b"{}").unwrap();
    drop(maintenance);
    assert!(Ownership::acquire(&root, Mode::Writer, "fixture").is_err());
    let maintenance = Ownership::acquire(&root, Mode::Maintenance, "fixture").unwrap();
    drop(maintenance);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
