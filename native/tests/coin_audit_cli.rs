//! Actual audit command retries corrupt art without losing cached IDs or reporting it as blank.
use _native::{logging::RunLog, tooling_environment, tooling_process, tooling_test_runner};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

#[path = "support/read_only_api.rs"]
mod read_only_api;
use read_only_api::{Fixture, Reply};

fn listing() -> Reply {
    Reply::api(
        "getStickerSet",
        "name",
        "cryptoemoji1_by_YourEmojiBot",
        json!({"stickers":[{"custom_emoji_id":"a","file_id":"cached"},
            {"custom_emoji_id":"b","file_id":"download"}]}),
    )
}
fn download(bytes: &[u8]) -> [Reply; 2] {
    [
        Reply::api(
            "getFile",
            "file_id",
            "download",
            json!({"file_path":"fixture.png"}),
        ),
        Reply::file("fixture.png", bytes),
    ]
}
fn empty_scratch(root: &Path) {
    let temporary = root.join("logs/tmp");
    assert!(
        std::fs::read_dir(temporary.join("tooling"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(!_native::process_owner::cleanup_pending(&temporary));
    assert_eq!(std::fs::read_dir(&temporary).unwrap().count(), 1);
}

#[test]
fn actual_audit_downloads_only_missing_art_and_reports_current_live_positions() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let project =
        tooling_environment::physical_root(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())
            .unwrap();
    let parent = tooling_environment::confined_directory(&project, "logs/tmp", true).unwrap();
    let root = parent.join(format!("coin audit CLI café {}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let root = tooling_environment::physical_root(&root).unwrap();
    std::fs::create_dir(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/panel.html"), b"fixture root marker").unwrap();
    std::fs::write(root.join("pyproject.toml"), b"# fixture root marker\n").unwrap();
    let coins = root.join("coins");
    std::fs::create_dir(&coins).unwrap();
    let state = coins.join("rebuild_dedup_state.json");
    _native::atomic::write_json(
        &state,
        &json!({"sets":[{"index":1,"name":"cryptoemoji1_by_YourEmojiBot"}]}),
    )
    .unwrap();
    let before_state = std::fs::read(&state).unwrap();
    let cache_path = coins.join("pack_audit4.json");
    _native::atomic::write_json(
        &cache_path,
        &json!({"a":{"hash":"old","blank":false},"gone":{"hash":"old","blank":false},
            "b":{"error":"retry this image"}}),
    )
    .unwrap();
    let fixture_path = project.join("tests/fixtures/native/coin-audit-image.json");
    let fixture_bytes = std::fs::read(&fixture_path).unwrap();
    let fixture_text = std::str::from_utf8(&fixture_bytes)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(
        Sha256::digest(fixture_text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        "6f1fed3b033a76dc7bcd5a6c01520347c917ca34953016764b33d8c5fa2351a4"
    );
    let fixture: Value = serde_json::from_str(&fixture_text).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let blank = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| {
            case["input"] == "tests/fixtures/native/provider-image/visible-0-alpha-255.png"
        })
        .unwrap();
    let input_path = project.join(blank["input"].as_str().unwrap());
    tooling_environment::reject_links(&input_path, "audit fixture cannot follow links").unwrap();
    assert_eq!(
        _native::tooling_manifest::digest(&input_path).unwrap(),
        blank["input_sha256"]
    );
    let bytes = std::fs::read(&input_path).unwrap();
    let corrupt = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case.get("data").and_then(Value::as_str).is_some())
        .unwrap();
    assert_eq!(corrupt["data"], "not an image");
    assert_eq!(corrupt["expected"]["error_type"], "UnidentifiedImageError");
    let mut script = vec![
        listing(),
        Reply::envelope(
            "getFile",
            "file_id",
            "download",
            json!({"ok":false,"error_code":400,
                "description":format!("download refused {}", read_only_api::TOKEN)}),
        ),
        listing(),
    ];
    script.extend(download(corrupt["data"].as_str().unwrap().as_bytes()));
    script.push(listing());
    script.extend(download(&bytes));
    script.push(listing());
    let executable = root.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let mut environment = tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    environment.insert("TELEGRAM_BOT_TOKEN".into(), read_only_api::TOKEN.into());
    environment.insert("EMOJI_LOG_RETENTION_DAYS".into(), "0".into());
    let mut environment = tooling_environment::environment(&root, &environment, false).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(&root, "audit-cli-test", vec![read_only_api::TOKEN.into()]);
    assert!(log.path.is_some());
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut server = Fixture::new(script);
    environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
    let report_path = coins.join("pack_audit4_report.txt");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut last_cache = Vec::new();
        let mut last_report = Vec::new();
        for (iteration, expected) in [3, 3, 0, 0].into_iter().enumerate() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "audit deadline elapsed; fixture retained"
            );
            let completed = runtime
                .block_on(tooling_process::capture(
                    &root,
                    &[
                        executable.as_os_str().to_owned(),
                        OsString::from("coin-check-packs"),
                    ],
                    &environment,
                    remaining.min(Duration::from_secs(15)),
                    remaining.min(Duration::from_secs(5)),
                    &log,
                ))
                .unwrap();
            assert_eq!(completed.code, expected, "{}", completed.stderr);
            for text in [&completed.stdout, &completed.stderr] {
                assert!(!text.contains(read_only_api::TOKEN));
            }
            let report_bytes = std::fs::read(&report_path).unwrap();
            let report = std::str::from_utf8(&report_bytes).unwrap();
            assert_eq!(
                completed.stdout.replace("\r\n", "\n"),
                format!("\n{report}\n")
            );
            assert!(report.contains("live stickers: 2"));
            assert!(report.contains("DUPLICATE image groups: 0"));
            assert!(!report.contains(read_only_api::TOKEN));
            let cache_bytes = std::fs::read(&cache_path).unwrap();
            let cache: Value = serde_json::from_slice(&cache_bytes).unwrap();
            assert!(cache.get("gone").is_none());
            assert_eq!(cache["a"], json!({"hash":"old","blank":false}));
            assert!(
                !std::str::from_utf8(&cache_bytes)
                    .unwrap()
                    .contains(read_only_api::TOKEN)
            );
            if iteration < 2 {
                assert!(
                    cache["b"]["error"]
                        .as_str()
                        .is_some_and(|error| !error.is_empty())
                );
                assert!(cache["b"].get("blank").is_none());
                assert!(report.contains("FAILED (retried on next run): 1"));
                assert!(report.contains("BLANK stickers: 0"));
                assert!(report.contains("failed cid=b set=1 pos=1"));
                if iteration == 0 {
                    assert!(report.contains("download refused [REDACTED]"));
                    assert!(cache["b"]["error"].as_str().unwrap().contains("[REDACTED]"));
                }
            } else {
                assert_eq!(cache["b"], blank["expected"]);
                assert!(report.contains("FAILED (retried on next run): 0"));
                assert!(report.contains("blank cid=b set=1 pos=1"));
                if iteration == 3 {
                    assert_eq!(cache_bytes, last_cache);
                    assert_eq!(report_bytes, last_report);
                }
            }
            last_cache = cache_bytes;
            last_report = report_bytes;
            assert_eq!(std::fs::read(&state).unwrap(), before_state);
            assert_eq!(std::fs::read(&input_path).unwrap(), bytes);
            assert!(!std::fs::read_dir(&coins).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    == Some("tmp")
            }));
            empty_scratch(&root);
        }
    }));
    let observed = server.finish();
    if let Err(error) = result {
        if let Err(server_error) = observed {
            eprintln!("Audit read-only fixture: {server_error}");
        }
        std::panic::resume_unwind(error);
    }
    assert_eq!(observed.unwrap().len(), 9);
    assert!(
        Instant::now() < deadline,
        "audit deadline elapsed; fixture retained"
    );
    let logs = std::fs::read_dir(root.join("logs"))
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("coin-check-packs_")
                .then_some(path)
        })
        .collect::<Vec<_>>();
    assert_eq!(logs.len(), 4);
    let mut exits = BTreeMap::<i32, usize>::new();
    for path in logs {
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(read_only_api::TOKEN));
        assert!(text.contains("[INFO] [runtime] native backend started"));
        assert!(text.contains("[INFO] [coins] audit_cached=2"));
        assert!(!text.contains("[CRITICAL]"));
        let code = if text.contains("exit=3 ") {
            3
        } else {
            assert!(text.contains("exit=0 "));
            0
        };
        *exits.entry(code).or_default() += 1;
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            drop(
                std::fs::OpenOptions::new()
                    .write(true)
                    .share_mode(0)
                    .open(&path)
                    .unwrap(),
            );
        }
    }
    assert_eq!(exits, BTreeMap::from([(0, 2), (3, 2)]));
    assert_eq!(std::fs::read(&fixture_path).unwrap(), fixture_bytes);
    log.finish(0);
    drop(log);
    std::fs::remove_dir_all(root).expect("clean verified audit command fixture");
}

