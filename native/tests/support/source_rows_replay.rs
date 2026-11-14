//! Original renderer observations share the existing Git corpus and canonical registered binding.
use _native::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};
use serde_json::{Value, json};
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, sources: &[Source]) {
    let rows = sources
        .iter()
        .find(|source| source.path == "tests/oracles/pack_rows.py")
        .unwrap();
    let test = sources
        .iter()
        .find(|source| source.path == "tests/test_pack_rows.py")
        .unwrap();
    let unavailable = root.join("logs/tmp/not-a-rows-oracle-executable");
    for dependency in [false, true] {
        let original = if dependency { test } else { rows };
        for mutation in ["revision", "path", "claimed-hash", "bytes"] {
            let mut forged = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => forged.baseline_revision = "0".repeat(40),
                "path" => forged.path = "tests/unverified_rows.py".into(),
                "claimed-hash" => forged.sha256 = "0".repeat(64),
                "bytes" => forged
                    .text
                    .push_str("\nraise AssertionError('must never execute')"),
                _ => unreachable!(),
            }
            let (source, test) = if dependency {
                (rows, &forged)
            } else {
                (&forged, test)
            };
            let error = runtime
                .block_on(_native::source_oracle_worker::generate_rows(
                    root,
                    &unavailable,
                    source,
                    test,
                ))
                .unwrap_err();
            assert!(
                error.contains("source identity drift"),
                "{mutation}: {error}"
            );
            assert!(
                _native::source_rows_oracle::generate(source, test)
                    .unwrap_err()
                    .contains("source identity drift")
            );
        }
    }
    let executable = Path::new(env!("CARGO_BIN_EXE_numera-compat"));
    let report = runtime
        .block_on(_native::source_oracle_worker::generate_rows(
            root, executable, rows, test,
        ))
        .unwrap();
    assert_eq!(report["baseline_revision"], rows.baseline_revision);
    assert_eq!(report["source_hash"], rows.sha256);
    assert_eq!(report["test_source_hash"], test.sha256);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["cases_checked"], 2);
    assert_eq!(report["keywords_checked"], 1);
    let tables = report["tables"].as_array().unwrap();
    assert_eq!(tables.len(), 2);
    assert_eq!(
        tables[0]["source_case"],
        "TheSharedRenderer.test_pipes_and_newlines_are_escaped_and_empty_is_a_dash"
    );
    assert_eq!(
        tables[1]["source_case"],
        "TheSharedRenderer.test_source_splitlines_and_alignment_match_native_table"
    );
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = PyModule::new(py, "_native_source_rows_test")?;
        _native::_native(&module)?;
        let callable = module.getattr("markdown_table_json")?;
        assert!(callable.is_callable());
        let json_module = py.import("json")?;
        for table in tables {
            let input = json_module
                .call_method1("loads", (table["rows"].to_string(),))?
                .cast_into::<PyList>()?;
            let mut rows = Vec::new();
            let columns = table["columns"]
                .as_array()
                .unwrap()
                .iter()
                .map(|column| {
                    (
                        column[0].as_str().unwrap().to_owned(),
                        column[1].as_str().unwrap().to_owned(),
                        column.get(2).and_then(Value::as_str) == Some("right"),
                    )
                })
                .collect::<Vec<_>>();
            for row in input.iter() {
                let row = row.cast_into::<PyDict>()?;
                let mut converted = serde_json::Map::new();
                for (key, _, _) in &columns {
                    let text = match row.get_item(key)? {
                        None => String::new(),
                        Some(value) if value.is_none() => String::new(),
                        // The original wrapper stringifies cells before crossing the JSON binding.
                        Some(value) => value.str()?.to_str()?.to_owned(),
                    };
                    converted.insert(key.clone(), json!(text));
                }
                rows.push(Value::Object(converted));
            }
            let observed: String = callable
                .call1((json!(rows).to_string(), columns))?
                .extract()?;
            assert_eq!(
                observed,
                table["markdown"].as_str().unwrap(),
                "{}",
                table["source_case"]
            );
        }
        Ok(())
    })
    .unwrap();
    let keywords = &report["keywords"];
    assert_eq!(
        keywords["source_case"],
        "TheSharedRenderer.test_every_keyword_is_kept"
    );
    assert_eq!(
        _native::pack_rows::keywords(&json!({"keywords": keywords["input"]})),
        keywords["value"].as_str().unwrap()
    );
}
