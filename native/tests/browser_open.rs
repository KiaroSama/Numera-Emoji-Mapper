//! No-open never invokes a desktop browser; URL construction is numeric-loopback only.
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
