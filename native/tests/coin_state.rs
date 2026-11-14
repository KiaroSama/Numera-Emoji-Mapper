//! Original source resume refusals protect cursor/order/intent before any coin mutation.
use serde_json::{Value, json};

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
    provider_state_contracts();
}

fn provider_state_contracts() {
    let record = json!({"sets":[{"index":1,"name":"s1","title":"T","live":1}],
        "order":[],"cursor":0,"in_flight":null});
    assert_eq!(_native::coin_state::problem(&record, None), "");
    let intent =
        json!({"key":"aaa","operation":"add","set_name":"s1","set_index":1,"expected_before":0});
    let shared = json!({"sets":[{"index":1,"name":"s1","title":"T"}],"order":[],"cursor":0,
        "in_flight":null,"provider_in_flight":intent});
    assert_eq!(_native::coin_state::problem(&shared, None), "");
    let mut collided = shared.clone();
    collided
        .as_object_mut()
        .unwrap()
        .remove("provider_in_flight");
    collided["in_flight"] = intent;
    assert_eq!(
        _native::coin_state::problem(&collided, Some(&[json!({"rep":"zzz"})])),
        "in_flight 'aaa' is not plan entry 0, the one the cursor was moved past to run it"
    );
    let mut state = json!({"sets":[{"index":1,"name":"s1","title":"T"}],
        "order":["plan-a","plan-b"],"cursor":2,"in_flight":null});
    _native::coin_provider_proof::record(&mut state, "newcoin").unwrap();
    assert_eq!(state["provider_added"], json!(["newcoin"]));
    assert_eq!(
        state["order"].as_array().unwrap().len()
            + state["provider_added"].as_array().unwrap().len(),
        3
    );
    assert_eq!(_native::coin_state::problem(&state, None), "");
    let mut tally = json!({"provider_added":["aaa"]});
    _native::coin_provider_proof::record(&mut tally, "aaa").unwrap();
    assert_eq!(tally, json!({"provider_added":["aaa"]}));
    assert_eq!(
        _native::coin_state::problem(
            &json!({"sets":[],"order":[],"cursor":0,"provider_added":["aaa","aaa"]}),
            None
        ),
        "'provider_added' repeats ['aaa']"
    );
}
