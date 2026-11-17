// Ported from native/tests/browser_open.rs at a32dbab6e3e694f1fac9656997fa2edb6e3b2ede.
// Original source SHA256: 8eb011358ce9857dcc087504feec86c64dbe84197f1dd9349bd32c16269fa128.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// No-open never invokes a desktop browser; URL construction is numeric-loopback only.
#[test]
fn no_open_returns_without_platform_side_effects() {
    assert_eq!(
        _native::browser_open::panel_url(9450, false),
        Some("http://127.0.0.1:9450/".into())
    );
    assert_eq!(_native::browser_open::panel_url(9450, true), None);
    assert_eq!(_native::browser_open::panel_url(0, false), None);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime
        .block_on(_native::browser_open::open(9450, true))
        .unwrap();
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
