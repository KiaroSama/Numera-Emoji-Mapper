//! Port the original NativeIntent public-domain assertions, retaining independent source fixtures.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn fixture(text: &str, source: &str, expected_hash: &str) -> Value {
    let hash = Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(hash, expected_hash, "independent source fixture drift");
    let fixture: Value = serde_json::from_str(text).unwrap();
    _native::source_fixture::cases(&fixture, source, 8).unwrap();
    fixture
}

#[test]
fn original_intent_cases_preserve_results_refusals_and_all_seven_invalid_targets() {
    let fixture = fixture(
        include_str!("../../tests/fixtures/backend/intent.json"),
        "d173116:emojikit/panel_plan.py",
        "9217bd704f96051c163d2b889fc1dac122830f0b83e35ee97b7c96f56d361b0b",
    );
    for case in fixture["cases"].as_array().unwrap() {
        let mut request = case.clone();
        let object = request.as_object_mut().unwrap();
        object.remove("name");
        object.remove("expected");
        object.remove("error");
        object.insert("written_utc".into(), fixture["utc"].clone());
        let result = _native::intent::execute(request);
        if let Some(error) = case.get("error") {
            assert_eq!(
                result.unwrap_err(),
                error.as_str().unwrap(),
                "{}",
                case["name"]
            );
        } else {
            assert_eq!(result.unwrap(), case["expected"], "{}", case["name"]);
        }
    }
    // These are the exact variants in NativeIntent.test_invalid_targets_are_refused.
    for target in [
        json!(true),
        json!(false),
        json!(0),
        json!(-1),
        json!(1.5),
        json!("1"),
        Value::Null,
    ] {
        assert!(
            _native::intent::execute(json!({"operation":"targets","previous":{
                "targets":[["a",target]],"moves":[],"held":[]
            }}))
            .is_err(),
            "invalid target {target}"
        );
    }
}

#[test]
fn original_panel_capacity_counts_the_logo_without_treating_it_as_artwork() {
    let view = json!([
        {"key":"__logo__","isLogo":true,"included":true},
        {"key":"a","included":true,"pack":1},
        {"key":"b","included":true,"pack":1}
    ]);
    let plan = _native::intent::execute(json!({"operation":"merge","previous":null,
        "view":view,"targets":{"a":1,"b":1},"scope":["a","b"],
        "per_set":2,"written_utc":"fixture"}))
    .unwrap();
    assert_eq!(plan["counts"], json!({"1":2}));
    assert_eq!(plan["over_capacity"], json!({"1":3}));
    assert_eq!(plan["logo_slots"], json!({"1":1}));
    assert_eq!(plan["moves"], json!([]));
    assert_eq!(plan["held"], json!([]));
    assert_eq!(plan["targets"], json!([["a", 1], ["b", 1]]));
}

