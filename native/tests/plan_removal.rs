//! Original removal commands retain durable intent, distinct unknown state and counted budgets.
use _native::catalog::Catalog;
use serde_json::{Value, json};
#[path = "support/retained_cli.rs"]
mod retained_cli;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;
#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/plan_bounded.rs"]
mod plan_bounded;
#[path = "support/plan_family.rs"]
mod plan_family;
#[path = "support/plan_move.rs"]
mod plan_move;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server, TOKEN};
use tooling_fixture::Root;

#[test]
fn original_cross_pack_move_add_reorder_and_repeat_keep_content_and_retired_identity() {
    original_removal_budget_unknown_and_flood_wait_keep_journal_and_exit_contracts();
    plan_move::run();
    plan_bounded::run();
}

#[test]
fn original_unpublish_forgets_one_family_once_without_changing_other_items() {
    let root = Root::new("unpublish family café");
    let mut catalog = Catalog::open(&root.0.join("catalog.db"), "fixture UTC", &root.0).unwrap();
    for key in ["a", "b"] {
        assert!(catalog.insert(&json!({"content_key":key,"fmt":"static","file_path":root.0.join(format!("{key}.png"))}), "fixture UTC").unwrap());
        catalog
            .uploaded(
                &json!({"content_key":key,"custom_emoji_id":format!("cid-{key}"),
            "base":"cryptoemoji","set_name":"fixture1_by_YourEmojiBot"}),
                "fixture UTC",
            )
            .unwrap();
    }
    catalog
        .uploaded(
            &json!({"content_key":"a","custom_emoji_id":"cid-other",
        "base":"otherbase","set_name":"other1_by_x"}),
            "fixture UTC",
        )
        .unwrap();
    let rows = catalog.all(None).unwrap();
    for expected in [true, false] {
        assert_eq!(
            catalog
                .dispatch(json!({"operation":"unpublish","base":"cryptoemoji","content_key":"a"}))
                .unwrap(),
            expected
        );
    }
    for (base, key, expected) in [
        ("cryptoemoji", "a", false),
        ("cryptoemoji", "b", true),
        ("otherbase", "a", true),
    ] {
        assert_eq!(
            catalog
                .dispatch(json!({"operation":"published","base":base,"content_key":key}))
                .unwrap(),
            expected
        );
    }
    assert_eq!(catalog.all(None).unwrap(), rows);
    drop(catalog);
    let mut catalog = Catalog::open(&root.0.join("catalog.db"), "fixture UTC", &root.0).unwrap();
    assert_eq!(
        catalog
            .dispatch(json!({"operation":"custom_id","base":"otherbase","content_key":"a"}))
            .unwrap(),
        "cid-other"
    );
    assert_eq!(catalog.all(None).unwrap(), rows);
    drop(catalog);
    root.clean().unwrap();
}

