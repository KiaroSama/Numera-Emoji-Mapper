//! Thirteen original HTTP runtime methods share the admitted corpus and existing worker.
use _native::{source_git_inventory::Source, source_http_tests as oracle};
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = oracle::source_paths()
        .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let missing = root
        .join("logs/tmp")
        .join(format!("HTTP source refusal {}", std::process::id()));
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
                "path" => changed.path = "tests/not-original-http.py".into(),
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
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &sources,
        ))
        .unwrap();
    assert_eq!(actual["source_revision"], sources[0].baseline_revision);
    assert_eq!(actual["source_runtime"], "CPython3.11.9");
    assert_eq!(actual["tests_run"], 13);
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(actual[field], 0, "{field}");
    }
    let expected = [
        "SharedHttpClient.test_400_and_404_are_no_match_and_are_never_retried",
        "SharedHttpClient.test_402_is_a_quota_answer_and_is_never_retried",
        "SharedHttpClient.test_a_rejected_cmc_key_stops_the_run_and_names_the_key",
        "SharedHttpClient.test_a_success_is_decoded_and_costs_one_call",
        "SharedHttpClient.test_a_transient_failure_is_retried_and_then_reported_as_TRANSIENT",
        "SharedHttpClient.test_a_transport_exception_is_retried_like_a_bad_status",
        "SharedHttpClient.test_an_absurd_retry_after_is_capped",
        "SharedHttpClient.test_an_unanswered_cmc_lookup_is_unknown_not_no_match",
        "SharedHttpClient.test_fetch_logos_still_raises_after_its_retries",
        "SharedHttpClient.test_retry_after_beats_the_backoff_ladder",
        "PagingDelay.test_a_typo_falls_back_instead_of_crashing_at_import",
        "PagingDelay.test_the_default_is_short",
        "PagingDelay.test_the_env_var_overrides_it",
    ];
    assert_eq!(actual["cases"], serde_json::json!(expected));
    for source in sources {
        assert_eq!(actual["source_hashes"][&source.path], source.sha256);
    }
    assert!(
        actual["ports"]
            .as_str()
            .unwrap()
            .contains("architecture scanner excluded")
    );
}
