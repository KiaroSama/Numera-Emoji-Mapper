use super::*;
use crate::baseline_packs_support::{
    family::{BASE, Family, name},
    *,
};
fn native_media(
    root: &std::path::Path,
    scratch: std::path::PathBuf,
) -> Result<NativeMediaAdapter, String> {
    Ok(NativeMediaAdapter {
        executable: binary(),
        root: root.into(),
        scratch,
    })
}
fn apply(f: &Family, logo: bool) -> Result<u8, String> {
    let log = RunLog::open(&f.root.0, "plan-apply-case", vec![http::TOKEN.into()]);
    run_with(
        &f.config(logo),
        &f.args(true),
        &log,
        |_| Ok(f.bot.telegram()),
        native_media,
    )
}
fn moved() -> Family {
    let f = Family::new(&[(1, 2), (2, 1)], 0, "mixed");
    let mut t = f.current();
    t.insert(f.by_pack[&1][0].clone(), 2);
    f.plan(t, &[], 200, json!({}));
    f
}
fn refused(f: &Family, needle: &str) {
    let state = std::fs::read(f.data.join(format!("publish_{BASE}.json"))).unwrap();
    let e = apply(f, false).unwrap_err();
    assert!(e.contains(needle), "{e:?} expected {needle}");
    assert!(f.kinds().is_empty());
    assert_eq!(
        std::fs::read(f.data.join(format!("publish_{BASE}.json"))).unwrap(),
        state
    );
    assert!(!f.data.join(format!("plan_apply_{BASE}.json")).exists());
}
static CRASH_DATA: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);
pub(crate) fn after_delete(data: &std::path::Path) {
    if CRASH_DATA.lock().unwrap().as_deref() == Some(data) {
        std::process::exit(91);
    }
}

// Original: tests.test_plan_apply.DryRun.test_lists_the_work_retires_ids_and_writes_nothing
#[test]
#[allow(non_snake_case)]
fn plan_apply__DryRun__test_lists_the_work_retires_ids_and_writes_nothing() {
    let f = Family::new(&[(1, 3), (2, 3)], 1, "mixed");
    let mut t = f.current();
    t.insert(f.by_pack[&1][2].clone(), 2);
    t.insert(f.by_pack[&2][0].clone(), 1);
    t.insert(f.extra[0].clone(), 2);
    f.plan(t, &[], 200, json!({}));
    let before = f.snapshot();
    let (code, output) = f.cli_dry();
    assert_eq!(code, 3);
    let s = f.steps();
    assert_eq!(
        (
            s["removals"].as_array().unwrap().len(),
            s["adds"]
                .as_object()
                .unwrap()
                .values()
                .map(|v| v.as_array().unwrap().len())
                .sum::<usize>(),
            s["counted"].as_u64().unwrap()
        ),
        (2, 3, 5)
    );

    for text in ["5 counted change(s): 1 run(s)", "cid-1-3", "cid-2-1"] {
        assert!(output.contains(text), "{output}");
    }
    assert_eq!(f.snapshot(), before);
    assert!(f.kinds().is_empty());
}

// Original: tests.test_plan_apply.DryRun.test_a_plan_the_packs_already_match_exits_0
#[test]
#[allow(non_snake_case)]
fn plan_apply__DryRun__test_a_plan_the_packs_already_match_exits_0() {
    let f = Family::new(&[(1, 3)], 0, "mixed");
    f.plan(f.current(), &[], 200, json!({}));
    let (code, output) = f.cli_dry();
    assert_eq!(code, 0);
    assert!(output.contains("nothing pending"));
}

// Original: tests.test_plan_apply.DryRun.test_a_held_emoji_that_is_live_stays_and_is_not_a_removal
#[test]
#[allow(non_snake_case)]
fn plan_apply__DryRun__test_a_held_emoji_that_is_live_stays_and_is_not_a_removal() {
    let f = Family::new(&[(1, 2), (2, 2)], 0, "mixed");
    let held = f.by_pack[&2][1].clone();
    let mut t = f.current();
    t.remove(&held);
    f.plan(t, std::slice::from_ref(&held), 200, json!({}));
    let s = f.steps();
    assert_eq!(s["removals"], json!([]));
    assert_eq!(s["held_live"], json!([[held, 2]]));
    let (_, output) = f.cli_dry();
    assert!(output.contains("held, stays live until placed"));
}

