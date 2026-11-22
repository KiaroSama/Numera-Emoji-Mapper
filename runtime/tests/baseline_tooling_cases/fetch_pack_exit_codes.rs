use super::*;
// tests.test_entry_point_contracts.FetchPackExitCodes.test_every_pack_failing_exits_nonzero
#[test]
fn test_every_pack_failing_exits_nonzero() {
    let root = Root::new();
    root.install();
    let server = FailureServer::new();
    let result = root.run(
        &[
            "fetch-pack",
            "--data-dir",
            root.0.join("collection").to_str().unwrap(),
            "gone_by_bot",
            "https://t.me/addemoji/also_gone",
        ],
        &server.env(),
    );
    assert_eq!(
        *server.lookups.lock().unwrap(),
        ["gone_by_bot", "also_gone"]
    );
    assert_eq!(result.code, 4);
    assert!(result.stdout.contains("packs_failed=2"));
}
// tests.test_entry_point_contracts.FetchPackExitCodes.test_negative_limit_is_rejected_before_any_api_call
#[test]
fn test_negative_limit_is_rejected_before_any_api_call() {
    let root = Root::new();
    root.install();
    let server = FailureServer::new();
    let result = root.run(&["fetch-pack", "somepack", "--limit", "-1"], &server.env());
    assert_eq!(result.code, 2);
    assert!(server.lookups.lock().unwrap().is_empty());
}
// tests.test_entry_point_contracts.FetchPackExitCodes.test_out_of_range_phash_threshold_is_a_usage_error
#[test]
fn test_out_of_range_phash_threshold_is_a_usage_error() {
    let root = Root::new();
    root.install();
    let server = FailureServer::new();
    let result = root.run(
        &["fetch-pack", "somepack", "--phash-threshold", "64"],
        &server.env(),
    );
    assert_eq!(result.code, 2);
    assert!(result.stderr.contains("out of range"));
    assert!(server.lookups.lock().unwrap().is_empty());
}
// tests.test_entry_point_contracts.FetchPackExitCodes.test_usable_phash_threshold_is_still_accepted
#[test]
fn test_usable_phash_threshold_is_still_accepted() {
    let root = Root::new();
    root.install();
    let server = FailureServer::new();
    let result = root.run(
        &["fetch-pack", "somepack", "--phash-threshold", "4"],
        &server.env(),
    );
    assert_eq!(result.code, 4);
    assert_eq!(*server.lookups.lock().unwrap(), ["somepack"]);
}
