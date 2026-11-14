//! Original identity/provenance assertions join the existing corpus and process owner once.
use _native::{source_git_inventory::Source, source_identity_tests as oracle};
use std::collections::BTreeSet;
use std::path::Path;

#[path = "identity_bundle_commands.rs"]
mod bundle_commands;
#[path = "identity_report_commands.rs"]
mod commands;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources =
        oracle::source_paths().map(|path| corpus.iter().find(|s| s.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let absent = parent.join(format!("identity source refusal {}", std::process::id()));
    assert!(!absent.exists());
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
                "path" => changed.path = "tests/not-original-identity.py".into(),
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
            assert!(
                runtime
                    .block_on(oracle::bundle_owned(
                        root, &absent, &absent, None, "prepare", &admitted
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!absent.exists());
        }
    }
    let workspace = parent.join(format!("original identity café {}", std::process::id()));
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
    let result = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(result["source_revision"], sources[0].baseline_revision);
    assert_eq!(result["source_runtime"], "CPython3.11.9");
    assert_eq!(result["tests_run"], 59);
    assert_eq!(result["temporary_calls"], 65);
    commands::check(root, runtime, &result["reports"]);
    bundle_commands::check(root, runtime, &sources);
    assert_eq!(
        result["excluded"],
        serde_json::json!([
            "MigrationCannotCertifyDamage.test_native_death_after_second_move_replays_via_cli",
            "ActualWriterOwnership.test_first_family_publisher_and_catalog_exclude_migration_in_real_children",
            "ActualWriterOwnership.test_pending_journal_blocks_ingest_archive_publisher_and_panel_save"
        ])
    );
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(result[field], 0);
    }
    for source in sources {
        assert_eq!(result["source_hashes"][&source.path], source.sha256);
    }
    let cases = result["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(cases.len(), 59);
    let coverage: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/native/coverage.json")).unwrap();
    let expected = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            [sources[0], sources[1], sources[14]]
                .iter()
                .any(|source| row["source_file"] == source.path)
        })
        .map(|row| row["source_case"].as_str().unwrap())
        .filter(|name| {
            !result["excluded"]
                .as_array()
                .unwrap()
                .iter()
                .any(|excluded| excluded.as_str() == Some(*name))
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        cases, expected,
        "original identity method identities drifted"
    );
    for (class, expected) in [
        ("EveryDurableReferenceMoves", 7),
        ("MigratedPathsStayDataRelative", 2),
        ("NothingIsAppliedFromAnIncompletePicture", 10),
        ("APreexistingPartialMigrationIsRecoverable", 4),
        ("TheBackupIsWalSafe", 5),
        ("AnInterruptedMigrationIsResumable", 5),
        ("LegacyRecoveryRequiresProvenance", 9),
        ("MigrationCannotCertifyDamage", 15),
        ("ActualWriterOwnership", 2),
    ] {
        assert_eq!(
            cases
                .iter()
                .filter(|name| name.starts_with(&format!("{class}.")))
                .count(),
            expected
        );
    }
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
