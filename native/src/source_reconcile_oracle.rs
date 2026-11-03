//! Reproduce source nomination results and callback order with the original controlled evidence.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const PATH: &str = "tests/oracles/collection_reconcile.py";
const HASH: &str = "d22315d90e504d7acd622c02111eb9f2b9b3f33889fd483aa068cfe2b71d3dac";
const FIXTURE: &str = "d173116:collection_reconcile._near_catalog_match";
const RECIPE_HASH: &str = "eba14460b2fe430e7b4df37cebdd80a9c71a3a1970cdc073b6f9e8a8b9323266";

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
        return Err("reconcile oracle source identity drift; refusing execution".into());
    }
    Ok(())
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    crate::source_fixture::recipe_with_observations(fixture, FIXTURE, 8, RECIPE_HASH, &["calls"])
}

#[pyclass]
struct EvidencePath {
    value: String,
}
#[pymethods]
impl EvidencePath {
    fn is_file(&self) -> bool {
        true
    }
}

#[pyclass]
struct PathFactory;
#[pymethods]
impl PathFactory {
    fn __call__(&self, py: Python<'_>, value: String) -> PyResult<Py<EvidencePath>> {
        Py::new(py, EvidencePath { value })
    }
}

#[pyclass]
struct Catalog {
    items: Py<PyList>,
}
#[pymethods]
impl Catalog {
    fn all_items(&self, py: Python<'_>) -> Py<PyList> {
        self.items.clone_ref(py)
    }
}

#[pyclass]
struct Identity {
    probe: Option<u64>,
    comparison: Value,
    calls: Arc<Mutex<Vec<String>>>,
}
#[pymethods]
impl Identity {
    fn perceptual_hash(&self, _path: &Bound<'_, PyAny>, _format: &str) -> Option<u64> {
        self.probe
    }
    fn content_key(&self, _path: &Bound<'_, PyAny>, _format: &str) -> &'static str {
        "v:abc"
    }
    fn hamming(&self, a: u64, b: u64) -> u32 {
        // The admitted original stub inputs are unsigned byte-sized hashes only.
        (a ^ b).count_ones()
    }
    fn same_image(
        &self,
        source: PyRef<'_, EvidencePath>,
        _other: &Bound<'_, PyAny>,
        _format: &str,
    ) -> PyResult<Option<bool>> {
        self.calls
            .lock()
            .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("oracle call recording failed"))?
            .push(source.value.clone());
        match self.comparison.get(&source.value) {
            Some(Value::Bool(value)) => Ok(Some(*value)),
            Some(Value::Null) => Ok(None),
            _ => Err(pyo3::exceptions::PyValueError::new_err(
                "unexpected oracle comparison",
            )),
        }
    }
}

fn observe(py: Python<'_>, source: &Source, case: &Value) -> PyResult<(Value, Vec<String>)> {
    let json_module = py.import("json")?;
    let text = serde_json::to_string(&case["items"])
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid oracle items"))?;
    let input = json_module
        .call_method1("loads", (text,))?
        .cast_into::<PyList>()?;
    let items = PyList::empty(py);
    let namespace = py.import("types")?.getattr("SimpleNamespace")?;
    for item in input.iter() {
        let item = item.cast_into::<PyDict>()?;
        items.append(namespace.call((), Some(&item))?)?;
    }
    let catalog = Py::new(
        py,
        Catalog {
            items: items.unbind(),
        },
    )?;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let identity = Py::new(
        py,
        Identity {
            probe: case["probe"].as_u64(),
            comparison: case["comparison"].clone(),
            calls: calls.clone(),
        },
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("Path", Py::new(py, PathFactory)?)?;
    bindings.set_item("identity", identity)?;
    bindings.set_item("AMBIGUOUS", "ambiguous")?;
    bindings.set_item("UNDECIDABLE", "undecidable")?;
    bindings.set_item("SEARCH_PHASH_TOLERANCE", 2)?;
    let globals = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["_near_catalog_match"],
        &["Exception", "len"],
        &bindings,
    )?;
    let function = globals
        .get_item("_near_catalog_match")?
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing reconciliation oracle"))?;
    let format = case["fmt"]
        .as_str()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid oracle format"))?;
    let result = function.call1((catalog, "download", format))?;
    let text: String = json_module.call_method1("dumps", (result,))?.extract()?;
    let result = serde_json::from_str(&text).map_err(|_| {
        pyo3::exceptions::PyValueError::new_err("invalid source reconciliation result")
    })?;
    let calls = calls
        .lock()
        .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("oracle call recording failed"))?
        .clone();
    Ok((result, calls))
}

pub fn check(source: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source)?;
    fixture_identity(fixture)?;
    let cases = crate::source_fixture::cases(fixture, FIXTURE, 8)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        for case in cases {
            let (result, calls) = observe(py, source, case).map_err(|error| {
                format!("reconcile oracle source failed: {}: {error}", case["name"])
            })?;
            if case.get("error").is_some()
                || result != case["expected"]
                || json!(calls) != case["calls"]
            {
                return Err(format!("reconcile fixture drift: {}", case["name"]));
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_runtime":"CPython3.11.9","source_hash":HASH,"fixture_drift":false}),
        )
    })
}
