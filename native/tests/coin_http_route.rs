//! Endpoint substitution is a hermetic-test capability, never a production setting.
#[test]
fn provider_test_origin_preserves_paths_queries_and_cannot_enable_remote_requests() {
    let remote = "https://static.coinpaprika.com/coin/alpha%20coin/logo.png?q=A%2BB&x=1&x=2";
    assert_eq!(
        _native::coin_http::endpoint(remote, false, Some("not a URL"))
            .unwrap()
            .as_str(),
        remote
    );
    assert_eq!(
        _native::coin_http::endpoint(remote, true, Some("http://127.0.0.1:12345"))
            .unwrap()
            .as_str(),
        "http://127.0.0.1:12345/coin/alpha%20coin/logo.png?q=A%2BB&x=1&x=2"
    );
    let local = "http://127.0.0.1:12345/already-local?x=1";
    assert_eq!(
        _native::coin_http::endpoint(local, true, None)
            .unwrap()
            .as_str(),
        local
    );
    assert!(_native::coin_http::endpoint(remote, true, None).is_err());
    for base in [
        "http://localhost:12345",
        "https://example.invalid",
        "http://127.0.0.2:12345",
        "file:///tmp/fixture",
        "http://user@127.0.0.1:12345",
        "http://127.0.0.1:12345/path",
        "http://127.0.0.1:12345?query=1",
        "http://127.0.0.1:12345#fragment",
    ] {
        assert!(
            _native::coin_http::endpoint(remote, true, Some(base)).is_err(),
            "{base}"
        );
    }
    for url in [
        "http://user:password@example.invalid/logo.png",
        "file:///tmp/logo.png",
        "https://example.invalid/logo.png#fragment",
    ] {
        assert!(_native::coin_http::endpoint(url, true, Some("http://127.0.0.1:12345")).is_err());
    }
}