#[test]
fn interrupted_second_listing_preserves_the_first_set_checkpoint_without_a_report() {
    let project =
        tooling_environment::physical_root(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())
            .unwrap();
    let parent = tooling_environment::confined_directory(&project, "logs/tmp", true).unwrap();
    let root = parent.join(format!("audit checkpoint café {}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let root = tooling_environment::physical_root(&root).unwrap();
    std::fs::create_dir(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/panel.html"), b"fixture root marker").unwrap();
    std::fs::write(root.join("pyproject.toml"), b"# fixture root marker\n").unwrap();
    std::fs::create_dir(root.join("coins")).unwrap();
    let state = root.join("coins/rebuild_dedup_state.json");
    _native::atomic::write_json(
        &state,
        &json!({"sets":[
            {"index":1,"name":"cryptoemoji1_by_YourEmojiBot"},
            {"index":2,"name":"cryptoemoji2_by_YourEmojiBot"}
        ]}),
    )
    .unwrap();
    let before_state = std::fs::read(&state).unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-audit-image.json"
    ))
    .unwrap();
    _native::source_coin_audit_oracle::image_fixture_identity(&fixture).unwrap();
    let image = &fixture["cases"][0];
    let input = project.join(image["input"].as_str().unwrap());
    assert_eq!(
        _native::tooling_manifest::digest(&input).unwrap(),
        image["input_sha256"]
    );
    let bytes = std::fs::read(&input).unwrap();
    let (held, ready) = Reply::api(
        "getStickerSet",
        "name",
        "cryptoemoji2_by_YourEmojiBot",
        json!({"stickers":[]}),
    )
    .hold();
    let mut script = vec![Reply::api(
        "getStickerSet",
        "name",
        "cryptoemoji1_by_YourEmojiBot",
        json!({"stickers":[{"custom_emoji_id":"a","file_id":"download"}]}),
    )];
    script.extend(download(&bytes));
    script.push(held);
    let mut server = Fixture::new(script);
    let executable = root.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let mut environment = tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    environment.insert("TELEGRAM_BOT_TOKEN".into(), read_only_api::TOKEN.into());
    environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
    environment.insert("EMOJI_LOG_RETENTION_DAYS".into(), "0".into());
    let environment = tooling_environment::environment(&root, &environment, false).unwrap();
    let log = RunLog::open(
        &root,
        "audit-checkpoint-test",
        vec![read_only_api::TOKEN.into()],
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let argv = [
            executable.as_os_str().to_owned(),
            OsString::from("coin-check-packs"),
        ];
        runtime.block_on(async {
            let execution = tooling_process::capture(&root, &argv, &environment,
                Duration::from_secs(12), Duration::from_secs(5), &log);
            tokio::pin!(execution);
            tokio::select! {
                result = &mut execution => panic!("audit exited before second listing: {:?}", result.map(|value| value.code)),
                ready = tokio::time::timeout(Duration::from_secs(8), ready) => {
                    ready.expect("second listing readiness timed out").expect("fixture ended before second listing");
                    let cache: Value = serde_json::from_slice(&std::fs::read(root.join("coins/pack_audit4.json")).unwrap()).unwrap();
                    assert_eq!(cache, json!({"a":image["expected"]}));
                    assert!(!root.join("coins/pack_audit4_report.txt").exists());
                }
            }
            // Dropping this in-flight capture invokes the actual owned-tree cancellation path.
        });
        let cache: Value =
            serde_json::from_slice(&std::fs::read(root.join("coins/pack_audit4.json")).unwrap())
                .unwrap();
        assert_eq!(cache, json!({"a":image["expected"]}));
        assert!(!root.join("coins/pack_audit4_report.txt").exists());
        assert_eq!(std::fs::read(&state).unwrap(), before_state);
        assert_eq!(std::fs::read(&input).unwrap(), bytes);
    }));
    let observed = server.finish();
    if let Err(error) = result {
        if let Err(server_error) = observed {
            eprintln!("Audit interruption fixture: {server_error}");
        }
        std::panic::resume_unwind(error);
    }
    assert_eq!(observed.unwrap().len(), 4);
    assert!(!_native::process_owner::cleanup_pending(
        &root.join("logs/tmp")
    ));
    let tooling = root.join("logs/tmp/tooling");
    // Cancellation preserves capture files for diagnosis, but the child owner must be gone.
    let captures = std::fs::read_dir(&tooling)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(captures.len(), 1);
    assert_eq!(
        tooling_environment::physical_root(&captures[0]).unwrap(),
        captures[0]
    );
    assert_eq!(captures[0].parent(), Some(tooling.as_path()));
    assert!(
        captures[0]
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(&format!("child-{}-", std::process::id()))
    );
    assert!(!_native::process_owner::cleanup_pending(&captures[0]));
    assert!(
        matches!(std::fs::symlink_metadata(captures[0].join("owner")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound)
    );
    let mut names = std::fs::read_dir(&captures[0])
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, [OsString::from("stderr"), OsString::from("stdout")]);
    for name in ["stdout", "stderr"] {
        assert!(
            !std::fs::read_to_string(captures[0].join(name))
                .unwrap()
                .contains(read_only_api::TOKEN)
        );
    }
    std::fs::remove_dir_all(&captures[0]).unwrap();
    empty_scratch(&root);
    log.finish(0);
    drop(log);
    std::fs::remove_dir_all(root).expect("clean verified interrupted audit fixture");
}