#[test]
fn original_removal_restart_settles_applied_deletes_and_retains_unknown_without_resending() {
    use _native::{plan_remove, telegram::Telegram};
    let root = Root::new("removal settlement café");
    for variant in ["applied", "not applied", "unknown", "flood"] {
        let directory = root.0.join(variant);
        let (mut state, plan) = fixture(&directory, true);
        let data = directory.join("collection");
        let journal_path = data.join("plan_apply_fixture.json");
        let mut journal = read(&journal_path);
        let before_state = std::fs::read(data.join("publish_fixture.json")).unwrap();
        let before_journal = std::fs::read(&journal_path).unwrap();
        let mut cat = Catalog::open(&data.join("catalog.db"), "fixture UTC", &directory).unwrap();
        let live = json!([{"custom_emoji_id":"111111111","file_id":"fixture-file","file_unique_id":"fixture-unique"}]);
        let mut deletion = Reply::new(
            "deleteStickerFromSet",
            &[("sticker", "fixture-file")],
            if variant == "flood" {
                json!({"ok":false,"error_code":429,
                "description":"Too Many Requests: retry after 900","parameters":{"retry_after":900}})
            } else {
                json!({"ok":true,"result":true})
            },
        );
        deletion.observation = Some((
            journal_path.clone(),
            "/intent/cid".into(),
            json!("111111111"),
        ));
        let replies = match variant {
            "applied" => vec![listing(json!([]))],
            "not applied" => vec![
                listing(live.clone()),
                listing(live),
                deletion,
                listing(json!([])),
            ],
            "unknown" => vec![Reply::new(
                "getStickerSet",
                &[("name", "fixture1_by_YourEmojiBot")],
                json!({"ok":true,"result":[]}),
            )],
            "flood" => vec![listing(live.clone()), listing(live), deletion],
            _ => unreachable!(),
        };
        let mut server = Server::new(replies);
        let mut tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        tg.max_flood_wait = Some(300);
        let settled =
            plan_remove::settle(&tg, &mut cat, &mut state, &mut journal, &data, "fixture");
        if variant == "unknown" {
            assert!(settled.unwrap_err().contains("live state unknown"));
            assert_eq!(std::fs::read(&journal_path).unwrap(), before_journal);
            assert_eq!(
                std::fs::read(data.join("publish_fixture.json")).unwrap(),
                before_state
            );
        } else {
            settled.unwrap();
            if variant != "applied" {
                assert!(journal["intent"].is_null());
                assert_eq!(journal["retired"], json!([]));
                let removed = plan_remove::remove(
                    &tg,
                    &mut cat,
                    &mut state,
                    &mut journal,
                    &data,
                    "fixture",
                    ("a", 1),
                );
                if variant == "flood" {
                    assert!(removed.unwrap_err().starts_with("FLOOD_WAIT "));
                    assert!(journal["intent"].is_null());
                    assert_eq!(journal["retired"], json!([]));
                    assert_eq!(
                        std::fs::read(data.join("publish_fixture.json")).unwrap(),
                        before_state
                    );
                } else {
                    removed.unwrap();
                }
            }
        }
        if variant == "flood" {
            assert!(read(&journal_path)["intent"].is_null());
            assert_eq!(read(&journal_path)["retired"], json!([]));
        }
        let applied = matches!(variant, "applied" | "not applied");
        if applied {
            assert_eq!(state["sets"][0]["keys"], json!([]));
            assert_eq!(state["sets"][0]["live"], 0);
            assert_eq!(journal["retired"].as_array().unwrap().len(), 1);
            assert_eq!(journal["retired"][0]["cid"], "111111111");
            assert_eq!(journal["retired"][0]["key"], "a");
            let before = std::fs::read(&journal_path).unwrap();
            plan_remove::settle(&tg, &mut cat, &mut state, &mut journal, &data, "fixture").unwrap();
            assert_eq!(std::fs::read(&journal_path).unwrap(), before);
        }
        assert_eq!(
            cat.dispatch(json!({"operation":"published","base":"fixture","content_key":"a"}))
                .unwrap(),
            !applied
        );
        assert_eq!(read(&data.join("pack_plan.json")), plan);
        let requests = server.finish().unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.method == "deleteStickerFromSet")
                .count(),
            usize::from(matches!(variant, "not applied" | "flood"))
        );
        assert!(requests.iter().all(|request| matches!(
            request.method.as_str(),
            "getStickerSet" | "deleteStickerFromSet"
        )));
        drop(tg);
        drop(cat);
        let mut reopened =
            Catalog::open(&data.join("catalog.db"), "fixture UTC", &directory).unwrap();
        assert_eq!(
            reopened
                .dispatch(json!({"operation":"published","base":"fixture","content_key":"a"}))
                .unwrap(),
            !applied
        );
        drop(reopened);
    }
    root.clean().unwrap();
}

fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn fixture(directory: &Path, unknown: bool) -> (Value, Value) {
    std::fs::create_dir_all(directory.join("assets")).unwrap();
    std::fs::write(directory.join("assets/panel.html"), b"isolated fixture").unwrap();
    std::fs::write(directory.join("pyproject.toml"), b"# isolated fixture\n").unwrap();
    let data = directory.join("collection");
    let mut catalog = Catalog::open(
        &data.join("catalog.db"),
        "2026-10-07 00:00:00 UTC",
        directory,
    )
    .unwrap();
    assert!(
        catalog
            .insert(
                &json!({"content_key":"a","fmt":"static","file_path":directory.join("unused.png")}),
                "2026-10-07 00:00:00 UTC"
            )
            .unwrap()
    );
    catalog.uploaded(&json!({"content_key":"a","custom_emoji_id":"111111111","base":"fixture","set_name":"fixture1_by_YourEmojiBot"}), "2026-10-07 00:00:00 UTC").unwrap();
    drop(catalog);
    let state = json!({"base":"fixture","sets":[
        {"name":"fixture1_by_YourEmojiBot","title":"Fixture 1","fmt":"mixed","index":1,"keys":["a"],"live":1,"logo":false},
        {"name":"fixture2_by_YourEmojiBot","title":"Fixture 2","fmt":"mixed","index":2,"keys":[],"live":0,"logo":false}],
        "sent":[],"sent_full":[],"skipped":[]});
    let plan = json!({"version":1,"targets":[["a",2]],"known":["a"],"excluded":[],"moves":[],"held":[],"per_set":200});
    _native::atomic::write_json(&data.join("publish_fixture.json"), &state).unwrap();
    _native::atomic::write_json(&data.join("pack_plan.json"), &plan).unwrap();
    if unknown {
        _native::atomic::write_json(&data.join("plan_apply_fixture.json"), &json!({"version":1,"base":"fixture",
            "intent":{"op":"remove","key":"a","set":"fixture1_by_YourEmojiBot","cid":"111111111"},"retired":[]})).unwrap();
    }
    (state, plan)
}
fn listing(stickers: Value) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", "fixture1_by_YourEmojiBot")],
        json!({"ok":true,"result":{"stickers":stickers}}),
    )
}
fn command(
    root: &Path,
    executable: &Path,
    directory: &Path,
    environment: &BTreeMap<String, String>,
    runtime: &tokio::runtime::Runtime,
    log: &_native::logging::RunLog,
    usage: bool,
) -> _native::tooling_process::Completed {
    let child = directory.join(executable.file_name().unwrap());
    std::fs::rename(executable, &child).unwrap();
    let mut argv = vec![child.into_os_string(), "plan-apply".into()];
    if usage {
        argv.extend(["--max-changes".into(), "0".into()]);
    } else {
        argv.extend([
            "--base".into(),
            "fixture".into(),
            "--data-dir".into(),
            directory.join("collection").into_os_string(),
            "--apply".into(),
            "--user-id".into(),
            "111111111".into(),
            "--max-changes".into(),
            "1".into(),
        ]);
    }
    let result = runtime.block_on(_native::tooling_process::capture(
        root,
        &argv,
        environment,
        Duration::from_secs(5),
        Duration::from_secs(3),
        log,
    ));
    std::fs::rename(directory.join(executable.file_name().unwrap()), executable).unwrap();
    result.unwrap()
}

