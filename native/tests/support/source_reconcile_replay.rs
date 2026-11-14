//! Original reconciliation assertions reuse the one admitted Git corpus and bounded source owner.
use _native::{source_git_inventory::Source, source_reconcile_tests as oracle};
use serde_json::{Value, json};
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "tests/reference/collection_reconcile.py",
        "tests/test_build_collection_state.py",
        "tests/_bc_fixtures.py",
        "tests/_media_fixtures.py",
        "emojikit/media.py",
        "emojikit/identity.py",
        "tests/reference/catalog.py",
        "emojikit/media_paths.py",
        "tests/reference/ingest.py",
        "tests/_similarity_oracle.py",
        "tests/reference/collection_state.py",
        "tests/reference/build_collection.py",
        "emojikit/telegram_api.py",
        "emojikit/errors.py",
        "tests/test_reconcile_identity.py",
        "tests/test_reconcile_uniqueness.py",
        "tests/test_plan_apply.py",
        "tests/reference/plan_apply.py",
        "tests/reference/panel_plan.py",
        "tests/reference/sync_order.py",
        "tests/reference/packstate.py",
        "emojikit/state_artifacts.py",
        "tests/oracles/telegram_api.py",
        "tests/reference/collection_media_check.py",
        "tests/reference/collection_names.py",
        "emojikit/operator_config.py",
        "emojikit/cli_env.py",
        "tests/test_telegram_client.py",
        "tests/test_telegram_body_shape.py",
        "tests/_pack_fixtures.py",
        "tests/reference/build_pack.py",
        "tests/test_resume_safety.py",
        "tests/test_unresolved_mutation.py",
        "tests/test_build_pack_announce.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("reconcile source refusal {}", std::process::id()));
    assert!(!missing.exists());
    for selected in 0..sources.len() {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let original = sources[selected];
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
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[selected] = &changed;
            assert!(
                runtime
                    .block_on(oracle::observe_owned(root, &missing, &missing, &admitted))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!("original reconcile {}", std::process::id()));
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
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(report["source_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["Pillow"], "12.3.0");
    assert_eq!(report["tests_run"], 145);
    assert_eq!(report["temporary_calls"], 127);
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
    let coverage: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/native/coverage.json")).unwrap();
    let expected = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            case["source_file"] == sources[1].path
                && case["candidate"]["target_file"] == "native/tests/reconcile_commit.rs"
                && !case["source_case"]
                    .as_str()
                    .unwrap()
                    .starts_with("FreshUploadIdentity.")
        })
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 145);
    assert_eq!(
        cases
            .iter()
            .map(|case| case.as_str().unwrap())
            .filter(|name| [
                "LiveSetDrift.",
                "ForeignIdentityOnARecordedPosition.",
                "UnattributedTail.",
                "OurEmojiBehindAForeignSticker."
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix)))
            .collect::<std::collections::BTreeSet<_>>(),
        expected
    );
    let apply = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["source_file"] == sources[16].path)
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(apply.len(), 20);
    assert_eq!(
        cases
            .iter()
            .filter_map(Value::as_str)
            .filter(|name| apply.contains(name))
            .collect::<std::collections::BTreeSet<_>>(),
        apply
    );
    let fresh = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            case["source_file"] == sources[1].path
                && case["source_case"]
                    .as_str()
                    .unwrap()
                    .starts_with("FreshUploadIdentity.")
        })
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(fresh.len(), 5);
    assert_eq!(
        cases
            .iter()
            .filter_map(Value::as_str)
            .filter(|name| fresh.contains(name))
            .collect::<std::collections::BTreeSet<_>>(),
        fresh
    );
    let transport = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            [sources[27].path.as_str(), sources[28].path.as_str()]
                .contains(&case["source_file"].as_str().unwrap())
                && case.get("candidate").is_some()
        })
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(transport.len(), 37);
    assert_eq!(
        cases
            .iter()
            .filter_map(Value::as_str)
            .filter(|name| transport.contains(name))
            .collect::<std::collections::BTreeSet<_>>(),
        transport
    );
    let ticker = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            case["source_file"] == sources[31].path
                && ["PerSetLimit.", "ResumeAfterSkippedImage."]
                    .iter()
                    .any(|prefix| case["source_case"].as_str().unwrap().starts_with(prefix))
        })
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ticker.len(), 12);
    assert_eq!(
        cases
            .iter()
            .filter_map(Value::as_str)
            .filter(|name| ticker.contains(name))
            .collect::<std::collections::BTreeSet<_>>(),
        ticker
    );
    let unresolved = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["source_file"] == sources[32].path)
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unresolved.len(), 17);
    assert_eq!(
        cases
            .iter()
            .filter_map(Value::as_str)
            .filter(|name| unresolved.contains(name))
            .collect::<std::collections::BTreeSet<_>>(),
        unresolved
    );
    let announcer = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["source_file"] == sources[33].path)
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(announcer.len(), 2);
    assert_eq!(
        cases
            .iter()
            .filter_map(Value::as_str)
            .filter(|name| announcer.contains(name))
            .collect::<std::collections::BTreeSet<_>>(),
        announcer
    );
    let identity = cases
        .iter()
        .filter(|case| {
            let name = case.as_str().unwrap();
            !apply.contains(name)
                && !fresh.contains(name)
                && !transport.contains(name)
                && !ticker.contains(name)
                && !unresolved.contains(name)
                && !announcer.contains(name)
        })
        .map(|case| case.as_str().unwrap())
        .filter(|name| {
            ![
                "LiveSetDrift.",
                "ForeignIdentityOnARecordedPosition.",
                "UnattributedTail.",
                "OurEmojiBehindAForeignSticker.",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(identity.len(), 28);
    let declared = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            [sources[14].path.as_str(), sources[15].path.as_str()]
                .contains(&case["source_file"].as_str().unwrap())
        })
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(identity, declared);
    let observations = report["observations"].as_array().unwrap();
    assert_eq!(observations.len(), 24);
    for observation in observations {
        assert_eq!(observation["catalog"]["items"], 2);
    }
    let observed = |case: &str| {
        &observations.iter().find(|row| row["case"] == case).unwrap()["catalog"]["publications"]
    };
    for case in [
        "LiveSetDrift.test_cids_are_never_written_from_a_drifted_position",
        "ForeignIdentityOnARecordedPosition.test_an_unidentifiable_fresh_position_never_falls_back_to_order",
        "UnattributedTail.test_a_tail_whose_download_fails_is_refused_not_closed",
        "OurEmojiBehindAForeignSticker.test_an_unrecorded_upload_behind_a_foreign_sticker_is_refused",
    ] {
        assert_eq!(observed(case), &json!([]));
    }
    let matching = observed("LiveSetDrift.test_cids_are_written_when_the_manifest_matches");
    let rows = matching.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    for (index, cid) in [(0, "c0"), (1, "c1")] {
        let row = rows.iter().find(|row| row["key_index"] == index).unwrap();
        assert_eq!(row["custom_emoji_id"], cid);
        assert_eq!(row["base"], "pk");
    }
    assert!(
        report["ports"]
            .as_str()
            .unwrap()
            .contains("no-op native prerequisite/catalog ownership")
    );
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
