//! Three original prebuilt assertion methods reuse the single pinned source corpus and bounded worker.
use _native::source_git_inventory::Source;
use serde_json::json;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "scripts/install_prebuilt.py",
        "scripts/local_environment.py",
        "tests/test_prebuilt_environment.py",
        "scripts/build_job.py",
        "tests/test_build_process_owner.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    _native::source_prebuilt_oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("prebuilt forged sources {}", std::process::id()));
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
                "path" => changed.path = "tests/unverified-prebuilt.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                _ => changed
                    .text
                    .push_str("\nraise RuntimeError('must not execute')"),
            }
            let mut selected = sources;
            selected[index] = &changed;
            let error = runtime
                .block_on(_native::source_oracle_worker::observe_prebuilt_tests(
                    root, &missing, &missing, &selected,
                ))
                .unwrap_err();
            assert!(error.contains("source identity drift"), "{error}");
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!("original prebuilt tests {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let caller = workspace.join("caller.txt");
    std::fs::write(&caller, b"caller state must survive refusal").unwrap();
    assert!(
        _native::source_prebuilt_oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(&caller).unwrap(),
        b"caller state must survive refusal"
    );
    assert!(
        _native::source_prebuilt_oracle::observe(root, root, &sources)
            .unwrap_err()
            .contains("inside project scratch")
    );
    std::fs::remove_file(caller).unwrap();
    #[cfg(unix)]
    {
        let link = workspace.join("linked workspace");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            _native::source_prebuilt_oracle::observe(root, &link, &sources)
                .unwrap_err()
                .contains("linked/reparse")
        );
        assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 1);
        std::fs::remove_file(link).unwrap();
    }
    let actual = runtime
        .block_on(_native::source_oracle_worker::observe_prebuilt_tests(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(actual["source_revision"], sources[0].baseline_revision);
    assert_eq!(actual["source_runtime"], "CPython3.11.9");
    assert_eq!(actual["tests_run"], 4);
    assert_eq!(
        actual["process_case"],
        "BuildProcessOwner.test_partial_member_inventory_closes_already_pinned_handles"
    );
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(actual[field], 0, "{field}");
    }
    assert_eq!(
        actual["cases"],
        json!([
            "PrebuiltEnvironment.test_existing_installation_refuses_before_writing",
            "PrebuiltEnvironment.test_owned_temp_and_caches_override_external_values",
            "PrebuiltEnvironment.test_linked_cache_refuses_before_using_outside_directory",
        ])
    );
    for source in sources {
        assert_eq!(actual["source_hashes"][&source.path], source.sha256);
    }
    assert!(
        actual["ports"]
            .as_str()
            .unwrap()
            .contains("no successful install/signing")
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    std::fs::remove_dir(workspace).unwrap();
}
