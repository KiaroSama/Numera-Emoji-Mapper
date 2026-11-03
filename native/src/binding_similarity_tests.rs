//! Original similarity assertions at the registered Python boundary, not just Rust policy.
use pyo3::exceptions::{PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyModule};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn fixture() -> Value {
    let inventory: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    assert_eq!(
        inventory["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    for (path, hash) in [
        (
            "tests/test_native_similarity.py",
            "1ab3bc95d93fa181d7214b65ff83cdf9efa89bc00b08aa15761987248cbe3b7e",
        ),
        (
            "tests/_similarity_oracle.py",
            "9be48166b64ab2052798c67e2f7426c2dd0008fc27b4d668a785877fba7ac44e",
        ),
    ] {
        let rows = inventory["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["source"] == path)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1, "original similarity identity: {path}");
        assert_eq!(rows[0]["sha256"], hash);
    }
    let text = include_str!("../../tests/fixtures/similarity.json").replace("\r\n", "\n");
    let hash = Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        hash,
        "34df25f072dc51a8812006223465259493c8499815e8e91d9df52c3f0a680dcb"
    );
    let document: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(document["source"], "695c895");
    assert_eq!(document["greedy"].as_array().unwrap().len(), 6);
    assert_eq!(document["near"].as_array().unwrap().len(), 24);
    document
}

#[test]
fn registered_similarity_preserves_source_results_exact_errors_and_concurrent_calls() {
    let document = fixture();
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = PyModule::new(py, "_native_similarity_test")?;
        crate::_native(&module)?;
        let greedy = module.getattr("greedy_indices")?;
        let near = module.getattr("near_indices")?;
        assert!(greedy.is_callable() && near.is_callable());
        for case in document["greedy"].as_array().unwrap() {
            let hashes: Vec<u64> = serde_json::from_value(case["hashes"].clone()).unwrap();
            let actual: Vec<usize> = greedy.call1((hashes,))?.extract()?;
            assert_eq!(serde_json::to_value(actual).unwrap(), case["expected"]);
        }
        for case in document["near"].as_array().unwrap() {
            let hashes: Vec<Option<u64>> = serde_json::from_value(case["hashes"].clone()).unwrap();
            let actual: Vec<usize> = near
                .call1((
                    hashes,
                    case["query"].as_u64().unwrap(),
                    case["threshold"].as_i64().unwrap(),
                ))?
                .extract()?;
            assert_eq!(serde_json::to_value(actual).unwrap(), case["expected"]);
        }
        let json = py.import("json")?;
        for (input, overflow) in [
            ("-1", true),
            ("18446744073709551616", true),
            ("1.5", false),
            ("\"1\"", false),
        ] {
            let value = json.call_method1("loads", (input,))?;
            let error = greedy.call1((PyList::new(py, [value])?,)).unwrap_err();
            let expected = if overflow {
                py.get_type::<PyOverflowError>()
            } else {
                py.get_type::<PyTypeError>()
            };
            assert!(error.get_type(py).is(&expected), "input {input}: {error}");
        }
        for threshold in [-2, 17] {
            let error = near
                .call1((vec![Some(0_u64)], 0_u64, threshold))
                .unwrap_err();
            assert!(error.get_type(py).is(&py.get_type::<PyValueError>()));
        }
        let callables = [greedy.clone().unbind(), greedy.unbind()];
        // The registered function detaches internally; keeping this attachment while
        // joining would prevent the two callers from acquiring Python in the first place.
        py.detach(|| -> PyResult<()> {
            std::thread::scope(|scope| {
                let workers = callables
                    .into_iter()
                    .enumerate()
                    .map(|(worker, callable)| {
                        let cases = document["greedy"].as_array().unwrap();
                        scope.spawn(move || {
                            Python::attach(|py| -> PyResult<()> {
                                for case in cases.iter().skip(worker).step_by(2) {
                                    let hashes: Vec<u64> =
                                        serde_json::from_value(case["hashes"].clone()).unwrap();
                                    let actual: Vec<usize> =
                                        callable.bind(py).call1((hashes,))?.extract()?;
                                    assert_eq!(
                                        serde_json::to_value(actual).unwrap(),
                                        case["expected"]
                                    );
                                }
                                Ok(())
                            })
                        })
                    })
                    .collect::<Vec<_>>();
                for worker in workers {
                    worker.join().expect("similarity caller panicked")?;
                }
                Ok(())
            })
        })
    })
    .unwrap();
}
