//! Original redaction results share the pinned corpus and existing bounded worker.
use _native::source_git_inventory::Source;
use serde_json::json;
use std::path::Path;

#[path = "tooling_fixture.rs"]
mod tooling_fixture;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = ["emojikit/logsetup.py", "tests/test_logsetup.py"]
        .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    let unavailable = root.join("logs/tmp/not-a-logging-oracle-executable");
    for selected in 0..sources.len() {
        for mutation in ["revision", "path", "claimed hash", "actual bytes"] {
            let original = sources[selected];
            let mut forged = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => forged.baseline_revision = "0".repeat(40),
                "path" => forged.path = "tests/unverified_logging.py".into(),
                "claimed hash" => forged.sha256 = "0".repeat(64),
                "actual bytes" => forged
                    .text
                    .push_str("\nraise RuntimeError('must never execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[selected] = &forged;
            assert!(
                _native::source_logging_oracle::generate(admitted[0], admitted[1])
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::generate_logging(
                        root,
                        &unavailable,
                        admitted[0],
                        admitted[1],
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            let missing = root.join("logs/tmp/forged-secret-scan-must-not-create");
            assert!(!missing.exists());
            assert!(
                _native::source_logging_secrets::generate(root, &missing, admitted[0], admitted[1])
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::generate_logging_secrets(
                        root,
                        &unavailable,
                        &missing,
                        admitted[0],
                        admitted[1]
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let report = runtime
        .block_on(_native::source_oracle_worker::generate_logging(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            sources[0],
            sources[1],
        ))
        .unwrap();
    assert_eq!(report["baseline_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["source_hash"], sources[0].sha256);
    assert_eq!(report["test_source_hash"], sources[1].sha256);
    assert_eq!(report["tests_run"], 7);
    assert_eq!(
        report["original_methods"],
        json!([
            "TestRedaction.test_token_shaped_is_masked",
            "TestRedaction.test_bot_url_is_masked",
            "TestRedaction.test_registered_secret_value_is_masked",
            "TestRedaction.test_short_value_not_registered",
            "TestRedaction.test_content_key_and_ids_preserved",
            "TestRedaction.test_a_token_glued_to_bot_is_masked",
            "TestRedaction.test_the_worker_secrets_are_masked_by_name",
        ])
    );
    let secret_keys = report["secret_keys"].as_array().unwrap();
    assert_eq!(secret_keys.len(), 9);
    for key in [
        "WORKER_PUBLISH_SECRET",
        "GENERAL_WEBHOOK_SECRET",
        "COIN_WEBHOOK_SECRET",
    ] {
        assert!(secret_keys.iter().any(|value| value == key));
    }
    let observations = report["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 27);
    let mut names = std::collections::BTreeSet::new();
    let mut exact = 0;
    let mut conservative = 0;
    for case in observations {
        let name = case["name"].as_str().unwrap();
        assert!(names.insert(name), "duplicate source recipe: {name}");
        let input = case["input"].as_str().unwrap();
        let secrets = case["secrets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let expected = case["output"].as_str().unwrap();
        let actual = _native::logging::redact(input, &secrets);
        match case["parity"].as_bool().unwrap() {
            true => {
                assert_eq!(actual, expected, "{name}");
                exact += 1;
            }
            false => {
                // Keep the known source difference visible; do not weaken secret masking for parity.
                assert!(matches!(
                    name,
                    "conservative-boundary-x"
                        | "conservative-boundary-_"
                        | "conservative-boundary--"
                ));
                assert_eq!(expected, input, "historical lookbehind changed: {name}");
                assert_eq!(
                    actual,
                    format!("{}[REDACTED]", input.chars().next().unwrap())
                );
                assert_ne!(actual, expected);
                conservative += 1;
            }
        }
    }
    assert_eq!((exact, conservative), (24, 3));
    check_formatting(&report["formatting"]);
    check_retention(root, runtime, corpus);
    check_secrets(root, runtime, sources);
}

fn check_formatting(report: &serde_json::Value) {
    assert_eq!(report["tests_run"], 2);
    assert_eq!(
        report["original_methods"],
        json!([
            "TestRedaction.test_formatter_redacts_record",
            "TestRunOutcome.test_exit_code_is_recorded_and_returned",
        ])
    );
    let root = tooling_fixture::Root::new("original logging format é");
    let log = _native::logging::RunLog::open(&root.0, "format-source", vec![]);
    let path = log.path.clone().unwrap();
    log.event("INFO", "fixture", report["message"].as_str().unwrap());
    let outcomes = report["outcomes"].as_array().unwrap();
    assert_eq!(outcomes.len(), 2);
    for (outcome, code) in outcomes.iter().zip([0, 3]) {
        assert_eq!(outcome["input"], code);
        assert_eq!(outcome["returned"], code);
        assert_eq!(outcome["recorded"], code);
        assert_eq!(log.finish(code), code);
    }
    drop(log);
    let text = std::fs::read_to_string(path).unwrap();
    let payload = report["formatted"]
        .as_str()
        .unwrap()
        .strip_prefix("[INFO] [deadbeef] ")
        .unwrap();
    assert_eq!(payload, "token [REDACTED]");
    assert!(text.contains(&format!("[INFO] [fixture] {payload} run=")));
    assert!(!text.contains(report["message"].as_str().unwrap()));
    for code in [0, 3] {
        assert!(text.contains(&format!("exit={code} duration_ms=")));
    }
    let summary = &report["summary"];
    assert_eq!(
        summary["counts"],
        json!({"WARNING":2,"ERROR":1,"CRITICAL":1})
    );
    assert_eq!(summary["shutdown_calls"], 2);
    assert_eq!(summary["duration_seconds"], 2.5);
    let log = _native::logging::RunLog::open(&root.0, "summary-source", vec![]);
    let path = log.path.clone().unwrap();
    for level in summary["levels"].as_array().unwrap() {
        log.event(level.as_str().unwrap(), "fixture", "diagnostic event");
    }
    for (observation, code) in summary["summaries"].as_array().unwrap().iter().zip([0, 3]) {
        assert_eq!(observation["code"], code);
        assert_eq!(log.finish(code), code);
    }
    drop(log);
    let text = std::fs::read_to_string(path).unwrap();
    assert_eq!(summary["summaries"].as_array().unwrap().len(), 2);
    for (observation, code) in summary["summaries"].as_array().unwrap().iter().zip([0, 3]) {
        let original = observation["text"].as_str().unwrap();
        assert!(original.contains("finished in 2.50s"));
        let counts = original
            .split_once(" | warnings=")
            .unwrap()
            .1
            .strip_suffix(" ===\n")
            .unwrap();
        let outcome = original
            .split_once(" | ")
            .unwrap()
            .1
            .split_once(" | exit=")
            .unwrap()
            .0;
        let actual = text
            .lines()
            .find(|line| line.contains(&format!("[runtime] exit={code} ")))
            .unwrap();
        assert!(actual.contains(&format!("outcome={outcome} warnings={counts}")));
    }
    let exceptions = summary["exceptions"].as_array().unwrap();
    assert_eq!(exceptions.len(), 5);
    for (case, mode) in exceptions.iter().zip([
        "ordinary",
        "thread",
        "unnamed-thread",
        "traceback",
        "keyboard-interrupt",
    ]) {
        assert_eq!(case["mode"], mode);
        let logged = case["logged"].as_str().unwrap();
        let stderr = case["stderr"].as_str().unwrap();
        assert!(!logged.contains("synthetic-never-issued-exception-credential"));
        assert!(!stderr.contains("synthetic-never-issued-exception-credential"));
        let token = report["message"]
            .as_str()
            .unwrap()
            .strip_prefix("token ")
            .unwrap();
        assert!(!logged.contains(token) && !stderr.contains(token));
        if mode == "keyboard-interrupt" {
            assert!(logged.is_empty() && stderr.is_empty());
            assert_eq!(case["previous_calls"], 1);
        } else {
            assert_eq!(case["previous_calls"], 0);
            assert!(logged.contains("[CRITICAL] [deadbeef] UNCAUGHT"));
            assert!(logged.contains("controlled [REDACTED] [REDACTED]"));
            match mode {
                "traceback" => {
                    for sink in [logged, stderr] {
                        assert!(sink.contains("Traceback (most recent call last):"));
                        assert!(sink.contains("in record_exit_code"));
                        assert!(
                            sink.contains("ValueError: invalid literal for int() with base 10:")
                        );
                        assert!(sink.contains("controlled [REDACTED] [REDACTED]"));
                    }
                }
                "ordinary" => {
                    assert!(stderr.contains("ValueError: controlled [REDACTED] [REDACTED]"))
                }
                "thread" => {
                    assert!(logged.contains("UNCAUGHT in thread fixture-thread: ValueError"));
                    assert!(stderr.is_empty());
                }
                _ => {
                    assert!(logged.contains("UNCAUGHT in thread ?: ValueError"));
                    assert!(stderr.is_empty());
                }
            }
        }
    }
    let labels = report["labels"].as_array().unwrap();
    assert_eq!(labels.len(), 10);
    for case in labels {
        let log = _native::logging::RunLog::open(&root.0, case["input"].as_str().unwrap(), vec![]);
        let path = log.path.clone().unwrap();
        assert_eq!(path.parent().unwrap(), root.0.join("logs"));
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(&format!("{}_", case["expected"].as_str().unwrap()))
        );
        drop(log);
    }
    // Compare count/outcome and exception-redaction semantics, not automatic hook installation.
    root.clean().unwrap();
}

fn check_secrets(root: &Path, runtime: &tokio::runtime::Runtime, sources: [&Source; 2]) {
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let workspace = parent.join(format!("original tracked secrets é {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let marker = workspace.join("caller.txt");
    std::fs::write(&marker, b"caller bytes survive refusal").unwrap();
    assert!(
        _native::source_logging_secrets::generate(root, &workspace, sources[0], sources[1])
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(&marker).unwrap(),
        b"caller bytes survive refusal"
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 1);
    assert!(
        _native::source_logging_secrets::generate(root, root, sources[0], sources[1])
            .unwrap_err()
            .contains("project-contained workspace")
    );
    std::fs::remove_file(marker).unwrap();
    let report = runtime
        .block_on(_native::source_oracle_worker::generate_logging_secrets(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            sources[0],
            sources[1],
        ))
        .unwrap();
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["source_hash"], sources[0].sha256);
    assert_eq!(report["test_source_hash"], sources[1].sha256);
    assert_eq!(
        report["original_method"],
        "TestNoCommittedSecrets.test_no_real_secret_is_committed"
    );
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    let keys = report["secret_keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|key| key.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), 9);
    let observations = report["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 3);
    for (case, (name, count)) in
        observations
            .iter()
            .zip([("clean", 0), ("leak", 1), ("unicode-duplicate", 2)])
    {
        assert_eq!(case["name"], name);
        assert_eq!(case["tests_run"], 1);
        assert_eq!(case["assertions"], 1);
        assert_eq!(case["deliberate_failures"], usize::from(count != 0));
        let bytes = case["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|byte| u8::try_from(byte.as_u64().unwrap()).unwrap())
            .collect::<Vec<_>>();
        std::fs::write(workspace.join("tracked é.txt"), bytes).unwrap();
        std::fs::write(
            workspace.join("untracked.txt"),
            b"synthetic-never-issued-credential",
        )
        .unwrap();
        let large = std::fs::File::create(workspace.join("large.txt")).unwrap();
        large.set_len(2 * 1024 * 1024 + 1).unwrap();
        drop(large);
        std::fs::create_dir(workspace.join("directory.txt")).unwrap();
        let tracked = case["tracked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|path| path.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let actual = _native::tracked_secrets::scan(
            &workspace,
            case["configuration"].as_str().unwrap(),
            &keys,
            &tracked,
        )
        .unwrap()
        .into_iter()
        .map(|(key, path)| format!("{key} -> {path}"))
        .collect::<Vec<_>>();
        assert_eq!(actual.len(), count);
        assert_eq!(json!(actual), case["leaks"]);
        for file in ["tracked é.txt", "untracked.txt", "large.txt"] {
            std::fs::remove_file(workspace.join(file)).unwrap();
        }
        std::fs::remove_dir(workspace.join("directory.txt")).unwrap();
    }
    // Failure leaves the source/native fixture intact; only a successful replay removes it.
    std::fs::remove_dir(workspace).unwrap();
}

fn check_retention(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "emojikit/logsetup.py",
        "tests/test_logsetup.py",
        "emojikit/cli_env.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("logging source refusal {}", std::process::id()));
    assert!(!missing.exists());
    for selected in 0..sources.len() {
        for mutation in ["revision", "path", "claimed hash", "actual bytes"] {
            let original = sources[selected];
            let mut forged = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => forged.baseline_revision = "0".repeat(40),
                "path" => forged.path = "tests/unverified_logging.py".into(),
                "claimed hash" => forged.sha256 = "0".repeat(64),
                "actual bytes" => forged
                    .text
                    .push_str("\nraise RuntimeError('must never execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[selected] = &forged;
            assert!(
                _native::source_logging_oracle::generate_retention(root, &missing, &admitted)
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::generate_logging_retention(
                        root,
                        &parent.join("not-a-logging-oracle-executable"),
                        &missing,
                        &admitted,
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!("original logging retention {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let retained = workspace.join("caller-data.txt");
    std::fs::write(&retained, b"caller bytes survive refusal").unwrap();
    assert!(
        _native::source_logging_oracle::generate_retention(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(&retained).unwrap(),
        b"caller bytes survive refusal"
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 1);
    assert!(
        _native::source_logging_oracle::generate_retention(root, root, &sources)
            .unwrap_err()
            .contains("project-contained workspace")
    );
    std::fs::remove_file(retained).unwrap();
    #[cfg(unix)]
    {
        let link = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            _native::source_logging_oracle::generate_retention(root, &link, &sources)
                .unwrap_err()
                .contains("linked/reparse")
        );
        std::fs::remove_file(link).unwrap();
    }
    let report = runtime
        .block_on(_native::source_oracle_worker::generate_logging_retention(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(report["baseline_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["source_hashes"].as_object().unwrap().len(), 3);
    for source in sources {
        assert_eq!(report["source_hashes"][&source.path], source.sha256);
    }
    assert_eq!(report["tests_run"], 6);
    assert_eq!(report["temporary_calls"], 2);
    assert_eq!(
        report["original_methods"],
        json!([
            "TestRetentionIsReadWhenItIsUsed.test_a_value_set_after_import_is_honoured",
            "TestRetentionIsReadWhenItIsUsed.test_prune_takes_its_default_at_call_time",
            "TestRetentionIsReadWhenItIsUsed.test_a_malformed_value_falls_back_instead_of_raising",
            "TestRetentionIsReadWhenItIsUsed.test_a_negative_value_cannot_mean_prune_everything",
            "TestLogRetention.test_old_logs_are_pruned_and_recent_ones_kept",
            "TestLogRetention.test_retention_can_be_disabled",
        ])
    );
    let observations = report["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 19);
    for (case, expected_raw) in observations.iter().zip([
        None,
        Some(""),
        Some("7"),
        Some("0"),
        Some("  +۷  "),
        Some("1_0"),
        Some("-۰"),
        Some("\u{1c}+۷\u{1f}"),
        Some("_1"),
        Some("1_"),
        Some("1__0"),
        Some("0x10"),
        Some("²"),
        Some("1\0"),
        Some("+"),
        Some("- 1"),
        Some("-۰۰۹"),
        Some("-1"),
        Some("not-a-number"),
    ]) {
        assert_eq!(case["raw"], json!(expected_raw));
        let (days, warning) = _native::logging::retention_days(expected_raw).unwrap();
        assert_eq!(json!(days), case["days"]);
        assert_eq!(
            warning.as_deref().unwrap_or(""),
            case["warning"].as_str().unwrap()
        );
    }
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    let directory = workspace.join("logs");
    std::fs::create_dir(&directory).unwrap();
    let now = std::time::SystemTime::now();
    for (name, days) in [
        ("old.log", 90),
        ("new.log", 1),
        ("caller.txt", 90),
        ("upper.LOG", 90),
        (".log", 90),
    ] {
        let path = directory.join(name);
        std::fs::write(&path, b"synthetic retained bytes").unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(now - std::time::Duration::from_secs(days * 86400)),
            )
            .unwrap();
    }
    std::fs::create_dir(directory.join("directory.log")).unwrap();
    let pruning = report["pruning"].as_array().unwrap();
    assert_eq!(pruning.len(), 2);
    for (case, days) in pruning.iter().zip([0, 30]) {
        assert_eq!(case["days"], days);
        let removed = _native::logging::prune_old_logs(&workspace, days).unwrap();
        assert_eq!(case["removed"], json!(removed));
        let mut members = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        members.sort();
        assert_eq!(case["members"], json!(members));
        for name in members.into_iter().filter(|name| name != "directory.log") {
            assert_eq!(
                std::fs::read(directory.join(name)).unwrap(),
                b"synthetic retained bytes"
            );
        }
    }
    // The replay owns this synthetic tree; failed comparisons retain it for diagnosis.
    std::fs::remove_dir_all(directory).unwrap();
    std::fs::remove_dir(workspace).unwrap();
}
