//! Original shared-renderer row assertion through the production-registered Python callable.
//! Source-comparison, other writer integrations and announcement HTTP cases remain separate.
use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde_json::{Value, json};

#[test]
fn registered_markdown_table_preserves_original_hostile_row_and_empty_dash() {
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
            "tests/test_pack_rows.py",
            "800c96312877a469e224798dd456885d2e3bea4e2dc5077127bd1e91f6a19503",
        ),
        (
            "tests/oracles/pack_rows.py",
            "1c0c0fc15ab5b6abcecb182f138f89da55c566c7210102c1eaaa5ad115343121",
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
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = PyModule::new(py, "_native_reporting_test")?;
        crate::_native(&module)?;
        let callable = module.getattr("markdown_table_json")?;
        assert!(callable.is_callable());
        let rows = json!([{"a":"left|right\nnext line","b":""}]);
        let columns = vec![
            ("a".to_owned(), "A".to_owned(), false),
            ("b".to_owned(), "B".to_owned(), false),
        ];
        let table: String = callable.call1((rows.to_string(), columns))?.extract()?;
        // Exact literal asserted by TheSharedRenderer's original first method.
        assert_eq!(table.lines().nth(2), Some("| left\\|right next line | — |"));
        Ok(())
    })
    .unwrap();
}
