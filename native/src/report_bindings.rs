//! Shared table rendering for retained coin manifests; no second Python renderer.
use pyo3::prelude::*;
use serde_json::Value;
#[pyfunction]
pub fn markdown_table_json(rows: String, columns: Vec<(String, String, bool)>) -> PyResult<String> {
    let rows: Vec<Value> = serde_json::from_str(&rows)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
    if rows.iter().any(|row| !row.is_object()) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "table rows must be objects",
        ));
    }
    let columns = columns
        .iter()
        .map(|(key, title, right)| (key.as_str(), title.as_str(), *right))
        .collect::<Vec<_>>();
    Ok(crate::pack_rows::aligned(&rows, &columns))
}
