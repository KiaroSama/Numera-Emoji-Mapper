//! Original redaction assertions and file-log lifetime; all credentials here are synthetic.
use _native::logging::{RunLog, redact};
use serde_json::Value;

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

#[test]
fn original_redaction_and_unicode_boundary_contracts() {
    let token = "1234567890:AAxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    assert_eq!(
        redact(&format!("using {token} now"), &[]),
        "using [REDACTED] now"
    );
    assert_eq!(
        redact(&format!("GET /bot{token}/getMe"), &[]),
        "GET /bot[REDACTED]/getMe"
    );
    let secret = "00000000000000000000000000000000".to_owned();
    assert_eq!(
        redact(&format!("key={secret}"), &[secret]),
        "key=[REDACTED]"
    );
    assert_eq!(redact("abc", &[]), "abc");
    let identities = "key s:01f6c1284261975d0000934b7fe6f3c3 cid 111111111";
    assert_eq!(redact(identities, &[]), identities);
    let suffix = "ABCDEFGHIJKLMNOPQRSTUVWXYZ012";
    assert_eq!(
        redact(
            &format!("POST https://api.telegram.org/bot123456789:{suffix}"),
            &[]
        ),
        "POST https://api.telegram.org/[REDACTED]"
    );
    for digits in ["۱۲۳۴۵۶", "１２３４５６", "123456"] {
        assert_eq!(redact(&format!("{digits}:{suffix}"), &[]), "[REDACTED]");
        assert_eq!(redact(&format!("bot{digits}:{suffix}"), &[]), "[REDACTED]");
    }
    assert_eq!(
        redact(&format!("123456:{}", "A".repeat(20)), &[]),
        "[REDACTED]"
    );
    assert_eq!(
        redact(&format!("123456:{}", "A".repeat(19)), &[]),
        format!("123456:{}", "A".repeat(19))
    );
    for prefix in ["x", "_", "-"] {
        let input = format!("{prefix}123456:{suffix}");
        // Retain the existing conservative mask; historical lookbehind parity stays unresolved.
        assert_eq!(redact(&input, &[]), format!("{prefix}[REDACTED]"));
    }
    // Literal registration remains stronger than token recognition, even next to ASCII text.
    assert_eq!(redact(&format!("x{token}"), &[token.into()]), "x[REDACTED]");
}