// Original: tests.test_plan_apply.DryRun.test_two_hundred_moves_plan_offline_in_seconds
#[test]
#[allow(non_snake_case)]
fn plan_apply__DryRun__test_two_hundred_moves_plan_offline_in_seconds() {
    let keys = (0..400).map(|i| format!("s:{i:04}")).collect::<Vec<_>>();
    let items = keys
        .iter()
        .enumerate()
        .map(|(i, k)| (k.clone(), json!({"included":true,"pos":i})))
        .collect::<serde_json::Map<_, _>>();
    let state = json!({"sets":[{"fmt":"mixed","index":1,"name":"a","logo":true,"keys":&keys[..199]},{"fmt":"mixed","index":2,"name":"b","logo":true,"keys":&keys[199..398]}]});
    let targets = keys[..199]
        .iter()
        .map(|k| json!([k, 2]))
        .chain(keys[199..398].iter().map(|k| json!([k, 1])))
        .collect::<Vec<_>>();
    let ids = keys
        .iter()
        .map(|k| (k.clone(), json!(format!("cid-{k}"))))
        .collect::<serde_json::Map<_, _>>();
    let start = std::time::Instant::now();
    let s = crate::plan_steps::compute(
        &json!({"plan":{"targets":targets},"state":state,"items":items,"ids":ids}),
    )
    .unwrap();
    assert!(start.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(s["removals"].as_array().unwrap().len(), 398);
}

// Original: tests.test_plan_apply.ApplyInBoundedRuns.test_twenty_five_changes_take_two_runs_then_nothing
#[test]
#[allow(non_snake_case)]
fn plan_apply__ApplyInBoundedRuns__test_twenty_five_changes_take_two_runs_then_nothing() {
    let f = Family::new(&[(1, 10), (2, 2)], 5, "mixed");
    let t = f.keys.iter().map(|k| (k.clone(), 2)).collect();
    f.plan(t, &[], 200, json!({}));
    assert_eq!(f.steps()["counted"], 25);
    assert_eq!(apply(&f, false).unwrap(), 3);
    assert_eq!(
        f.kinds()
            .iter()
            .filter(|k| ["delete", "add", "create"].contains(&k.as_str()))
            .count(),
        20
    );
    f.invariant();
    assert_eq!(apply(&f, false).unwrap(), 0);
    let kinds = f.kinds();
    assert_eq!(
        (
            kinds.iter().filter(|k| *k == "delete").count(),
            kinds.iter().filter(|k| *k == "add").count()
        ),
        (10, 15)
    );
    f.invariant();
    let mut cat = f.cat();
    let want = f
        .keys
        .iter()
        .map(|k| {
            cat.dispatch(json!({"operation":"custom_id","base":BASE,"content_key":k}))
                .unwrap()
        })
        .collect::<Vec<_>>();
    drop(cat);
    let m = f.model.lock().unwrap();
    assert_eq!(
        m.sets[&name(2)][1..]
            .iter()
            .map(|s| s["custom_emoji_id"].clone())
            .collect::<Vec<_>>(),
        want
    );
    assert_eq!(m.sets[&name(1)].len(), 1);
    drop(m);
    assert_eq!(f.journal()["retired"].as_array().unwrap().len(), 10);
    let writes = f.kinds().len();
    assert_eq!(apply(&f, false).unwrap(), 0);
    assert_eq!(f.kinds().len(), writes);
}

// Original: tests.test_plan_apply.ApplyInBoundedRuns.test_the_next_new_pack_is_created_with_the_logo_first
#[test]
#[allow(non_snake_case)]
fn plan_apply__ApplyInBoundedRuns__test_the_next_new_pack_is_created_with_the_logo_first() {
    let f = Family::new(&[(1, 1), (2, 1)], 2, "mixed");
    let mut t = f.current();
    for k in &f.extra {
        t.insert(k.clone(), 3);
    }
    f.plan(t, &[], 200, json!({}));
    let s = f.steps();
    assert_eq!(
        (s["create"].as_u64(), s["counted"].as_u64()),
        (Some(3), Some(3))
    );
    f.root.write(
        "collection/brand-logo.png",
        include_bytes!("fixtures/family-999.png"),
    );
    assert_eq!(apply(&f, true).unwrap(), 0);
    assert_eq!(f.kinds(), ["create", "add", "add"]);
    assert_eq!(
        f.model.lock().unwrap().sets[&name(3)][0]["file_unique_id"],
        "FU-new-1"
    );
    let state = f.state();
    let rec = state["sets"].as_array().unwrap().last().unwrap();
    assert_eq!(rec["index"], 3);
    assert_eq!(rec["title"], "Test Pack 3");
    assert_eq!(rec["keys"], json!(f.extra));
    f.invariant();
}

// Original: tests.test_plan_apply.ApplyInBoundedRuns.test_a_delete_applied_before_a_crash_is_settled_not_resent
#[test]
#[allow(non_snake_case)]
fn plan_apply__ApplyInBoundedRuns__test_a_delete_applied_before_a_crash_is_settled_not_resent() {
    if let Ok(data) = std::env::var("NUMERA_PACKS_CRASH_DATA") {
        let data = std::path::PathBuf::from(data);
        *CRASH_DATA.lock().unwrap() = Some(data.clone());
        let root = data.parent().unwrap().to_path_buf();
        let env = std::collections::BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("COLLECTION_PACK_BASE".into(), BASE.into()),
            ("PACK_OWNER_USER_ID".into(), "111111111".into()),
            ("GENERAL_BOT_TOKEN".into(), http::TOKEN.into()),
            ("BRAND_LOGO_BOTS".into(), "".into()),
        ]);
        let config = Config::load_with_environment(root.clone(), &env).unwrap();
        let log = RunLog::open(&root, "crash-child", vec![]);
        let api = std::env::var("NUMERA_PACKS_CRASH_API").unwrap();
        let result = run_with(
            &config,
            &[
                "--base".into(),
                BASE.into(),
                "--data-dir".into(),
                data.to_str().unwrap().into(),
                "--apply".into(),
            ],
            &log,
            |_| Telegram::new(http::TOKEN.into(), api.clone()).map_err(|e| e.to_string()),
            native_media,
        );
        panic!("child missed exact after-delete crash boundary: {result:?}");
    }
    let f = moved();
    let moved = f.by_pack[&1][0].clone();
    let child=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","plan_apply::baseline_packs_tests::plan_apply__ApplyInBoundedRuns__test_a_delete_applied_before_a_crash_is_settled_not_resent","--nocapture"]).env("NUMERA_PACKS_CRASH_DATA",&f.data).env("NUMERA_PACKS_CRASH_API",&f.bot.base).output().unwrap();
    assert_eq!(
        child.status.code(),
        Some(91),
        "{} {}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert_eq!(f.journal()["intent"]["key"], moved);
    assert_eq!(apply(&f, false).unwrap(), 0);
    assert_eq!(f.kinds().iter().filter(|k| *k == "delete").count(), 1);
    assert_eq!(f.kinds().iter().filter(|k| *k == "add").count(), 1);
    f.invariant();
}

