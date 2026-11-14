//! Original roster methods share the existing pinned corpus and one bounded source worker.
use _native::{source_git_inventory::Source, source_roster_tests as oracle};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "tests/reference/pack_manifest.py",
        "tests/test_pack_manifest.py",
        "tests/reference/pack_gallery.py",
        "tests/oracles/pack_rows.py",
        "tests/reference/script_json.py",
        "emojikit/operator_config.py",
        "tests/reference/packstate.py",
        "emojikit/media_paths.py",
        "emojikit/media.py",
        "tests/_media_fixtures.py",
        "emojikit/media_lottie.py",
        "emojikit/errors.py",
        "tests/test_pack_rows.py",
        "tests/reference/collection_notify.py",
        "tests/reference/pack_archive.py",
        "coins/write_manifests.py",
        "tests/reference/catalog.py",
        "emojikit/pack_rows.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let absent = parent.join(format!("roster source refusal {}", std::process::id()));
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
                "path" => changed.path = "tests/unverified_roster.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[index] = &changed;
            assert!(
                oracle::observe(root, &absent, &admitted)
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(oracle::observe_owned(root, &absent, &absent, &admitted))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!absent.exists());
        }
    }
    let workspace = parent.join(format!("original roster café {}", std::process::id()));
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
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(report["source_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["tests_run"], 24);
    assert_eq!(report["temporary_calls"], 16);
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
    assert_eq!(report["excluded"], json!([]));
    assert_eq!(report["encoder_calls"], 1);
    assert_eq!(
        report["lottie_input_sha256"],
        "f12c38be112dddb2ba6d3217034c8396479e6cfcf86cfeca22fa236381b0f9d1"
    );
    let thumbnails = report["thumbnails"].as_object().unwrap();
    assert_eq!(
        thumbnails
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "thumbs/111.webp",
            "thumbs/222.webp",
            "thumbs/222_still.webp",
            "thumbs/333.webm"
        ])
    );
    for value in thumbnails.values() {
        assert!(value["bytes"].as_u64().unwrap() > 0);
        assert_eq!(value["sha256"].as_str().unwrap().len(), 64);
    }
    let coverage: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/native/coverage.json")).unwrap();
    let expected = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["source_file"] == sources[1].path)
        .map(|row| row["source_case"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 24);
    assert_eq!(
        cases
            .iter()
            .map(|case| case.as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        expected
    );
    let reports = &report["reports"];
    assert_eq!(reports["tests_run"], 7);
    assert_eq!(reports["identity_calls"], 1);
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(reports[field], 0);
    }
    let expected_reports = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["source_file"] == "tests/test_pack_rows.py")
        .map(|row| row["source_case"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let report_cases = reports["cases"].as_array().unwrap();
    assert_eq!(report_cases.len(), 7);
    assert_eq!(
        report_cases
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        expected_reports
    );
    let indexes = report["family_indexes"].as_array().unwrap();
    assert_eq!(indexes.len(), 2);
    for index in indexes {
        assert_eq!(index["pack_count"], 2);
        assert!(index["by_current_id"].get("g1").is_some());
        assert!(index["by_current_id"].get("c-new").is_some());
        assert!(index["by_current_id"].get("c-old").is_none());
        assert_eq!(index["by_source_id"], json!({"src-g":"g1","src-c":"c-new"}));
        assert_eq!(index["by_previous_id"], json!({"g0":"g1"}));
        assert_eq!(index["inputs"]["catalog.db"], json!([1, 1]));
        assert_eq!(index["inputs"]["publish_state.json"], json!([2, 2]));
        assert_eq!(index["inputs"]["rebuild_dedup_state.json"], json!([5, 5]));
        assert_eq!(index["inputs"]["ticker_to_id.json"], json!([6, 6]));
    }
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