#[test]
fn original_committed_secret_policy_reports_keys_and_paths_without_values() {
    let root = Root::new("native tracked secret policy é");
    let value = "synthetic-never-issued-credential";
    let configuration = format!(
        "# synthetic only\nTOKEN = \"{value}\"\u{85}PUBLIC_CONFIG={value}\nAPI_KEY=short\n"
    );
    std::fs::write(
        root.0.join("tracked é.txt"),
        format!("before\u{fffd}{value}after"),
    )
    .unwrap();
    std::fs::write(root.0.join("untracked.txt"), value).unwrap();
    std::fs::write(root.0.join("large.txt"), value).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(root.0.join("large.txt"))
        .unwrap()
        .set_len(2 * 1024 * 1024 + 1)
        .unwrap();
    std::fs::create_dir(root.0.join("directory.txt")).unwrap();
    let tracked = ["tracked é.txt", "large.txt", "missing.txt", "directory.txt"].map(String::from);
    let keys = ["TOKEN", "API_KEY"].map(String::from);
    let leaks = _native::tracked_secrets::scan(&root.0, &configuration, &keys, &tracked).unwrap();
    assert_eq!(leaks, vec![("TOKEN".into(), "tracked é.txt".into())]);
    let inherited = std::env::vars().collect();
    let mut environment = _native::tooling_environment::environment(
        &root.0,
        &_native::tooling_test_runner::scrubbed_environment(&inherited),
        false,
    )
    .unwrap();
    let git_config = root.0.join("empty-git-config");
    std::fs::write(&git_config, b"").unwrap();
    for key in ["GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM"] {
        environment.insert(key.into(), git_config.to_str().unwrap().into());
    }
    let template = root.0.join("empty-template");
    std::fs::create_dir(&template).unwrap();
    environment.insert("GIT_TEMPLATE_DIR".into(), template.to_str().unwrap().into());
    let log = RunLog::open(&root.0, "tracked-secret-fixture", vec![value.into()]);
    let git = _native::codec_process::executable("git").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for arguments in [
        vec!["init", "--quiet"],
        vec!["add", "--", "tracked é.txt", "large.txt"],
    ] {
        let mut argv = vec![git.clone().into_os_string()];
        argv.extend(arguments.into_iter().map(std::ffi::OsString::from));
        let completed = runtime
            .block_on(_native::tooling_process::capture(
                &root.0,
                &argv,
                &environment,
                std::time::Duration::from_secs(8),
                std::time::Duration::from_secs(4),
                &log,
            ))
            .unwrap();
        assert_eq!(completed.code, 0, "{}", completed.stderr);
    }
    let enumerated = runtime
        .block_on(_native::tracked_secrets::tracked(&root.0))
        .unwrap();
    assert_eq!(enumerated, ["large.txt", "tracked é.txt"]);
    assert_eq!(
        _native::tracked_secrets::scan(&root.0, &configuration, &keys, &enumerated).unwrap(),
        leaks
    );
    drop(log);
    std::fs::OpenOptions::new()
        .write(true)
        .open(root.0.join("large.txt"))
        .unwrap()
        .set_len(2 * 1024 * 1024)
        .unwrap();
    assert_eq!(
        _native::tracked_secrets::scan(&root.0, &configuration, &keys, &["large.txt".into()])
            .unwrap(),
        vec![("TOKEN".into(), "large.txt".into())]
    );
    assert!(!format!("{leaks:?}").contains(value));
    assert!(
        _native::tracked_secrets::scan(&root.0, &configuration, &keys, &["../outside".into()])
            .unwrap_err()
            .contains("confined")
    );
    assert_eq!(
        std::fs::read_to_string(root.0.join("untracked.txt")).unwrap(),
        value
    );
    #[cfg(unix)]
    {
        let linked = root.0.join("linked.txt");
        std::os::unix::fs::symlink(root.0.join("untracked.txt"), &linked).unwrap();
        assert!(
            _native::tracked_secrets::scan(&root.0, &configuration, &keys, &["linked.txt".into()])
                .is_err()
        );
        std::fs::remove_file(linked).unwrap();
    }
    root.clean().unwrap();
}

#[test]
fn original_retention_is_resolved_per_call_with_safe_numeric_failures() {
    use _native::logging::retention_days;
    for (raw, days, warning) in [
        (None, 30, None),
        (Some(""), 30, None),
        (Some("7"), 7, None),
        (Some("0"), 0, None),
        (Some("  +۷  "), 7, None),
        (Some("1_0"), 10, None),
        (Some("-۰"), 0, None),
        (Some("\u{1c}+۷\u{1f}"), 7, None),
        (
            Some("-1"),
            0,
            Some("WARNING: EMOJI_LOG_RETENTION_DAYS=-1 below 0; using 0.\n"),
        ),
        (
            Some("not-a-number"),
            30,
            Some("WARNING: EMOJI_LOG_RETENTION_DAYS is not a whole number; using 30.\n"),
        ),
    ] {
        let actual = retention_days(raw).unwrap();
        assert_eq!(actual.0, days);
        assert_eq!(actual.1.as_deref(), warning);
    }
    for raw in ["_1", "1_", "1__0", "0x10", "²", "1\0", "+", "- 1"] {
        let actual = retention_days(Some(raw)).unwrap();
        assert_eq!(actual.0, 30);
        assert!(actual.1.unwrap().contains("not a whole number"));
    }
    assert_eq!(retention_days(Some(&"0".repeat(4300))).unwrap(), (0, None));
    assert!(retention_days(Some(&"0".repeat(4301))).unwrap().1.is_some());
    assert_eq!(
        retention_days(Some("-۰۰۹")).unwrap().1.as_deref(),
        Some("WARNING: EMOJI_LOG_RETENTION_DAYS=-9 below 0; using 0.\n")
    );
    assert!(
        retention_days(Some("18446744073709551616"))
            .unwrap_err()
            .contains("refusing pruning")
    );
}

