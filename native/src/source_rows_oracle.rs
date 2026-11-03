//! Fixed original shared-renderer observations, generated only from admitted historical source.
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const ROWS: &str = "tests/oracles/pack_rows.py";
const ROWS_HASH: &str = "1c0c0fc15ab5b6abcecb182f138f89da55c566c7210102c1eaaa5ad115343121";
const TEST: &str = "tests/test_pack_rows.py";
const TEST_HASH: &str = "800c96312877a469e224798dd456885d2e3bea4e2dc5077127bd1e91f6a19503";

pub fn source_identity(rows: &Source, test: &Source) -> Result<(), String> {
    for (source, path, hash) in [(rows, ROWS, ROWS_HASH), (test, TEST, TEST_HASH)] {
        let actual = Sha256::digest(source.text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || actual != hash
        {
            return Err("shared rows oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn table(
    py: Python<'_>,
    function: &Bound<'_, PyAny>,
    name: &str,
    rows: Value,
    columns: &[(&str, &str, Option<&str>)],
) -> PyResult<Value> {
    let input = PyList::empty(py);
    for row in rows
        .as_array()
        .ok_or_else(|| PyValueError::new_err("invalid fixed row recipe"))?
    {
        let dictionary = PyDict::new(py);
        for (key, value) in row
            .as_object()
            .ok_or_else(|| PyValueError::new_err("invalid fixed row"))?
        {
            match value {
                Value::String(value) => dictionary.set_item(key, value)?,
                Value::Bool(value) => dictionary.set_item(key, *value)?,
                Value::Number(value) => dictionary.set_item(
                    key,
                    value
                        .as_i64()
                        .ok_or_else(|| PyValueError::new_err("invalid fixed row integer"))?,
                )?,
                _ => return Err(PyValueError::new_err("unsupported fixed row input")),
            }
        }
        input.append(dictionary)?;
    }
    let python_columns = PyList::empty(py);
    let mut encoded_columns = Vec::new();
    for &(key, title, alignment) in columns {
        let mut parts = vec![key, title];
        if let Some(alignment) = alignment {
            parts.push(alignment);
        }
        python_columns.append(PyTuple::new(py, parts.iter().copied())?)?;
        encoded_columns.push(json!(parts));
    }
    let markdown: String = function.call1((&input, &python_columns))?.extract()?;
    Ok(json!({
        "name":name,
        "source_case":format!("TheSharedRenderer.{name}"),
        "rows":rows,
        "columns":encoded_columns,
        "markdown":markdown
    }))
}

pub fn generate(rows: &Source, test: &Source) -> Result<Value, String> {
    source_identity(rows, test)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let observed = (|| -> PyResult<Value> {
            let bindings = PyDict::new(py);
            bindings.set_item("EMPTY", "—")?;
            let globals = crate::source_oracle_runtime::namespace(
                py,
                rows,
                &["_cell", "markdown_table", "keyword_cell"],
                &["str", "len", "max"],
                &bindings,
            )?;
            let function = globals
                .get_item("markdown_table")?
                .ok_or_else(|| PyValueError::new_err("missing original markdown_table"))?;
            // These are the two original methods' fixed inputs, not native outputs.
            let tables = [
                table(
                    py,
                    &function,
                    "test_pipes_and_newlines_are_escaped_and_empty_is_a_dash",
                    json!([{"a":"left|right\nnext line","b":""}]),
                    &[("a", "A", None), ("b", "B", None)],
                )?,
                table(
                    py,
                    &function,
                    "test_source_splitlines_and_alignment_match_native_table",
                    json!([{"a":"one\r\ntwo\u{001c}next","b":true},{"a":"","b":123}]),
                    &[("a", "Label", None), ("b", "Count", Some("right"))],
                )?,
            ];
            let keyword_input = ["a", "", "b", "c"];
            let keywords: String = globals
                .get_item("keyword_cell")?
                .ok_or_else(|| PyValueError::new_err("missing original keyword_cell"))?
                .call1((PyList::new(py, keyword_input)?,))?
                .extract()?;
            Ok(json!({
                "baseline_revision":BASELINE,
                "source_runtime":"CPython3.11.9",
                "source_hash":ROWS_HASH,
                "test_source_hash":TEST_HASH,
                "cases_checked":tables.len(),
                "keywords_checked":1,
                "tables":tables,
                "keywords":{
                    "source_case":"TheSharedRenderer.test_every_keyword_is_kept",
                    "input":keyword_input,
                    "value":keywords
                }
            }))
        })();
        observed.map_err(|error| format!("shared rows oracle source failed: {error}"))
    })
}
