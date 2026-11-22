use super::{Ticker, intent, mutations, resume_state, set};
use crate::baseline_recovery_support::{Bot, Model, sticker};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_a_foreign_tail_sticker_does_not_resolve_our_in_flight_add
#[test]
fn resumeafterskippedimage_test_a_foreign_tail_sticker_does_not_resolve_our_in_flight_add() {
    let f = Ticker::new(true, 3);
    resume_state(&f, 0, intent("b", "add", 1, 0));
    let bot = Bot::new(f.set(1, &[Some(false)]));
    assert_eq!(f.ordinary(&bot, &[]), 4);
    assert!(!f.read()["done"].as_array().unwrap().contains(&json!("b")));
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_a_removal_masking_our_add_does_not_re_upload_a_live_image
#[test]
fn resumeafterskippedimage_test_a_removal_masking_our_add_does_not_re_upload_a_live_image() {
    let f = Ticker::new(true, 3);
    resume_state(&f, 2, intent("b", "add", 1, 2));
    let bot = Bot::new(f.set(2, &[Some(false), Some(true)]));
    assert_ne!(f.ordinary(&bot, &[]), 4);
    assert!(!mutations(&bot).iter().any(|s| s == "b.png"));
    assert!(f.read()["done"].as_array().unwrap().contains(&json!("b")));
    assert!(f.read()["in_flight"].is_null());
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_corrupt_state_does_not_restart_from_zero
#[test]
fn resumeafterskippedimage_test_corrupt_state_does_not_restart_from_zero() {
    let f = Ticker::new(true, 3);
    let bytes = b"{\"base\": \"t\", \"done\": [trunca";
    std::fs::write(&f.state, bytes).unwrap();
    let bot = Bot::new(f.model());
    assert_eq!(f.ordinary(&bot, &[]), 4);
    assert!(mutations(&bot).is_empty());
    assert_eq!(std::fs::read(&f.state).unwrap(), bytes);
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_in_flight_record_attributes_the_upload_exactly
#[test]
fn resumeafterskippedimage_test_in_flight_record_attributes_the_upload_exactly() {
    let f = Ticker::new(true, 3);
    resume_state(&f, 0, json!("b"));
    let bot = Bot::new(f.set(1, &[Some(true)]));
    assert_eq!(f.ordinary(&bot, &[]), 3);
    let uploads = mutations(&bot);
    assert!(!uploads.iter().any(|s| s == "b.png"));
    assert!(uploads.iter().any(|s| s == "c.png"));
    assert!(f.read()["done"].as_array().unwrap().contains(&json!("b")));
    assert!(f.read()["in_flight"].is_null());
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_intent_is_recorded_before_each_upload
#[test]
fn resumeafterskippedimage_test_intent_is_recorded_before_each_upload() {
    let f = Ticker::new(true, 3);
    let bot = Bot::new(f.model());
    let observed = Arc::new(Mutex::new(vec![]));
    assert_eq!(f.run(&bot, &[], None, observed.clone()).unwrap(), 3);
    let seen = observed.lock().unwrap();
    assert_eq!(
        seen.iter()
            .map(|v| v["key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["b", "c"]
    );
    assert_eq!(
        seen.iter()
            .map(|v| v["operation"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["create", "add"]
    );
    assert!(
        seen.iter()
            .all(|v| v["set_name"].as_str().is_some_and(|s| !s.is_empty()))
    );
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_ours_behind_a_foreign_sticker_is_reconciled_not_stranded
#[test]
fn resumeafterskippedimage_test_ours_behind_a_foreign_sticker_is_reconciled_not_stranded() {
    let f = Ticker::new(true, 3);
    resume_state(&f, 0, intent("b", "add", 1, 0));
    let bot = Bot::new(f.set(2, &[Some(false), Some(true)]));
    assert_ne!(f.ordinary(&bot, &[]), 4);
    let saved = f.read();
    assert!(saved["done"].as_array().unwrap().contains(&json!("b")));
    assert!(saved["sets"][0]["count"].as_u64().unwrap() >= 2);
    assert!(!mutations(&bot).iter().any(|s| s == "b.png"));
}

// Original: tests.test_resume_safety.ResumeAfterSkippedImage.test_unexplained_drift_refuses_to_guess
#[test]
fn resumeafterskippedimage_test_unexplained_drift_refuses_to_guess() {
    let f = Ticker::new(true, 3);
    resume_state(&f, 0, Value::Null);
    let bot = Bot::new(f.set(2, &[Some(true), Some(true)]));
    assert_eq!(f.ordinary(&bot, &[]), 4);
    assert!(mutations(&bot).is_empty());
}

fn later() -> Ticker {
    let f = Ticker::new(false, 2);
    let mut s = f.initial(1);
    s["done"] = json!(["a"]);
    s["sets"] = json!([set(1, 1)]);
    s["in_flight"] = intent("b", "create", 2, 0);
    f.save(s);
    f
}
fn later_model(f: &Ticker) -> Model {
    let mut m = f.model();
    m.sets
        .insert("t1_by_bot".into(), vec![sticker("f1", "cid1")]);
    m.bodies
        .insert("f1".into(), std::fs::read(f.src.join("a.png")).unwrap());
    m
}
fn recorded() -> Ticker {
    let f = Ticker::new(false, 1);
    let mut s = f.initial(200);
    s["done"] = json!(["x"]);
    s["sets"] = json!([set(5, 1)]);
    f.save(s);
    f
}

// Original: tests.test_unresolved_mutation.AmbiguousCreateForLaterSets.test_a_create_target_holding_EXTRA_stickers_is_never_adopted
#[test]
fn ambiguouscreateforlatersets_test_a_create_target_holding_extra_stickers_is_never_adopted() {
    let f = later();
    let mut m = later_model(&f);
    m.sets.insert(
        "t2_by_bot".into(),
        vec![sticker("f2", "cid2"), sticker("extra", "cid-extra")],
    );
    m.bodies
        .insert("f2".into(), std::fs::read(f.src.join("b.png")).unwrap());
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &["--per-set", "1"]), 4);
    assert!(
        !f.read()["sets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "t2_by_bot")
    );
    assert!(!f.read()["done"].as_array().unwrap().contains(&json!("b")));
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_unresolved_mutation.AmbiguousCreateForLaterSets.test_a_same_named_set_holding_a_FOREIGN_image_is_never_adopted
#[test]
fn ambiguouscreateforlatersets_test_a_same_named_set_holding_a_foreign_image_is_never_adopted() {
    let f = later();
    let mut m = later_model(&f);
    m.sets
        .insert("t2_by_bot".into(), vec![sticker("foreign", "cid2")]);
    m.bodies.insert(
        "foreign".into(),
        std::fs::read(f.src.join("a.png")).unwrap(),
    );
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &["--per-set", "1"]), 4);
    assert!(
        !f.read()["sets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["name"] == "t2_by_bot")
    );
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_unresolved_mutation.AmbiguousCreateForLaterSets.test_an_unverifiable_create_target_stops_retryably
#[test]
fn ambiguouscreateforlatersets_test_an_unverifiable_create_target_stops_retryably() {
    let f = later();
    let mut m = later_model(&f);
    m.sets
        .insert("t2_by_bot".into(), vec![sticker("x", "cid2")]);
    m.bodies.insert("x".into(), b"undecodable".to_vec());
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &["--per-set", "1"]), 3);
    assert!(!f.read()["in_flight"].is_null());
}

// Original: tests.test_unresolved_mutation.AmbiguousCreateForLaterSets.test_landed_create_of_set_two_is_adopted
#[test]
fn ambiguouscreateforlatersets_test_landed_create_of_set_two_is_adopted() {
    let f = later();
    let mut m = later_model(&f);
    m.sets
        .insert("t2_by_bot".into(), vec![sticker("f2", "cid2")]);
    m.bodies
        .insert("f2".into(), std::fs::read(f.src.join("b.png")).unwrap());
    let bot = Bot::new(m);
    f.ordinary(&bot, &["--per-set", "1"]);
    assert!(mutations(&bot).is_empty());
    let s = f.read();
    assert_eq!(
        s["sets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["t1_by_bot", "t2_by_bot"]
    );
    assert!(s["done"].as_array().unwrap().contains(&json!("b")));
    assert!(s["in_flight"].is_null());
}

// Original: tests.test_unresolved_mutation.AmbiguousCreateForLaterSets.test_missing_create_of_set_two_leaves_the_item_pending
#[test]
fn ambiguouscreateforlatersets_test_missing_create_of_set_two_leaves_the_item_pending() {
    let f = later();
    let bot = Bot::new(later_model(&f));
    f.ordinary(&bot, &["--per-set", "1"]);
    assert!(f.read()["done"].as_array().unwrap().contains(&json!("b")));
    assert!(bot.edit(|m| {
        m.requests
            .iter()
            .any(|r| r.path.ends_with("createNewStickerSet") && r.form["name"] == "t2_by_bot")
    }));
}

// Original: tests.test_unresolved_mutation.AmbiguousCreateForLaterSets.test_unknown_create_stops_retryably
#[test]
fn ambiguouscreateforlatersets_test_unknown_create_stops_retryably() {
    let f = later();
    let mut m = later_model(&f);
    m.unknown.insert("t2_by_bot".into());
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &["--per-set", "1"]), 3);
    assert!(mutations(&bot).is_empty());
    assert!(!f.read()["in_flight"].is_null());
}

// Original: tests.test_unresolved_mutation.RecordedSetIntegrity.test_deleted_recorded_set_is_an_integrity_stop
#[test]
fn recordedsetintegrity_test_deleted_recorded_set_is_an_integrity_stop() {
    let f = recorded();
    let bot = Bot::new(f.model());
    assert_eq!(f.ordinary(&bot, &[]), 4);
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_unresolved_mutation.RecordedSetIntegrity.test_shrunken_set_is_an_integrity_stop
#[test]
fn recordedsetintegrity_test_shrunken_set_is_an_integrity_stop() {
    let f = recorded();
    let mut m = f.model();
    m.sets
        .insert("t1_by_bot".into(), vec![sticker("f", "c"); 3]);
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &[]), 4);
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_unresolved_mutation.RecordedSetIntegrity.test_unknown_recorded_set_is_retryable
#[test]
fn recordedsetintegrity_test_unknown_recorded_set_is_retryable() {
    let f = recorded();
    let mut m = f.model();
    m.unknown.insert("t1_by_bot".into());
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &[]), 3);
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_a_strangers_set_is_not_adopted_after_an_ambiguous_create
#[test]
fn unresolvedmutationstopstherun_test_a_strangers_set_is_not_adopted_after_an_ambiguous_create() {
    let f = Ticker::new(false, 3);
    let bot = Bot::new(f.model());
    assert_eq!(
        f.run(&bot, &[], Some("foreign-create"), Arc::default())
            .unwrap(),
        3
    );
    let s = f.read();
    assert_eq!(s["sets"], json!([]));
    assert_eq!(s["done"], json!([]));
    assert!(!s["in_flight"].is_null());
    assert!(bot.edit(|m| m.messages.is_empty()));
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_ambiguous_add_stops_before_the_next_item
#[test]
fn unresolvedmutationstopstherun_test_ambiguous_add_stops_before_the_next_item() {
    let f = Ticker::new(false, 3);
    let bot = Bot::new(f.model());
    assert_eq!(
        f.run(&bot, &[], Some("ambiguous-add"), Arc::default())
            .unwrap(),
        3
    );
    let s = f.read();
    assert_eq!(s["in_flight"]["operation"], "add");
    assert_eq!(s["in_flight"]["key"], "b");
    assert_eq!(
        bot.edit(|m| m
            .requests
            .iter()
            .filter(|r| r.path.ends_with("addStickerToSet"))
            .count()),
        1
    );
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_ambiguous_create_records_the_target_set
#[test]
fn unresolvedmutationstopstherun_test_ambiguous_create_records_the_target_set() {
    let f = Ticker::new(false, 3);
    let bot = Bot::new(f.model());
    assert_eq!(
        f.run(&bot, &[], Some("missing-create"), Arc::default())
            .unwrap(),
        3
    );
    let s = f.read();
    assert_eq!(s["in_flight"]["operation"], "create");
    assert!(!s["in_flight"]["set_name"].as_str().unwrap().is_empty());
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_an_unverifiable_set_is_not_adopted_after_an_ambiguous_create
#[test]
fn unresolvedmutationstopstherun_test_an_unverifiable_set_is_not_adopted_after_an_ambiguous_create()
{
    let f = Ticker::new(false, 3);
    let bot = Bot::new(f.model());
    assert_eq!(
        f.run(&bot, &[], Some("unknown-create"), Arc::default())
            .unwrap(),
        3
    );
    let s = f.read();
    assert_eq!(s["sets"], json!([]));
    assert!(!s["in_flight"].is_null());
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_our_own_set_is_still_adopted_after_an_ambiguous_create
#[test]
fn unresolvedmutationstopstherun_test_our_own_set_is_still_adopted_after_an_ambiguous_create() {
    let f = Ticker::new(false, 3);
    let bot = Bot::new(f.model());
    f.run(&bot, &[], Some("own-create"), Arc::default())
        .unwrap();
    let s = f.read();
    assert_eq!(
        s["sets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["t1_by_bot"]
    );
    assert!(s["in_flight"].is_null());
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_state_for_a_different_base_is_refused
#[test]
fn unresolvedmutationstopstherun_test_state_for_a_different_base_is_refused() {
    let f = Ticker::new(false, 3);
    let mut s = f.initial(200);
    s["base"] = json!("OTHER");
    f.save(s);
    let bot = Bot::new(f.model());
    assert_eq!(f.ordinary(&bot, &[]), 4);
    assert!(mutations(&bot).is_empty());
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_unknown_live_state_on_resume_keeps_the_intent
#[test]
fn unresolvedmutationstopstherun_test_unknown_live_state_on_resume_keeps_the_intent() {
    let f = Ticker::new(false, 3);
    resume_state(&f, 0, intent("a", "add", 1, 0));
    let mut m = f.model();
    m.unknown.insert("t1_by_bot".into());
    let bot = Bot::new(m);
    assert_eq!(f.ordinary(&bot, &[]), 3);
    assert!(mutations(&bot).is_empty());
    assert!(!f.read()["in_flight"].is_null());
}

// Original: tests.test_unresolved_mutation.UnresolvedMutationStopsTheRun.test_unusable_images_make_the_run_partial
#[test]
fn unresolvedmutationstopstherun_test_unusable_images_make_the_run_partial() {
    let f = Ticker::new(false, 3);
    std::fs::write(f.src.join("bad.png"), b"").unwrap();
    let bot = Bot::new(f.model());
    assert_eq!(f.ordinary(&bot, &[]), 3);
}