#[test]
fn original_panel_plan_records_only_explicit_moves_holds_counts_and_durable_output() {
    let card = |key: &str, pack: Option<u64>, included: bool, logo: bool| {
        let mut value = json!({"key":key,"label":key,"included":included});
        if let Some(pack) = pack {
            value["pack"] = json!(pack);
        }
        if logo {
            value["isLogo"] = json!(true);
        }
        value
    };
    let build = |view: Value, targets: Value, capacity| {
        _native::intent::execute(json!({"operation":"build","view":view,"targets":targets,
            "per_set":capacity,"written_utc":"fixture"}))
        .unwrap()
    };
    let moved = build(
        json!([
            card("a", Some(2), true, false),
            card("b", Some(3), true, false)
        ]),
        json!({"a":3,"b":3}),
        200,
    );
    assert_eq!(
        moved["moves"],
        json!([{"key":"a","label":"a","from_pack":2,"to_pack":3}])
    );
    let omitted = build(
        json!([
            card("a", Some(2), true, false),
            card("b", Some(2), true, false)
        ]),
        json!({"a":2}),
        200,
    );
    assert_eq!(omitted["moves"], json!([]));
    assert_eq!(omitted["counts"], json!({"2":1}));
    let held = build(
        json!([
            card("a", Some(3), false, false),
            card("b", Some(3), true, false)
        ]),
        json!({"a":3,"b":3}),
        200,
    );
    assert_eq!(held["held"], json!([{"key":"a","label":"a","from_pack":3}]));
    assert_eq!(held["moves"], json!([]));
    assert_eq!(held["counts"], json!({"3":1}));
    let logo = build(
        json!([
            card("__logo_pack_1__", Some(1), true, true),
            card("a", Some(1), true, false)
        ]),
        json!({"__logo_pack_1__":2,"a":1}),
        200,
    );
    assert_eq!(logo["moves"], json!([]));
    assert_eq!(logo["counts"], json!({"1":1}));
    let new = build(
        json!([card("new", None, true, false)]),
        json!({"new":4}),
        200,
    );
    assert_eq!(new["moves"], json!([]));
    assert_eq!(new["counts"], json!({"4":1}));
    let full = build(
        json!([
            card("0", Some(1), true, false),
            card("1", Some(1), true, false),
            card("2", Some(1), true, false)
        ]),
        json!({"0":1,"1":1,"2":1}),
        2,
    );
    assert_eq!(full["over_capacity"], json!({"1":3}));
    assert_eq!(full["counts"], json!({"1":3}));
    let root = tooling_fixture::Root::new("original panel plan roundtrip");
    let path = root.0.join("pack_plan.json");
    let written = build(
        json!([card("a", Some(2), true, false)]),
        json!({"a":5}),
        200,
    );
    _native::atomic::write_json(&path, &written).unwrap();
    assert_eq!(path.file_name().unwrap(), "pack_plan.json");
    assert_eq!(_native::panel_save::read_plan(&path).unwrap(), written);
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    root.clean().unwrap();

    let merge = |previous: Value, live: Value| {
        _native::intent::execute(json!({
            "operation":"merge","previous":previous,"view":[card("here",Some(1),true,false)],
            "targets":{"here":1},"scope":["here"],"per_set":200,"live":live,"written_utc":"fixture"
        }))
        .unwrap()
    };
    let previous = json!({"version":1,"targets":[["gone",4],["stays",5]],
        "known":["gone","stays"],"excluded":[],"moves":[],"held":[]});
    let removed = merge(previous.clone(), json!(["here", "stays"]));
    assert_eq!(removed["targets"], json!([["here", 1], ["stays", 5]]));
    assert!(
        !removed["known"]
            .as_array()
            .unwrap()
            .contains(&json!("gone"))
    );
    assert!(removed["counts"].get("4").is_none());
    let mut previous = previous;
    previous["targets"] = json!([["gone", 4]]);
    previous["known"] = json!(["gone"]);
    assert_eq!(
        merge(previous, Value::Null)["targets"],
        json!([["gone", 4], ["here", 1]])
    );
    let previous = json!({"version":1,"targets":[],"known":["gone"],"excluded":["gone"],
        "moves":[],"held":[{"key":"gone","label":"gone","from_pack":3}]});
    let removed = merge(previous, json!(["here"]));
    assert_eq!(removed["held"], json!([]));
    assert_eq!(removed["excluded"], json!([]));
    let previous = json!({"version":1,"targets":[["gone",9]],"known":["gone"],
        "excluded":[],"moves":[],"held":[],"logo_slots":{"9":1,"1":1}});
    let removed = merge(previous, json!(["here"]));
    assert_eq!(removed["logo_slots"], json!({"1":1}));
    assert_eq!(removed["over_capacity"], json!({}));
    let previous = json!({"version":1,"targets":[["elsewhere",6]],"known":["elsewhere"],
        "excluded":[],"moves":[],"held":[]});
    assert_eq!(
        merge(previous, json!(["here", "elsewhere"]))["targets"],
        json!([["elsewhere", 6], ["here", 1]])
    );
}

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[test]
fn historical_oracle_rejects_recipe_drift_even_with_consistent_expected_results() {
    let original: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/backend/intent.json")).unwrap();
    _native::source_intent_oracle::fixture_identity(&original).unwrap();
    let mut reordered = original.clone();
    reordered.sort_all_objects();
    _native::source_intent_oracle::fixture_identity(&reordered).unwrap();
    // Outputs are checked by historical source replay, not by this independent input pin.
    let mut output_only = original.clone();
    output_only["cases"][0]["expected"] = json!({"target-derived":"must not pass replay"});
    _native::source_intent_oracle::fixture_identity(&output_only).unwrap();
    for mutation in [
        "consistent-result",
        "name",
        "order",
        "operation",
        "extra",
        "clock",
    ] {
        let mut changed = original.clone();
        match mutation {
            "consistent-result" => {
                changed["cases"][0]["per_set"] = json!(1);
                changed["cases"][0]["expected"]["per_set"] = json!(1);
            }
            "name" => changed["cases"][0]["name"] = json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "operation" => changed["cases"][0]["operation"] = json!("merge"),
            "extra" => changed["unchecked-metadata"] = json!(true),
            "clock" => changed["utc"] = json!("2026-10-08T00:00:00Z"),
            _ => unreachable!(),
        }
        assert!(
            _native::source_intent_oracle::fixture_identity(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
    assert_eq!(original["cases"][0]["per_set"], 200);
}

#[test]
fn resume_oracle_pins_the_original_recipe_but_replays_outputs_independently() {
    let original: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/backend/resume.json")).unwrap();
    _native::source_resume_oracle::fixture_identity(&original).unwrap();
    let mut reordered = original.clone();
    reordered.sort_all_objects();
    _native::source_resume_oracle::fixture_identity(&reordered).unwrap();
    let mut output_only = original.clone();
    output_only["cases"][0]["expected"] = json!({"target-derived":"must not pass replay"});
    _native::source_resume_oracle::fixture_identity(&output_only).unwrap();
    for mutation in ["consistent-result", "name", "order", "extra"] {
        let mut changed = original.clone();
        match mutation {
            "consistent-result" => {
                changed["cases"][0]["state"]["base"] = json!("changed-family");
                changed["cases"][0]["expected"]["base"] = json!("changed-family");
            }
            "name" => changed["cases"][0]["name"] = json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "extra" => changed["unchecked-metadata"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            _native::source_resume_oracle::fixture_identity(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
    assert_eq!(original["cases"][0]["state"]["base"], "fixture");
}

#[test]
fn original_collection_state_shape_cases_keep_format_indices_capacity_and_diagnostics() {
    let set = json!({"fmt":"static","index":1,"name":"pks1_by_YourEmojiBot",
        "title":"Pack 1","live":2,"logo":false,"keys":["s:first","s:second"]});
    let state = |sets: Vec<Value>| json!({"base":"pk","sets":sets,"sent":[]});
    let valid = state(vec![set.clone()]);
    let accepted = _native::resume::collection(valid.clone(), "pk").unwrap();
    assert_eq!(accepted["sets"], valid["sets"]);
    assert_eq!(accepted["sent_full"], json!([]));
    assert_eq!(accepted["skipped"], json!([]));
    let mut first = set.clone();
    first["keys"] = json!(["s:first"]);
    first["live"] = json!(1);
    let mut second = first.clone();
    second["fmt"] = json!("video");
    second["name"] = json!("pkv1_by_bot");
    second["keys"] = json!(["s:second"]);
    let accepted = _native::resume::collection(state(vec![first.clone(), second]), "pk").unwrap();
    assert_eq!(accepted["sets"].as_array().unwrap().len(), 2);
    assert!(
        accepted["sets"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["index"] == 1)
    );

    let mut cases = Vec::new();
    let mut other = valid.clone();
    other["base"] = json!("other");
    cases.push(("different base", other, "other"));
    for (field, value) in [
        ("fmt", json!("nope")),
        ("live", json!(-1)),
        ("live", json!(201)),
        ("live", json!(1)),
        ("logo", json!(true)),
        ("keys", json!(["s:first", "s:first"])),
        ("keys", json!("not-a-list")),
        ("keys", json!([1, 2])),
        ("logo", json!("yes")),
        ("live", json!("two")),
        ("live", json!(true)),
        ("index", json!(true)),
        ("title", json!("")),
        ("name", json!("")),
    ] {
        let mut changed = set.clone();
        changed[field] = value;
        if field == "live" && matches!(changed["live"].as_i64(), Some(-1 | 201)) {
            changed["keys"] = json!([]);
        }
        let fragment = if field == "live" && changed["live"] == 1 {
            "records 2"
        } else {
            ""
        };
        cases.push((field, state(vec![changed]), fragment));
    }
    for mutation in [
        "duplicate key",
        "duplicate name",
        "repeated index",
        "backwards index",
    ] {
        let mut a = first.clone();
        let mut b = first.clone();
        b["name"] = json!("pks2_by_YourEmojiBot");
        b["index"] = json!(2);
        b["keys"] = json!(["s:second"]);
        match mutation {
            "duplicate key" => b["keys"] = json!(["s:first"]),
            "duplicate name" => b["name"] = a["name"].clone(),
            "repeated index" => {
                a["index"] = json!(2);
                b["index"] = json!(1);
            }
            "backwards index" => {
                a["index"] = json!(2);
                b["index"] = json!(0);
            }
            _ => unreachable!(),
        }
        cases.push((mutation, state(vec![a, b]), ""));
    }
    for (field, value) in [
        ("sent", json!([null])),
        ("sent", json!([""])),
        ("skipped", json!([{"k":1}])),
    ] {
        let mut changed = state(vec![]);
        changed[field] = value;
        cases.push((field, changed, ""));
    }
    assert_eq!(cases.len(), 22);
    for (name, input, fragment) in cases {
        let error = _native::resume::collection(input, "pk").unwrap_err();
        assert!(error.contains(fragment), "{name}: {error}");
    }
    assert!(_native::resume::frozen(&json!({"static":"s:a"})).is_err());
    assert!(_native::resume::frozen(&json!({"static":["s:a"]})).is_ok());
}

#[test]
fn original_collection_resume_cases_keep_membership_defaults_and_refusals() {
    let fixture = fixture(
        include_str!("../../tests/fixtures/backend/resume.json"),
        "d173116:collection_state._validate_state",
        "ffd211b0b760b906e6b8d9d84dfde12c274e26730f52bdabe4e802beb1088b9a",
    );
    for case in fixture["cases"].as_array().unwrap() {
        let result = _native::resume::execute(json!({
            "operation":"collection","base":"fixture","state":case["state"]
        }));
        if case.get("error").is_some() {
            assert!(result.is_err(), "{}", case["name"]);
        } else {
            assert_eq!(result.unwrap(), case["expected"], "{}", case["name"]);
        }
    }
}
