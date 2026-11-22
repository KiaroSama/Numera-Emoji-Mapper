use super::*;
// tests.test_entry_points.SuiteIsHermetic.test_the_guard_was_installed_before_the_test_modules
#[test]
fn test_the_guard_was_installed_before_the_test_modules() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1"),
        "run through numera-runtime-tests; no guard may be enabled midway"
    );
    assert!(
        runtime::telegram::Telegram::new("synthetic".into(), "https://api.telegram.org".into())
            .is_err()
    );
}
// tests.test_entry_points.SuiteIsHermetic.test_the_real_dotenv_values_are_not_loaded
#[test]
fn test_the_real_dotenv_values_are_not_loaded() {
    let root = Root::new();
    root.file(
        ".env",
        b"SYNTHETIC_DO_NOT_LOAD=synthetic-private-value-12345\n",
    );
    let values = BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
    let config = Config::load_with_environment(root.0.clone(), &values).unwrap();
    assert!(config.value("SYNTHETIC_DO_NOT_LOAD").is_empty());
    assert!(!std::env::vars().any(|(key, _)| key == "SYNTHETIC_DO_NOT_LOAD"));
}
// tests.test_entry_points.SuiteIsHermetic.test_load_env_is_a_no_op_under_the_guard
#[test]
fn test_load_env_is_a_no_op_under_the_guard() {
    let root = Root::new();
    root.file(".env", b"SYNTHETIC_LOAD_CANARY=changed\n");
    let before = std::env::vars().collect::<BTreeMap<_, _>>();
    let input = BTreeMap::from([
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
        ("KEEP".into(), "untouched".into()),
    ]);
    let config = Config::load_with_environment(root.0.clone(), &input).unwrap();
    assert_eq!(config.value("KEEP"), "untouched");
    assert_eq!(config.value("SYNTHETIC_LOAD_CANARY"), "");
    assert!(std::env::vars().collect::<BTreeMap<_, _>>() == before);
}
// tests.test_entry_points.SuiteIsHermetic.test_outbound_connections_are_refused
#[test]
fn test_outbound_connections_are_refused() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    for base in [
        "https://api.telegram.org",
        "https://8.8.8.8",
        "http://localhost:443",
    ] {
        let error = runtime::telegram::Telegram::new("synthetic".into(), base.into())
            .err()
            .unwrap();
        assert!(error.to_string().contains("only to numeric loopback"));
    }
}
// tests.test_entry_points.SuiteIsHermetic.test_loopback_still_works
#[test]
fn test_loopback_still_works() {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let stream = std::net::TcpStream::connect_timeout(
        &server.local_addr().unwrap(),
        std::time::Duration::from_secs(5),
    )
    .unwrap();
    drop(stream);
}
