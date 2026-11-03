//! Existing independent Python goldens at the pure native computation seam, without Python code.
use serde_json::Value;
use sha2::{Digest, Sha256};

fn fixture() -> Value {
    // The baseline Git blob is LF; an existing Windows worktree may still contain CRLF.
    let text = include_str!("../../tests/fixtures/similarity.json").replace("\r\n", "\n");
    let bytes = text.as_bytes();
    let hash = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        hash,
        "34df25f072dc51a8812006223465259493c8499815e8e91d9df52c3f0a680dcb"
    );
    let value: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(value["source"], "695c895");
    assert_eq!(value["greedy"].as_array().unwrap().len(), 6);
    assert_eq!(value["near"].as_array().unwrap().len(), 24);
    value
}

#[test]
fn exact_greedy_ties_near_thresholds_and_concurrent_calls_match_all_source_variants() {
    let fixture = fixture();
    let greedy = fixture["greedy"].as_array().unwrap();
    // Exactly two independent workers; no global mutable state or nested runtime pool.
    std::thread::scope(|scope| {
        let workers = (0..2)
            .map(|worker| {
                let greedy = greedy.as_slice();
                scope.spawn(move || {
                    for case in greedy.iter().skip(worker).step_by(2) {
                        let hashes: Vec<u64> =
                            serde_json::from_value(case["hashes"].clone()).unwrap();
                        assert_eq!(
                            serde_json::to_value(_native::similarity::greedy(&hashes)).unwrap(),
                            case["expected"]
                        );
                    }
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker.join().unwrap();
        }
    });
    for case in fixture["near"].as_array().unwrap() {
        let hashes: Vec<Option<u64>> = serde_json::from_value(case["hashes"].clone()).unwrap();
        let actual = _native::similarity::near(
            &hashes,
            case["query"].as_u64().unwrap(),
            case["threshold"].as_i64().unwrap() as i32,
        )
        .unwrap();
        assert_eq!(serde_json::to_value(actual).unwrap(), case["expected"]);
    }
    for threshold in [-2, 17] {
        assert!(_native::similarity::near(&[Some(0)], 0, threshold).is_err());
    }
}
