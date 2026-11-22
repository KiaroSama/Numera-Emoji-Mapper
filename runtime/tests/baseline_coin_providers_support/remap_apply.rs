use super::*;
// tests.test_remap_ids.MainApplyTest.test_apply_refuses_ambiguous_matches
#[test]
fn test_apply_refuses_ambiguous_matches() {
    let f = Remap::new();
    f.local("btc", &block([200, 20, 20, 255]));
    let blobs = BTreeMap::from([
        ("a".into(), block([200, 20, 20, 255])),
        ("b".into(), block([201, 20, 20, 255])),
    ]);
    let server = Server::new(remap_script(&["a", "b"], &blobs, &[]));
    assert_eq!(f.run(&server, Some(100.0), true).unwrap(), 3);
    assert!(!f.files.output.exists());
    assert_eq!(doc(&f.files.candidates), json!({}));
    server.finish();
}
// tests.test_remap_ids.MainApplyTest.test_apply_writes_map_when_complete_and_unambiguous
#[test]
fn test_apply_writes_map_when_complete_and_unambiguous() {
    let f = Remap::new();
    write(&f.files.output, &json!({"btc":"STALE"}));
    f.local("btc", &block([200, 20, 20, 255]));
    f.local("eth", &block([20, 200, 20, 255]));
    let blobs = BTreeMap::from([
        ("a".into(), block([200, 20, 20, 255])),
        ("b".into(), block([20, 200, 20, 255])),
    ]);
    let server = Server::new(remap_script(&["a", "b"], &blobs, &[]));
    assert_eq!(f.run(&server, Some(100.0), true).unwrap(), 0);
    assert_eq!(doc(&f.files.output), json!({"btc":"a","eth":"b"}));
    assert_eq!(
        doc(&f.files.output.with_extension("prebroken.json")),
        json!({"btc":"STALE"})
    );
    server.finish();
}
// tests.test_remap_ids.MainApplyTest.test_far_match_is_omitted_not_mapped
#[test]
fn test_far_match_is_omitted_not_mapped() {
    let f = Remap::new();
    f.local("btc", &block([200, 20, 20, 255]));
    f.local("ghost", &block([20, 20, 200, 255]));
    let server = Server::new(remap_script(
        &["a"],
        &BTreeMap::from([("a".into(), block([200, 20, 20, 255]))]),
        &[],
    ));
    assert_eq!(f.run(&server, Some(50.0), true).unwrap(), 0);
    assert_eq!(doc(&f.files.output), json!({"btc":"a"}));
    server.finish();
}
// tests.test_remap_ids.MainApplyTest.test_apply_without_max_distance_is_a_usage_error
#[test]
fn test_apply_without_max_distance_is_a_usage_error() {
    let f = Remap::new();
    f.local("btc", &block([200, 20, 20, 255]));
    let server = Server::new(vec![]);
    let result = command(
        &f.root,
        "http://127.0.0.1:9",
        "http://127.0.0.1:9",
        &ids(&[
            "coin-remap-ids",
            "--emoji-dir",
            f.files.images.to_str().unwrap(),
            "--apply",
        ]),
        &[],
    );
    assert_eq!(result.code, 2);
    assert!(result.stderr.contains("--max-distance"));
    assert!(!f.files.output.exists());
    assert!(server.finish().is_empty());
}
// tests.test_remap_ids.MainApplyTest.test_no_local_logos_fails_instead_of_crashing
#[test]
fn test_no_local_logos_fails_instead_of_crashing() {
    let f = Remap::new();
    let server = Server::new(remap_script(
        &["a"],
        &BTreeMap::from([("a".into(), block([200, 20, 20, 255]))]),
        &[],
    ));
    assert_eq!(f.run(&server, None, false).unwrap(), 4);
    server.finish();
}
