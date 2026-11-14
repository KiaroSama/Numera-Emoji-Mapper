//! Independent CPython AST fixtures; raising module bodies must never execute during discovery.
use serde_json::Value;
#[path = "support/coin_source_contract_cases.rs"]
mod coin_source_contract_cases;
#[path = "support/entry_point_cases.rs"]
mod entry_point_cases;
#[path = "support/media_source_contract_cases.rs"]
mod media_source_contract_cases;
#[path = "support/publication_source_contract_cases.rs"]
mod publication_source_contract_cases;

#[test]
fn historical_coin_shared_bindings_reject_duplicate_definitions_and_http_bypasses() {
    coin_source_contract_cases::canaries();
}

#[test]
fn historical_media_ownership_rejects_split_aliases_and_operator_defaults() {
    media_source_contract_cases::canaries();
}

#[test]
fn historical_publication_ownership_rejects_split_announcers_and_destination_resolvers() {
    publication_source_contract_cases::canaries();
}

#[test]
fn original_entry_point_discovery_rejects_empty_unknown_and_unadmitted_sources() {
    entry_point_cases::check_canaries();
}

#[test]
fn source_inventory_keeps_methods_assertions_imports_decorators_and_cli_arguments() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-inventory.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let result = _native::source_test_inventory::parse(
            case["path"].as_str().unwrap(),
            case["source"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(result, case["expected"], "{}", case["name"]);
    }
    let loader = serde_json::from_str::<Value>(include_str!(
        "../../tests/fixtures/native/source-discovery.json"
    ))
    .unwrap();
    let prefix = loader["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "prefix and runTest")
        .unwrap();
    let parsed = _native::source_test_inventory::parse(
        "tests/test_fixture.py",
        prefix["source"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(
        parsed["declared_test_methods"], 2,
        "unittest prefix is test, not test_"
    );
    let errors = fixture["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2);
    for case in errors {
        let error = _native::source_test_inventory::parse(
            case["path"].as_str().unwrap(),
            case["source"].as_str().unwrap(),
        )
        .unwrap_err();
        assert!(
            error.contains(case["error_kind"].as_str().unwrap()),
            "{error}"
        );
    }
    for path in [
        "../outside.py",
        "/outside.py",
        "C:/outside.py",
        "tests\\outside.py",
        "tests/test.rs",
        "",
    ] {
        assert!(_native::source_test_inventory::parse(path, "pass\n").is_err());
    }
    assert!(
        _native::source_test_inventory::parse(
            "tests/test_large.py",
            &" ".repeat(2 * 1024 * 1024 + 1)
        )
        .is_err()
    );
}
