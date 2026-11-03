//! One fixed ordering replay from the existing corpus; source and native outcomes stay distinct.
use _native::{source_git_inventory::Source, source_sync_order_oracle as oracle};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "tests/reference/sync_order.py",
        "tests/test_sync_order.py",
        "tests/reference/collection_state.py",
        "tests/reference/packstate.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let absent = parent.join(format!("order source refusal {}", std::process::id()));
    assert!(!absent.exists());
    for index in 0..sources.len() {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let source = sources[index];
            let mut forged = Source {
                baseline_revision: source.baseline_revision.clone(),
                path: source.path.clone(),
                sha256: source.sha256.clone(),
                text: source.text.clone(),
            };
            match mutation {
                "revision" => forged.baseline_revision = "0".repeat(40),
                "path" => forged.path = "tests/unverified_order.py".into(),
                "hash" => forged.sha256 = "0".repeat(64),
                "bytes" => forged
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[index] = &forged;
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
    let workspace = parent.join(format!("original order café {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    let sentinel = workspace.join("caller");
    std::fs::write(&sentinel, b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"caller bytes");
    std::fs::remove_file(sentinel).unwrap();
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
    assert_eq!(report["tests_run"], 13);
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
    let coverage: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/native/coverage.json")).unwrap();
    let expected = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["source_file"] == sources[1].path)
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let cases = report["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 13);
    assert_eq!(
        cases
            .iter()
            .map(|case| case.as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        expected
    );
    let plans = report["plans"].as_array().unwrap();
    assert_eq!(plans.len(), 5);
    for plan in plans {
        let live = plan["live"].as_array().unwrap();
        let desired = plan["desired"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| id.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let pinned = usize::try_from(plan["pinned"].as_u64().unwrap()).unwrap();
        assert_eq!(
            json!(_native::sync_order::moves(live, &desired, pinned).unwrap()),
            plan["moves"]
        );
    }
    let records = report["records"].as_array().unwrap();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0]["record"]["keys"], json!(["s:a", "s:b", "s:c"]));
    assert_eq!(records[0]["moves"], json!([["f-a", 1], ["f-b", 2]]));
    assert_eq!(records[1]["record"]["keys"], json!(["s:b", "s:a"]));
    assert_eq!(records[1]["apply"], false);
    assert_eq!(records[1]["moves"], json!([]));
    assert_eq!(records[2]["record"]["keys"], json!(["s:a", "s:b"]));
    assert_eq!(records[2]["moves"], json!([]));
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
