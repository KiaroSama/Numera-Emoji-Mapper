//! Original benchmark policy observations reuse the one Git corpus, never native-derived goldens.
use _native::source_git_inventory::Source;
use _native::tooling_similarity_benchmark::{Failure, finish, load_hashes, summarize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

fn hash(values: &[u64]) -> String {
    Sha256::digest(serde_json::to_vec(values).unwrap())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn comparison(case: &Value) {
    let hashes = case["input_hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_u64().unwrap())
        .collect::<Vec<_>>();
    let events = case["events"].as_array().unwrap();
    let mismatch = case["name"] == "greedy-mismatch";
    assert_eq!(events.len(), if mismatch { 6 } else { 30 });
    let mut source_samples = [0.0; 5];
    let mut native_samples = [0.0; 5];
    for (index, pair) in events.chunks_exact(6).enumerate() {
        assert_eq!(
            pair.iter()
                .map(|event| event[0].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["clock", "source", "clock", "clock", "target-port", "clock"]
        );
        assert_eq!(pair[1][1], case["input_hashes"]);
        assert_eq!(pair[4][1], case["input_hashes"]);
        let actual = json!(_native::similarity::greedy(&hashes));
        assert_eq!(actual, pair[1][2]);
        if mismatch {
            assert_ne!(pair[1][2], pair[4][2]);
        } else {
            assert_eq!(actual, pair[4][2]);
        }
        source_samples[index] = pair[2][1].as_f64().unwrap() - pair[0][1].as_f64().unwrap();
        native_samples[index] = pair[5][1].as_f64().unwrap() - pair[3][1].as_f64().unwrap();
    }
    if mismatch {
        assert_eq!(case["observation"]["raised"]["type"], "RuntimeError");
        assert!(
            !case["observation"]["raised"]["message"]
                .as_str()
                .unwrap()
                .is_empty()
        );
    } else {
        let report = summarize(hashes.len(), source_samples, native_samples).unwrap();
        assert_eq!(
            json!([report.python_median, report.rust_median]),
            case["observation"]["returned"]
        );
    }
}

fn policy(root: &Path, report: &Value) {
    let cases = report["main"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    let mut names = std::collections::BTreeSet::new();
    let mut default_checked = false;
    let expected_names = [
        "below-regression-boundary",
        "regression-boundary",
        "equal-medians",
        "zero-native-floor",
        "empty",
        "maximum-input",
        "oversized",
        "near-mismatch",
        "caught-os",
        "caught-sqlite",
        "caught-runtime",
        "caught-int-value",
        "uncaught-int-type",
        "uncaught-int-overflow",
        "uncaught-interrupt",
        "default-random-input",
    ];
    assert_eq!(
        cases
            .iter()
            .map(|case| case["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected_names
    );
    for case in cases {
        let name = case["name"].as_str().unwrap();
        assert!(names.insert(name));
        let events = case["events"].as_array().unwrap();
        assert_eq!(events.first().unwrap()[0], "setup-logging");
        let compare = events.iter().find(|event| event[0] == "compare-port");
        let logs = events
            .iter()
            .filter(|event| event[0] == "log")
            .map(|event| (event[1].as_str().unwrap(), event[2].as_str().unwrap()))
            .collect::<Vec<_>>();
        if let Some(raised) = case["observation"].get("raised") {
            assert!(matches!(
                raised["type"].as_str(),
                Some("TypeError" | "OverflowError" | "KeyboardInterrupt")
            ));
            assert!(
                logs.is_empty(),
                "uncaught source exceptions must not become caught refusals"
            );
            continue;
        }
        let source_code = case["observation"]["returned"].as_u64().unwrap();
        let log = _native::logging::RunLog::open(root, "benchmark-policy", vec![]);
        let path = log.path.clone().expect("policy log must exist");
        let result = if logs.first().is_some_and(|(level, _)| *level == "INFO") {
            let count = compare.unwrap()[1]["hashes"].as_u64().unwrap() as usize;
            let source = case["supplied_medians"][0].as_f64().unwrap();
            let native = case["supplied_medians"][1].as_f64().unwrap();
            let summary = summarize(count, [source; 5], [native; 5]).unwrap();
            assert_eq!(summary.line(), logs[0].1);
            Ok(summary)
        } else {
            assert_eq!(logs.len(), 1);
            assert_eq!(logs[0].0, "ERROR");
            // Only cardinality has a real native input here. Supplied source exception ports
            // cannot prove that the native loader classifies those exceptions correctly.
            if !matches!(name, "empty" | "oversized") {
                if name == "maximum-input" {
                    assert_eq!(compare.unwrap()[1]["hashes"], 10000);
                }
                drop(log);
                std::fs::remove_file(path).unwrap();
                continue;
            }
            assert!(
                compare.is_none(),
                "invalid cardinality reached source compare"
            );
            let count = case["supplied_count"].as_u64().unwrap() as usize;
            Err(Failure::Verification(
                summarize(count, [1.0; 5], [1.0; 5]).unwrap_err(),
            ))
        };
        assert_eq!(u64::from(finish(result, &log)), source_code, "{name}");
        drop(log);
        let text = std::fs::read_to_string(&path).unwrap();
        let mut consumed = 0;
        for (level, message) in logs {
            let needle = format!("[{level}] [benchmark_similarity] {message}");
            let offset = text[consumed..].find(&needle).unwrap();
            consumed += offset + needle.len();
        }
        std::fs::remove_file(path).unwrap();
        if name == "default-random-input" {
            let hashes = load_hashes(None).unwrap();
            assert_eq!(hashes.len(), 1500);
            assert_eq!(compare.unwrap()[1]["sha256"], hash(&hashes));
            assert_eq!(compare.unwrap()[1]["head"], json!(&hashes[..4]));
            let near = events
                .iter()
                .filter(|event| event[0] == "source-near")
                .collect::<Vec<_>>();
            assert_eq!(near.len(), 60);
            let optional = hashes.iter().copied().map(Some).collect::<Vec<_>>();
            for event in near {
                assert_eq!(
                    json!(
                        _native::similarity::near(
                            &optional,
                            event[1].as_u64().unwrap(),
                            event[2].as_i64().unwrap() as i32
                        )
                        .unwrap()
                    ),
                    event[3]
                );
            }
            default_checked = true;
        }
    }
    assert!(default_checked);
}

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, sources: &[Source]) {
    let get = |path: &str| sources.iter().find(|source| source.path == path).unwrap();
    let benchmark = get("scripts/benchmark_similarity.py");
    let reference = get("tests/_similarity_oracle.py");
    let executable = Path::new(env!("CARGO_BIN_EXE_numera-emoji"));
    for selected in 0..2 {
        for mutation in ["revision", "path", "claimed hash", "actual bytes"] {
            let original = [benchmark, reference][selected];
            let mut changed = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "unverified.py".into(),
                "claimed hash" => changed.sha256 = "0".repeat(64),
                _ => changed
                    .text
                    .push_str("\nraise RuntimeError('must never execute')"),
            }
            let mut admitted = [benchmark, reference];
            admitted[selected] = &changed;
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::generate_benchmark_policy(
                        root,
                        &root.join("not-a-benchmark-worker"),
                        admitted[0],
                        admitted[1]
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
        }
    }
    let report = runtime
        .block_on(_native::source_oracle_worker::generate_benchmark_policy(
            root, executable, benchmark, reference,
        ))
        .unwrap();
    assert_eq!(report["source_revision"], benchmark.baseline_revision);
    assert_eq!(report["source_hash"], benchmark.sha256);
    assert_eq!(report["reference_hash"], reference.sha256);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["compare"].as_array().unwrap().len(), 2);
    for case in report["compare"].as_array().unwrap() {
        comparison(case);
    }
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let scratch = parent.join(format!("benchmark policy replay {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    // Preserve diagnostics on failure; no source process remains after the bounded worker returns.
    policy(&scratch, &report);
    std::fs::remove_dir_all(scratch).unwrap();
}
