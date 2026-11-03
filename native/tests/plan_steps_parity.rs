//! Original plan-step assertions use the pinned source corpus, not native-generated expectations.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

fn report_state() -> Value {
    json!({"sets":[{"index":1,"name":"mine1_by_bot","live":4,"keys":["s:a","s:b","s:c"]},
        {"index":2,"name":"mine2_by_bot","live":2,"keys":["s:d"]}]})
}
fn report_plan() -> Value {
    json!({"version":1,"per_set":200,"targets":[["s:a",1],["s:c",1],["s:d",2]],
        "counts":{"1":2,"2":1},"logo_slots":{"1":1,"2":1},
        "over_capacity":{},"moves":[],"held":[],"excluded":[]})
}

#[test]
fn original_plan_report_counts_moves_holding_candidates_and_over_capacity() {
    let state = report_state();
    let mut plan = report_plan();
    plan["targets"] = json!([["s:a", 1], ["s:b", 1], ["s:c", 1], ["s:d", 2]]);
    assert_eq!(
        _native::plan_status::summarize(&plan, &state).unwrap()["pending"],
        false
    );
    plan["counts"]["1"] = json!(200);
    assert_eq!(
        _native::plan_status::summarize(&plan, &state).unwrap()["pending"],
        true
    );
    let mut plan = report_plan();
    plan["targets"] = json!([["s:a", 2], ["s:c", 1], ["s:d", 2], ["s:new", 2]]);
    plan["moves"] = json!([{"key":"s:a","label":"a","from_pack":1,"to_pack":2}]);
    plan["held"] = json!([{"key":"s:b","label":"b","from_pack":1}]);
    plan["excluded"] = json!(["s:b"]);
    let summary = _native::plan_status::summarize(&plan, &state).unwrap();
    assert_eq!(summary["packs"]["1"]["move_out"], json!(["s:a"]));
    assert_eq!(summary["packs"]["2"]["move_in"], json!(["s:a"]));
    assert_eq!(summary["packs"]["1"]["held_live"], json!(["s:b"]));
    assert_eq!(summary["packs"]["2"]["candidates"], json!(["s:new"]));
    let ids = BTreeMap::from([("s:a".into(), "5000000000000000001".into())]);
    let report = _native::plan_status::render(&summary, &ids, None).unwrap();
    assert!(report.contains("5000000000000000001  s:a"));
    assert!(report.contains("RETIRE"));
    let absent = _native::plan_status::render(&summary, &ids, Some(&BTreeSet::new())).unwrap();
    assert!(absent.contains("5000000000000000001  s:a  (not live now)"));
}

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

#[test]
fn original_four_hundred_key_plan_finds_all_398_removals_within_ten_seconds() {
    let keys = (0..400)
        .map(|index| format!("s:{index:04}"))
        .collect::<Vec<_>>();
    let items = keys
        .iter()
        .enumerate()
        .map(|(index, key)| (key.clone(), json!({"included":true,"pos":index})))
        .collect::<serde_json::Map<_, _>>();
    let state = json!({"sets":[
        {"fmt":"mixed","index":1,"name":"a","logo":true,"keys":&keys[..199]},
        {"fmt":"mixed","index":2,"name":"b","logo":true,"keys":&keys[199..398]}]});
    let targets = keys[..199]
        .iter()
        .map(|key| json!([key, 2]))
        .chain(keys[199..398].iter().map(|key| json!([key, 1])))
        .collect::<Vec<_>>();
    let ids = keys
        .iter()
        .map(|key| (key.clone(), json!(format!("cid-{key}"))))
        .collect::<serde_json::Map<_, _>>();
    let request = json!({"plan":{"targets":targets},"state":state,"items":items,"ids":ids});
    let started = std::time::Instant::now();
    let result = _native::plan_steps::compute(&request).unwrap();
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(result["removals"].as_array().unwrap().len(), 398);
    assert_eq!(result["counted"], 796);
}

fn snapshot(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    for entry in std::fs::read_dir(directory).unwrap() {
        assert!(result.len() < 8, "plan report fixture grew unexpectedly");
        let path = entry.unwrap().path();
        _native::tooling_environment::reject_links(&path, "linked report fixture").unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        assert!(metadata.is_file() && metadata.len() <= 65536);
        let bytes = std::fs::read(&path).unwrap();
        result.insert(path, bytes);
    }
    result
}

