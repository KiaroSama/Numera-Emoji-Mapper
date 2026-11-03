//! Original ordering assertions through planning, real catalog/transport and the actual command.
use _native::{catalog::Catalog, sync_order, telegram::Telegram};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server, TOKEN};
use tooling_fixture::Root;

fn live(ids: &[&str]) -> Vec<Value> {
    ids.iter()
        .map(|id| json!({"custom_emoji_id":id,"file_id":format!("f-{id}")}))
        .collect()
}
fn listing(name: &str, ids: &[&str]) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", name)],
        json!({"ok":true,"result":{"stickers":live(ids)}}),
    )
}
fn moved(id: &str, position: &str, result: bool) -> Reply {
    Reply::new(
        "setStickerPositionInSet",
        &[("sticker", &format!("f-{id}")), ("position", position)],
        if result {
            json!({"ok":true,"result":true})
        } else {
            json!({"ok":false,"description":"Bad Request: synthetic reorder refusal"})
        },
    )
}
fn catalog(data: &Path, project: &Path, ids: &[Option<&str>]) -> Catalog {
    let mut cat = Catalog::open(&data.join("catalog.db"), "fixture", project).unwrap();
    for (index, id) in ids.iter().enumerate() {
        let name = id.map_or_else(|| format!("unpublished-{index}"), str::to_owned);
        let path = data.join(format!("{name}.png"));
        std::fs::write(&path, format!("unchanged source {name}")).unwrap();
        let key = format!("s:{name}");
        cat.insert(
            &json!({"content_key":key,"fmt":"static","file_path":path}),
            "fixture",
        )
        .unwrap();
        if let Some(id) = id {
            cat.uploaded(
                &json!({"content_key":key,"custom_emoji_id":id,"base":"pk","set_name":"fixture"}),
                "fixture",
            )
            .unwrap();
        }
    }
    cat
}

#[test]
fn original_move_plans_pin_the_logo_sort_reversals_and_become_idempotent() {
    let reversed = (0..30).rev().map(|n| format!("e{n}")).collect::<Vec<_>>();
    let wanted = (0..30).map(|n| format!("e{n}")).collect::<Vec<_>>();
    for (ids, desired, pinned) in [
        (vec!["logo", "a", "b", "c"], vec!["a", "b", "c"], 1),
        (vec!["logo", "c", "a", "b"], vec!["a", "b", "c"], 1),
        (vec!["logo", "c", "b", "a"], vec!["a", "b", "c"], 1),
        (vec!["logo", "b", "c", "a"], vec!["a", "b", "c"], 1),
        (
            std::iter::once("logo")
                .chain(reversed.iter().map(String::as_str))
                .collect(),
            wanted.iter().map(String::as_str).collect(),
            1,
        ),
        (vec!["b", "a"], vec!["a", "b"], 0),
    ] {
        let original = live(&ids);
        let desired = desired.into_iter().map(str::to_owned).collect::<Vec<_>>();
        let moves = sync_order::moves(&original, &desired, pinned).unwrap();
        let mut reordered = original.clone();
        for row in &moves {
            let target = usize::try_from(row[0].as_u64().unwrap()).unwrap();
            let cid = row[1].as_str().unwrap();
            assert!(target >= pinned && cid != "logo");
            assert_eq!(row[2], format!("f-{cid}"));
            let index = reordered
                .iter()
                .position(|item| item["custom_emoji_id"] == cid)
                .unwrap();
            let sticker = reordered.remove(index);
            reordered.insert(target, sticker);
        }
        let expected = if pinned == 1 {
            std::iter::once("logo")
                .chain(desired.iter().map(String::as_str))
                .collect::<Vec<_>>()
        } else {
            desired.iter().map(String::as_str).collect()
        };
        assert_eq!(reordered, live(&expected));
        assert!(moves.len() <= desired.len());
        assert!(
            sync_order::moves(&reordered, &desired, pinned)
                .unwrap()
                .is_empty()
        );
        assert_eq!(original, live(&ids));
        if ids == ["logo", "a", "b", "c"] {
            assert!(moves.is_empty());
        }
    }
    assert!(sync_order::moves(&live(&["logo", "a"]), &["a".into(), "b".into()], 1).is_err());
    assert!(sync_order::moves(&live(&["logo", "b"]), &["a".into()], 1).is_err());
}

