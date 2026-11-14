//! Original preflight assertions and five CLI observations share one admitted corpus and process owner.
use _native::{
    catalog::Catalog, source_git_inventory::Source, source_preflight_oracle as oracle,
    tooling_process,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, sources: &[Source]) {
    let source = sources
        .iter()
        .find(|source| source.path == "tests/oracles/collection_preflight.py")
        .unwrap();
    let test = sources
        .iter()
        .find(|source| source.path == "tests/test_native_publish_cli.py")
        .unwrap();
    let unavailable = root.join("logs/tmp/not-a-preflight-executable");
    for dependency in [false, true] {
        let original = if dependency { test } else { source };
        for mutation in ["revision", "path", "hash", "bytes"] {
            let mut tampered = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => tampered.baseline_revision = "0".repeat(40),
                "path" => tampered.path = "tests/unverified_preflight.py".into(),
                "hash" => tampered.sha256 = "0".repeat(64),
                "bytes" => tampered
                    .text
                    .push_str("\nraise RuntimeError('must not execute')\n"),
                _ => unreachable!(),
            }
            let (source, test) = if dependency {
                (source, &tampered)
            } else {
                (&tampered, test)
            };
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::generate_preflight(
                        root,
                        &unavailable,
                        source,
                        test,
                    ))
                    .unwrap_err()
                    .contains("source identity drift"),
                "{mutation}"
            );
            assert!(
                _native::source_preflight_oracle::generate(source, test)
                    .unwrap_err()
                    .contains("source identity drift"),
                "{mutation}"
            );
        }
    }
    let admitted = oracle::source_paths()
        .map(|path| sources.iter().find(|source| source.path == path).unwrap());
    oracle::outcome_identity(&admitted).unwrap();
    for index in 0..admitted.len() {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let original = admitted[index];
            let mut changed = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "tests/not-original.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                _ => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
            }
            let mut altered = admitted;
            altered[index] = &changed;
            assert!(
                runtime
                    .block_on(oracle::observe_owned(
                        root,
                        &unavailable,
                        &unavailable,
                        &altered,
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!unavailable.exists());
        }
    }
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let workspace = parent.join(format!("original preflight {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let caller = workspace.join("caller");
    std::fs::write(&caller, b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, &admitted)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(std::fs::read(&caller).unwrap(), b"caller bytes");
    assert!(
        oracle::observe(root, root, &admitted)
            .unwrap_err()
            .contains("inside project scratch")
    );
    std::fs::remove_file(caller).unwrap();
    let observed = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            &admitted,
        ))
        .unwrap();
    assert_eq!(observed["source_revision"], source.baseline_revision);
    assert_eq!(observed["source_runtime"], "CPython3.11.9");
    assert_eq!(observed["Pillow"], "12.3.0");
    assert_eq!(observed["tests_run"], 11);
    assert_eq!(observed["temporary_calls"], 8);
    assert_eq!(observed["sleep_calls"], 26);
    assert_eq!(observed["identity_comparisons"], 0);
    assert_eq!(observed["prerequisite_calls"], 8);
    assert_eq!(observed["writer_calls"], 8);
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(observed[field], 0, "{field}");
    }
    for source in admitted {
        assert_eq!(observed["source_hashes"][&source.path], source.sha256);
    }
    let actual = observed["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case.as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let coverage: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/native/coverage.json")).unwrap();
    let expected = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["source_file"] == "tests/test_preflight_outcomes.py")
        .map(|case| {
            assert!(case.get("candidate").is_some());
            case["source_case"].as_str().unwrap()
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual.len(), 11);
    assert_eq!(actual, expected);
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
    let reports = &observed["report_observations"];
    assert_eq!(reports["cases_checked"], 5);
    assert_eq!(reports["reports"].as_array().unwrap().len(), 5);
    assert_eq!(reports["source_hash"], source.sha256);
    assert_eq!(reports["test_source_hash"], test.sha256);
    assert_eq!(reports["source_runtime"], "CPython3.11.9");
    run_cli(root, runtime, reports);
}

fn request(stream: &mut std::net::TcpStream, deadline: Instant) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let end = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "preflight request exceeded wall bound"
        );
        stream
            .set_read_timeout(Some(remaining.min(Duration::from_secs(2))))
            .unwrap();
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0 && bytes.len() + count <= 2 * 1024 * 1024);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let header = std::str::from_utf8(&bytes[..end]).unwrap().to_owned();
    let length = header
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    assert!(length <= 2 * 1024 * 1024 - end);
    while bytes.len() < end + length {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "preflight request exceeded wall bound"
        );
        stream
            .set_read_timeout(Some(remaining.min(Duration::from_secs(2))))
            .unwrap();
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0 && bytes.len() + count <= end + length);
        bytes.extend_from_slice(&buffer[..count]);
    }
    header
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap()
        .to_owned()
}

