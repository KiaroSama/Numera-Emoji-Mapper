//! Actual requested-ID command parity; retained codecs remain distinct from native consumer proof.
use _native::{catalog::Catalog, logging::RunLog, tooling_environment, tooling_process};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "support/fetch_pack_commands.rs"]
mod fetch_pack_commands;
#[path = "support/read_only_api.rs"]
mod read_only_api;
#[path = "support/retained_cli.rs"]
mod retained_cli;
use read_only_api::{Fixture, Reply};

fn resolved(ids: &[String], stickers: &[Value]) -> Vec<Reply> {
    vec![
        Reply::api("getMe", "", "", json!({"username":"YourEmojiBot"})),
        Reply::api(
            "getCustomEmojiStickers",
            "custom_emoji_ids",
            &json!(ids).to_string(),
            json!(stickers),
        ),
    ]
}
fn download(file: &str, bytes: &[u8]) -> [Reply; 2] {
    [
        Reply::api("getFile", "file_id", file, json!({"file_path":file})),
        Reply::file(file, bytes),
    ]
}
fn invoke(
    root: &Path,
    executable: &Path,
    runtime: &tokio::runtime::Runtime,
    environment: &BTreeMap<String, String>,
    args: &[String],
    log: &RunLog,
    deadline: Instant,
) -> tooling_process::Completed {
    let remaining = deadline.saturating_duration_since(Instant::now());
    assert!(
        !remaining.is_zero(),
        "command deadline elapsed; fixture retained"
    );
    let mut argv = vec![
        executable.as_os_str().to_owned(),
        OsString::from("fetch-emoji-ids"),
    ];
    argv.extend(args.iter().map(OsString::from));
    runtime
        .block_on(tooling_process::capture(
            root,
            &argv,
            environment,
            remaining.min(Duration::from_secs(30)),
            remaining.min(Duration::from_secs(10)),
            log,
        ))
        .unwrap()
}
fn empty_scratch(root: &Path, data: &Path) {
    assert!(
        std::fs::read_dir(root.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
    let temporary = data.join("tmp");
    let paths = std::fs::read_dir(&temporary)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(paths, [temporary.join("native-codec")]);
    assert!(std::fs::read_dir(&paths[0]).unwrap().next().is_none());
    assert!(!_native::process_owner::cleanup_pending(&temporary));
}
fn admitted(project: &Path, case: &Value) -> Vec<u8> {
    let path = project.join(case["input"].as_str().unwrap());
    tooling_environment::reject_links(&path, "fixture input cannot follow links").unwrap();
    assert_eq!(
        _native::tooling_manifest::digest(&path).unwrap(),
        case["input_sha256"]
    );
    std::fs::read(path).unwrap()
}

#[test]
fn actual_requested_id_command_preserves_tint_source_binding_and_partial_mixed_results() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let project =
        tooling_environment::physical_root(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())
            .unwrap();
    let parent = tooling_environment::confined_directory(&project, "logs/tmp", true).unwrap();
    let root = parent.join(format!("requested CLI café {}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let root = tooling_environment::physical_root(&root).unwrap();
    let (executable, mut environment) = retained_cli::install(&project, &root);
    environment.insert("GENERAL_BOT_TOKEN".into(), read_only_api::TOKEN.into());
    environment.insert("EMOJI_LOG_RETENTION_DAYS".into(), "0".into());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(
        &root,
        "requested-cli-test",
        vec![read_only_api::TOKEN.into()],
    );
    assert!(log.path.is_some());
    let deadline = Instant::now() + Duration::from_secs(60);
    let refused_data = root.join("refused collection");
    let malformed = root.join("malformed inventory.txt");
    // This owned byte fixture exercises strict UTF-8 rejection, not a text encoding alternative.
    std::fs::write(&malformed, b"premium-id:\xff111111111").unwrap();
    for (options, message) in [
        (
            vec!["--phash-threshold".into(), "999".into()],
            "phash threshold",
        ),
        (
            vec![
                "--id".into(),
                "111111111".into(),
                "--tint".into(),
                "#fff".into(),
            ],
            "tint must be #RRGGBB",
        ),
        (
            vec!["--ids-file".into(), malformed.to_str().unwrap().into()],
            "UTF-8",
        ),
        (
            vec!["--id".into(), "not-an-id".into()],
            "no valid custom-emoji IDs",
        ),
    ] {
        let mut args = options;
        args.extend(["--data-dir".into(), refused_data.to_str().unwrap().into()]);
        let completed = invoke(
            &root,
            &executable,
            &runtime,
            &environment,
            &args,
            &log,
            deadline,
        );
        assert_eq!(completed.code, 2, "{}", completed.stderr);
        assert!(completed.stdout.is_empty());
        assert!(completed.stderr.contains(message), "{}", completed.stderr);
        assert!(!completed.stderr.contains(read_only_api::TOKEN));
        assert!(!refused_data.exists());
    }
    assert_eq!(
        std::fs::read(&malformed).unwrap(),
        b"premium-id:\xff111111111"
    );
    let static_fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let alpha = static_fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "alpha-edge")
        .unwrap();
    let input = admitted(&project, alpha);
    let source = root.join("source.png");
    std::fs::write(&source, &input).unwrap();
    let tint_fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/media-continuation/prepare-tint.json"
    ))
    .unwrap();
    let tinted = tint_fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "success-reencode")
        .unwrap();
    assert_eq!(tinted["input_sha256"], alpha["input_sha256"]);
    let data = root.join("collection");
    let key = alpha["expected"]["content_key"].as_str().unwrap();
    let mut catalog = Catalog::open(&data.join("catalog.db"), "fixture UTC", &root).unwrap();
    assert!(
        catalog
            .insert(
                &json!({"content_key":key,"fmt":"static","file_path":source,
        "phash":alpha["expected"]["phash"],"emojis":["😀"],"keywords":[],"sources":[]}),
                "fixture UTC"
            )
            .unwrap()
    );
    catalog.record_seen("known-source", key).unwrap();
    let before = catalog.get(key).unwrap().unwrap();
    drop(catalog);
    let ids = ["111111111111111".to_owned()];
    let sticker = json!({"custom_emoji_id":ids[0],"file_id":"source.png","file_unique_id":"known-source",
        "emoji":"😀","needs_repainting":true});
    let inventory = root.join("requested inventory.txt");
    let inventory_text = format!(
        "example premium-id:222222222\npremium-id:{}\npremium-id:{}\n",
        ids[0], ids[0]
    );
    std::fs::write(&inventory, inventory_text.as_bytes()).unwrap();
    let mut script = Vec::new();
    for _ in 0..2 {
        script.extend(resolved(&ids, std::slice::from_ref(&sticker)));
        script.extend(download("source.png", &input));
    }
    let mut server = Fixture::new(script);
    environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for (new, dedup, tint) in [(1, 0, "1a80ff"), (0, 1, "\u{1c}##1a80ff\u{1f}")] {
            let completed = invoke(
                &root,
                &executable,
                &runtime,
                &environment,
                &[
                    "--ids-file".into(),
                    inventory.to_str().unwrap().into(),
                    "--id".into(),
                    ids[0].clone(),
                    "--tint".into(),
                    tint.into(),
                    "--data-dir".into(),
                    data.to_str().unwrap().into(),
                ],
                &log,
                deadline,
            );
            assert_eq!(completed.code, 0, "{}", completed.stderr);
            assert_eq!(
                completed.stdout,
                format!(
                    "Done. unique_ids=1 new={new} dedup={dedup} failed=0 missing=0 repainted=1\n  catalog static: 2 total (2 pending upload)\n"
                )
            );
            assert!(!completed.stderr.contains(read_only_api::TOKEN));
            let mut catalog =
                Catalog::open(&data.join("catalog.db"), "fixture UTC", &root).unwrap();
            assert_eq!(catalog.all(None).unwrap().len(), 2);
            assert_eq!(catalog.get(key).unwrap().unwrap(), before);
            assert_eq!(
                catalog
                    .dispatch(json!({"operation":"seen","file_unique_id":"known-source"}))
                    .unwrap(),
                key
            );
            let row = catalog
                .get(tinted["expected"]["content_key"].as_str().unwrap())
                .unwrap()
                .unwrap();
            assert_eq!(row["phash"], tinted["expected"]["phash"]);
            assert_eq!(
                row["keywords"],
                json!([format!("premium-id:{}", ids[0]), "tint:#1A80FF"])
            );
            assert_eq!(row["sources"], json!(["bot-inventory"]));
            let path = Path::new(row["file_path"].as_str().unwrap());
            assert_eq!(
                _native::tooling_manifest::digest(path).unwrap(),
                tinted["output"]["sha256"]
            );
            assert_eq!(path.metadata().unwrap().len(), tinted["output"]["bytes"]);
            empty_scratch(&root, &data);
        }
    }));
    let requests = server.finish();
    result.unwrap();
    assert_eq!(requests.unwrap().len(), 8);
    assert_eq!(std::fs::read(&source).unwrap(), input);
    assert_eq!(
        std::fs::read(&inventory).unwrap(),
        inventory_text.as_bytes()
    );

    let fixtures = [
        static_fixture,
        serde_json::from_str::<Value>(include_str!(
            "../../tests/fixtures/native/animated-key.json"
        ))
        .unwrap(),
        serde_json::from_str::<Value>(include_str!("../../tests/fixtures/native/video-codec.json"))
            .unwrap(),
    ];
    let paths = [
        "tests/fixtures/native/static-codec/alpha-edge.png",
        "tests/fixtures/native/tgs-package/valid.json",
        "tests/fixtures/native/video-codec/alpha.webm",
    ];
    let cases = std::array::from_fn::<_, 3, _>(|i| {
        fixtures[i]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["input"] == paths[i])
            .unwrap()
    });
    let inputs = cases.map(|case| admitted(&project, case));
    let ids = (111111111111111_u64..111111111111116)
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    let formats = ["static", "animated", "video", "bad"];
    let stickers = formats.iter().enumerate().map(|(i, format)| json!({"custom_emoji_id":ids[i],"file_id":format,
        "file_unique_id":format!("fixture-{format}"),"emoji":"😀","is_video":*format=="video","is_animated":*format=="animated"})).collect::<Vec<_>>();
    let mut script = resolved(&ids, &stickers);
    for (i, format) in formats[..3].iter().enumerate() {
        script.extend(download(format, &inputs[i]));
    }
    script.extend(download("bad", b"not a decodable image"));
    let mut server = Fixture::new(script);
    environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
    let mixed = root.join("mixed collection");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut args = vec![
            "--data-dir".into(),
            mixed.to_str().unwrap().into(),
            "--repaintable".into(),
            "keep".into(),
        ];
        for id in &ids {
            args.extend(["--id".into(), id.clone()]);
        }
        let completed = invoke(
            &root,
            &executable,
            &runtime,
            &environment,
            &args,
            &log,
            deadline,
        );
        assert_eq!(completed.code, 3, "{}", completed.stderr);
        assert_eq!(
            completed.stdout,
            "Done. unique_ids=5 new=3 dedup=0 failed=1 missing=1\n  catalog animated: 1 total (1 pending upload)\n  catalog static: 1 total (1 pending upload)\n  catalog video: 1 total (1 pending upload)\n"
        );
        assert!(!completed.stderr.contains(read_only_api::TOKEN));
        let mut catalog = Catalog::open(&mixed.join("catalog.db"), "fixture UTC", &root).unwrap();
        assert_eq!(catalog.all(None).unwrap().len(), 3);
        for (i, case) in cases.iter().enumerate() {
            let key = case["expected"]["content_key"].as_str().unwrap();
            let row = catalog.get(key).unwrap().unwrap();
            assert_eq!(row["fmt"], formats[i]);
            assert_eq!(row["phash"], case["expected"]["phash"]);
            assert!(Path::new(row["file_path"].as_str().unwrap()).is_file());
            assert_eq!(catalog.dispatch(json!({"operation":"seen","file_unique_id":format!("fixture-{}", formats[i])})).unwrap(), key);
        }
        assert!(
            catalog
                .dispatch(json!({"operation":"seen","file_unique_id":"fixture-bad"}))
                .unwrap()
                .is_null()
        );
        empty_scratch(&root, &mixed);
    }));
    let requests = server.finish();
    result.unwrap();
    assert_eq!(requests.unwrap().len(), 10);
    for (i, path) in paths.iter().enumerate() {
        assert_eq!(std::fs::read(project.join(path)).unwrap(), inputs[i]);
    }
    empty_scratch(&root, &data);
    empty_scratch(&root, &mixed);
    assert!(
        Instant::now() < deadline,
        "requested-ID commands exceeded fixture deadline"
    );
    fetch_pack_commands::check(&root, &executable, &runtime, &environment, &log, deadline);
    let logs = std::fs::read_dir(root.join("logs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("fetch-emoji-ids_")
        })
        .collect::<Vec<PathBuf>>();
    assert_eq!(logs.len(), 7);
    let texts = logs
        .iter()
        .map(|path| std::fs::read_to_string(path).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        texts
            .iter()
            .filter(|text| text.contains("exit=0 duration_ms="))
            .count(),
        2
    );
    assert_eq!(
        texts
            .iter()
            .filter(|text| text.contains("exit=3 duration_ms="))
            .count(),
        1
    );
    assert_eq!(
        texts
            .iter()
            .filter(|text| text.contains("exit=2 duration_ms="))
            .count(),
        4
    );
    for message in [
        format!(
            "requested inventory.txt: 2 real entries (1 unique), 1 example/prose mention(s) skipped, within-file duplicate(s): {}x2",
            ids[0]
        ),
        "collected 3 real id occurrences -> 1 unique (1 ids duplicated across files)".into(),
        format!("duplicate ids (fetched once each): {}x3", ids[0]),
    ] {
        assert_eq!(
            texts.iter().filter(|text| text.contains(&message)).count(),
            2
        );
    }
    assert!(
        texts
            .iter()
            .all(|text| !text.contains(read_only_api::TOKEN))
    );
    assert!(
        Instant::now() < deadline,
        "combined collector commands exceeded fixture deadline"
    );
    fetch_pack_commands::defaults(&root, &executable, &runtime, &environment, &log, deadline);
    fetch_pack_commands::id_arguments(&root, &executable, &runtime, &environment, &log, deadline);
    assert!(
        Instant::now() < deadline,
        "default repaint commands exceeded fixture deadline"
    );
    log.finish(0);
    drop(log);
    runtime.shutdown_timeout(Duration::from_secs(1));
    // Failure retains package/catalogs; all command owners and loopback threads are closed here.
    std::fs::remove_dir_all(root).expect("clean verified requested-ID command fixture");
}