#[test]
fn original_record_and_panel_order_follow_only_successful_live_reorders_without_changing_ids() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("sync policy café");
    for (label, ids, initial, keys, apply, error, expected, calls) in [
        (
            "panel",
            vec![Some("b"), None, Some("x"), Some("a")],
            vec!["logo", "a", "b"],
            vec!["s:a", "s:b"],
            true,
            false,
            vec!["s:b", "s:a"],
            vec![("b", "1", true)],
        ),
        (
            "reorder",
            vec![Some("a"), Some("b"), Some("c")],
            vec!["logo", "c", "a", "b"],
            vec!["s:c", "s:a", "s:b"],
            true,
            false,
            vec!["s:a", "s:b", "s:c"],
            vec![("a", "1", true), ("b", "2", true)],
        ),
        (
            "stale",
            vec![Some("a"), Some("b")],
            vec!["logo", "a", "b"],
            vec!["s:b", "s:a"],
            true,
            false,
            vec!["s:a", "s:b"],
            vec![],
        ),
        (
            "report",
            vec![Some("a"), Some("b")],
            vec!["logo", "b", "a"],
            vec!["s:b", "s:a"],
            false,
            false,
            vec!["s:b", "s:a"],
            vec![],
        ),
        (
            "foreign",
            vec![Some("a"), Some("b")],
            vec!["logo", "foreign", "a"],
            vec!["s:b", "s:a"],
            true,
            true,
            vec!["s:b", "s:a"],
            vec![],
        ),
        (
            "duplicate",
            vec![Some("a"), Some("b")],
            vec!["logo", "a", "a"],
            vec!["s:b", "s:a"],
            true,
            true,
            vec!["s:b", "s:a"],
            vec![],
        ),
        (
            "missing-logo",
            vec![Some("a")],
            vec![],
            vec!["s:a"],
            true,
            true,
            vec!["s:a"],
            vec![],
        ),
        (
            "refused",
            vec![Some("a"), Some("b")],
            vec!["logo", "b", "a"],
            vec!["s:b", "s:a"],
            true,
            true,
            vec!["s:b", "s:a"],
            vec![("a", "1", false)],
        ),
    ] {
        let data = root.0.join(label);
        let cat = catalog(&data, &root.0, &ids);
        let before = cat.all(None).unwrap();
        let bytes = before
            .iter()
            .map(|item| {
                let path = PathBuf::from(item["file_path"].as_str().unwrap());
                let bytes = std::fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect::<Vec<_>>();
        let name = "pks1_by_YourEmojiBot";
        let mut script = vec![listing(name, &initial)];
        script.extend(
            calls
                .iter()
                .map(|(id, position, ok)| moved(id, position, *ok)),
        );
        let mut server = Server::new(script);
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let mut record =
            json!({"name":name,"logo":true,"keys":keys,"index":1,"live":initial.len()});
        let result = sync_order::sync(&tg, &cat, &mut record, apply);
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), calls.len() + 1, "{label}");
        assert_eq!(result.is_err(), error, "{label}: {result:?}");
        if let Ok(count) = result {
            assert_eq!(count, if label == "report" { 1 } else { calls.len() });
        }
        assert_eq!(record["keys"], json!(expected), "{label}");
        assert_eq!(requests[0].method, "getStickerSet");
        for (request, (id, position, _)) in requests[1..].iter().zip(calls) {
            assert_eq!(request.method, "setStickerPositionInSet");
            assert_eq!(
                request.form,
                BTreeMap::from([
                    ("sticker".into(), format!("f-{id}")),
                    ("position".into(), position.into())
                ])
            );
        }
        assert_eq!(cat.all(None).unwrap(), before);
        drop(tg);
        drop(cat);
        let reopened = Catalog::open(&data.join("catalog.db"), "fixture", &root.0).unwrap();
        assert_eq!(reopened.all(None).unwrap(), before);
        drop(reopened);
        for (path, original) in bytes {
            assert_eq!(std::fs::read(path).unwrap(), original);
        }
    }
    root.clean().unwrap();
}

