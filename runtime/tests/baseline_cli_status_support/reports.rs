use crate::support::*;
use numera_runtime::plan_status;
use rusqlite::Connection;
use serde_json::{Value, json};

// tests.test_status.TheStatusCommand.test_a_stale_roster_exits_3_and_names_the_fix
#[test]
fn stale_roster_names_refresh_and_exits_three() {
    let f = Fixture::new();
    f.current_roster();
    write(
        &f.data().join("catalog.db"),
        b"changed since roster was written",
    );
    let r = f.run(&["status"], &[]);
    r.code(3);
    assert!(r.out.contains("pack-manifest --refresh"));
}
// tests.test_status.TheStatusCommand.test_everything_current_exits_0
#[test]
fn current_status_is_zero() {
    let f = Fixture::new();
    f.current_roster();
    let r = f.run(&["status"], &[]);
    r.code(0);
    assert!(!r.out.contains("STALE"));
}
// tests.test_status.TheStatusCommand.test_a_stale_archive_is_stale_too
#[test]
fn stale_archive_names_sync() {
    let f = Fixture::new();
    document(
        &f.data().join("publish_fixture.json"),
        &json!({"sets":[{"name":"fixture1","title":"Pack 1","live":200}]}),
    );
    f.current_roster();
    let r = f.run(
        &["status"],
        &[(
            "EMOJI_ARCHIVE_DIR",
            f.root.join("archive").display().to_string(),
        )],
    );
    r.code(3);
    assert!(r.out.contains("pack-archive --sync"));
}
// tests.test_status.TheStatusCommand.test_unpublished_emoji_are_counted_without_writing_the_catalog
#[test]
fn waiting_publication_is_information_and_does_not_write() {
    let f = Fixture::new();
    pending_database(&f);
    f.current_roster();
    let db = f.data().join("catalog.db");
    let before = std::fs::read(&db).unwrap();
    let r = f.run(&["status"], &[]);
    r.code(0);
    assert!(
        r.out
            .contains("1 included emoji not yet published to fixture")
    );
    assert_eq!(std::fs::read(db).unwrap(), before);
}
fn pending_database(f: &Fixture) {
    std::fs::create_dir_all(f.data()).unwrap();
    let db = Connection::open(f.data().join("catalog.db")).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT,included INTEGER);CREATE TABLE publications(base TEXT,content_key TEXT);INSERT INTO items VALUES('s:a',1),('s:b',1),('s:c',0);INSERT INTO publications VALUES('fixture','s:a');").unwrap();
}
// tests.test_native_status_cli.NativeStatusCLI.test_waiting_publication_is_info_and_stale_roster_is_exit_three
#[test]
fn native_status_waiting_then_missing_roster() {
    let f = Fixture::new();
    pending_database(&f);
    f.current_roster();
    let path = f.data().join("catalog.db");
    let before = std::fs::read(&path).unwrap();
    let r = f.run(&["status"], &[]);
    r.code(0);
    assert!(
        r.out
            .contains("1 included emoji not yet published to fixture")
    );
    assert!(r.out.contains("skip  archive"));
    assert!(!r.out.contains("STALE"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::remove_file(f.root.join("packs/index.json")).unwrap();
    let r = f.run(&["status"], &[]);
    r.code(3);
    assert!(r.out.contains("numera-emoji pack-manifest --refresh"));
    assert_eq!(std::fs::read(path).unwrap(), before);
}
fn state() -> Value {
    json!({"sets":[{"index":1,"name":"mine1_by_bot","live":4,"keys":["s:a","s:b","s:c"]},{"index":2,"name":"mine2_by_bot","live":2,"keys":["s:d"]}]})
}
fn plan() -> Value {
    json!({"version":1,"per_set":200,"targets":[["s:a",1],["s:c",1],["s:d",2]],"counts":{"1":2,"2":1},"logo_slots":{"1":1,"2":1},"over_capacity":{},"moves":[],"held":[],"excluded":[]})
}
// tests.test_plan_status.WhatThePlanWouldChange.test_a_plan_that_matches_the_packs_is_not_pending
#[test]
fn matching_plan_has_no_pending_work() {
    let mut p = plan();
    p["targets"] = json!([["s:a", 1], ["s:b", 1], ["s:c", 1], ["s:d", 2]]);
    assert_eq!(
        plan_status::summarize(&p, &state()).unwrap()["pending"],
        false
    );
}
// tests.test_plan_status.WhatThePlanWouldChange.test_moves_held_and_candidates_are_counted_per_pack
#[test]
fn plan_counts_moves_held_and_candidates_per_pack() {
    let mut p = plan();
    p["targets"] = json!([["s:a", 2], ["s:c", 1], ["s:d", 2], ["s:new", 2]]);
    p["moves"] = json!([{"key":"s:a","label":"a","from_pack":1,"to_pack":2}]);
    p["held"] = json!([{"key":"s:b","label":"b","from_pack":1}]);
    p["excluded"] = json!(["s:b"]);
    let packs = &plan_status::summarize(&p, &state()).unwrap()["packs"];
    assert_eq!(packs["1"]["move_out"], json!(["s:a"]));
    assert_eq!(packs["2"]["move_in"], json!(["s:a"]));
    assert_eq!(packs["1"]["held_live"], json!(["s:b"]));
    assert_eq!(packs["2"]["candidates"], json!(["s:new"]));
}
// tests.test_plan_status.WhatThePlanWouldChange.test_over_capacity_is_pending
#[test]
fn over_capacity_plan_is_pending() {
    let mut p = plan();
    p["targets"] = json!([["s:a", 1], ["s:b", 1], ["s:c", 1], ["s:d", 2]]);
    p["counts"] = json!({"1":200,"2":1});
    assert_eq!(
        plan_status::summarize(&p, &state()).unwrap()["pending"],
        true
    );
}
// tests.test_plan_status.WhatThePlanWouldChange.test_the_command_reports_the_ids_it_would_retire_and_exits_3
#[test]
fn plan_status_cli_reports_retired_ids_without_writes() {
    let f = Fixture::new();
    let mut p = plan();
    p["moves"] = json!([{"key":"s:a","label":"a","from_pack":1,"to_pack":2}]);
    document(&f.data().join("pack_plan.json"), &p);
    document(&f.data().join("publish_mine.json"), &state());
    let db = Connection::open(f.data().join("catalog.db")).unwrap();
    db.execute_batch("CREATE TABLE publications(base TEXT,content_key TEXT,custom_emoji_id TEXT);INSERT INTO publications VALUES('mine','s:a','5000000000000000001');").unwrap();
    drop(db);
    let before = snapshot(&f.data());
    let r = f.run(&["plan-status", "--base", "mine"], &[]);
    r.code(3);
    assert!(r.out.contains("5000000000000000001"));
    assert!(r.out.contains("RETIRE"));
    assert_eq!(snapshot(&f.data()), before);
}
// tests.test_plan_status.WhatThePlanWouldChange.test_no_saved_plan_is_nothing_pending
#[test]
fn absent_plan_is_nothing_pending() {
    let f = Fixture::new();
    f.run(&["plan-status", "--base", "mine"], &[]).code(0);
}
