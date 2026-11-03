//! One original status replay shares the admitted corpus and bounded historical worker.
use _native::{source_git_inventory::Source, source_status_tests as oracle};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "tests/reference/status.py",
        "tests/test_status.py",
        "emojikit/operator_config.py",
        "emojikit/errors.py",
        "tests/reference/plan_status.py",
        "tests/test_plan_status.py",
        "tests/reference/panel_plan.py",
        "emojikit/state_artifacts.py",
        "tests/test_native_plan_status_cli.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let absent = parent.join(format!("status source refusal {}", std::process::id()));
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
                "path" => changed.path = "tests/unverified_status.py".into(),
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
    let workspace = parent.join(format!("original status café {}", std::process::id()));
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
    assert_eq!(report["temporary_calls"], 3);
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
    assert_eq!(cases.len(), 4);
    assert_eq!(
        cases
            .iter()
            .map(|case| case.as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        expected
    );
    let reports = report["reports"].as_array().unwrap();
    assert_eq!(reports.len(), 4);
    for (index, observation) in reports.iter().enumerate() {
        assert_eq!(observation[0], index == 0 || index == 2);
        let lines = observation[1].as_array().unwrap();
        assert_eq!(lines.len(), 3);
        match index {
            0 => assert!(
                lines[0]
                    .as_str()
                    .unwrap()
                    .contains("python -m emojikit.pack_manifest --refresh")
            ),
            1 => assert!(
                lines
                    .iter()
                    .all(|line| !line.as_str().unwrap().contains("STALE"))
            ),
            2 => assert!(
                lines[1]
                    .as_str()
                    .unwrap()
                    .contains("python -m emojikit.pack_archive --sync")
            ),
            3 => assert!(
                lines[2]
                    .as_str()
                    .unwrap()
                    .contains("1 included emoji not yet published to mypacks")
            ),
            _ => unreachable!(),
        }
    }
    let plans = &report["plans"];
    assert_eq!(plans["tests_run"], 5);
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(plans[field], 0);
    }
    let expected = coverage["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["source_file"] == sources[5].path)
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    let cases = plans["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    assert_eq!(
        cases
            .iter()
            .map(|case| case.as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        expected
    );
    let inputs = &plans["native_inputs"];
    let summary = _native::plan_status::summarize(&inputs["plan"], &inputs["state"]).unwrap();
    assert_eq!(summary, plans["native_summary"]);
    let ids: std::collections::BTreeMap<String, String> =
        serde_json::from_value(inputs["ids"].clone()).unwrap();
    assert_eq!(
        _native::plan_status::render(&summary, &ids, None).unwrap(),
        plans["offline"]
    );
    let live = BTreeSet::from(["111111111".to_owned()]);
    assert_eq!(
        _native::plan_status::render(&summary, &ids, Some(&live)).unwrap(),
        plans["live"]
    );
    assert!(plans["offline"].as_str().unwrap().contains("OVER CAPACITY"));
    assert!(
        plans["live"]
            .as_str()
            .unwrap()
            .contains("222222222  s:held  (not live now)")
    );
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