fn native_reorder(
    root: &Path,
    executable: &Path,
    environment: &BTreeMap<String, String>,
    runtime: &tokio::runtime::Runtime,
    log: &_native::logging::RunLog,
) {
    let data = root.join("native reorder");
    let cat = catalog(&data, root, &[Some("111111111"), Some("222222222")]);
    let before = cat.all(None).unwrap();
    drop(cat);
    let name = "pks1_by_YourEmojiBot";
    let state = data.join("publish_pk.json");
    _native::atomic::write_json(
        &state,
        &json!({"base":"pk","sets":[{
        "name":name,"title":"Fixture 1","fmt":"mixed","index":1,
        "keys":["s:222222222","s:111111111"],"live":3,"logo":true}],
        "sent":[],"sent_full":[],"skipped":[]}),
    )
    .unwrap();
    let mut server = Server::new(vec![
        listing(name, &["333333333", "222222222", "111111111"]),
        moved("111111111", "1", true),
        listing(name, &["333333333", "111111111", "222222222"]),
    ]);
    let mut environment = environment.clone();
    environment.insert("GENERAL_BOT_TOKEN".into(), TOKEN.into());
    environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
    let argv = [
        executable.as_os_str().to_owned(),
        "sync-order".into(),
        "--base".into(),
        "pk".into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
        "--apply".into(),
    ];
    for _ in 0..2 {
        let result = runtime
            .block_on(_native::tooling_process::capture(
                root,
                &argv,
                &environment,
                Duration::from_secs(5),
                Duration::from_secs(3),
                log,
            ))
            .unwrap();
        assert_eq!(result.code, 0, "{}", result.stderr);
        assert_eq!(
            result.stdout, "",
            "apply reports through logging, not JSON stdout"
        );
    }
    let requests = server.finish().unwrap();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.method.as_str())
            .collect::<Vec<_>>(),
        ["getStickerSet", "setStickerPositionInSet", "getStickerSet"]
    );
    assert_eq!(
        requests[1].form,
        BTreeMap::from([
            ("sticker".into(), "f-111111111".into()),
            ("position".into(), "1".into()),
        ])
    );
    let saved: Value = serde_json::from_slice(&std::fs::read(state).unwrap()).unwrap();
    assert_eq!(
        saved["sets"][0]["keys"],
        json!(["s:111111111", "s:222222222"])
    );
    assert_eq!(saved["sets"][0]["logo"], true);
    let cat = Catalog::open(&data.join("catalog.db"), "fixture", root).unwrap();
    assert_eq!(
        cat.all(None).unwrap(),
        before,
        "reorder must not mint or reassign IDs"
    );
    drop(cat);
    for id in ["111111111", "222222222"] {
        assert_eq!(
            std::fs::read(data.join(format!("{id}.png"))).unwrap(),
            format!("unchanged source {id}").as_bytes()
        );
    }
}

