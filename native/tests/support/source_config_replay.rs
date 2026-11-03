//! Original dotenv BOM assertion and file parsing share the pinned corpus and worker.
use _native::source_git_inventory::Source;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "emojikit/cli_env.py",
        "tests/test_cli_env.py",
        "tests/test_resume_safety.py",
        "tests/reference/announce.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!(
        "configuration source refusal {}",
        std::process::id()
    ));
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
                "path" => forged.path = "tests/unverified_configuration.py".into(),
                "claimed hash" => forged.sha256 = "0".repeat(64),
                "actual bytes" => forged
                    .text
                    .push_str("\nraise RuntimeError('must never execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[selected] = &forged;
            assert!(
                _native::source_config_oracle::generate(
                    root,
                    &missing,
                    admitted[0],
                    admitted[1],
                    admitted[2],
                    admitted[3]
                )
                .unwrap_err()
                .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::generate_configuration(
                        root,
                        &parent.join("not-a-configuration-oracle-executable"),
                        &missing,
                        admitted[0],
                        admitted[1],
                        admitted[2],
                        admitted[3],
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!("original configuration é {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let retained = workspace.join("caller.txt");
    std::fs::write(&retained, b"caller bytes survive refusal").unwrap();
    assert!(
        _native::source_config_oracle::generate(
            root, &workspace, sources[0], sources[1], sources[2], sources[3]
        )
        .unwrap_err()
        .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(&retained).unwrap(),
        b"caller bytes survive refusal"
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 1);
    assert!(
        _native::source_config_oracle::generate(
            root, root, sources[0], sources[1], sources[2], sources[3]
        )
        .unwrap_err()
        .contains("project-contained workspace")
    );
    std::fs::remove_file(retained).unwrap();
    #[cfg(unix)]
    {
        let link = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            _native::source_config_oracle::generate(
                root, &link, sources[0], sources[1], sources[2], sources[3]
            )
            .unwrap_err()
            .contains("linked/reparse")
        );
        std::fs::remove_file(link).unwrap();
    }
    let report = runtime
        .block_on(_native::source_oracle_worker::generate_configuration(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            sources[0],
            sources[1],
            sources[2],
            sources[3],
        ))
        .unwrap();
    assert_eq!(report["baseline_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["source_hash"], sources[0].sha256);
    assert_eq!(report["test_source_hash"], sources[1].sha256);
    assert_eq!(report["tests_run"], 1);
    assert_eq!(
        report["original_method"],
        "ADotEnvWithAByteOrderMark.test_the_first_key_is_read_under_its_own_name"
    );
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    let runners = report["runner"]["observations"].as_array().unwrap();
    assert_eq!(runners.len(), 7);
    for (case, (entry, marker, expected)) in runners.iter().zip([
        (None, None, false),
        (None, Some("unittest"), true),
        (None, Some("pytest"), true),
        (Some("unittest.__main__"), None, true),
        (Some("pytest"), None, true),
        (Some("unittest_extra"), None, false),
        (Some("ordinary.unittest"), None, false),
    ]) {
        assert_eq!(case["entry"], json!(entry));
        assert_eq!(case["marker"], json!(marker));
        assert_eq!(case["result"], expected);
    }
    let observations = report["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 4);
    let file = workspace.join(".env");
    let inherited = BTreeMap::new();
    for (case, name) in observations.iter().zip([
        "original-bom",
        "two-bom-and-lines",
        "quotes-and-first-duplicate",
        "invalid-utf8",
    ]) {
        assert_eq!(case["name"], name);
        let bytes = case["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|byte| u8::try_from(byte.as_u64().unwrap()).unwrap())
            .collect::<Vec<_>>();
        std::fs::write(&file, &bytes).unwrap();
        let parsed = _native::config::Config::load_with_environment(workspace.clone(), &inherited);
        if name == "invalid-utf8" {
            assert_eq!(case["result"]["error_kind"], "UnicodeDecodeError");
            assert!(parsed.err().unwrap().contains("cannot read configuration"));
        } else {
            let parsed = parsed.unwrap();
            let expected = case["result"]["values"].as_object().unwrap();
            assert!(!expected.is_empty());
            for (key, value) in expected {
                assert_eq!(json!(parsed.value(key)), *value, "{name}: {key}");
            }
            if name == "two-bom-and-lines" {
                assert_eq!(parsed.value("FIRST"), "");
                assert_eq!(parsed.value("\u{feff}FIRST"), "one");
            }
        }
        assert_eq!(std::fs::read(&file).unwrap(), bytes);
    }
    let numeric = &report["numeric"];
    assert_eq!(numeric["test_source_hash"], sources[2].sha256);
    assert_eq!(numeric["tests_run"], 4);
    assert_eq!(numeric["patch_calls"], 6);
    assert_eq!(
        numeric["original_methods"],
        json!([
            "SafeConfigParsing.test_valid_value",
            "SafeConfigParsing.test_garbage_falls_back_instead_of_raising",
            "SafeConfigParsing.test_absent_and_blank_use_the_default",
            "SafeConfigParsing.test_values_are_clamped",
        ])
    );
    let observations = numeric["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 24);
    for case in observations {
        let mut inherited = BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
        if let Some(raw) = case["raw"].as_str() {
            inherited.insert("X_TEST_NUM".into(), raw.into());
        }
        let before = inherited.clone();
        let parsed =
            _native::config::Config::load_with_environment(workspace.clone(), &inherited).unwrap();
        let value = parsed.integer(
            "X_TEST_NUM",
            case["default"].as_i64().unwrap(),
            case["minimum"].as_i64().unwrap_or(i64::MIN),
            case["maximum"].as_i64().unwrap_or(i64::MAX),
        );
        assert_eq!(value.to_string(), case["value"].as_str().unwrap());
        let policy = _native::config_integer::resolve(
            case["raw"].as_str(),
            "X_TEST_NUM",
            case["default"].as_i64().unwrap(),
            case["minimum"].as_i64(),
            case["maximum"].as_i64(),
        );
        assert_eq!(policy.0, case["value"].as_str().unwrap());
        assert_eq!(
            policy.1.as_deref().unwrap_or(""),
            case["stderr"].as_str().unwrap()
        );
        assert_eq!(inherited, before);
    }
    let exits = &report["exits"];
    assert_eq!(exits["tests_run"], 5);
    assert_eq!(
        exits["original_methods"],
        json!([
            "ExitCodes.test_clean_run_is_zero",
            "ExitCodes.test_partial_run_is_retryable",
            "ExitCodes.test_total_failure_is_terminal",
            "ExitCodes.test_empty_run_is_not_a_failure",
            "ExitCodes.test_codes_are_distinct",
        ])
    );
    use _native::collector::{EXIT_FAILED, EXIT_OK, EXIT_PARTIAL, EXIT_USAGE, ingest_exit_code};
    for (name, value) in [
        ("EXIT_OK", EXIT_OK),
        ("EXIT_USAGE", EXIT_USAGE),
        ("EXIT_PARTIAL", EXIT_PARTIAL),
        ("EXIT_FAILED", EXIT_FAILED),
    ] {
        assert_eq!(exits["codes"][name], value);
    }
    let observations = exits["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 4);
    for (case, (succeeded, failed)) in
        observations
            .iter()
            .zip([(5_u64, 0_u64), (3, 2), (0, 4), (0, 0)])
    {
        assert_eq!(case["succeeded"], succeeded);
        assert_eq!(case["failed"], failed);
        assert_eq!(case["code"], ingest_exit_code(succeeded, failed));
    }
    let links = &report["links"];
    assert_eq!(links["source_hash"], sources[3].sha256);
    assert_eq!(links["test_source_hash"], sources[2].sha256);
    assert_eq!(links["tests_run"], 4);
    assert_eq!(links["patch_calls"], 4);
    assert_eq!(
        links["original_methods"],
        json!([
            "LinksDestination.test_unset_falls_back_to_the_owner",
            "LinksDestination.test_numeric_channel_id_is_used_as_an_int",
            "LinksDestination.test_at_username_is_passed_through",
            "LinksDestination.test_surrounding_whitespace_is_tolerated",
        ])
    );
    assert_eq!(
        links["deferred_methods"],
        json!(["LinksDestination.test_every_publisher_resolves_the_same_destination"])
    );
    let observations = links["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 22);
    for case in observations {
        let mut inherited = BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
        if let Some(raw) = case["raw"].as_str() {
            inherited.insert("PACK_LINKS_CHAT_ID".into(), raw.into());
        }
        let before = inherited.clone();
        let config =
            _native::config::Config::load_with_environment(workspace.clone(), &inherited).unwrap();
        let actual = _native::announce::links_chat_id(&config, case["owner"].as_u64().unwrap());
        if case["result"]["error_kind"] == "ValueError" {
            assert!(!case["result"]["error"].as_str().unwrap().is_empty());
            assert_eq!(actual.unwrap_err(), "invalid PACK_LINKS_CHAT_ID");
        } else {
            assert!(matches!(
                case["result"]["kind"].as_str(),
                Some("int" | "str")
            ));
            assert_eq!(actual.unwrap(), case["result"]["value"].as_str().unwrap());
        }
        assert_eq!(inherited, before);
    }
    // Successful comparisons clean only the owned fixture; a failure retains its inputs.
    std::fs::remove_file(file).unwrap();
    std::fs::remove_dir(workspace).unwrap();
    operator(root, runtime, corpus);
}

fn operator(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    use _native::source_operator_oracle as oracle;
    let sources = [
        "emojikit/operator_config.py",
        "tests/test_operator_config.py",
        "emojikit/errors.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("operator source refusal {}", std::process::id()));
    assert!(!missing.exists());
    for index in 0..sources.len() {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let source = sources[index];
            let mut changed = Source {
                baseline_revision: source.baseline_revision.clone(),
                path: source.path.clone(),
                sha256: source.sha256.clone(),
                text: source.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "tests/unverified_operator.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[index] = &changed;
            assert!(
                runtime
                    .block_on(oracle::observe_owned(root, &missing, &missing, &admitted))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!("original operator {}", std::process::id()));
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
    assert_eq!(report["tests_run"], 8);
    assert_eq!(report["temporary_calls"], 1);
    for source in sources {
        assert_eq!(report["source_hashes"][&source.path], source.sha256);
    }
    for field in ["errors", "failures", "skipped"] {
        assert_eq!(report[field], 0);
    }
    let coverage: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/native/coverage.json")).unwrap();
    let expected = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["source_file"] == sources[1].path)
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    assert_eq!(
        cases
            .iter()
            .map(|case| case.as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
    let observations = report["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 6);
    for observation in observations {
        let key = if observation.get("bots").is_some() {
            "BRAND_LOGO_BOTS"
        } else {
            "BRAND_LOGO_KEYWORDS"
        };
        let environment = BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            (key.into(), observation["raw"].as_str().unwrap().into()),
        ]);
        let config =
            _native::config::Config::load_with_environment(workspace.clone(), &environment)
                .unwrap();
        if key == "BRAND_LOGO_BOTS" {
            assert_eq!(json!(config.logo_bots(true).unwrap()), observation["bots"]);
        } else {
            assert_eq!(json!(config.logo_keywords()), observation["keywords"]);
        }
    }
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