// Original: tests.test_plan_apply.ApplyInBoundedRuns.test_an_intent_whose_delete_never_landed_is_cleared_and_redone_once
#[test]
#[allow(non_snake_case)]
fn plan_apply__ApplyInBoundedRuns__test_an_intent_whose_delete_never_landed_is_cleared_and_redone_once()
 {
    let f = moved();
    let key = &f.by_pack[&1][0];
    atomic::write_json(&f.data.join(format!("plan_apply_{BASE}.json")),&json!({"version":1,"base":BASE,"retired":[],"intent":{"op":"remove","key":key,"set":name(1),"cid":"cid-1-1","at":"2026-09-29T00:00:00Z"}})).unwrap();
    assert_eq!(apply(&f, false).unwrap(), 0);
    assert_eq!(f.kinds().iter().filter(|k| *k == "delete").count(), 1);
    f.invariant();
}

// Original: tests.test_plan_apply.LongFloodWaitEndsTheRunCleanly.test_on_a_delete_the_intent_is_cleared_and_nothing_is_recorded
#[test]
#[allow(non_snake_case)]
fn plan_apply__LongFloodWaitEndsTheRunCleanly__test_on_a_delete_the_intent_is_cleared_and_nothing_is_recorded()
 {
    let f = moved();
    f.model.lock().unwrap().flood_on.insert("delete".into());
    assert_eq!(apply(&f, false).unwrap(), 3);
    let j = f.journal();
    assert!(j["intent"].is_null());
    assert_eq!(j["retired"], json!([]));
    assert!(f.kinds().is_empty());
    f.invariant();
    assert_eq!(apply(&f, false).unwrap(), 0);
    f.invariant();
}

// Original: tests.test_plan_apply.LongFloodWaitEndsTheRunCleanly.test_on_an_add_the_removal_stays_recorded_and_the_next_run_adds
#[test]
#[allow(non_snake_case)]
fn plan_apply__LongFloodWaitEndsTheRunCleanly__test_on_an_add_the_removal_stays_recorded_and_the_next_run_adds()
 {
    let f = moved();
    f.model.lock().unwrap().flood_on.insert("add".into());
    assert_eq!(apply(&f, false).unwrap(), 3);
    assert_eq!(f.kinds(), ["delete"]);
    f.invariant();
    assert_eq!(apply(&f, false).unwrap(), 0);
    assert_eq!(
        f.kinds()
            .iter()
            .filter(|k| *k != "move")
            .cloned()
            .collect::<Vec<_>>(),
        ["delete", "add"]
    );
    f.invariant();
}

