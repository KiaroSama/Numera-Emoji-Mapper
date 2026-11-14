//! Audit recipes and original assertion classes share the existing admitted Git corpus.
use super::source_oracle_evidence::refusal;
use _native::source_git_inventory::Source;
use serde_json::{Value, json};
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    use _native::source_coin_audit_oracle as oracle;
    let sources = ["coins/check_all_packs.py", "emojikit/cli_env.py"]
        .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/coin-audit.json"
    ))
    .unwrap();
    let missing = root.join("logs/tmp/not-an-audit-oracle-executable");
    let executable = Path::new(env!("CARGO_BIN_EXE_numera-compat"));
    assert!(!missing.exists());
    for index in 0..2 {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let original = sources[index];
            let mut changed = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "tests/not-the-audit-source.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let mut selected = sources;
            selected[index] = &changed;
            assert!(
                oracle::check(selected[0], selected[1], &fixture)
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::check_coin_audit(
                        root,
                        &missing,
                        selected[0],
                        selected[1],
                        &fixture,
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
        }
    }
    let mut changed = fixture.clone();
    changed["cases"][1]["input"]["analysis"]["a"] = json!(["changed", false]);
    changed["cases"][1]["expected"]["code"] = json!(4);
    assert!(
        oracle::check(sources[0], sources[1], &changed)
            .unwrap_err()
            .contains("recipe drift")
    );
    assert!(
        runtime
            .block_on(_native::source_oracle_worker::check_coin_audit(
                root, &missing, sources[0], sources[1], &changed,
            ))
            .unwrap_err()
            .contains("recipe drift")
    );
    assert!(!missing.exists());
    let observed = runtime
        .block_on(_native::source_oracle_worker::check_coin_audit(
            root, executable, sources[0], sources[1], &fixture,
        ))
        .unwrap();
    assert_eq!(observed["cases_checked"], 7);
    assert_eq!(observed["source_revision"], sources[0].baseline_revision);
    assert_eq!(observed["source_hash"], sources[0].sha256);
    assert_eq!(observed["environment_hash"], sources[1].sha256);
    assert_eq!(observed["source_runtime"], "CPython3.11.9");
    assert_eq!(observed["fixture_drift"], false);
    assert!(
        observed["ports"]
            .as_str()
            .unwrap()
            .contains("no original unittest")
    );
    let mut changed = fixture.clone();
    changed["cases"][0]["expected"]["code"] = json!(4);
    refusal(
        root,
        "coin-audit",
        &changed,
        runtime.block_on(_native::source_oracle_worker::check_coin_audit(
            root, executable, sources[0], sources[1], &changed,
        )),
        "original audit fixture drift",
    )
    .clean();
    let images: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/coin-audit-image.json"
    ))
    .unwrap();
    let media = corpus
        .iter()
        .find(|source| source.path == "emojikit/media.py")
        .unwrap();
    let image_sources = [sources[0], media];
    for index in 0..2 {
        let original = image_sources[index];
        let mut changed = Source {
            baseline_revision: original.baseline_revision.clone(),
            path: original.path.clone(),
            sha256: original.sha256.clone(),
            text: original.text.clone(),
        };
        changed
            .text
            .push_str("\nraise AssertionError('must not execute')");
        let mut selected = image_sources;
        selected[index] = &changed;
        assert!(
            oracle::check_images(&missing, selected[0], selected[1], &images)
                .unwrap_err()
                .contains("source identity drift")
        );
        assert!(
            runtime
                .block_on(_native::source_oracle_worker::check_coin_audit_images(
                    root,
                    &missing,
                    selected[0],
                    selected[1],
                    &images,
                ))
                .unwrap_err()
                .contains("source identity drift")
        );
    }
    let mut changed = images.clone();
    changed["cases"][0]["input"] = json!("tests/fixtures/foreign.png");
    assert!(
        oracle::check_images(&missing, sources[0], media, &changed)
            .unwrap_err()
            .contains("recipe drift")
    );
    assert!(
        runtime
            .block_on(_native::source_oracle_worker::check_coin_audit_images(
                root, &missing, sources[0], media, &changed,
            ))
            .unwrap_err()
            .contains("recipe drift")
    );
    let observed = runtime
        .block_on(_native::source_oracle_worker::check_coin_audit_images(
            root, executable, sources[0], media, &images,
        ))
        .unwrap();
    assert_eq!(observed["cases_checked"], 7);
    assert_eq!(observed["source_hash"], sources[0].sha256);
    assert_eq!(observed["media_hash"], media.sha256);
    assert_eq!(observed["source_runtime"], "CPython3.11.9");
    assert_eq!(observed["pillow"], "12.3.0");
    assert_eq!(observed["fixture_drift"], false);
    let mut changed = images.clone();
    changed["cases"][0]["expected"]["hash"] = json!("not-an-original-observation");
    refusal(
        root,
        "coin-audit-images",
        &changed,
        runtime.block_on(_native::source_oracle_worker::check_coin_audit_images(
            root, executable, sources[0], media, &changed,
        )),
        "original audit image drift",
    )
    .clean();
    assert!(!missing.exists());
    check_original_classes(root, runtime, corpus);
}