fn command(executable: &Path, data: &Path, mode: &str) -> Vec<OsString> {
    vec![
        executable.as_os_str().to_owned(),
        "build-collection".into(),
        "--base".into(),
        "fixture".into(),
        "--title".into(),
        "Fixture".into(),
        "--mixed".into(),
        "--no-brand-logo".into(),
        "--user-id".into(),
        "111111111".into(),
        "--data-dir".into(),
        data.as_os_str().to_owned(),
        mode.into(),
    ]
}

fn run_cli(root: &Path, runtime: &tokio::runtime::Runtime, reports: &Value) {
    let scratch = root
        .join("logs/tmp")
        .join(format!("preflight source cli {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        std::fs::create_dir(scratch.join("assets")).unwrap();
        std::fs::write(scratch.join("assets/panel.html"), b"isolated fixture").unwrap();
        std::fs::write(scratch.join("pyproject.toml"), b"# isolated fixture\n").unwrap();
        let executable = scratch.join(if cfg!(windows) {
            "numera-emoji.exe"
        } else {
            "numera-emoji"
        });
        std::fs::copy(env!("CARGO_BIN_EXE_numera-compat"), &executable).unwrap();
        let data = scratch.join("collection");
        let image = scratch.join("missing.png");
        let key = "s:missing";
        {
            let mut catalog = Catalog::open(&data.join("catalog.db"), "fixture", &scratch).unwrap();
            catalog
                .insert(
                    &json!({"content_key":key,"fmt":"static","file_path":image}),
                    "fixture",
                )
                .unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (stop, stopped) = mpsc::channel::<()>();
        let received = Arc::new(Mutex::new(Vec::new()));
        let response = Arc::new(Mutex::new(
            json!({"ok":true,"result":{"file_id":"fixture-upload"}}),
        ));
        let server_received = received.clone();
        let server_response = response.clone();
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(35);
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(
                    !remaining.is_zero(),
                    "preflight fixture exceeded wall bound"
                );
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        // A stop message wakes this bounded wait without a busy-spin or blind sleep.
                        match stopped.recv_timeout(remaining.min(Duration::from_millis(5))) {
                            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                            Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        }
                    }
                    Err(error) => panic!("preflight fixture: {error}"),
                };
                // Winsock inherits the listener's nonblocking mode. Read/write timeouts
                // bound the accepted connection once it is explicitly blocking.
                stream.set_nonblocking(false).unwrap();
                let method = request(&mut stream, deadline);
                server_received.lock().unwrap().push(method.clone());
                let body = if method == "getMe" {
                    json!({"ok":true,"result":{"username":"YourEmojiBot"}})
                } else {
                    assert_eq!(method, "uploadStickerFile", "preflight may not publish");
                    server_response.lock().unwrap().clone()
                }
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let mut remaining_bytes = response.as_bytes();
                while !remaining_bytes.is_empty() {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    assert!(
                        !remaining.is_zero(),
                        "preflight response exceeded wall bound"
                    );
                    stream
                        .set_write_timeout(Some(remaining.min(Duration::from_secs(2))))
                        .unwrap();
                    let written = stream.write(remaining_bytes).unwrap();
                    assert!(written > 0, "preflight response made no progress");
                    remaining_bytes = &remaining_bytes[written..];
                }
            }
        });
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut environment = BTreeMap::new();
            for name in [
                "PATH",
                "SystemRoot",
                "SYSTEMROOT",
                "WINDIR",
                "LD_LIBRARY_PATH",
                "DYLD_LIBRARY_PATH",
                "DYLD_FALLBACK_LIBRARY_PATH",
                "PYTHONHOME",
            ] {
                if let Ok(value) = std::env::var(name) {
                    environment.insert(name.into(), value);
                }
            }
            environment.extend([
                ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
                ("GENERAL_BOT_TOKEN".into(), "test-only-token".into()),
                ("NUMERA_TEST_API_BASE".into(), base),
            ]);
            let environment =
                _native::tooling_environment::environment(&scratch, &environment, false).unwrap();
            let log = _native::logging::RunLog::open(
                &scratch,
                "preflight-test-owner",
                vec!["test-only-token".into()],
            );
            for (index, name) in ["missing", "empty", "accepted", "refused", "unknown"]
                .iter()
                .enumerate()
            {
                if *name == "empty" || *name == "accepted" {
                    let mut catalog =
                        Catalog::open(&data.join("catalog.db"), "fixture", &scratch).unwrap();
                    catalog
                        .inclusion(&if *name == "empty" {
                            vec![key.into()]
                        } else {
                            vec![]
                        })
                        .unwrap();
                }
                if *name == "accepted" {
                    let art = root.join("assets/numera-emoji-mapper-logo.png");
                    assert_eq!(
                        _native::tooling_manifest::digest(&art).unwrap(),
                        "6f85376de3893dfa1e78045ce264103b0055acec400dd8a35222405d5c4f6951"
                    );
                    std::fs::copy(&art, &image).unwrap();
                    assert_eq!(
                        _native::tooling_manifest::digest(&image).unwrap(),
                        _native::tooling_manifest::digest(&art).unwrap()
                    );
                }
                if *name == "refused" {
                    *response.lock().unwrap() =
                        json!({"ok":false,"description":"Bad Request: wrong file type"});
                } else if *name == "unknown" {
                    *response.lock().unwrap() = json!({"ok":true});
                }
                received.lock().unwrap().clear();
                let args = command(&executable, &data, "--preflight");
                let completed = runtime
                    .block_on(tooling_process::capture(
                        &scratch,
                        &args,
                        &environment,
                        Duration::from_secs(5),
                        Duration::from_secs(3),
                        &log,
                    ))
                    .unwrap();
                let expected = &reports["reports"][index];
                assert_eq!(expected["name"], *name);
                assert_eq!(
                    completed.code,
                    expected["code"].as_i64().unwrap() as i32,
                    "{}",
                    completed.stderr
                );
                assert_eq!(
                    completed.stdout,
                    expected["stdout"].as_str().unwrap(),
                    "{name}"
                );
                let methods = received.lock().unwrap().clone();
                assert_eq!(
                    methods,
                    if index < 2 {
                        vec!["getMe"]
                    } else {
                        vec!["getMe", "uploadStickerFile"]
                    }
                );
                assert!(!data.join("publish_fixture.json").exists());
                assert!(!data.join("publish_plan_fixture.json").exists());
                {
                    let mut catalog =
                        Catalog::open(&data.join("catalog.db"), "fixture", &scratch).unwrap();
                    assert_eq!(
                        catalog
                            .dispatch(json!({"operation":"published_keys"}))
                            .unwrap(),
                        json!([])
                    );
                }
                if index >= 2 {
                    assert!(image.is_file(), "preflight removed its input: {name}");
                    assert_eq!(
                        _native::tooling_manifest::digest(&image).unwrap(),
                        "6f85376de3893dfa1e78045ce264103b0055acec400dd8a35222405d5c4f6951"
                    );
                } else {
                    assert!(!image.exists());
                }
                assert!(
                    std::fs::read_dir(scratch.join("logs/tmp/tooling"))
                        .unwrap()
                        .next()
                        .is_none()
                );
            }
            // Original corrupt-resume guard: catalog must not be opened/migrated first.
            received.lock().unwrap().clear();
            let corrupt = scratch.join("corrupt");
            std::fs::create_dir(&corrupt).unwrap();
            let database = corrupt.join("catalog.db");
            let db = rusqlite::Connection::open(&database).unwrap();
            db.execute_batch("CREATE TABLE untouched(value TEXT)")
                .unwrap();
            db.close().unwrap();
            let before = std::fs::read(&database).unwrap();
            let state = corrupt.join("publish_fixture.json");
            let plan = corrupt.join("publish_plan_fixture.json");
            let bad_shape = json!({"base":"fixture","sets":[{
                "fmt":"mixed","index":1,"name":"fixture1_by_YourEmojiBot",
                "title":"Pack 1","live":1,"logo":false,"keys":["s:first","s:second"]
            }],"sent":[]})
            .to_string();
            for bytes in [
                b"{truncated".as_slice(),
                br#"{"base": "fixture", "sets": [{"name": "pks1"#.as_slice(),
                bad_shape.as_bytes(),
            ] {
                std::fs::write(&state, bytes).unwrap();
                let completed = runtime
                    .block_on(tooling_process::capture(
                        &scratch,
                        &command(&executable, &corrupt, "--dry-run"),
                        &environment,
                        Duration::from_secs(5),
                        Duration::from_secs(3),
                        &log,
                    ))
                    .unwrap();
                assert_eq!(completed.code, 4, "{}", completed.stderr);
                assert_eq!(std::fs::read(&database).unwrap(), before);
                assert_eq!(std::fs::read(&state).unwrap(), bytes);
                assert!(!plan.exists(), "invalid state must not refreeze the plan");
                assert!(received.lock().unwrap().is_empty());
            }
            let invalid_plan = scratch.join("invalid plan");
            {
                let catalog =
                    Catalog::open(&invalid_plan.join("catalog.db"), "fixture", &scratch).unwrap();
                drop(catalog);
            }
            let frozen = invalid_plan.join("publish_plan_fixture.json");
            for bytes in [
                b"{oops".as_slice(),
                br#"{"static": ["s:a""#.as_slice(),
                br#"{"mixed":"s:a"}"#.as_slice(),
            ] {
                std::fs::write(&frozen, bytes).unwrap();
                let completed = runtime
                    .block_on(tooling_process::capture(
                        &scratch,
                        &command(&executable, &invalid_plan, "--dry-run"),
                        &environment,
                        Duration::from_secs(5),
                        Duration::from_secs(3),
                        &log,
                    ))
                    .unwrap();
                assert_eq!(completed.code, 4, "{}", completed.stderr);
                assert_eq!(std::fs::read(&frozen).unwrap(), bytes);
                assert!(!invalid_plan.join("publish_fixture.json").exists());
                assert!(received.lock().unwrap().is_empty());
            }

            // The saved queue survives, but inclusion and this family's publication still filter it.
            let dry = scratch.join("dry run");
            let mut catalog = Catalog::open(&dry.join("catalog.db"), "fixture", &scratch).unwrap();
            for key in ["a", "b", "c"] {
                catalog.insert(&json!({"content_key":key,"fmt":"static","file_path":scratch.join(format!("{key}.png"))}), "fixture").unwrap();
            }
            catalog.inclusion(&["b".into()]).unwrap();
            catalog.uploaded(&json!({"content_key":"a","custom_emoji_id":"111111111","base":"fixture","set_name":"fixture1"}), "fixture").unwrap();
            drop(catalog);
            let frozen = dry.join("publish_plan_fixture.json");
            let saved = b"{\"mixed\":[\"a\",\"b\"]}\n";
            std::fs::write(&frozen, saved).unwrap();
            let completed = runtime
                .block_on(tooling_process::capture(
                    &scratch,
                    &command(&executable, &dry, "--dry-run"),
                    &environment,
                    Duration::from_secs(5),
                    Duration::from_secs(3),
                    &log,
                ))
                .unwrap();
            assert_eq!(completed.code, 0, "{}", completed.stderr);
            assert!(
                completed
                    .stdout
                    .contains("mixed: 1 emoji -> 1 set(s) of up to 200")
            );
            assert_eq!(std::fs::read(&frozen).unwrap(), saved);
            assert!(!dry.join("publish_fixture.json").exists());
            assert!(received.lock().unwrap().is_empty());
            let mut catalog = Catalog::open(&dry.join("catalog.db"), "fixture", &scratch).unwrap();
            assert_eq!(
                catalog
                    .dispatch(json!({"operation":"published","base":"fixture","content_key":"a"}))
                    .unwrap(),
                json!(true)
            );
            assert_eq!(
                catalog
                    .dispatch(json!({"operation":"published","base":"fixture","content_key":"c"}))
                    .unwrap(),
                json!(false)
            );
            assert!(
                std::fs::read_dir(scratch.join("logs/tmp/tooling"))
                    .unwrap()
                    .next()
                    .is_none()
            );
            log.finish(0);
        }));
        let _ = stop.send(());
        let stopped = server.join();
        if let Err(error) = outcome {
            std::panic::resume_unwind(error);
        }
        stopped.unwrap();
    }));
    // A failed child finalization retains its owner marker under tooling/. Never erase that
    // recovery evidence while unwinding an assertion, even when Windows permits tree removal.
    match std::fs::read_dir(scratch.join("logs/tmp/tooling")) {
        Ok(mut entries) => assert!(
            entries.next().is_none(),
            "preflight child ownership unresolved; fixture retained for recovery"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("cannot verify preflight child cleanup; fixture retained: {error}"),
    }
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    std::fs::remove_dir_all(&scratch).unwrap();
}
