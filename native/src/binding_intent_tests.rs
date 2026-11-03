//! Original NativeIntent cases through production-registered Python callables.
//! Fixture drift regeneration is a separate historical-worker gate, not performed here.
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn source_metadata() {
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
            "tests/test_native_state.py",
            "f2ffe4070d4002a3dfc9aec2a0c052b4be30b019b8eb06d0a42f1db4c6f76304",
        ),
        (
            "tests/oracles/panel_plan.py",
            "4c6b43a35ea5fa5907cc1933524b1a5c79214d12bb13cccc269e34d7ae38b1e4",
        ),
        (
            "tests/oracles/collection_state.py",
            "f8c6e96a1ea7c2fa34484ab5bd7c5cdcda4b7099d86f793acd4464e7284faf99",
        ),
    ] {
        let records = inventory["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|record| record["source"] == path)
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 1, "source provenance for {path}");
        assert_eq!(records[0]["sha256"], hash, "source provenance for {path}");
    }
}

fn fixture(text: &str, source: &str, expected_hash: &str) -> Value {
    let hash = Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(hash, expected_hash, "independent source fixture drift");
    let document: Value = serde_json::from_str(text).unwrap();
    crate::source_fixture::cases(&document, source, 8).unwrap();
    document
}

fn value_error(py: Python<'_>, error: &PyErr, case: &str) {
    assert!(
        error.get_type(py).is(&py.get_type::<PyValueError>()),
        "{case}: expected exact Python ValueError, got {error}"
    );
}

fn test_intent_matches_pinned_source(
    py: Python<'_>,
    binding: &Bound<'_, PyAny>,
    document: &Value,
) -> PyResult<()> {
    for case in document["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut request = case.clone();
        let fields = request.as_object_mut().unwrap();
        for field in ["name", "expected", "error"] {
            fields.remove(field);
        }
        fields.insert("written_utc".into(), document["utc"].clone());
        let result = binding.call1((request.to_string(),));
        if let Some(expected) = case.get("error") {
            let error = result.unwrap_err();
            value_error(py, &error, name);
            assert_eq!(
                error.value(py).str()?.to_str()?,
                expected.as_str().unwrap(),
                "{name}"
            );
        } else {
            let result: String = result?.extract()?;
            assert_eq!(
                serde_json::from_str::<Value>(&result).unwrap(),
                case["expected"],
                "{name}"
            );
        }
    }
    Ok(())
}

fn test_collection_resume_matches_source(
    py: Python<'_>,
    binding: &Bound<'_, PyAny>,
    document: &Value,
) -> PyResult<()> {
    for case in document["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let request = json!({
            "operation":"collection", "base":"fixture", "state":case["state"]
        });
        let result = binding.call1((request.to_string(),));
        if case.get("error").is_some() {
            // The original resume assertion checks the exception class, not its text.
            value_error(py, &result.unwrap_err(), name);
        } else {
            let result: String = result?.extract()?;
            assert_eq!(
                serde_json::from_str::<Value>(&result).unwrap(),
                case["expected"],
                "{name}"
            );
        }
    }
    Ok(())
}

fn test_invalid_targets_are_refused(py: Python<'_>, binding: &Bound<'_, PyAny>) {
    for target in [
        json!(true),
        json!(false),
        json!(0),
        json!(-1),
        json!(1.5),
        json!("1"),
        Value::Null,
    ] {
        let request = json!({"operation":"targets","previous":{
            "targets":[["a",target]],"moves":[],"held":[]
        }});
        let error = binding.call1((request.to_string(),)).unwrap_err();
        value_error(py, &error, &format!("invalid target {target}"));
    }
}

#[test]
fn registered_native_intent_and_resume_preserve_original_public_json_assertions() {
    source_metadata();
    let intent = fixture(
        include_str!("../../tests/fixtures/backend/intent.json"),
        "d173116:emojikit/panel_plan.py",
        "9217bd704f96051c163d2b889fc1dac122830f0b83e35ee97b7c96f56d361b0b",
    );
    let resume = fixture(
        include_str!("../../tests/fixtures/backend/resume.json"),
        "d173116:collection_state._validate_state",
        "ffd211b0b760b906e6b8d9d84dfde12c274e26730f52bdabe4e802beb1088b9a",
    );
    crate::source_intent_oracle::fixture_identity(&intent).unwrap();
    crate::source_resume_oracle::fixture_identity(&resume).unwrap();
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = PyModule::new(py, "_native_intent_test")?;
        crate::_native(&module)?;
        let intent_json = module.getattr("intent_json")?;
        let resume_json = module.getattr("resume_json")?;
        assert!(intent_json.is_callable() && resume_json.is_callable());
        test_intent_matches_pinned_source(py, &intent_json, &intent)?;
        test_collection_resume_matches_source(py, &resume_json, &resume)?;
        test_invalid_targets_are_refused(py, &intent_json);
        Ok(())
    })
    .unwrap();
}
