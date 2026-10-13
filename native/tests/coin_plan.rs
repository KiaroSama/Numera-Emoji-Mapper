//! Pinned source plan representatives/keyword order/alias replay without file or live mutations.
use serde_json::Value;

#[test]
fn frozen_coin_groups_and_alias_replay_match_source() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/coin-plan.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["coins/_dedup_plan.py"],
        "d5fe1edbd9ecf37cc59eada51b2108a756d49edece5b1fd47db4a1b325ec5bd3"
    );
    for case in fixture["plans"].as_array().unwrap() {
        let inventory: Vec<String> = serde_json::from_value(case["inventory"].clone()).unwrap();
        let result = _native::coin_plan::group(
            case["files"].as_array().unwrap(),
            &inventory,
            case["keywords"].as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(result, case["expected"]);
    }
    for case in fixture["replays"].as_array().unwrap() {
        let approved: Vec<String> = serde_json::from_value(case["approved"].clone()).unwrap();
        assert_eq!(
            _native::coin_plan::replay(
                case["new"].as_object().unwrap(),
                case["current"].as_object().unwrap(),
                &approved
            )
            .unwrap(),
            case["expected"]
        );
    }
    assert!(
        _native::coin_plan::group(&[serde_json::json!({})], &[], &serde_json::Map::new()).is_err()
    );
}
