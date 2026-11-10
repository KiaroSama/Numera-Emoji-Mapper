//! Original cross-pack commands: real artwork, interrupted deletion, flood recovery and repeat.
use super::bot_api::{Reply, Server, TOKEN};
use super::tooling_fixture::Root;
use _native::catalog::Catalog;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::retained_cli;

const UTC: &str = "2026-10-07 00:00:00 UTC";
const NAMES: [&str; 3] = [
    "fixture1_by_YourEmojiBot",
    "fixture2_by_YourEmojiBot",
    "fixture3_by_YourEmojiBot",
];
const OLD: [&str; 3] = ["111111111", "222222222", "333333333"];
fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn listing(name: &str, stickers: &[Value]) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", name)],
        json!({"ok":true,"result":{"name":name,"stickers":stickers}}),
    )
}
fn get_me() -> Reply {
    Reply::new(
        "getMe",
        &[],
        json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
    )
}
fn flood() -> Value {
    json!({"ok":false,"error_code":429,"description":"Too Many Requests: retry after 900",
        "parameters":{"retry_after":900}})
}
fn interrupted_capture(root: &Path) {
    let directory = root.join("logs/tmp/tooling");
    _native::tooling_environment::reject_links(&directory, "linked capture directory").unwrap();
    let captures = std::fs::read_dir(&directory)
        .unwrap()
        .take(2)
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(captures.len(), 1);
    let path = &captures[0];
    assert_eq!(
        _native::tooling_environment::physical_root(path).unwrap(),
        *path
    );
    let prefix = format!("child-{}-", std::process::id());
    let suffix = path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix(&prefix)
        .unwrap();
    assert!(!suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()));
    assert!(!_native::process_owner::cleanup_pending(path));
    assert!(
        matches!(std::fs::symlink_metadata(path.join("owner")), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
    );
    let mut names = std::fs::read_dir(path)
        .unwrap()
        .take(3)
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        [
            std::ffi::OsString::from("stderr"),
            std::ffi::OsString::from("stdout")
        ]
    );
    for name in names {
        let file = path.join(name);
        _native::tooling_environment::reject_links(&file, "linked capture file").unwrap();
        let metadata = std::fs::symlink_metadata(&file).unwrap();
        assert!(metadata.is_file() && metadata.len() <= 8 * 1024 * 1024);
        assert!(!std::fs::read_to_string(file).unwrap().contains(TOKEN));
    }
    std::fs::remove_dir_all(path).unwrap();
}