// Original: tests.test_plan_apply.RefuseBeforeTouchingAnything.test_a_pack_over_its_cap
#[test]
#[allow(non_snake_case)]
fn plan_apply__RefuseBeforeTouchingAnything__test_a_pack_over_its_cap() {
    let f = Family::new(&[(1, 3)], 2, "mixed");
    let t = f.keys.iter().map(|k| (k.clone(), 1)).collect();
    f.plan(t, &[], 5, json!({}));
    refused(&f, "over the cap of 5");
}

// Original: tests.test_plan_apply.RefuseBeforeTouchingAnything.test_the_panel_flagged_over_capacity
#[test]
#[allow(non_snake_case)]
fn plan_apply__RefuseBeforeTouchingAnything__test_the_panel_flagged_over_capacity() {
    let f = Family::new(&[(1, 1)], 0, "mixed");
    f.plan(f.current(), &[], 200, json!({"1":201}));
    refused(&f, "overfills");
}

// Original: tests.test_plan_apply.RefuseBeforeTouchingAnything.test_a_key_the_catalog_no_longer_holds
#[test]
#[allow(non_snake_case)]
fn plan_apply__RefuseBeforeTouchingAnything__test_a_key_the_catalog_no_longer_holds() {
    let f = Family::new(&[(1, 1)], 0, "mixed");
    let mut t = f.current();
    t.insert("s:gone".into(), 1);
    f.plan(t, &[], 200, json!({}));
    refused(&f, "s:gone");
}

// Original: tests.test_plan_apply.RefuseBeforeTouchingAnything.test_a_per_format_family
#[test]
#[allow(non_snake_case)]
fn plan_apply__RefuseBeforeTouchingAnything__test_a_per_format_family() {
    let f = Family::new(&[(1, 1)], 0, "static");
    f.plan(f.current(), &[], 200, json!({}));
    refused(&f, "--mixed");
}

// Original: tests.test_plan_apply.RefuseBeforeTouchingAnything.test_a_gap_in_the_pack_numbers
#[test]
#[allow(non_snake_case)]
fn plan_apply__RefuseBeforeTouchingAnything__test_a_gap_in_the_pack_numbers() {
    let f = Family::new(&[(1, 2), (2, 1)], 0, "mixed");
    let mut t = f.current();
    t.insert(f.by_pack[&1][0].clone(), 9);
    f.plan(t, &[], 200, json!({}));
    refused(&f, "[9]");
}

// Original: tests.test_plan_apply.RefuseBeforeTouchingAnything.test_an_unreadable_live_set_stops_before_changing_it
#[test]
#[allow(non_snake_case)]
fn plan_apply__RefuseBeforeTouchingAnything__test_an_unreadable_live_set_stops_before_changing_it()
{
    let f = moved();
    f.model.lock().unwrap().unreadable.insert(name(1));
    assert!(matches!(
        f.bot.telegram().probe(&name(1)),
        (crate::telegram::SetState::Unknown, _)
    ));
    let state = std::fs::read(f.data.join(format!("publish_{BASE}.json"))).unwrap();
    assert_eq!(apply(&f, false).unwrap(), 4);
    assert_eq!(
        std::fs::read(f.data.join(format!("publish_{BASE}.json"))).unwrap(),
        state
    );
    assert!(f.kinds().is_empty());
}

// Original: tests.test_plan_apply.RefuseBeforeTouchingAnything.test_the_family_lock_held_by_another_writer
#[test]
#[allow(non_snake_case)]
fn plan_apply__RefuseBeforeTouchingAnything__test_the_family_lock_held_by_another_writer() {
    let f = moved();
    let _held =
        crate::locks::Lease::acquire(&f.data.join(format!("publish_{BASE}.lock")), UTC).unwrap();
    let l = RunLog::open(&f.root.0, "held-family", vec![]);
    let e = run_with(
        &f.config(false),
        &f.args(true),
        &l,
        |_| panic!("held family reached Telegram constructor"),
        native_media,
    )
    .unwrap_err();
    assert!(
        e.contains("lock") || e.contains("writer") || e.contains("already"),
        "{e}"
    );
    assert!(f.kinds().is_empty());
}