fn check_original_classes(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "coins/check_all_packs.py",
        "emojikit/media.py",
        "emojikit/cli_env.py",
        "tests/test_check_all_packs.py",
        "tests/_coin_fixtures.py",
        "tests/reference/packstate.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let workspace = parent.join(format!("original audit assertions {}", std::process::id()));
    assert!(!workspace.exists());
    let missing = parent.join("not-an-audit-test-executable");
    for index in 0..sources.len() {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let original = sources[index];
            let mut changed = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "tests/unadmitted-audit.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let mut selected = sources;
            selected[index] = &changed;
            assert!(
                _native::source_coin_audit_tests::observe(root, &workspace, &selected)
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::observe_coin_audit(
                        root, &missing, &workspace, &selected,
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!workspace.exists());
        }
    }
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("keep"), b"caller data").unwrap();
    assert!(
        _native::source_coin_audit_tests::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(workspace.join("keep")).unwrap(),
        b"caller data"
    );
    std::fs::remove_file(workspace.join("keep")).unwrap();
    assert!(
        _native::source_coin_audit_tests::observe(root, &parent, &sources)
            .unwrap_err()
            .contains("inside project scratch")
    );
    #[cfg(unix)]
    {
        let linked = workspace.join("linked-workspace");
        std::os::unix::fs::symlink(&workspace, &linked).unwrap();
        assert!(
            _native::source_coin_audit_tests::observe(root, &linked, &sources)
                .unwrap_err()
                .contains("linked/reparse")
        );
        assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 1);
        std::fs::remove_file(linked).unwrap();
    }
    let report = runtime
        .block_on(_native::source_oracle_worker::observe_coin_audit(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(report["tests_run"], 7);
    assert_eq!(report["source_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["Pillow"], "12.3.0");
    let methods = sources[3]
        .text
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("def test_")
                .and_then(|line| line.split_once('('))
                .map(|(method, _)| format!("test_{method}"))
        })
        .collect::<Vec<_>>();
    assert_eq!(methods.len(), 7);
    let actual = report["cases"].as_array().unwrap();
    assert_eq!(actual.len(), 7);
    for method in methods {
        assert!(
            actual
                .iter()
                .any(|case| case.as_str().unwrap().ends_with(&format!(".{method}")))
        );
    }
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(report[field], 0, "{field}");
    }
    for source in sources {
        assert_eq!(report["source_hashes"][&source.path], source.sha256);
    }
    let observations = report["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 7);
    let interrupted = observations
        .iter()
        .find(|row| {
            row["case"] == "CheckAllPacksListingRetry.test_each_set_boundary_checkpoints_the_audit"
        })
        .unwrap();
    assert!(interrupted["report"].is_null());
    assert_eq!(
        interrupted["audit"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["a"]
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    assert!(!_native::process_owner::cleanup_pending(&workspace));
    std::fs::remove_dir(workspace).unwrap();
}