pub fn run() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("cross-pack move café");
    let project = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    // Production still uses the retained codec; this fixture is not package/cutover acceptance.
    let (executable, mut environment) = retained_cli::install(&project, &root.0);
    let mut keys = Vec::new();
    let mut paths = Vec::new();
    let mut identities = Vec::new();
    let mut inputs = Vec::new();
    for (index, (name, hash)) in [
        (
            "numera-emoji-mapper-logo.png",
            "6f85376de3893dfa1e78045ce264103b0055acec400dd8a35222405d5c4f6951",
        ),
        (
            "numera-emoji-mapper-logo-circle.png",
            "8c278a42ab70a8cff3eaec0c7a6ed6cbc2d0ead11791cbadaf0aa8e06df3909c",
        ),
        (
            "numera-coin-emoji-mapper-logo.png",
            "f1ed763b31339e5dbb398abaa8e3870572acf0bed87d512279a84a88a23059a3",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let source = project.join("assets").join(name);
        assert_eq!(_native::tooling_manifest::digest(&source).unwrap(), hash);
        let input = std::fs::read(&source).unwrap();
        let path = root.0.join(format!("source-{index}.png"));
        _native::codec_image::to_static_png(&source, &path).unwrap();
        let identity = _native::codec_static_identity::fingerprint(&path).unwrap();
        keys.push(identity["content_key"].as_str().unwrap().to_owned());
        identities.push(identity);
        inputs.push((source, input));
        paths.push(path);
    }
    assert_eq!(
        keys.iter().collect::<BTreeSet<_>>().len(),
        3,
        "distinct real art required"
    );
    let originals = paths
        .iter()
        .map(|path| std::fs::read(path).unwrap())
        .collect::<Vec<_>>();
    assert!(
        originals
            .iter()
            .all(|bytes| !bytes.is_empty() && bytes.len() < 60 * 1024)
    );
    for key in [
        "GENERAL_BOT_TOKEN",
        "TELEGRAM_BOT_TOKEN",
        "COLLECTION_PACK_BASE",
        "BRAND_LOGO_PATH",
        "BRAND_LOGO_BOTS",
        "PACK_OWNER_USER_ID",
    ] {
        environment.remove(key);
    }
    environment.insert("GENERAL_BOT_TOKEN".into(), TOKEN.into());
    environment.insert("BRAND_LOGO_BOTS".into(), String::new());
    let eof = root.0.join("empty-input");
    std::fs::write(&eof, b"").unwrap();
    let log = _native::logging::RunLog::open(&root.0, "plan-move-owner", vec![TOKEN.into()]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for variant in [
        "normal",
        "applied",
        "unapplied",
        "delete flood",
        "add flood",
        "unknown",
    ] {
        let started = Instant::now();
        let data = root.0.join(format!("collection-{variant}"));
        let mut catalog = Catalog::open(&data.join("catalog.db"), UTC, &root.0).unwrap();
        for index in 0..3 {
            assert!(
                catalog
                    .insert(
                        &json!({"content_key":keys[index],"fmt":"static",
                "file_path":paths[index],"phash":identities[index]["phash"],"emojis":["😀"]}),
                        UTC
                    )
                    .unwrap()
            );
            catalog
                .record_seen(&format!("unique-{index}"), &keys[index])
                .unwrap();
            catalog
                .uploaded(
                    &json!({"content_key":keys[index],"custom_emoji_id":OLD[index],
                "base":"fixture","set_name":NAMES[index]}),
                    UTC,
                )
                .unwrap();
        }
        assert_eq!(catalog.order(&keys).unwrap(), 3);
        let rows = catalog.all(None).unwrap();
        drop(catalog);
        let state = json!({"base":"fixture","sets":(0..3).map(|index| json!({
            "name":NAMES[index],"title":format!("Fixture {}",index+1),"fmt":"mixed",
            "index":index+1,"keys":[keys[index]],"live":1,"logo":false})).collect::<Vec<_>>(),
            "sent":NAMES,"sent_full":[],"skipped":[]});
        let state_path = data.join("publish_fixture.json");
        let plan_path = data.join("pack_plan.json");
        let journal_path = data.join("plan_apply_fixture.json");
        _native::atomic::write_json(&state_path, &state).unwrap();
        _native::atomic::write_json(
            &plan_path,
            &json!({"version":1,
            "targets":[[keys[0],2],[keys[1],2],[keys[2],3]],"known":keys,
            "excluded":[],"moves":[],"held":[],"per_set":200}),
        )
        .unwrap();
        let state_bytes = std::fs::read(&state_path).unwrap();
        let plan_bytes = std::fs::read(&plan_path).unwrap();
        if variant == "unapplied" {
            _native::atomic::write_json(
                &journal_path,
                &json!({"version":1,"base":"fixture",
                "retired":[],"intent":{"op":"remove","key":keys[0],"set":NAMES[0],
                "cid":OLD[0],"at":"2026-09-29T00:00:00Z"}}),
            )
            .unwrap();
        }
        let live = (0..3)
            .map(|index| {
                json!({"custom_emoji_id":OLD[index],
            "file_id":format!("file-{index}"),"file_unique_id":format!("unique-{index}")})
            })
            .collect::<Vec<_>>();
        let destination = [
            live[1].clone(),
            json!({"custom_emoji_id":"444444444",
            "file_id":"moved-file","file_unique_id":"moved-unique"}),
        ];
        let deletion = |payload| {
            let mut reply = Reply::new("deleteStickerFromSet", &[("sticker", "file-0")], payload);
            reply.observation = Some((journal_path.clone(), "/intent/cid".into(), json!(OLD[0])));
            reply
        };
        let input_sticker = json!({"sticker":"attach://file0","format":"static",
            "emoji_list":["😀"],"keywords":[]})
        .to_string();
        let addition = |payload| {
            let mut reply = Reply::new(
                "addStickerToSet",
                &[
                    ("user_id", "111111111"),
                    ("name", NAMES[1]),
                    ("sticker", &input_sticker),
                ],
                payload,
            );
            reply.files.insert("file0".into(), originals[0].clone());
            reply.observation = Some((state_path.clone(), "/in_flight/key".into(), json!(keys[0])));
            reply
        };
        let success = || json!({"ok":true,"result":true});
        let mut replies = Vec::new();
        let mut interruption = None;
        match variant {
            "applied" => {
                let (reply, ready, release) = deletion(success()).hold();
                replies.extend([get_me(), listing(NAMES[0], &live[..1]), reply]);
                interruption = Some((ready, release));
                replies.extend([get_me(), listing(NAMES[0], &[])]);
            }
            "delete flood" => {
                replies.extend([get_me(), listing(NAMES[0], &live[..1]), deletion(flood())]);
                replies.extend([
                    get_me(),
                    listing(NAMES[0], &live[..1]),
                    deletion(success()),
                    listing(NAMES[0], &[]),
                ]);
            }
            "add flood" => {
                replies.extend([
                    get_me(),
                    listing(NAMES[0], &live[..1]),
                    deletion(success()),
                    listing(NAMES[0], &[]),
                    listing(NAMES[1], &live[1..2]),
                    listing(NAMES[1], &live[1..2]),
                    addition(flood()),
                    get_me(),
                    listing(NAMES[1], &live[1..2]),
                    listing(NAMES[1], &live[1..2]),
                ]);
            }
            "unknown" => {
                replies.extend([
                    get_me(),
                    Reply::new(
                        "getStickerSet",
                        &[("name", NAMES[0])],
                        json!({"ok":true,"result":[]}),
                    ),
                    get_me(),
                    listing(NAMES[0], &live[..1]),
                    deletion(success()),
                    listing(NAMES[0], &[]),
                ]);
            }
            "normal" | "unapplied" => {
                replies.push(get_me());
                if variant == "unapplied" {
                    replies.push(listing(NAMES[0], &live[..1]));
                }
                replies.extend([
                    listing(NAMES[0], &live[..1]),
                    deletion(success()),
                    listing(NAMES[0], &[]),
                ]);
            }
            _ => unreachable!(),
        }
        replies.extend([
            listing(NAMES[1], &live[1..2]),
            listing(NAMES[1], &live[1..2]),
            addition(success()),
            listing(NAMES[1], &destination),
            Reply::new(
                "getFile",
                &[("file_id", "moved-file")],
                json!({"ok":true,"result":{"file_path":"moved.png"}}),
            ),
            Reply {
                method: "GET moved.png".into(),
                fields: BTreeMap::new(),
                payload: Value::Null,
                status: axum::http::StatusCode::OK,
                headers: axum::http::HeaderMap::new(),
                observation: None,
                files: BTreeMap::new(),
                upload_sha256: BTreeMap::new(),
                download: Some(originals[0].clone()),
                download_upload: None,
                held: None,
            },
            listing(NAMES[1], &destination),
            listing(NAMES[2], &live[2..]),
            listing(NAMES[1], &destination),
            Reply::new(
                "setStickerPositionInSet",
                &[("sticker", "moved-file"), ("position", "0")],
                success(),
            ),
            get_me(),
        ]);
        let expected = replies
            .iter()
            .map(|reply| (reply.method.clone(), reply.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        let argv = [
            executable.as_os_str().to_owned(),
            "plan-apply".into(),
            "--base".into(),
            "fixture".into(),
            "--data-dir".into(),
            data.as_os_str().to_owned(),
            "--apply".into(),
            "--user-id".into(),
            "111111111".into(),
            "--max-changes".into(),
            "2".into(),
        ];
        let execute = || {
            _native::tooling_process::capture_with_input(
                &root.0,
                &argv,
                &environment,
                &eof,
                Duration::from_secs(15),
                Duration::from_secs(8),
                &log,
            )
        };
        if variant == "unapplied" {
            let journal_bytes = std::fs::read(&journal_path).unwrap();
            let lock_path = data.join("publish_fixture.lock");
            let lease = _native::locks::Lease::acquire(&lock_path, UTC).unwrap();
            let locked = runtime.block_on(execute()).unwrap();
            assert_eq!(locked.code, 2, "{}", locked.stderr);
            assert!(
                locked.stderr.contains("OS lock acquisition failed"),
                "{}",
                locked.stderr
            );
            assert_eq!(std::fs::read(&state_path).unwrap(), state_bytes);
            assert_eq!(std::fs::read(&journal_path).unwrap(), journal_bytes);
            let catalog = Catalog::open(&data.join("catalog.db"), UTC, &root.0).unwrap();
            assert_eq!(catalog.all(None).unwrap(), rows);
            drop(catalog);
            assert_eq!(std::fs::read(&plan_path).unwrap(), plan_bytes);
            drop(lease);
        }
        if let Some((ready, release)) = interruption {
            runtime.block_on(async {
                let execution = execute();
                tokio::pin!(execution);
                tokio::select! {
                    result = &mut execution => panic!("move exited before delete interruption: {:?}", result.map(|value| value.code)),
                    result = tokio::time::timeout(Duration::from_secs(8), ready) => {
                        result.expect("delete readiness exceeded8s").expect("delete readiness dropped");
                        assert_eq!(read(&journal_path)["intent"]["key"], keys[0]);
                        assert_eq!(std::fs::read(&state_path).unwrap(), state_bytes);
                    }
                }
                // Drop the actual capture only after the external fixture accepted the delete.
            });
            // Dropping the owned client may already disconnect and drop the held receiver.
            // Readiness above proves the delete landed; custody and journal checks below
            // prove cancellation without requiring a disconnected HTTP handler to stay alive.
            let _ = release.send(());
            interrupted_capture(&root.0);
            assert_eq!(read(&journal_path)["intent"]["cid"], OLD[0]);
        } else if variant == "unknown" {
            let refused = runtime.block_on(execute()).unwrap();
            assert_eq!(refused.code, 4, "{}", refused.stderr);
            assert!(refused.stderr.contains("live state unknown"));
            assert_eq!(std::fs::read(&state_path).unwrap(), state_bytes);
            assert!(!journal_path.exists());
            assert!(!data.join("tmp/native-codec").exists());
            let cat = Catalog::open(&data.join("catalog.db"), UTC, &root.0).unwrap();
            assert_eq!(cat.all(None).unwrap(), rows);
            drop(cat);
        } else if variant.ends_with("flood") {
            let refused = runtime.block_on(execute()).unwrap();
            assert_eq!(refused.code, 3, "{}", refused.stderr);
            assert!(refused.stderr.contains("FLOOD_WAIT"));
            let journal = read(&journal_path);
            assert!(journal["intent"].is_null());
            let saved = read(&state_path);
            if variant == "delete flood" {
                assert_eq!(journal["retired"], json!([]));
                assert_eq!(std::fs::read(&state_path).unwrap(), state_bytes);
                assert!(!data.join("tmp/native-codec").exists());
            } else {
                assert_eq!(journal["retired"].as_array().unwrap().len(), 1);
                assert_eq!(journal["retired"][0]["cid"], OLD[0]);
                assert_eq!(saved["sets"][0]["keys"], json!([]));
                assert_eq!(saved["sets"][0]["live"], 0);
                assert_eq!(saved["sets"][1], state["sets"][1]);
                assert_eq!(saved["in_flight"]["key"], keys[0]);
            }
            assert_eq!(saved["sets"][2], state["sets"][2]);
            let mut cat = Catalog::open(&data.join("catalog.db"), UTC, &root.0).unwrap();
            assert_eq!(
                cat.dispatch(
                    json!({"operation":"published","base":"fixture","content_key":keys[0]})
                )
                .unwrap(),
                variant == "delete flood"
            );
            drop(cat);
        }
        let mut saved_after_first = None;
        let mut journal_after_first = None;
        for run in 0..2 {
            let result = runtime.block_on(execute()).unwrap();
            assert_eq!(result.code, 0, "{variant} run{run}: {}", result.stderr);
            let saved = read(&state_path);
            assert_eq!(saved["sets"][0]["keys"], json!([]));
            assert_eq!(saved["sets"][0]["live"], 0);
            assert_eq!(saved["sets"][1]["keys"], json!(&keys[..2]));
            assert_eq!(saved["sets"][1]["live"], 2);
            assert_eq!(saved["sets"][2], state["sets"][2]);
            assert!(saved.get("in_flight").is_none_or(Value::is_null));
            let journal = read(&journal_path);
            assert!(journal["intent"].is_null());
            assert_eq!(journal["retired"].as_array().unwrap().len(), 1);
            assert_eq!(journal["retired"][0]["cid"], OLD[0]);
            assert_eq!(journal["retired"][0]["key"], keys[0]);
            if run == 0 {
                assert!(
                    result
                        .stdout
                        .contains(if matches!(variant, "applied" | "add flood") {
                            "1 counted change(s)"
                        } else {
                            "2 counted change(s)"
                        })
                );
                saved_after_first = Some(saved);
                journal_after_first = Some(journal);
            } else {
                assert!(result.stdout.contains("nothing pending"));
                assert_eq!(saved_after_first.as_ref(), Some(&saved));
                assert_eq!(journal_after_first.as_ref(), Some(&journal));
            }
            let mut cat = Catalog::open(&data.join("catalog.db"), UTC, &root.0).unwrap();
            let expected_ids = ["444444444", OLD[1], OLD[2]];
            let published_sets = cat.dispatch(json!({"operation":"published_sets"})).unwrap();
            for index in 0..3 {
                assert_eq!(
                    cat.dispatch(
                        json!({"operation":"custom_id","base":"fixture","content_key":keys[index]})
                    )
                    .unwrap(),
                    expected_ids[index]
                );
                assert_eq!(
                    published_sets[&keys[index]],
                    NAMES[if index == 0 { 1 } else { index }]
                );
                let mut expected = rows[index].clone();
                expected["custom_emoji_id"] = json!(expected_ids[index]);
                assert_eq!(cat.get(&keys[index]).unwrap(), Some(expected));
            }
            assert_eq!(
                cat.dispatch(json!({"operation":"seen","file_unique_id":"moved-unique"}))
                    .unwrap(),
                keys[0]
            );
            drop(cat);
            assert_eq!(std::fs::read(&plan_path).unwrap(), plan_bytes);
            assert_eq!(
                std::fs::read_dir(data.join("tmp/native-codec"))
                    .unwrap()
                    .count(),
                0
            );
            assert_eq!(
                std::fs::read_dir(data.join("tmp"))
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name())
                    .collect::<Vec<_>>(),
                [std::ffi::OsString::from("native-codec")]
            );
            for (path, bytes) in paths.iter().zip(&originals) {
                assert_eq!(std::fs::read(path).unwrap(), *bytes);
            }
        }
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), expected.len());
        for (request, (method, fields)) in requests.iter().zip(&expected) {
            assert_eq!(&request.method, method);
            assert_eq!(&request.form, fields);
        }
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.method == "deleteStickerFromSet")
                .count(),
            if variant == "delete flood" { 2 } else { 1 }
        );
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.method == "addStickerToSet")
                .count(),
            if variant == "add flood" { 2 } else { 1 }
        );
        for request in &requests {
            match request.method.as_str() {
                "getMe" | "GET moved.png" => assert!(request.form.is_empty()),
                "addStickerToSet" => assert_eq!(
                    request.form,
                    BTreeMap::from([
                        ("user_id".into(), "111111111".into()),
                        ("name".into(), NAMES[1].into()),
                        ("sticker".into(), input_sticker.clone())
                    ])
                ),
                "deleteStickerFromSet" => assert_eq!(
                    request.form,
                    BTreeMap::from([("sticker".into(), "file-0".into())])
                ),
                _ => {}
            }
        }
        assert!(
            started.elapsed() < Duration::from_secs(45),
            "{variant} cross-pack tracer exceeded45s; evidence retained"
        );
    }
    for (path, bytes) in inputs {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    runtime.shutdown_timeout(Duration::from_secs(1));
    log.finish(0);
    drop(log);
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    root.clean().unwrap();
}
