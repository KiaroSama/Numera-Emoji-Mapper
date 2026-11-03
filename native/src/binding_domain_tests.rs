//! Original pure JSON domain assertions through production-registered Python callables.
//! Historical regeneration, source-drift and generator-isolation gates remain separate.
use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};
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
            "tests/test_native_bot.py",
            "861a038f2f2a806b976674fac3eb72f6b23c80166942247f933a0be835dd8afd",
        ),
        (
            "tests/test_native_roster.py",
            "a258bbd2f5da9c2530f78779363e83d0d872e6f69cfa78e8130a777eb0a3456b",
        ),
        (
            "tests/test_native_plan_steps.py",
            "c888549d995db0b0a6858d7343e98c38d789ae71fc88753a994b9376b7f89041",
        ),
        (
            "tests/test_native_panel.py",
            "185be0d2bfc8132b71a016e5abdeb6f58f685cabf9bc82be9183ef184cc0f4a9",
        ),
        (
            "tests/test_native_reconcile.py",
            "9ca69df8d206f1d4fbf5f482f1334996c483e711ca78eb02e5a38331db004bf6",
        ),
        (
            "tests/oracles/emoji_bot.py",
            "e7ac5c65647d1e3e7b74e7fa8f7b027814439733aef1e1d0baab5033f53b175c",
        ),
        (
            "tests/oracles/pack_manifest.py",
            "f335b1e8ce4f508a40c20d13cffca0cb6c8506fbeb9b0363b53acd094d86ec13",
        ),
        (
            "tests/oracles/plan_apply.py",
            "73da0b3c0e3a606f7bcdce49fcf1abddb4bc3c18fc6f59faa9344b739ccdefa3",
        ),
        (
            "tests/oracles/panel_plan.py",
            "4c6b43a35ea5fa5907cc1933524b1a5c79214d12bb13cccc269e34d7ae38b1e4",
        ),
        (
            "tests/oracles/panel_view.py",
            "8935c34372dea19ec47d586369344390eecd44f0fc9b65a24f9581f13be5cd27",
        ),
        (
            "tests/oracles/collection_reconcile.py",
            "d22315d90e504d7acd622c02111eb9f2b9b3f33889fd483aa068cfe2b71d3dac",
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

fn fixture(bytes: &[u8], hash: &str, source: &str, count: usize) -> Value {
    let actual = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(actual, hash, "independent source fixture drift");
    let document: Value = serde_json::from_slice(bytes).unwrap();
    crate::source_fixture::cases(&document, source, count).unwrap();
    document
}

fn json_cases(
    py: Python<'_>,
    module: &Bound<'_, PyModule>,
    binding: &str,
    document: &Value,
) -> PyResult<()> {
    let callable = module.getattr(binding)?;
    assert!(callable.is_callable(), "{binding}");
    for case in document["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut request = case.clone();
        match binding {
            "panel_view_json" => request = case["request"].clone(),
            "plan_steps_json" => {
                let fields = request.as_object_mut().unwrap();
                for field in ["name", "expected", "error"] {
                    fields.remove(field);
                }
            }
            _ => {}
        }
        let result = callable.call1((request.to_string(),));
        if let Some(expected) = case.get("error") {
            let error = result.unwrap_err();
            assert!(
                error.get_type(py).is(&py.get_type::<PyValueError>()),
                "{binding}/{name}: {error}"
            );
            assert_eq!(
                error.value(py).str()?.to_str()?,
                expected.as_str().unwrap(),
                "{binding}/{name}"
            );
        } else {
            let result: String = result?.extract()?;
            assert_eq!(
                serde_json::from_str::<Value>(&result).unwrap(),
                case["expected"],
                "{binding}/{name}"
            );
        }
    }
    Ok(())
}

#[pyclass]
struct Compare {
    evidence: Py<PyDict>,
    calls: Py<PyList>,
}
#[pymethods]
impl Compare {
    fn __call__(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        self.calls.bind(py).append(key)?;
        // Return the supplied Python bool/None, never its truthiness or a golden result.
        self.evidence
            .bind(py)
            .get_item(key)?
            .map(|value| value.unbind())
            .ok_or_else(|| PyKeyError::new_err("missing reconciliation comparison evidence"))
    }
}

fn reconciliation(py: Python<'_>, module: &Bound<'_, PyModule>, document: &Value) -> PyResult<()> {
    let callable = module.getattr("near_catalog_json")?;
    assert!(callable.is_callable());
    let json_module = py.import("json")?;
    for case in document["cases"].as_array().unwrap() {
        let calls = PyList::empty(py);
        let evidence = json_module
            .call_method1("loads", (case["comparison"].to_string(),))?
            .cast_into::<PyDict>()?;
        let compare = Py::new(
            py,
            Compare {
                evidence: evidence.unbind(),
                calls: calls.clone().unbind(),
            },
        )?;
        let result: String = callable.call1((case.to_string(), compare))?.extract()?;
        assert_eq!(
            serde_json::from_str::<Value>(&result).unwrap(),
            case["expected"],
            "{}",
            case["name"]
        );
        assert_eq!(
            json!(calls.extract::<Vec<String>>()?),
            case["calls"],
            "{}",
            case["name"]
        );
    }
    Ok(())
}

#[test]
fn registered_domain_json_bindings_preserve_original_results_refusals_and_callbacks() {
    source_metadata();
    let bot = fixture(
        include_bytes!("../../tests/fixtures/backend/bot.json"),
        "00f3a5e7f703fc95fb027bb6ad1278a428c98f801f6619214a1a5933b37c04ba",
        "d173116:emoji_bot.pure_payloads",
        6,
    );
    let roster = fixture(
        include_bytes!("../../tests/fixtures/backend/roster.json"),
        "039eee42bf316e1f8ad21f6d6f2dca7d8a2be99970381dadfaa9583256f907b1",
        "d173116:pack_manifest._row",
        4,
    );
    let steps = fixture(
        include_bytes!("../../tests/fixtures/backend/steps.json"),
        "63e38f56182a4e98aa3883e9ababd1cba53a7baac3b645f2125be87e35a000c3",
        "d173116:plan_apply.compute_steps",
        6,
    );
    let view = fixture(
        include_bytes!("../../tests/fixtures/backend/view.json"),
        "4c54a770153bc4c3887512215f689f571e37ae2311a22dd5aa6bdcba93359122",
        "d173116:panel_view.build_view",
        3,
    );
    let near = fixture(
        include_bytes!("../../tests/fixtures/backend/reconcile.json"),
        "bc430cc2de230c5e5791601be1e9297e0d6f4c9702134e355f3d56a5465a27a7",
        "d173116:collection_reconcile._near_catalog_match",
        8,
    );
    crate::source_bot_oracle::fixture_identity(&bot).unwrap();
    crate::source_roster_oracle::fixture_identity(&roster).unwrap();
    crate::source_steps_oracle::fixture_identity(&steps).unwrap();
    crate::source_view_oracle::fixture_identity(&view).unwrap();
    crate::source_reconcile_oracle::fixture_identity(&near).unwrap();
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = PyModule::new(py, "_native_domain_test")?;
        crate::_native(&module)?;
        for (binding, document) in [
            ("bot_json", &bot),
            ("roster_row_json", &roster),
            ("plan_steps_json", &steps),
            ("panel_view_json", &view),
        ] {
            json_cases(py, &module, binding, document)?;
        }
        reconciliation(py, &module, &near)
    })
    .unwrap();
}