#[test]
fn original_pack_selection_and_applied_records_use_the_actual_command_without_touching_other_sets()
{
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("sync command café");
    std::fs::create_dir(root.0.join("assets")).unwrap();
    std::fs::write(root.0.join("assets/panel.html"), b"isolated fixture").unwrap();
    std::fs::write(root.0.join("pyproject.toml"), b"# isolated fixture\n").unwrap();
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
    let log = _native::logging::RunLog::open(&root.0, "sync-test-owner", vec![TOKEN.into()]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    native_reorder(&root.0, &executable, &environment, &runtime, &log);
    for (label, selections, visited, apply, code) in [
        ("all", vec![], vec![1, 2, 5], false, 0),
        ("one", vec![2], vec![2], false, 0),
        ("repeat", vec![5, 2, 2], vec![2, 5], false, 0),
        ("unknown", vec![9], vec![], false, 2),
        ("apply-one", vec![2], vec![2], true, 0),
    ] {
        let data = root.0.join(label);
        let cat = catalog(&data, &root.0, &[Some("a"), Some("b"), Some("c")]);
        let before_rows = cat.all(None).unwrap();
        drop(cat);
        let state_path = data.join("publish_pk.json");
        let mut expected = json!({"base":"pk","sets":[],"sent":[],"sent_full":[],"skipped":[]});
        for (index, id) in [(1, "a"), (2, "b"), (5, "c")] {
            expected["sets"].as_array_mut().unwrap().push(json!({"index":index,"name":format!("pks{index}_by_YourEmojiBot"),
                "fmt":"static","title":format!("Pack {index}"),"live":2,"logo":true,"keys":[format!("s:old{index}")]}));
            assert!(before_rows.iter().any(|row| row["custom_emoji_id"] == id));
        }
        _native::atomic::write_json(&state_path, &expected).unwrap();
        let original = std::fs::read(&state_path).unwrap();
        let replies = visited
            .iter()
            .map(|index| {
                let id = match index {
                    1 => "a",
                    2 => "b",
                    5 => "c",
                    _ => unreachable!(),
                };
                listing(&format!("pks{index}_by_YourEmojiBot"), &["logo", id])
            })
            .collect();
        let mut server = Server::new(replies);
        let mut environment = environment.clone();
        environment.insert("GENERAL_BOT_TOKEN".into(), TOKEN.into());
        environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        let mut args = vec![
            executable.as_os_str().to_owned(),
            "sync-order".into(),
            "--base".into(),
            "pk".into(),
            "--data-dir".into(),
            data.as_os_str().to_owned(),
        ];
        for index in selections {
            args.extend(["--pack".into(), index.to_string().into()]);
        }
        if apply {
            args.push("--apply".into());
        }
        let completed = runtime
            .block_on(_native::tooling_process::capture(
                &root.0,
                &args,
                &environment,
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log,
            ))
            .unwrap();
        let requests = server.finish().unwrap();
        assert_eq!(completed.code, code, "{label}: {}", completed.stderr);
        assert_eq!(requests.len(), visited.len());
        for (request, index) in requests.iter().zip(&visited) {
            assert_eq!(request.method, "getStickerSet");
            assert_eq!(
                request.form,
                BTreeMap::from([("name".into(), format!("pks{index}_by_YourEmojiBot"))])
            );
        }
        if code == 2 {
            assert!(completed.stderr.contains("requested pack is not recorded"));
        }
        if apply {
            expected["sets"][1]["keys"] = json!(["s:b"]);
            let actual: Value =
                serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
            assert_eq!(actual, expected);
        } else {
            assert_eq!(std::fs::read(&state_path).unwrap(), original);
        }
        let cat = Catalog::open(&data.join("catalog.db"), "fixture", &root.0).unwrap();
        assert_eq!(cat.all(None).unwrap(), before_rows);
        drop(cat);
    }
    runtime.shutdown_timeout(Duration::from_secs(1));
    drop(log);
    root.clean().unwrap();
}

#[test]
fn original_order_sources_refuse_before_runtime_or_workspace_access() {
    use _native::source_git_inventory::Source;
    let sources = [
        "tests/reference/sync_order.py",
        "tests/test_sync_order.py",
        "tests/reference/collection_state.py",
        "tests/reference/packstate.py",
    ]
    .map(|path| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: "0".repeat(64),
        text: "raise AssertionError('must not execute')".into(),
    });
    let absent = PathBuf::from("must-not-be-inspected");
    assert!(
        _native::source_sync_order_oracle::observe(
            &absent,
            &absent,
            &sources.iter().collect::<Vec<_>>()
        )
        .unwrap_err()
        .contains("source identity drift")
    );
    assert!(_native::source_sync_order_oracle::source_identity(&[]).is_err());
}
