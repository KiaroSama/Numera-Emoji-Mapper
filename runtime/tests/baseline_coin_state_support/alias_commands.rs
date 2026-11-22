use super::*;
use numera_runtime as rt;
use serde_json::json;
// tests.test_coin_ticker_map.AliasMapAmbiguity.test_ambiguous_name_is_reported_not_guessed
#[test]
fn test_ambiguous_name_is_reported_not_guessed() {
    let root = alias_fixture();
    let server = Server::new(vec![]);
    let completed = command(
        &root,
        &server.base,
        &server.base,
        &ids(&["coin-alias-map", "--coin-dir", root.0.to_str().unwrap()]),
        &[],
    );
    assert_eq!(
        completed.code, 0,
        "{}{}",
        completed.stdout, completed.stderr
    );
    let mapping = doc(&root.0.join("ticker_to_id.json"));
    assert!(mapping.get("ccc").is_none());
    assert!(completed.stdout.contains("AMBIGUOUS ccc"));
    assert!(completed.stdout.contains("aaa, bbb"));
    assert!(filled(&root).contains("ticker: ccc\n   premium-id:\n"));
    assert!(server.finish().is_empty());
}
// tests.test_coin_ticker_map.AliasMapAmbiguity.test_unique_name_is_still_applied
#[test]
fn test_unique_name_is_still_applied() {
    let root = alias_fixture();
    let server = Server::new(vec![]);
    let completed = command(
        &root,
        &server.base,
        &server.base,
        &ids(&["coin-alias-map", "--coin-dir", root.0.to_str().unwrap()]),
        &[],
    );
    assert_eq!(completed.code, 0);
    assert_eq!(doc(&root.0.join("ticker_to_id.json"))["eee"], "333");
    assert!(filled(&root).contains("premium-id: 333"));
    assert!(server.finish().is_empty());
}
// tests.test_coin_ticker_map.CanonicalMapWritersCannotLoseAnUpdate.test_a_held_lock_stops_the_map_tools_without_writing
#[test]
fn test_a_held_lock_stops_the_map_tools_without_writing() {
    let root = Root::new();
    write(&root.0.join("ticker_to_id.json"), &json!({"btc":"111"}));
    std::fs::write(
        root.0.join("currency-emoji-inventory.md"),
        "   ticker: btcbsc\n   premium-id:\n",
    )
    .unwrap();
    std::fs::write(root.0.join("keywords.csv"), "ticker,name\nbtc,Bitcoin\n").unwrap();
    let before = std::fs::read(root.0.join("ticker_to_id.json")).unwrap();
    let server = Server::new(vec![]);
    for tool in ["coin-alias-map", "coin-enhance-map"] {
        let hold = rt::locks::Lease::acquire(
            &rt::locks::canonical_map_path(&root.0),
            "actual other map tool",
        )
        .unwrap();
        let completed = command(
            &root,
            &server.base,
            &server.base,
            &ids(&[tool, "--coin-dir", root.0.to_str().unwrap()]),
            &[],
        );
        assert_eq!(
            completed.code, 4,
            "{tool}: {}{}",
            completed.stdout, completed.stderr
        );
        assert_eq!(
            std::fs::read(root.0.join("ticker_to_id.json")).unwrap(),
            before
        );
        drop(hold);
    }
    assert!(server.finish().is_empty());
}
