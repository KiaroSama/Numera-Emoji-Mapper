// Ported from native/tests/sandbox_lease_windows.rs at a32dbab6e3e694f1fac9656997fa2edb6e3b2ede.
// Original source SHA256: 8d109d5487a650b109e82165f5b41750b9fcccf0b09ab5d34d04e845ea2ff5db.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// Windows uses the wrapper's private native Job, not an inherited flock.
#[cfg(windows)]
#[test]
fn no_inherited_flock_on_the_windows_job_path() {
    assert!(
        _native::sandbox_lease::inherit(std::path::Path::new("unused"))
            .unwrap()
            .is_none()
    );
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
