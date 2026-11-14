//! Original publisher assertions share the existing admitted corpus and historical process owner.
use _native::{source_git_inventory::Source, source_publication_tests as oracle};
use serde_json::{Value, json};
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources =
        oracle::source_paths().map(|path| corpus.iter().find(|s| s.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("publication source refusal {}", std::process::id()));
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
                "path" => changed.path = "tests/not-original.py".into(),
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
    let workspace = parent.join(format!("original publication {}", std::process::id()));
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
    let actual = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(actual["source_revision"], sources[0].baseline_revision);
    assert_eq!(actual["source_runtime"], "CPython3.11.9");
    assert_eq!(actual["Pillow"], "12.3.0");
    assert_eq!(actual["tests_run"], 84);
    assert_eq!(actual["temporary_calls"], 80);
    assert_eq!(
        actual["policy_differences"],
        json!([
            "VerifiedRetryTest.test_without_expected_before_keeps_legacy_retry: historical blind retry is retained as source evidence; native no-snapshot mutation remains Ambiguous and never retries"
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
    let cases = actual["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(cases.len(), 84);
    for original in [
        "AFilledPackAnnouncesAgain.test_the_capacity_call_site_asks_for_the_full_milestone",
        "MixedPublishesOneFamily.test_a_family_started_per_format_cannot_switch_to_mixed",
    ] {
        assert!(
            cases.contains(original),
            "original source-reading method missing"
        );
    }
    let coverage: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/native/coverage.json")).unwrap();
    let selected = coverage["cases"].as_array().unwrap().iter().filter(|row| {
        let source = row["source_file"].as_str().unwrap();
        let name = row["source_case"].as_str().unwrap();
        source == sources[27].path
            || (source == sources[28].path && super::source_coverage::selected(source, name))
            || (source == sources[29].path && (name.starts_with("AClosedSetIsNeverAppendedTo.")
                || name.starts_with("MixedFamiliesStillCheckEachItemsOwnFormat.")))
            || (source == sources[34].path && (name.starts_with("AnUnusableLogoStopsThePublish.")
                || name.starts_with("PublishFormatLogoFirst.")))
            || (source == sources[35].path && super::source_coverage::selected(source, name)
                && !name.starts_with("PackTitlesAreOneSequence.")
                && name != "EveryPublisherSharesOneAnnouncer.test_the_single_pack_build_and_the_coin_rebuild_both_import_it")
    }).map(|row| {
        assert!(row.get("candidate").is_some(), "selected original method lacks a native candidate");
        row["source_case"].as_str().unwrap()
    }).collect::<std::collections::BTreeSet<_>>();
    assert_eq!(cases, selected);
    assert!(
        actual["ports"]
            .as_str()
            .unwrap()
            .contains("original manifest/preflight/notification")
    );
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
