//! Native supplemental structural plan invariants; no interpreter boundary.
use numera_runtime as _native;
use serde_json::{Value, json};
// Native supplemental: native/tests/plan_steps_parity.rs::original_applier_plans_keep_held_slots_retirement_counts_and_structural_refusals
#[test]
fn original_applier_plans_keep_held_slots_retirement_counts_and_structural_refusals() {
    let state = json!({"sets":[
        {"fmt":"mixed","index":1,"name":"first","logo":true,"keys":["a","b","c"]},
        {"fmt":"mixed","index":2,"name":"second","logo":true,"keys":["d","e","f"]}]});
    let items = json!({"a":{"included":true,"pos":0},"b":{"included":true,"pos":1},
        "c":{"included":true,"pos":2},"d":{"included":true,"pos":3},
        "e":{"included":true,"pos":4},"f":{"included":true,"pos":5},
        "new":{"included":true,"pos":6}});
    let ids = json!({"a":"cid-1-1","b":"cid-1-2","c":"cid-1-3",
        "d":"cid-2-1","e":"cid-2-2","f":"cid-2-3"});
    let matching = json!({"targets":[["a",1],["b",1],["c",1],["d",2],["e",2],["f",2]],
        "per_set":200,"moves":[],"held":[],"excluded":[]});
    let request = json!({"state":state,"items":items,"ids":ids,"plan":matching});
    let matched = _native::plan_steps::compute(&request).unwrap();
    assert_eq!(matched["pending"], false);
    assert_eq!(matched["counted"], 0);
    let mut moved = request.clone();
    moved["plan"]["targets"] = json!([
        ["a", 1],
        ["b", 1],
        ["c", 2],
        ["d", 1],
        ["e", 2],
        ["f", 2],
        ["new", 2]
    ]);
    let steps = _native::plan_steps::compute(&moved).unwrap();
    assert_eq!(steps["removals"], json!([["c", 1], ["d", 2]]));
    assert_eq!(steps["adds"], json!({"1":["d"],"2":["c","new"]}));
    assert_eq!(steps["counted"], 5);
    let report = _native::plan_steps::render(&steps, &ids, 20).unwrap();
    assert!(report.contains("5 counted change(s): 1 run(s)"));
    assert!(report.contains("cid-1-3"));
    assert!(report.contains("cid-2-1"));
    let mut held = request.clone();
    held["plan"]["targets"] = json!([["a", 1], ["b", 1], ["c", 1], ["d", 2], ["e", 2]]);
    held["plan"]["excluded"] = json!(["f"]);
    let steps = _native::plan_steps::compute(&held).unwrap();
    assert_eq!(steps["removals"], json!([]));
    assert_eq!(steps["held_live"], json!([["f", 2]]));
    assert!(
        _native::plan_steps::render(&steps, &ids, 20)
            .unwrap()
            .contains("held, stays live until placed")
    );
    for variant in [
        "capacity",
        "panel capacity",
        "missing",
        "per format",
        "gap",
        "excluded",
        "missing CID",
    ] {
        let mut changed = moved.clone();
        let fragment = match variant {
            "capacity" => {
                changed["plan"]["per_set"] = json!(4);
                "over the cap of 4"
            }
            "panel capacity" => {
                changed["plan"]["over_capacity"] = json!({"1":201});
                "overfills"
            }
            "missing" => {
                changed["plan"]["targets"] = json!([["s:gone", 1]]);
                "s:gone"
            }
            "per format" => {
                changed["state"]["sets"][0]["fmt"] = json!("static");
                "--mixed"
            }
            "gap" => {
                changed["plan"]["targets"] = json!([["a", 9]]);
                "[9]"
            }
            "excluded" => {
                changed["items"]["a"]["included"] = json!(false);
                "excluded catalog key a"
            }
            "missing CID" => {
                changed["ids"]["c"] = Value::Null;
                "custom_emoji_id is not recorded"
            }
            _ => unreachable!(),
        };
        let before = changed.clone();
        let error = _native::plan_steps::compute(&changed).unwrap_err();
        assert!(error.contains(fragment), "{variant}: {error}");
        assert_eq!(changed, before);
    }
}
