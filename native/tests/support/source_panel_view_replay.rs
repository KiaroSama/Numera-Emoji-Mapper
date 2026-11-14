//! Original view assertions and random reference orders reuse the one pinned Git corpus/owner.
use _native::{source_git_inventory::Source, source_panel_view_tests as oracle};
use serde_json::json;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = oracle::source_paths()
        .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("panel view source refusal {}", std::process::id()));
    assert!(!missing.exists());
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
                "path" => changed.path = "tests/not-original-panel-view.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                _ => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
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
    let workspace = parent.join(format!("original panel view {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let caller = workspace.join("caller");
    std::fs::write(&caller, b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(std::fs::read(&caller).unwrap(), b"caller bytes");
    assert!(
        oracle::observe(root, root, &sources)
            .unwrap_err()
            .contains("inside project scratch")
    );
    std::fs::remove_file(caller).unwrap();
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
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(report["source_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["Pillow"], "12.3.0");
    assert_eq!(report["tests_run"], 146);
    assert_eq!(report["temporary_calls"], 52);
    assert_eq!(
        report["excluded"],
        json!([
            "PanelIntentPersistence.test_actual_video_plan_survives_migration_reload_and_restore"
        ])
    );
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 146);
    assert!(cases.iter().any(|case| case
        == "RefreshMustActuallyRefresh.test_the_reload_replaces_the_shared_view_in_place"));
    assert!(
        cases.iter().any(|case| case
            == "TheMediaMapIsNeverEmpty.test_a_reader_never_misses_a_key_both_maps_hold")
    );
    assert_eq!(
        cases
            .iter()
            .map(|case| case.as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        146
    );
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
    let orders = report["orders"].as_array().unwrap();
    assert_eq!(orders.len(), 3);
    for (case, seed) in orders.iter().zip([1, 1234, 99999]) {
        assert_eq!(case["seed"], seed);
        let input = case["items"].as_array().unwrap();
        assert_eq!(input.len(), 220);
        let before = input.clone();
        let sorted = _native::panel_view::similarity(input).unwrap();
        let actual = sorted
            .iter()
            .map(|item| item["identity"].as_u64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            json!(actual),
            case["expected"],
            "original random seed {seed}"
        );
        assert_eq!(
            actual
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            (0..220).collect()
        );
        assert_eq!(*input, before);
    }
    let copies = report["copies"].as_array().unwrap();
    assert_eq!(copies.len(), 6);
    for case in copies {
        assert_eq!(
            json!(_native::panel_view::copy_id(
                case["label"].as_str().unwrap()
            )),
            case["expected"]
        );
    }
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
