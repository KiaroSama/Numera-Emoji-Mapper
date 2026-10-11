//! Windows uses the wrapper's private native Job, not an inherited flock.
#[cfg(windows)]
#[test]
fn no_inherited_flock_on_the_windows_job_path() {
    assert!(
        _native::sandbox_lease::inherit(std::path::Path::new("unused"))
            .unwrap()
            .is_none()
    );
}
