//! Original pure and command bot assertions share the admitted Git corpus and bounded worker.
use _native::{source_bot_tests as oracle, source_git_inventory::Source};
use serde_json::json;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let source = corpus
        .iter()
        .find(|source| source.path == "tests/reference/emoji_bot.py")
        .unwrap();
    let test = corpus
        .iter()
        .find(|source| source.path == "tests/test_emoji_bot.py")
        .unwrap();
    oracle::source_identity(source, test).unwrap();
    let missing = root.join("logs/tmp/not-a-bot-assertion-executable");
    assert!(!missing.exists());
    for dependency in [false, true] {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let original = if dependency { test } else { source };
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
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let (source, test) = if dependency {
                (source, &changed)
            } else {
                (&changed, test)
            };
            assert!(
                oracle::observe(source, test)
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(oracle::observe_owned(root, &missing, source, test))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let report = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            source,
            test,
        ))
        .unwrap();
    assert_eq!(report["source_revision"], source.baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["source_hashes"][&source.path], source.sha256);
    assert_eq!(report["source_hashes"][&test.path], test.sha256);
    assert_eq!(report["tests_run"], 26);
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(report[field], 0);
    }
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 26);
    assert_eq!(
        cases
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        26
    );
    let coverage: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/native/coverage.json")).unwrap();
    let expected = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            case["source_file"] == test.path
                && case["candidate"]["target_file"] == "native/tests/bot_roster_parity.rs"
        })
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        cases
            .iter()
            .map(|name| name.as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
    assert_eq!(
        report["excluded"],
        json!([
            "TestIdsTypedAsText.test_it_resolves_the_ids_rather_than_trusting_them",
            "TestIdsTypedAsText.test_when_nothing_resolves_it_says_so_once",
        ])
    );
    let observations = report["typed_observations"].as_array().unwrap();
    assert_eq!(observations.len(), 8);
    for (index, case) in observations.iter().enumerate() {
        let text = case["text"].as_str().unwrap();
        let actual = _native::bot_payloads::typed(text);
        assert_eq!(json!(actual), case["ids"]);
        if index != 5 {
            assert_eq!(case["ids"], json!([]));
        }
    }
    let unicode = "۵۸۹۳۰۹۸۷۴۱۰۷۳۷۱۵۴۷۱";
    assert_eq!(report["unicode_ids"], json!([unicode]));
    assert_eq!(
        json!(_native::bot_payloads::extract(&json!({"entities":[{
            "type":"custom_emoji","custom_emoji_id":unicode,
        }]}))),
        report["unicode_ids"]
    );
    assert!(
        report["ports"]
            .as_str()
            .unwrap()
            .contains("no transport, account, environment, files or polling")
    );
    commands(root, runtime, corpus);
}

fn commands(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    use _native::source_bot_command_tests as oracle;
    let sources = [
        "tests/reference/emoji_bot.py",
        "tests/test_emoji_bot.py",
        "emojikit/cli_env.py",
        "tests/reference/packstate.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("bot command refusal {}", std::process::id()));
    assert!(!missing.exists());
    for selected in 0..sources.len() {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let original = sources[selected];
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
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[selected] = &changed;
            assert!(
                runtime
                    .block_on(oracle::observe_owned(root, &missing, &missing, &admitted))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!("original bot command {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("caller"), b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(workspace.join("caller")).unwrap(),
        b"caller bytes"
    );
    std::fs::remove_file(workspace.join("caller")).unwrap();
    assert!(
        oracle::observe(root, root, &sources)
            .unwrap_err()
            .contains("inside project scratch")
    );
    #[cfg(unix)]
    {
        let link = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            oracle::observe(root, &link, &sources)
                .unwrap_err()
                .contains("linked/reparse")
        );
        std::fs::remove_file(link).unwrap();
    }
    let report = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(report["source_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["tests_run"], 18);
    assert_eq!(report["temporary_calls"], 5);
    for source in sources {
        assert_eq!(report["source_hashes"][&source.path], source.sha256);
    }
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(report[field], 0);
    }
    let coverage: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/native/coverage.json")).unwrap();
    let expected = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            case["source_file"] == sources[1].path
                && case["candidate"]["target_file"] == "native/tests/bot_commands.rs"
        })
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    assert_eq!(
        cases
            .iter()
            .map(|case| case.as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
    assert_eq!(report["allowlists"].as_array().unwrap().len(), 4);
    for (index, ids) in [
        json!([1, 2, 3]),
        json!([1, 2, 30]),
        json!([-1001234567890_i64, -5]),
        json!([-1001234567890_i64, -5]),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(report["allowlists"][index]["ids"], ids);
    }
    assert!(
        report["ports"]
            .as_str()
            .unwrap()
            .contains("original KeyboardInterrupt retained")
    );
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