fn original_removal_budget_unknown_and_flood_wait_keep_journal_and_exit_contracts() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("plan removal café");
    let executable = root.0.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let environment =
        _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    let environment =
        _native::tooling_environment::environment(&root.0, &environment, false).unwrap();
    let log = _native::logging::RunLog::open(&root.0, "plan-removal-owner", vec![TOKEN.into()]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let dry = root.0.join("dry run");
    fixture(&dry, false);
    let data = dry.join("collection");
    let mut cat = Catalog::open(&data.join("catalog.db"), "fixture UTC", &dry).unwrap();
    assert!(
        cat.insert(
            &json!({"content_key":"b","fmt":"static","file_path":dry.join("b.png")}),
            "fixture UTC"
        )
        .unwrap()
    );
    drop(cat);
    _native::catalog_read::plan_rows(&data.join("catalog.db"), "fixture").unwrap();
    let snapshot = || {
        let mut entries = BTreeMap::new();
        for entry in std::fs::read_dir(&data).unwrap() {
            assert!(entries.len() < 32, "dry-run fixture grew unexpectedly");
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap();
            if matches!(name, "catalog.db-wal" | "catalog.db-shm") {
                continue;
            }
            _native::tooling_environment::reject_links(&path, "linked dry-run fixture").unwrap();
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            let bytes = if metadata.is_dir() {
                None
            } else {
                assert!(metadata.is_file() && metadata.len() <= 65536);
                Some(std::fs::read(&path).unwrap())
            };
            entries.insert(path, bytes);
        }
        entries
    };
    let mut no_network = Server::new(vec![]);
    let mut settings = environment.clone();
    for key in [
        "GENERAL_BOT_TOKEN",
        "TELEGRAM_BOT_TOKEN",
        "COLLECTION_PACK_BASE",
        "PACK_OWNER_USER_ID",
    ] {
        settings.remove(key);
    }
    settings.insert("NUMERA_TEST_API_BASE".into(), no_network.base.clone());
    let child = dry.join(executable.file_name().unwrap());
    std::fs::rename(&executable, &child).unwrap();
    let mut unchanged = None;
    for (targets, held, excluded, code, expected) in [
        (
            json!([["a", 2], ["b", 1]]),
            json!([]),
            json!([]),
            3,
            concat!(
                "pack 1\n  remove 1: a\n  add 1 (0 moved in, 1 new): b\n",
                "pack 2\n  add 1 (1 moved in, 0 new): a\n",
                "\ncustom_emoji_ids these moves RETIRE (the re-add mints a new id):\n",
                "  111111111  a\n\n3 counted change(s): 1 run(s) at 20 per run\n"
            ),
        ),
        (
            json!([]),
            json!([{"key":"a","from_pack":1}]),
            json!(["a"]),
            0,
            "pack 1\n  held, stays live until placed: a\n\nnothing pending: the live packs match the plan\n",
        ),
        (
            json!([]),
            json!([]),
            json!([]),
            0,
            "\nnothing pending: the live packs match the plan\n",
        ),
        (
            json!([["a", 1]]),
            json!([]),
            json!([]),
            0,
            "\nnothing pending: the live packs match the plan\n",
        ),
    ] {
        _native::atomic::write_json(&data.join("pack_plan.json"), &json!({"version":1,
            "targets":targets,"known":["a","b"],"excluded":excluded,"moves":[],"held":held,"per_set":200})).unwrap();
        let before = snapshot();
        let argv = [
            child.as_os_str().to_owned(),
            "plan-apply".into(),
            "--base".into(),
            "fixture".into(),
            "--data-dir".into(),
            data.as_os_str().to_owned(),
        ];
        let result = runtime
            .block_on(_native::tooling_process::capture(
                &root.0,
                &argv,
                &settings,
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, code, "{}", result.stderr);
        assert_eq!(result.stdout, expected);
        assert_eq!(snapshot(), before);
        let mut durable = before;
        durable.remove(&data.join("pack_plan.json"));
        if let Some(previous) = &unchanged {
            assert_eq!(&durable, previous);
        } else {
            unchanged = Some(durable);
        }
        assert!(!data.join("plan_apply_fixture.json").exists());
    }
    let original_state = read(&data.join("publish_fixture.json"));
    settings.insert("GENERAL_BOT_TOKEN".into(), TOKEN.into());
    settings.insert("BRAND_LOGO_BOTS".into(), String::new());
    for variant in ["capacity", "panel capacity", "missing", "per format", "gap"] {
        let mut state = original_state.clone();
        let mut plan = json!({"version":1,"targets":[["a",2],["b",2]],"known":["a","b"],
            "excluded":[],"moves":[],"held":[],"per_set":200});
        let fragment = match variant {
            "capacity" => {
                plan["per_set"] = json!(1);
                "over the cap of 1"
            }
            "panel capacity" => {
                plan["over_capacity"] = json!({"1":201});
                "overfills"
            }
            "missing" => {
                plan["targets"] = json!([["s:gone", 1]]);
                "s:gone"
            }
            "per format" => {
                state["sets"][0]["fmt"] = json!("static");
                "--mixed"
            }
            "gap" => {
                plan["targets"] = json!([["a", 9]]);
                "[9]"
            }
            _ => unreachable!(),
        };
        let state_path = data.join("publish_fixture.json");
        let plan_path = data.join("pack_plan.json");
        _native::atomic::write_json(&state_path, &state).unwrap();
        _native::atomic::write_json(&plan_path, &plan).unwrap();
        let state_before = std::fs::read(&state_path).unwrap();
        let plan_before = std::fs::read(&plan_path).unwrap();
        let rows = _native::catalog_read::plan_rows(&data.join("catalog.db"), "fixture").unwrap();
        let argv = [
            child.as_os_str().to_owned(),
            "plan-apply".into(),
            "--base".into(),
            "fixture".into(),
            "--data-dir".into(),
            data.as_os_str().to_owned(),
            "--apply".into(),
            "--user-id".into(),
            "111111111".into(),
        ];
        let result = runtime
            .block_on(_native::tooling_process::capture(
                &root.0,
                &argv,
                &settings,
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, 2, "{variant}: {}", result.stderr);
        assert!(
            result.stderr.contains(fragment),
            "{variant}: {}",
            result.stderr
        );
        assert_eq!(std::fs::read(state_path).unwrap(), state_before);
        assert_eq!(std::fs::read(plan_path).unwrap(), plan_before);
        assert_eq!(
            _native::catalog_read::plan_rows(&data.join("catalog.db"), "fixture").unwrap(),
            rows
        );
        assert!(!data.join("plan_apply_fixture.json").exists());
    }
    std::fs::rename(child, &executable).unwrap();
    assert!(no_network.finish().unwrap().is_empty());
    for variant in ["budget", "unknown", "flood"] {
        let directory = root.0.join(variant);
        let (state, plan) = fixture(&directory, variant == "unknown");
        let data = directory.join("collection");
        let state_path = data.join("publish_fixture.json");
        let state_bytes = std::fs::read(&state_path).unwrap();
        let plan_path = data.join("pack_plan.json");
        let plan_bytes = std::fs::read(&plan_path).unwrap();
        let journal_path = data.join("plan_apply_fixture.json");
        let journal_before = std::fs::read(&journal_path).ok();
        let get_me = Reply::new(
            "getMe",
            &[],
            json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
        );
        let mut deletion = Reply::new(
            "deleteStickerFromSet",
            &[("sticker", "fixture-file")],
            json!({"ok":true,"result":true}),
        );
        deletion.observation = Some((
            journal_path.clone(),
            "/intent/cid".into(),
            json!("111111111"),
        ));
        let replies = match variant {
            "budget" => vec![
                get_me,
                listing(
                    json!([{"custom_emoji_id":"111111111","file_id":"fixture-file","file_unique_id":"fixture-unique"}]),
                ),
                deletion,
                listing(json!([])),
            ],
            "unknown" => vec![
                get_me,
                Reply::new(
                    "getStickerSet",
                    &[("name", "fixture1_by_YourEmojiBot")],
                    json!({"ok":true,"result":[]}),
                ),
            ],
            "flood" => vec![Reply::new(
                "getMe",
                &[],
                json!({"ok":false,"error_code":429,"description":"Too Many Requests: retry after 301","parameters":{"retry_after":301}}),
            )],
            _ => unreachable!(),
        };
        let mut server = Server::new(replies);
        let mut settings = environment.clone();
        for key in [
            "GENERAL_BOT_TOKEN",
            "TELEGRAM_BOT_TOKEN",
            "COLLECTION_PACK_BASE",
            "BRAND_LOGO_PATH",
            "BRAND_LOGO_BOTS",
            "PACK_OWNER_USER_ID",
        ] {
            settings.remove(key);
        }
        settings.insert("GENERAL_BOT_TOKEN".into(), TOKEN.into());
        settings.insert("BRAND_LOGO_BOTS".into(), String::new());
        settings.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        let result = command(
            &root.0,
            &executable,
            &directory,
            &settings,
            &runtime,
            &log,
            false,
        );
        assert_eq!(
            result.code,
            if variant == "unknown" { 4 } else { 3 },
            "{variant}: {}",
            result.stderr
        );
        assert_eq!(std::fs::read(&plan_path).unwrap(), plan_bytes);
        assert_eq!(read(&plan_path), plan);
        match variant {
            "budget" => {
                let saved = read(&state_path);
                assert_eq!(saved["sets"][0]["keys"], json!([]));
                assert_eq!(saved["sets"][0]["live"], 0);
                assert_eq!(saved["sets"][1], state["sets"][1]);
                let journal = read(&journal_path);
                assert!(journal["intent"].is_null());
                let retired = journal["retired"].as_array().unwrap();
                assert_eq!(retired.len(), 1);
                assert_eq!(retired[0]["cid"], "111111111");
                assert_eq!(retired[0]["key"], "a");
                let mut catalog = Catalog::open(
                    &data.join("catalog.db"),
                    "2026-10-07 00:00:00 UTC",
                    &directory,
                )
                .unwrap();
                assert_eq!(
                    catalog
                        .dispatch(
                            json!({"operation":"published","base":"fixture","content_key":"a"})
                        )
                        .unwrap(),
                    false
                );
                drop(catalog);
                assert!(result.stdout.contains("cap reached"));
                assert!(
                    !data.join("tmp/native-codec").exists(),
                    "budget1 must not start media/add work"
                );
            }
            "unknown" => {
                assert_eq!(std::fs::read(&state_path).unwrap(), state_bytes);
                assert_eq!(
                    std::fs::read(&journal_path).unwrap(),
                    journal_before.unwrap()
                );
                assert_eq!(read(&journal_path)["intent"]["cid"], "111111111");
                assert!(result.stderr.contains("live state unknown"));
            }
            "flood" => {
                assert!(result.stderr.contains("FLOOD_WAIT"));
                assert_eq!(std::fs::read(&state_path).unwrap(), state_bytes);
                assert!(!journal_path.exists());
                let refused = command(
                    &root.0,
                    &executable,
                    &directory,
                    &settings,
                    &runtime,
                    &log,
                    true,
                );
                assert_eq!(refused.code, 2);
                assert!(refused.stderr.contains("--max-changes must be 1..20"));
            }
            _ => unreachable!(),
        }
        let requests = server.finish().unwrap();
        let methods = requests
            .iter()
            .map(|request| request.method.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            methods,
            match variant {
                "budget" => vec![
                    "getMe",
                    "getStickerSet",
                    "deleteStickerFromSet",
                    "getStickerSet"
                ],
                "unknown" => vec!["getMe", "getStickerSet"],
                "flood" => vec!["getMe"],
                _ => unreachable!(),
            }
        );
        for request in &requests {
            match request.method.as_str() {
                "getMe" => assert!(request.form.is_empty()),
                "getStickerSet" => assert_eq!(
                    request.form,
                    BTreeMap::from([("name".into(), "fixture1_by_YourEmojiBot".into())])
                ),
                "deleteStickerFromSet" => assert_eq!(
                    request.form,
                    BTreeMap::from([("sticker".into(), "fixture-file".into())])
                ),
                _ => unreachable!(),
            }
        }
    }
    runtime.shutdown_timeout(Duration::from_secs(1));
    drop(log);
    root.clean().unwrap();
}