#[test]
fn original_log_age_selection_preserves_recent_foreign_and_linked_files() {
    use std::fs::{FileTimes, OpenOptions};
    use std::time::{Duration, SystemTime};
    let root = Root::new("native retention é");
    let directory = root.0.join("logs");
    std::fs::create_dir(&directory).unwrap();
    let now = SystemTime::now();
    for (name, days) in [
        ("old.log", 90),
        ("new.log", 1),
        ("caller.txt", 90),
        ("upper.LOG", 90),
        (".log", 90),
    ] {
        let path = directory.join(name);
        std::fs::write(&path, b"synthetic retained bytes").unwrap();
        OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(FileTimes::new().set_modified(now - Duration::from_secs(days * 86400)))
            .unwrap();
    }
    std::fs::create_dir(directory.join("directory.log")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(directory.join("caller.txt"), directory.join("linked.log")).unwrap();
    assert_eq!(_native::logging::prune_old_logs(&root.0, 0).unwrap(), 0);
    assert!(directory.join("old.log").is_file());
    assert_eq!(
        _native::logging::prune_old_logs(&root.0, 30).unwrap(),
        if cfg!(windows) { 3 } else { 2 }
    );
    assert!(!directory.join("old.log").exists());
    assert!(!directory.join(".log").exists());
    assert_eq!(
        std::fs::read(directory.join("new.log")).unwrap(),
        b"synthetic retained bytes"
    );
    assert_eq!(
        std::fs::read(directory.join("caller.txt")).unwrap(),
        b"synthetic retained bytes"
    );
    assert!(directory.join("directory.log").is_dir());
    assert_eq!(directory.join("upper.LOG").exists(), !cfg!(windows));
    #[cfg(unix)]
    {
        assert!(
            std::fs::symlink_metadata(directory.join("linked.log"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        std::fs::remove_file(directory.join("linked.log")).unwrap();
    }
    assert!(_native::logging::prune_old_logs(&root.0, u64::MAX).is_err());
    root.clean().unwrap();
}

#[test]
fn configured_retention_prunes_only_the_selected_fixture_at_call_time() {
    use std::fs::{FileTimes, OpenOptions};
    use std::time::{Duration, SystemTime};
    let root = Root::new("native configured retention é");
    let directory = root.0.join("logs");
    std::fs::create_dir(&directory).unwrap();
    let old = directory.join("old.log");
    std::fs::write(&old, b"synthetic old log").unwrap();
    OpenOptions::new()
        .write(true)
        .open(&old)
        .unwrap()
        .set_times(
            FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(90 * 86400)),
        )
        .unwrap();
    let disabled = RunLog::open(&root.0, "retention-disabled", vec![]);
    disabled.apply_retention(&root.0, "0");
    assert!(old.is_file());
    drop(disabled);
    let enabled = RunLog::open(&root.0, "retention-enabled", vec![]);
    enabled.apply_retention(&root.0, "30");
    assert!(!old.exists());
    enabled.finish(0);
    drop(enabled);
    root.clean().unwrap();
}

#[test]
fn copied_public_command_resolves_retention_without_operator_data() {
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::fs::{FileTimes, OpenOptions};
    use std::time::{Duration, SystemTime};
    let root = Root::new("native logging cli é");
    std::fs::create_dir(root.0.join("assets")).unwrap();
    std::fs::write(root.0.join("assets/panel.html"), b"fixture root marker").unwrap();
    std::fs::write(
        root.0.join("pyproject.toml"),
        b"[project]\nname = 'fixture'\n",
    )
    .unwrap();
    let executable = root.0.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let inherited = std::env::vars().collect::<BTreeMap<_, _>>();
    let scrubbed = _native::tooling_test_runner::scrubbed_environment(&inherited);
    let mut environment =
        _native::tooling_environment::environment(&root.0, &scrubbed, false).unwrap();
    let log = RunLog::open(&root.0, "logging-cli-owner", vec![]);
    let old = root.0.join("logs/old.log");
    std::fs::write(&old, b"synthetic old log").unwrap();
    OpenOptions::new()
        .write(true)
        .open(&old)
        .unwrap()
        .set_times(
            FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(90 * 86400)),
        )
        .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for (days, argument, code) in [("0", "--help", 0), ("30", "--invalid-logging-fixture", 2)] {
        environment.insert("EMOJI_LOG_RETENTION_DAYS".into(), days.into());
        let completed = runtime
            .block_on(_native::tooling_process::capture(
                &root.0,
                &[
                    executable.clone().into_os_string(),
                    OsString::from("status"),
                    OsString::from(argument),
                ],
                &environment,
                Duration::from_secs(8),
                Duration::from_secs(4),
                &log,
            ))
            .unwrap();
        assert_eq!(completed.code, code, "{}", completed.stderr);
        assert_eq!(old.exists(), days == "0");
        if code == 0 {
            assert!(completed.stdout.contains("Usage: numera-emoji status"));
        } else {
            assert!(completed.stderr.contains("status takes no options"));
        }
    }
    let mut command_logs = Vec::new();
    for entry in std::fs::read_dir(root.0.join("logs")).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("status_")
        {
            command_logs.push(std::fs::read_to_string(path).unwrap());
        }
    }
    assert_eq!(command_logs.len(), 2);
    assert!(
        command_logs
            .iter()
            .any(|text| text.contains("exit=0 duration_ms="))
    );
    assert!(
        command_logs
            .iter()
            .any(|text| text.contains("exit=2 duration_ms=") && text.contains("pruned_logs=1"))
    );
    let credential = "synthetic-unlisted-credential-value";
    environment.insert("SOME_OTHER_CREDENTIAL".into(), credential.into());
    let worker_secrets = [
        ("WORKER_PUBLISH_SECRET", "synthetic-worker-publish-value"),
        ("GENERAL_WEBHOOK_SECRET", "synthetic-general-webhook-value"),
        ("COIN_WEBHOOK_SECRET", "synthetic-coin-webhook-value"),
    ];
    for (key, value) in worker_secrets {
        environment.insert(key.into(), value.into());
    }
    let rejected = std::iter::once(credential)
        .chain(worker_secrets.iter().map(|(_, value)| *value))
        .collect::<Vec<_>>()
        .join("|");
    let completed = runtime
        .block_on(_native::tooling_process::capture(
            &root.0,
            &[
                executable.into_os_string(),
                OsString::from("build-pack"),
                OsString::from("--token-env"),
                OsString::from("SOME_OTHER_CREDENTIAL"),
                OsString::from(rejected),
            ],
            &environment,
            Duration::from_secs(8),
            Duration::from_secs(4),
            &log,
        ))
        .unwrap();
    assert_eq!(completed.code, 2);
    assert!(
        completed
            .stderr
            .contains("unknown build-pack option: [REDACTED]")
    );
    assert!(!completed.stdout.contains(credential) && !completed.stderr.contains(credential));
    let mut build_logs = Vec::new();
    for entry in std::fs::read_dir(root.0.join("logs")).unwrap() {
        let path = entry.unwrap().path();
        if path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("build-pack_")
        {
            build_logs.push(std::fs::read_to_string(path).unwrap());
        }
    }
    assert_eq!(build_logs.len(), 1);
    assert!(build_logs[0].contains("unknown build-pack option: [REDACTED]"));
    assert!(!build_logs[0].contains(credential));
    assert!(build_logs[0].contains("[REDACTED]|[REDACTED]|[REDACTED]|[REDACTED]"));
    for (_, value) in worker_secrets {
        assert!(!completed.stdout.contains(value) && !completed.stderr.contains(value));
        assert!(!build_logs[0].contains(value));
    }
    assert_eq!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    for absent in [".env", "collection", "coins"] {
        assert!(!root.0.join(absent).exists());
    }
    drop(log);
    root.clean().unwrap();
}

#[test]
fn unavailable_log_directory_preserves_existing_bytes_and_does_not_claim_a_file() {
    let root = Root::new("native logging refusal é");
    let obstruction = root.0.join("logs");
    std::fs::write(&obstruction, b"existing caller bytes").unwrap();
    let log = RunLog::open(&root.0, "logging-refusal", vec![]);
    assert!(log.path.is_none());
    log.event("WARNING", "fixture", "controlled unavailable log directory");
    log.finish(1);
    drop(log);
    assert_eq!(
        std::fs::read(&obstruction).unwrap(),
        b"existing caller bytes"
    );
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    // Root's child-ownership probe requires a directory; remove only this test-owned obstruction.
    std::fs::remove_file(obstruction).unwrap();
    root.clean().unwrap();
}

#[test]
fn original_script_labels_stay_in_the_log_directory_without_changing_allowed_characters() {
    let root = Root::new("native log labels é");
    for (label, expected) in [
        ("", "script"),
        ("  ", "script"),
        ("collect.py", "collect.py"),
        ("__name__", "name"),
        ("../outside", ".._outside"),
        ("foo/bar\\baz : 😀.rs", "foo_bar_baz_.rs"),
        ("a__b", "a__b"),
        ("😀😀alphaسلامbeta", "alpha_beta"),
        ("._-", "._-"),
        ("\0\n", "script"),
    ] {
        let log = RunLog::open(&root.0, label, vec![]);
        let path = log.path.clone().expect("sanitized label must create a log");
        assert_eq!(path.parent().unwrap(), root.0.join("logs"));
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(&format!("{expected}_"))
        );
        assert_eq!(log.finish(0), 0);
        drop(log);
    }
    assert_eq!(std::fs::read_dir(root.0.join("logs")).unwrap().count(), 10);
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    root.clean().unwrap();
}

