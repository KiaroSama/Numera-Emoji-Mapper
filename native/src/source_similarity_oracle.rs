//! Regenerate the entire original deterministic corpus; never use native similarity for expectations.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const PATH: &str = "tests/_similarity_oracle.py";
const HASH: &str = "9be48166b64ab2052798c67e2f7426c2dd0008fc27b4d668a785877fba7ac44e";

pub fn source_identity(source: &Source) -> Result<(), String> {
    let actual = Sha256::digest(source.text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if source.baseline_revision != BASELINE
        || source.path != PATH
        || source.sha256 != HASH
        || actual != HASH
    {
        return Err("similarity oracle source identity drift; refusing execution".into());
    }
    Ok(())
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    if fixture["source"] != "695c895"
        || fixture["greedy"]
            .as_array()
            .is_none_or(|cases| cases.len() != 6)
        || fixture["near"]
            .as_array()
            .is_none_or(|cases| cases.len() != 24)
    {
        return Err("similarity fixture drift: invalid source corpus".into());
    }
    Ok(())
}

pub fn check(source: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source)?;
    fixture_identity(fixture)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let observed = (|| -> PyResult<Value> {
            let bindings = PyDict::new(py);
            bindings.set_item("random", py.import("random")?)?;
            let globals = crate::source_oracle_runtime::namespace(
                py,
                source,
                &["greedy_indices", "near_indices", "fixtures"],
                &["list", "range", "len", "enumerate"],
                &bindings,
            )?;
            let observed = globals
                .get_item("fixtures")?
                .ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err("missing similarity oracle")
                })?
                .call0()?;
            let text: String = py
                .import("json")?
                .call_method1("dumps", (observed,))?
                .extract()?;
            serde_json::from_str(&text).map_err(|_| {
                pyo3::exceptions::PyValueError::new_err("invalid similarity oracle result")
            })
        })()
        .map_err(|error| format!("similarity oracle source failed: {error}"))?;
        // This original generator owns its inputs too, so compare the complete document.
        if observed != *fixture {
            return Err("similarity fixture drift".into());
        }
        Ok(
            json!({"cases_checked":30,"source_runtime":"CPython3.11.9","source_hash":HASH,"fixture_drift":false}),
        )
    })
}
