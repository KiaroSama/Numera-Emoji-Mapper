//! Original snapshot assertions execute once through the shared pinned corpus and bounded worker.
use _native::{source_git_inventory::Source, source_snapshot_tests as oracle};
use std::collections::BTreeSet;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = oracle::source_paths()
        .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let absent = parent.join(format!("snapshot source refusal {}", std::process::id()));
    assert!(!absent.exists());
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
                "path" => changed.path = "tests/not-original-snapshot.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                _ => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
            }
            let mut admitted = sources;
            admitted[index] = &changed;
            assert!(
                runtime
                    .block_on(oracle::observe_owned(root, &absent, &absent, &admitted))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!absent.exists());
        }
    }
    let workspace = parent.join(format!("original snapshot café {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let caller = workspace.join("caller");
    std::fs::write(&caller, b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(std::fs::read(&caller).unwrap(), b"caller bytes");
    std::fs::remove_file(caller).unwrap();
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
    assert_eq!(report["tests_run"], 4);
    assert_eq!(report["temporary_calls"], 4);
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
    assert_eq!(
        report["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| case.as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "BoundedSnapshot.test_exclusive_source_lock_expires_and_removes_partial_backup",
            "BoundedSnapshot.test_locked_restore_destination_keeps_original_rows_and_can_retry",
            "BoundedSnapshot.test_committed_wal_rows_are_in_the_reopened_snapshot",
            "BoundedSnapshot.test_bad_budgets_refuse_before_writing",
        ])
    );
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
