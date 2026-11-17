use crate::{
    identity::{art, insert},
    support::{
        http::{Reply, Server},
        *,
    },
};
use numera_runtime as native;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
const FIRST: &str = "fixture1_by_YourEmojiBot";
const SECOND: &str = "fixture2_by_YourEmojiBot";
fn state() -> Value {
    json!({"base":"fixture","sets":[{"name":FIRST,"title":"Fixture 1","fmt":"mixed","index":1,"keys":["a"],"live":1,"logo":false},{"name":SECOND,"title":"Fixture 2","fmt":"mixed","index":2,"keys":[],"live":0,"logo":false}],"sent":[],"sent_full":[],"skipped":[]})
}
fn catalog(f: &Fixture, keys: &[&str]) {
    let mut cat = f.catalog();
    for key in keys {
        cat.insert(&json!({"content_key":key,"fmt":"static","file_path":f.root.join(format!("{key}.png"))}),UTC).unwrap();
    }
    cat.uploaded(
        &json!({"content_key":"a","custom_emoji_id":"111111111","base":"fixture","set_name":FIRST}),
        UTC,
    )
    .unwrap();
}
// tests.test_native_plan_cli.NativePlanCommand.test_dry_run_reads_existing_data_without_creating_or_rewriting_files
#[test]
fn plan_apply_dry_run_exact_preview_variants_and_no_durable_writes() {
    let f = Fixture::new();
    catalog(&f, &["a", "b"]);
    let state = state();
    let plan = json!({"version":1,"targets":[["a",2],["b",1]],"moves":[],"held":[],"known":["a","b"],"excluded":[],"per_set":200});
    document(&f.data().join("publish_fixture.json"), &state);
    let path = f.data().join("pack_plan.json");
    document(&path, &plan);
    let (items, ids) =
        native::catalog_read::plan_rows(&f.data().join("catalog.db"), "fixture").unwrap();
    let before = snapshot(&f.data());
    let mut held = plan.clone();
    held["targets"] = json!([]);
    held["held"] = json!([{"key":"a","from_pack":1}]);
    held["excluded"] = json!(["a"]);
    let mut empty = plan.clone();
    empty["targets"] = json!([]);
    for variant in [&plan, &held, &empty] {
        document(&path, variant);
        let expected = native::plan_steps::compute(
            &json!({"plan":variant,"state":state,"items":items,"ids":ids}),
        )
        .unwrap();
        let r = f.run(&["plan-apply", "--base", "fixture"], &[]);
        r.code(if expected["pending"] == true { 3 } else { 0 });
        assert_eq!(
            r.out,
            format!(
                "{}\n",
                native::plan_steps::render(&expected, &ids, 20).unwrap()
            )
        );
    }
    write(&path, &before[std::path::Path::new("pack_plan.json")]);
    assert_eq!(snapshot(&f.data()), before);
}
// tests.test_native_plan_status_cli.NativePlanStatusCLI.test_retired_ids_and_capacity_report_match_source_without_writes
#[test]
fn native_plan_status_retired_ids_capacity_and_live_report_are_read_only() {
    let f = Fixture::new();
    let state = json!({"sets":[{"index":1,"name":FIRST,"keys":["s:a","s:held"]},{"index":2,"name":SECOND,"keys":["s:b"]}]});
    let plan = json!({"version":1,"per_set":200,"targets":[["s:a",2],["s:b",2],["s:new",1]],"counts":{"1":200,"2":2},"logo_slots":{"1":1,"2":0},"moves":[{"key":"s:a","from_pack":1,"to_pack":2}],"held":[{"key":"s:held","from_pack":1}],"excluded":["s:held"]});
    document(&f.data().join("pack_plan.json"), &plan);
    document(&f.data().join("publish_fixture.json"), &state);
    let db = Connection::open(f.data().join("catalog.db")).unwrap();
    db.execute_batch("CREATE TABLE publications(base TEXT,content_key TEXT,custom_emoji_id TEXT);INSERT INTO publications VALUES('fixture','s:a','111111111'),('fixture','s:held','222222222');").unwrap();
    drop(db);
    let ids = BTreeMap::from([
        ("s:a".into(), "111111111".into()),
        ("s:held".into(), "222222222".into()),
    ]);
    let before = snapshot(&f.data());
    let summary = native::plan_status::summarize(&plan, &state).unwrap();
    let r = f.run(&["plan-status", "--base", "fixture"], &[]);
    r.code(3);
    assert_eq!(
        r.out,
        format!(
            "{}\n",
            native::plan_status::render(&summary, &ids, None).unwrap()
        )
    );
    assert_eq!(snapshot(&f.data()), before);
    let bot = Server::new(|r| {
        assert_eq!(r.method(), "getStickerSet");
        Reply::ok(json!({"stickers":[{"custom_emoji_id":"111111111"}]}))
    });
    let r = f.run(
        &["plan-status", "--base", "fixture", "--live"],
        &bot.environment(),
    );
    r.code(3);
    assert_eq!(
        r.out,
        format!(
            "{}\n",
            native::plan_status::render(
                &summary,
                &ids,
                Some(&BTreeSet::from(["111111111".into()]))
            )
            .unwrap()
        )
    );
    assert_eq!(bot.methods(), ["getStickerSet", "getStickerSet"]);
    assert_eq!(snapshot(&f.data()), before);
}
fn removal(mode: &'static str) {
    let f = Fixture::new();
    catalog(&f, &["a"]);
    let state = state();
    document(&f.data().join("publish_fixture.json"), &state);
    document(
        &f.data().join("pack_plan.json"),
        &json!({"version":1,"targets":[["a",2]],"known":["a"],"excluded":[],"moves":[],"held":[],"per_set":200}),
    );
    let journal = f.data().join("plan_apply_fixture.json");
    if mode == "unknown" {
        document(
            &journal,
            &json!({"version":1,"base":"fixture","intent":{"op":"remove","key":"a","set":FIRST,"cid":"111111111"},"retired":[]}),
        );
    }
    let live = Arc::new(Mutex::new(vec![
        json!({"custom_emoji_id":"111111111","file_id":"fixture-file","file_unique_id":"fixture-unique"}),
    ]));
    let shared = live.clone();
    let bot = Server::new(move |r| {
        if mode == "flood" {
            return Reply::json(
                json!({"ok":false,"error_code":429,"description":"Too Many Requests: retry after 301","parameters":{"retry_after":301}}),
            );
        }
        match r.method() {
            "getMe" => Reply::ok(json!({"username":"YourEmojiBot"})),
            "getStickerSet" => Reply::ok(if mode == "unknown" {
                json!([])
            } else {
                json!({"stickers":shared.lock().unwrap().clone()})
            }),
            "deleteStickerFromSet" => {
                assert_eq!(r.form()["sticker"], "fixture-file");
                shared.lock().unwrap().clear();
                Reply::ok(json!(true))
            }
            _ => panic!("unexpected {}", r.method()),
        }
    });
    let r = f.run(
        &[
            "plan-apply",
            "--base",
            "fixture",
            "--apply",
            "--user-id",
            "111111111",
            "--max-changes",
            "1",
        ],
        &bot.environment(),
    );
    if mode == "flood" {
        r.code(3);
        assert!(r.err.contains("FLOOD_WAIT"));
        assert_eq!(bot.methods(), ["getMe"]);
        f.run(&["plan-apply", "--max-changes", "0"], &bot.environment())
            .code(2);
        assert_eq!(bot.methods(), ["getMe"]);
    } else if mode == "unknown" {
        r.code(4);
        assert!(!bot.methods().iter().any(|m| m == "deleteStickerFromSet"));
        assert_eq!(read_json(&journal)["intent"]["cid"], "111111111");
        assert_eq!(read_json(&f.data().join("publish_fixture.json")), state);
    } else {
        r.code(3);
        let methods = bot.methods();
        assert_eq!(
            methods
                .iter()
                .filter(|m| *m == "deleteStickerFromSet")
                .count(),
            1
        );
        let state = read_json(&f.data().join("publish_fixture.json"));
        assert_eq!(state["sets"][0]["keys"], json!([]));
        assert_eq!(state["sets"][0]["live"], 0);
        let journal = read_json(&journal);
        assert!(journal["intent"].is_null());
        assert_eq!(journal["retired"][0]["cid"], "111111111");
        assert_eq!(
            f.catalog()
                .dispatch(json!({"operation":"published","base":"fixture","content_key":"a"}))
                .unwrap(),
            false
        );
        assert!(
            !methods
                .iter()
                .any(|m| ["createNewStickerSet", "addStickerToSet"].contains(&m.as_str()))
        );
    }
}
// tests.test_native_remove_cli.NativePlanRemoval.test_budgeted_removal_retires_id_without_starting_an_add
#[test]
fn budgeted_removal_retires_id_without_add() {
    removal("normal");
}
// tests.test_native_remove_cli.NativePlanRemoval.test_unknown_live_state_stops_with_runtime_exit_and_retains_intent
#[test]
fn unknown_live_state_retains_intent_and_stops() {
    removal("unknown");
}
// tests.test_native_remove_cli.NativePlanRemoval.test_flood_wait_keeps_pending_exit_and_planning_refusal_keeps_usage_exit
#[test]
fn flood_wait_pending_exit_and_invalid_budget_usage_exit() {
    removal("flood");
}
// tests.test_native_remove_cli.NativePlanRemoval.test_cross_pack_move_add_reorder_and_repeat_are_one_native_command
#[test]
fn cross_pack_move_add_reorder_and_repeat_are_one_native_command() {
    let f = Fixture::new();
    let names = [FIRST, SECOND, "fixture3_by_YourEmojiBot"];
    let old_ids = ["111111111", "222222222", "333333333"];
    let mut keys = Vec::new();
    let mut paths = Vec::new();
    {
        let mut cat = f.catalog();
        for (i, name) in [
            "numera-emoji-mapper-logo.png",
            "numera-emoji-mapper-logo-circle.png",
            "numera-coin-emoji-mapper-logo.png",
        ]
        .into_iter()
        .enumerate()
        {
            let path = art(&f, name, &format!("source-{i}.png"));
            let key = insert(&mut cat, &path, json!({"emojis":["😀"]}));
            cat.record_seen(&format!("unique-{i}"), &key).unwrap();
            cat.uploaded(&json!({"content_key":key,"custom_emoji_id":old_ids[i],"base":"fixture","set_name":names[i]}),UTC).unwrap();
            keys.push(key);
            paths.push(path);
        }
        assert_eq!(keys.iter().collect::<BTreeSet<_>>().len(), 3);
        cat.order(&keys).unwrap();
    }
    let sets=names.iter().enumerate().map(|(i,name)|json!({"name":name,"title":format!("Fixture {}",i+1),"fmt":"mixed","index":i+1,"keys":[keys[i]],"live":1,"logo":false})).collect::<Vec<_>>();
    let sentinel = sets[2].clone();
    document(
        &f.data().join("publish_fixture.json"),
        &json!({"base":"fixture","sets":sets,"sent":names,"sent_full":[],"skipped":[]}),
    );
    document(
        &f.data().join("pack_plan.json"),
        &json!({"version":1,"targets":[[keys[0],2],[keys[1],2],[keys[2],3]],"known":keys,"excluded":[],"moves":[],"held":[],"per_set":200}),
    );
    let live=Arc::new(Mutex::new(names.iter().enumerate().map(|(i,n)|(n.to_string(),vec![json!({"custom_emoji_id":old_ids[i],"file_id":format!("file-{i}"),"file_unique_id":format!("unique-{i}")})])).collect::<BTreeMap<_,_>>()));
    let shared = live.clone();
    let payload = std::fs::read(&paths[0]).unwrap();
    let journal = f.data().join("plan_apply_fixture.json");
    let observed_journal = journal.clone();
    let bot = Server::new(move |r| {
        if r.verb == "GET" {
            return Reply::Bytes(200, payload.clone());
        }
        let mut live = shared.lock().unwrap();
        match r.method() {
            "getMe" => Reply::ok(json!({"username":"YourEmojiBot"})),
            "getStickerSet" => {
                let name = &r.form()["name"];
                Reply::ok(json!({"name":name,"stickers":live[name]}))
            }
            "addStickerToSet" => {
                assert_eq!(r.field("name"), SECOND.as_bytes());
                assert_eq!(r.field("file0"), payload);
                live.get_mut(SECOND).unwrap().push(json!({"custom_emoji_id":"444444444","file_id":"moved-file","file_unique_id":"moved-unique"}));
                Reply::ok(json!(true))
            }
            "deleteStickerFromSet" => {
                assert_eq!(r.form()["sticker"], "file-0");
                assert!(!read_json(&observed_journal)["intent"].is_null());
                live.get_mut(FIRST).unwrap().clear();
                Reply::ok(json!(true))
            }
            "getFile" => {
                assert_eq!(r.form()["file_id"], "moved-file");
                Reply::ok(json!({"file_path":"moved.png"}))
            }
            "setStickerPositionInSet" => {
                assert_eq!(r.form()["sticker"], "moved-file");
                assert_eq!(r.form()["position"], "0");
                let set = live.get_mut(SECOND).unwrap();
                let item = set.pop().unwrap();
                set.insert(0, item);
                Reply::ok(json!(true))
            }
            _ => panic!("unexpected {}", r.method()),
        }
    });
    for _ in 0..2 {
        f.run(
            &[
                "plan-apply",
                "--base",
                "fixture",
                "--apply",
                "--user-id",
                "111111111",
                "--max-changes",
                "2",
            ],
            &bot.environment(),
        )
        .code(0);
        assert_eq!(
            bot.methods()
                .into_iter()
                .filter(|m| [
                    "deleteStickerFromSet",
                    "addStickerToSet",
                    "setStickerPositionInSet"
                ]
                .contains(&m.as_str()))
                .collect::<Vec<_>>(),
            [
                "deleteStickerFromSet",
                "addStickerToSet",
                "setStickerPositionInSet"
            ]
        );
        let state = read_json(&f.data().join("publish_fixture.json"));
        assert_eq!(state["sets"][0]["keys"], json!([]));
        assert_eq!(state["sets"][1]["keys"], json!(&keys[..2]));
        assert_eq!(state["sets"][2], sentinel);
        let journal = read_json(&journal);
        assert!(journal["intent"].is_null());
        assert_eq!(
            journal["retired"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["cid"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [old_ids[0]]
        );
        let live = live.lock().unwrap();
        assert_eq!(
            live[SECOND]
                .iter()
                .map(|s| s["custom_emoji_id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["444444444", old_ids[1]]
        );
        assert_eq!(live[names[2]][0]["custom_emoji_id"], old_ids[2]);
        drop(live);
        let mut cat = f.catalog();
        assert_eq!(
            cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":keys[0]}))
                .unwrap(),
            "444444444"
        );
        assert_eq!(
            cat.dispatch(json!({"operation":"published_sets"})).unwrap()[&keys[0]],
            SECOND
        );
        assert_eq!(
            cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":keys[2]}))
                .unwrap(),
            old_ids[2]
        );
        let scratch = f.data().join("tmp/native-codec");
        if scratch.exists() {
            assert_eq!(std::fs::read_dir(scratch).unwrap().count(), 0);
        }
    }
}
// tests.test_native_reorder_cli.NativeReorderCommand.test_move_keeps_custom_ids_and_logo_first
#[test]
fn reorder_keeps_logo_first_custom_ids_and_repeats_without_mutation() {
    let f = Fixture::new();
    catalog(&f, &["a", "b"]);
    f.catalog().uploaded(&json!({"content_key":"b","custom_emoji_id":"222222222","base":"fixture","set_name":FIRST}),UTC).unwrap();
    let mut state = state();
    state["sets"].as_array_mut().unwrap().truncate(1);
    state["sets"][0]["keys"] = json!(["b", "a"]);
    state["sets"][0]["live"] = json!(3);
    state["sets"][0]["logo"] = json!(true);
    document(&f.data().join("publish_fixture.json"), &state);
    let live = Arc::new(Mutex::new(vec![
        json!({"file_id":"logo","custom_emoji_id":"333333333"}),
        json!({"file_id":"file-b","custom_emoji_id":"222222222"}),
        json!({"file_id":"file-a","custom_emoji_id":"111111111"}),
    ]));
    let shared = live.clone();
    let bot = Server::new(move |r| {
        let mut live = shared.lock().unwrap();
        match r.method() {
            "getStickerSet" => Reply::ok(json!({"stickers":*live})),
            "setStickerPositionInSet" => {
                let form = r.form();
                let i = live
                    .iter()
                    .position(|s| s["file_id"] == form["sticker"])
                    .unwrap();
                let sticker = live.remove(i);
                live.insert(form["position"].parse().unwrap(), sticker);
                Reply::ok(json!(true))
            }
            _ => panic!("unexpected {}", r.method()),
        }
    });
    for _ in 0..2 {
        let r = f.run(
            &["sync-order", "--base", "fixture", "--apply"],
            &bot.environment(),
        );
        r.code(0);
        assert_eq!(r.out, "");
    }
    let live = live.lock().unwrap();
    assert_eq!(
        live.iter()
            .map(|s| s["file_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["logo", "file-a", "file-b"]
    );
    assert_eq!(
        live.iter()
            .map(|s| s["custom_emoji_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["333333333", "111111111", "222222222"]
    );
    assert_eq!(
        bot.methods()
            .iter()
            .filter(|m| *m == "setStickerPositionInSet")
            .count(),
        1
    );
    let state = read_json(&f.data().join("publish_fixture.json"));
    assert_eq!(state["sets"][0]["keys"], json!(["a", "b"]));
    assert_eq!(state["sets"][0]["logo"], true);
}