#[test]
fn distinct_utf8_utc_files_redact_before_writing_and_close_after_drop() {
    let root = Root::new("native logging é");
    let token = "1234567890:AAxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    let literal = "synthetic-registered-log-value";
    let first = RunLog::open(&root.0, "logging-parity", vec![literal.into()]);
    let first_path = first.path.clone().expect("actual file log must be created");
    first.event("INFO", "fixture", &format!("normal é 😀 {literal}"));
    first.event("WARNING", "fixture", &format!("controlled warning {token}"));
    first.event("ERROR", "fixture", "controlled failure");
    first.event("CRITICAL", "fixture", "controlled critical");
    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                for level in ["WARNING", "ERROR", "CRITICAL"] {
                    first.event(level, "fixture", "concurrent diagnostic");
                }
            });
        }
    });
    assert_eq!(first.finish(3), 3);
    assert_eq!(first.finish(0), 0);
    drop(first);
    let original = std::fs::read(&first_path).unwrap();
    let text = std::str::from_utf8(&original).unwrap();
    assert!(text.contains("normal é 😀 [REDACTED]"));
    assert!(text.contains("[WARNING] [fixture] controlled warning [REDACTED]"));
    assert!(text.contains("[ERROR] [fixture] controlled failure"));
    assert!(text.contains("[ERROR] [runtime] exit=3 duration_ms="));
    assert!(text.contains("[CRITICAL] [fixture] controlled critical"));
    let summaries = text
        .lines()
        .filter(|line| line.contains("[runtime] exit="))
        .collect::<Vec<_>>();
    assert_eq!(summaries.len(), 2);
    assert!(summaries[0].contains("outcome=FAILED(3) warnings=3 errors=3 critical=3"));
    assert!(summaries[1].contains("outcome=OK warnings=3 errors=3 critical=3"));
    assert!(!text.contains(token) && !text.contains(literal));
    let run_id = text.lines().next().unwrap().rsplit_once(" run=").unwrap().1;
    assert!(!run_id.is_empty());
    for line in text.lines() {
        assert!(line.ends_with(&format!(" run={run_id}")));
        assert!(line.starts_with('['));
        let timestamp = line.split_once(']').unwrap().0.trim_start_matches('[');
        chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S UTC").unwrap();
    }
    let name = first_path.file_name().unwrap().to_str().unwrap();
    assert!(
        name.starts_with("logging-parity_") && name.contains("_UTC_") && name.ends_with(".log")
    );
    let second = RunLog::open(&root.0, "logging-parity", vec![]);
    let second_path = second.path.clone().unwrap();
    assert_ne!(first_path, second_path);
    assert_eq!(second.finish(0), 0);
    drop(second);
    let second_text = std::fs::read_to_string(&second_path).unwrap();
    let second_id = second_text
        .lines()
        .next()
        .unwrap()
        .rsplit_once(" run=")
        .unwrap()
        .1;
    assert_ne!(run_id, second_id);
    assert_eq!(std::fs::read(&first_path).unwrap(), original);
    assert_eq!(std::fs::read_dir(root.0.join("logs")).unwrap().count(), 2);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        for path in [&first_path, &second_path] {
            drop(
                std::fs::OpenOptions::new()
                    .write(true)
                    .share_mode(0)
                    .open(path)
                    .unwrap(),
            );
        }
    }
    // Fixtures are private; provenance does not claim that the original formatter was executed.
    let source: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    assert!(
        source["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["source"] == "tests/test_logsetup.py"
                && entry["sha256"]
                    == "93a6bfb0cb7fea9679dfbab3014aaa1e9c273d59235c4c807e2e83a1cd8c4405")
    );
    root.clean().unwrap();
}
