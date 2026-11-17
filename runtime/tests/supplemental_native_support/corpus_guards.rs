//! Retained frozen input-identity/corpus integrity cases; separate from1516.
use serde_json::{Value, json};

// Original native/tests/bot_roster_parity.rs::bot_oracle_rejects_coordinated_input_or_name_drift_before_source_execution; source SHA256 fb7b3c1c152454a972ff15b307d4b84b2526d10c38bbb407a9c23ecba82c0d26.
#[test]
fn bot_oracle_rejects_coordinated_input_or_name_drift_before_source_execution() {
    let original: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/backend/bot.json")).unwrap();
    super::corpus_recipe::bot(&original).unwrap();
    let mut reordered = original.clone();
    reordered.sort_all_objects();
    super::corpus_recipe::bot(&reordered).unwrap();
    for mutation in ["consistent-result", "name", "order"] {
        let mut changed = original.clone();
        match mutation {
            "consistent-result" => {
                changed["cases"][4]["text"] = serde_json::json!("111111111111111");
                changed["cases"][4]["expected"] = serde_json::json!(["111111111111111"]);
            }
            "name" => changed["cases"][0]["name"] = serde_json::json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            _ => unreachable!(),
        }
        assert!(
            super::corpus_recipe::bot(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
}

// Original native/tests/bot_roster_parity.rs::roster_oracle_pins_live_id_history_and_case_recipe_before_replay; source SHA256 fb7b3c1c152454a972ff15b307d4b84b2526d10c38bbb407a9c23ecba82c0d26.
#[test]
fn roster_oracle_pins_live_id_history_and_case_recipe_before_replay() {
    let original: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/backend/roster.json")).unwrap();
    super::corpus_recipe::roster(&original).unwrap();
    let mut reordered = original.clone();
    reordered.sort_all_objects();
    super::corpus_recipe::roster(&reordered).unwrap();
    for mutation in ["consistent-result", "name", "order", "extra"] {
        let mut changed = original.clone();
        match mutation {
            "consistent-result" => {
                changed["cases"][0]["st"]["custom_emoji_id"] = serde_json::json!("555555555");
                changed["cases"][0]["expected"]["custom_emoji_id"] = serde_json::json!("555555555");
            }
            "name" => changed["cases"][0]["name"] = serde_json::json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "extra" => changed["unchecked-metadata"] = serde_json::json!(true),
            _ => unreachable!(),
        }
        assert!(
            super::corpus_recipe::roster(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
}

// Original native/tests/intent_parity.rs::historical_oracle_rejects_recipe_drift_even_with_consistent_expected_results; source SHA256 bec8dee03a8cbec32960672b30fdbbdf559e73da5884f313c877a226460063b7.
#[test]
fn historical_oracle_rejects_recipe_drift_even_with_consistent_expected_results() {
    let original: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/backend/intent.json")).unwrap();
    super::corpus_recipe::intent(&original).unwrap();
    let mut reordered = original.clone();
    reordered.sort_all_objects();
    super::corpus_recipe::intent(&reordered).unwrap();
    // Input pins do not certify outcomes; separate native corpus replays assert those.
    let mut output_only = original.clone();
    output_only["cases"][0]["expected"] = json!({"target-derived":"must not pass replay"});
    super::corpus_recipe::intent(&output_only).unwrap();
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
            super::corpus_recipe::intent(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
    assert_eq!(original["cases"][0]["per_set"], 200);
}

// Original native/tests/intent_parity.rs::resume_oracle_pins_the_original_recipe_but_replays_outputs_independently; source SHA256 bec8dee03a8cbec32960672b30fdbbdf559e73da5884f313c877a226460063b7.
#[test]
fn resume_oracle_pins_the_original_recipe_but_replays_outputs_independently() {
    let original: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/backend/resume.json")).unwrap();
    super::corpus_recipe::resume(&original).unwrap();
    let mut reordered = original.clone();
    reordered.sort_all_objects();
    super::corpus_recipe::resume(&reordered).unwrap();
    let mut output_only = original.clone();
    output_only["cases"][0]["expected"] = json!({"target-derived":"must not pass replay"});
    super::corpus_recipe::resume(&output_only).unwrap();
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
            super::corpus_recipe::resume(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
    assert_eq!(original["cases"][0]["state"]["base"], "fixture");
}

// Original native/tests/reconcile_view_parity.rs::reconciliation_oracle_pins_inputs_but_replays_results_and_call_order; source SHA256 b235f3c3c4128805e356fd9dbcb01a64922a8f9f8ea30f74303f9cba32803a6c.
#[test]
fn reconciliation_oracle_pins_inputs_but_replays_results_and_call_order() {
    let original: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/backend/reconcile.json"
    ))
    .unwrap();
    super::corpus_recipe::reconcile(&original).unwrap();
    let mut trace_only = original.clone();
    trace_only["cases"][4]["calls"] = json!(["s:blue", "s:red"]);
    super::corpus_recipe::reconcile(&trace_only).unwrap();
    let mut output_only = original.clone();
    output_only["cases"][0]["expected"] = json!("unverified");
    super::corpus_recipe::reconcile(&output_only).unwrap();
    for mutation in ["input", "name", "order", "comparison"] {
        let mut changed = original.clone();
        match mutation {
            "input" => changed["cases"][4]["probe"] = json!(255),
            "name" => changed["cases"][0]["name"] = json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "comparison" => changed["cases"][4]["comparison"]["s:red"] = json!(false),
            _ => unreachable!(),
        }
        assert!(
            super::corpus_recipe::reconcile(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
}

// Original native/tests/reconcile_view_parity.rs::view_oracle_rejects_coordinated_input_result_and_case_identity_drift; source SHA256 b235f3c3c4128805e356fd9dbcb01a64922a8f9f8ea30f74303f9cba32803a6c.
#[test]
fn view_oracle_rejects_coordinated_input_result_and_case_identity_drift() {
    let original: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/backend/view.json")).unwrap();
    super::corpus_recipe::view(&original).unwrap();
    for mutation in ["input-result", "name", "order", "extra"] {
        let mut changed = original.clone();
        match mutation {
            "input-result" => {
                changed["cases"][0]["request"]["items"][0]["keywords"] = json!(["changed"]);
                changed["cases"][0]["expected"]["view"][0]["label"] = json!("changed");
                changed["cases"][0]["expected"]["view"][0]["copyId"] = json!("");
            }
            "name" => changed["cases"][0]["name"] = json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "extra" => changed["unused"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            super::corpus_recipe::view(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
    let mut observed_only = original.clone();
    observed_only["cases"][0]["expected"]["hidden"] = json!(0);
    super::corpus_recipe::view(&observed_only).unwrap();
    let mut reordered_objects = original;
    reordered_objects.sort_all_objects();
    super::corpus_recipe::view(&reordered_objects).unwrap();
}

// Original native/tests/plan_steps_parity.rs::original_empty_duplicated_or_unannotated_corpora_cannot_pass; source SHA256 205964accf3453c96eabee4bbe138a224c9fb6b34419116516bd9d543b607c2e.
#[test]
fn original_empty_duplicated_or_unannotated_corpora_cannot_pass() {
    let original: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/backend/steps.json")).unwrap();
    for mutation in ["empty", "duplicate", "unannotated", "both", "wrong-source"] {
        let mut broken = original.clone();
        match mutation {
            "empty" => broken["cases"] = json!([]),
            "duplicate" => broken["cases"][1]["name"] = broken["cases"][0]["name"].clone(),
            "unannotated" => {
                broken["cases"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("expected");
            }
            "both" => broken["cases"][0]["error"] = json!("not a source result"),
            "wrong-source" => broken["source"] = json!("unverified"),
            _ => unreachable!(),
        }
        assert!(
            super::corpus_recipe::cases(&broken, "d173116:plan_apply.compute_steps", 6).is_err(),
            "{mutation}"
        );
    }
    // Presence is independent of truthiness: explicit null is a real expected result.
    let null = json!({"source":"fixture","cases":[{"name":"null", "expected":null}]});
    assert!(super::corpus_recipe::cases(&null, "fixture", 1).is_ok());
    for case in [
        json!({"name":"", "expected":0}),
        json!({"expected":0}),
        json!({"name":true,"expected":0}),
        json!({"name":"error","error":""}),
        json!({"name":"error","error":false}),
        json!({"name":"missing"}),
        json!([]),
    ] {
        let invalid = json!({"source":"fixture","cases":[case]});
        assert!(super::corpus_recipe::cases(&invalid, "fixture", 1).is_err());
    }
}
