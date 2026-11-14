//! Original CLI refusal assertions use the existing admitted corpus and historical worker.
use _native::{source_coin_cli_tests as oracle, source_git_inventory::Source};
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = oracle::source_paths()
        .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let missing = root
        .join("logs/tmp")
        .join(format!("coin CLI source refusal {}", std::process::id()));
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
                "path" => changed.path = "tests/not-original-cli.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                _ => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
            }
            let mut admitted = sources;
            admitted[index] = &changed;
            assert!(
                runtime
                    .block_on(oracle::observe_owned(&missing, &missing, &admitted))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let actual = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &sources,
        ))
        .unwrap();
    assert_eq!(actual["source_revision"], sources[0].baseline_revision);
    assert_eq!(actual["source_runtime"], "CPython3.11.9");
    assert_eq!(actual["tests_run"], 4);
    assert_eq!(actual["import_calls"], 8);
    assert_eq!(actual["loader_calls"], 4);
    assert_eq!(actual["network_patches"], 2);
    assert_eq!(
        actual["cases"],
        serde_json::json!([
            "AnUnrecognisedArgumentCannotSelectTheLiveBranch.test_a_misspelled_dry_flag_is_refused_by_both_providers",
            "AnUnrecognisedArgumentCannotSelectTheLiveBranch.test_an_unset_token_stops_a_live_run_before_any_download",
            "AnUnrecognisedArgumentCannotSelectTheLiveBranch.test_fetch_logos_page_count_is_not_read_at_import",
            "AnUnrecognisedArgumentCannotSelectTheLiveBranch.test_the_real_dry_flag_still_parses",
        ])
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
    for source in sources {
        assert_eq!(actual["source_hashes"][&source.path], source.sha256);
    }
}
