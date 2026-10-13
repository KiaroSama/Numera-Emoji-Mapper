//! Original source resume refusals protect cursor/order/intent before any coin mutation.
use serde_json::Value;

#[test]
fn coin_resume_shape_and_plan_walk_refusals_match_source() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/coin-state.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"],
        "d5fe1edbd9ecf37cc59eada51b2108a756d49edece5b1fd47db4a1b325ec5bd3"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 20);
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(
            _native::coin_state::problem(
                &case["state"],
                case["plan"].as_array().map(Vec::as_slice)
            ),
            case["expected"],
            "{index}"
        );
    }
}