#[test]
fn original_plan_report_commands_keep_retired_ids_live_notes_and_all_durable_bytes() {
    use bot_api::{Reply, Server, TOKEN};
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = tooling_fixture::Root::new("plan status café");
    std::fs::create_dir(root.0.join("assets")).unwrap();
    std::fs::write(root.0.join("assets/panel.html"), b"isolated fixture").unwrap();
    std::fs::write(root.0.join("pyproject.toml"), b"# isolated fixture\n").unwrap();
    let executable = root.0.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let data = root.0.join("collection");
    std::fs::create_dir(&data).unwrap();
    let mut environment =
        _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    for key in [
        "GENERAL_BOT_TOKEN",
        "TELEGRAM_BOT_TOKEN",
        "COLLECTION_PACK_BASE",
    ] {
        environment.remove(key);
    }
    let mut environment =
        _native::tooling_environment::environment(&root.0, &environment, false).unwrap();
    let log = _native::logging::RunLog::open(&root.0, "plan-status-owner", vec![TOKEN.into()]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut server = Server::new(
        ["fixture1_by_YourEmojiBot", "fixture2_by_YourEmojiBot"]
            .map(|name| {
                Reply::new(
                    "getStickerSet",
                    &[("name", name)],
                    json!({"ok":true,"result":{"stickers":[{"custom_emoji_id":"111111111"}]}}),
                )
            })
            .into(),
    );
    environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
    environment.insert("GENERAL_BOT_TOKEN".into(), TOKEN.into());
    let run = |base: &str, live: bool| {
        let mut argv = vec![
            executable.as_os_str().to_owned(),
            "plan-status".into(),
            "--base".into(),
            base.into(),
        ];
        if live {
            argv.push("--live".into());
        }
        runtime
            .block_on(_native::tooling_process::capture(
                &root.0,
                &argv,
                &environment,
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log,
            ))
            .unwrap()
    };
    let before = snapshot(&data);
    let absent = run("mine", false);
    assert_eq!(absent.code, 0, "{}", absent.stderr);
    assert!(absent.stdout.contains("nothing pending"));
    assert_eq!(snapshot(&data), before);
    let db = rusqlite::Connection::open(data.join("catalog.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE publications(base TEXT, content_key TEXT, custom_emoji_id TEXT);
        INSERT INTO publications VALUES('mine','s:a','5000000000000000001');",
    )
    .unwrap();
    db.close().unwrap();
    let mut plan = report_plan();
    plan["moves"] = json!([{"key":"s:a","label":"a","from_pack":1,"to_pack":2}]);
    _native::atomic::write_json(&data.join("pack_plan.json"), &plan).unwrap();
    _native::atomic::write_json(&data.join("publish_mine.json"), &report_state()).unwrap();
    let before = snapshot(&data);
    let retired = run("mine", false);
    assert_eq!(retired.code, 3, "{}", retired.stderr);
    assert!(retired.stdout.contains("5000000000000000001"));
    assert!(retired.stdout.contains("RETIRE"));
    assert_eq!(snapshot(&data), before);
    let db = rusqlite::Connection::open(data.join("catalog.db")).unwrap();
    db.execute_batch("INSERT INTO publications VALUES('fixture','s:a','111111111'),('fixture','s:held','222222222');").unwrap();
    db.close().unwrap();
    let state = json!({"sets":[{"index":1,"name":"fixture1_by_YourEmojiBot","keys":["s:a","s:held"]},
        {"index":2,"name":"fixture2_by_YourEmojiBot","keys":["s:b"]}]});
    let plan = json!({"version":1,"per_set":200,"targets":[["s:a",2],["s:b",2],["s:new",1]],
        "counts":{"1":200,"2":2},"logo_slots":{"1":1,"2":0},"over_capacity":{},
        "moves":[{"key":"s:a","from_pack":1,"to_pack":2}],
        "held":[{"key":"s:held","from_pack":1}],"excluded":["s:held"]});
    _native::atomic::write_json(&data.join("pack_plan.json"), &plan).unwrap();
    _native::atomic::write_json(&data.join("publish_fixture.json"), &state).unwrap();
    let before = snapshot(&data);
    // Layout derived from the original render body, never from the target renderer.
    let expected = concat!(
        "pack  target+logo/cap  in  out  held-live  candidates\n",
        "   1     200+1/200     0    1          1           1  OVER CAPACITY\n",
        "   2       2+0/200     1    0          0           0\n",
        "\ncustom_emoji_ids a move or removal would RETIRE (a re-add mints a new id; the old one stops working):\n",
        "  111111111  s:a\n  222222222  s:held\n",
        "\nPENDING: the plan asks for work (see docs/design/plan-applier.md)\n"
    );
    for live in [false, true] {
        let result = run("fixture", live);
        assert_eq!(result.code, 3, "{}", result.stderr);
        let expected = if live {
            expected.replace("222222222  s:held\n", "222222222  s:held  (not live now)\n")
        } else {
            expected.into()
        };
        assert_eq!(result.stdout, expected);
        assert_eq!(snapshot(&data), before);
    }
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), 2);
    for (request, name) in requests
        .iter()
        .zip(["fixture1_by_YourEmojiBot", "fixture2_by_YourEmojiBot"])
    {
        assert_eq!(request.method, "getStickerSet");
        assert_eq!(request.form, BTreeMap::from([("name".into(), name.into())]));
    }
    runtime.shutdown_timeout(Duration::from_secs(1));
    log.finish(0);
    drop(log);
    root.clean().unwrap();
}

fn document() -> Value {
    let bytes = include_bytes!("../../tests/fixtures/backend/steps.json");
    let hash = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        hash,
        "63e38f56182a4e98aa3883e9ababd1cba53a7baac3b645f2125be87e35a000c3"
    );
    serde_json::from_slice(bytes).unwrap()
}

#[test]
fn original_moves_holding_reorder_and_capacity_match_all_six_source_outcomes() {
    let document = document();
    let cases =
        _native::source_fixture::cases(&document, "d173116:plan_apply.compute_steps", 6).unwrap();
    for case in cases {
        let result = _native::plan_steps::compute(case);
        match case.get("error") {
            Some(error) => assert_eq!(
                result.unwrap_err(),
                error.as_str().unwrap(),
                "{}",
                case["name"]
            ),
            None => assert_eq!(result.unwrap(), case["expected"], "{}", case["name"]),
        }
    }
}

#[test]
fn original_empty_duplicated_or_unannotated_corpora_cannot_pass() {
    let original = document();
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
            _native::source_fixture::cases(&broken, "d173116:plan_apply.compute_steps", 6).is_err(),
            "{mutation}"
        );
    }
    // Presence is independent of truthiness: explicit null is a real expected result.
    let null = json!({"source":"fixture","cases":[{"name":"null", "expected":null}]});
    assert!(_native::source_fixture::cases(&null, "fixture", 1).is_ok());
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
        assert!(_native::source_fixture::cases(&invalid, "fixture", 1).is_err());
    }
}
