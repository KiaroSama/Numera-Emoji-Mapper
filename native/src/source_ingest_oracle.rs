//! Historical ingest decisions execute only after fixed source and independent recipe admission.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const PATH: &str = "tests/oracles/ingest.py";
const HASH: &str = "d2e7740602bd6993e51ab6fd0c0244764ca71e47b9dbafa2c6bdd5895050d0b6";
const FIXTURE: &str = "d173116:ingest.catalog_identity";
// Baseline Git fixture with only source results/errors and recorded comparison calls removed.
const RECIPE_HASH: &str = "7586304d1cf943197c46a2d316e8592e7018831132281a3644320d5694413494";

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    crate::source_fixture::recipe_with_observations(fixture, FIXTURE, 8, RECIPE_HASH, &["calls"])
}

pub fn source_identity(source: &Source, near: &Source) -> Result<(), String> {
    let actual = Sha256::digest(source.text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if source.baseline_revision != BASELINE
        || source.path != PATH
        || source.sha256 != HASH
        || actual != HASH
    {
        return Err("ingest oracle source identity drift; refusing execution".into());
    }
    crate::source_similarity_oracle::source_identity(near)
}

#[pyclass]
struct Catalog {
    #[pyo3(get)]
    phash_threshold: i64,
    items: Py<PyList>,
}
#[pymethods]
impl Catalog {
    fn all_items(&self, py: Python<'_>, _format: &str) -> Py<PyList> {
        self.items.clone_ref(py)
    }
    fn get(&self, py: Python<'_>, key: &str) -> PyResult<Option<Py<PyAny>>> {
        for item in self.items.bind(py).iter() {
            if item.getattr("content_key")?.extract::<String>()? == key {
                return Ok(Some(item.unbind()));
            }
        }
        Ok(None)
    }
}

#[pyclass]
struct Compare {
    evidence: Py<PyDict>,
    calls: Py<PyList>,
}
#[pymethods]
impl Compare {
    fn __call__(
        &self,
        py: Python<'_>,
        source: &Bound<'_, PyAny>,
        _other: &Bound<'_, PyAny>,
        _format: &str,
    ) -> PyResult<Py<PyAny>> {
        let key = source.str()?;
        self.calls.bind(py).append(&key)?;
        // Return the original Python bool/None object: truthiness is not tri-state evidence.
        self.evidence
            .bind(py)
            .get_item(&key)?
            .map(|value| value.unbind())
            .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("unexpected ingest comparison"))
    }
}

#[pyclass]
struct Collision {
    value: String,
}
#[pymethods]
impl Collision {
    fn collision_key(&self, _path: &Bound<'_, PyAny>, _lookup: &str) -> String {
        self.value.clone()
    }
}

fn observe(py: Python<'_>, source: &Source, near: &Source, case: &Value) -> PyResult<Value> {
    let json_module = py.import("json")?;
    let mut recipe = case.as_object().cloned().ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err("ingest oracle input must be an object")
    })?;
    for field in ["expected", "error", "calls"] {
        recipe.remove(field);
    }
    let encoded = serde_json::to_string(&recipe)
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("cannot encode ingest recipe"))?;
    // Decode only inputs freshly per case; neither goldens nor recorded calls reach a stub.
    let input = json_module
        .call_method1("loads", (encoded,))?
        .cast_into::<PyDict>()?;
    let unchanged: String = json_module.call_method1("dumps", (&input,))?.extract()?;
    let field = |key: &str| -> PyResult<Bound<'_, PyAny>> {
        input.get_item(key)?.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("missing ingest oracle {key}"))
        })
    };
    let rows = field("items")?.cast_into::<PyList>()?;
    let items = PyList::empty(py);
    let namespace = py.import("types")?.getattr("SimpleNamespace")?;
    for row in rows.iter() {
        let row = row.cast_into::<PyDict>()?;
        items.append(namespace.call((), Some(&row))?)?;
    }
    let item_refs = items.clone();
    let catalog = Py::new(
        py,
        Catalog {
            phash_threshold: field("threshold")?.extract()?,
            items: items.unbind(),
        },
    )?;
    let calls = PyList::empty(py);
    let comparison = field("comparison")?.cast_into::<PyDict>()?;
    let compare = Py::new(
        py,
        Compare {
            evidence: comparison.unbind(),
            calls: calls.clone().unbind(),
        },
    )?;
    let collision = Py::new(
        py,
        Collision {
            value: field("collision")?.extract()?,
        },
    )?;
    let error = py.import("builtins")?.getattr("type")?.call1((
        "MediaError",
        PyTuple::new(py, [py.get_type::<pyo3::exceptions::PyRuntimeError>()])?,
        PyDict::new(py),
    ))?;
    let near_globals = crate::source_oracle_runtime::namespace(
        py,
        near,
        &["near_indices"],
        &["enumerate"],
        &PyDict::new(py),
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("MediaError", &error)?;
    bindings.set_item(
        "near_indices",
        near_globals.get_item("near_indices")?.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err("missing historical near helper")
        })?,
    )?;
    bindings.set_item("equivalent_files", compare)?;
    bindings.set_item("identity", collision)?;
    let globals = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["catalog_identity"],
        &["set", "enumerate", "len"],
        &bindings,
    )?;
    let function = globals.get_item("catalog_identity")?.ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err("missing historical ingest function")
    })?;
    // These arguments are constants in the original generator, not selected from its outputs.
    let path = py
        .import("pathlib")?
        .getattr("Path")?
        .call1(("incoming",))?;
    let mut observed = json!({});
    match function.call1((catalog, "s:abc", "static", path, 0)) {
        Ok(value) => {
            let result: String = json_module.call_method1("dumps", (value,))?.extract()?;
            observed["expected"] = serde_json::from_str(&result).map_err(|_| {
                pyo3::exceptions::PyValueError::new_err("invalid historical ingest result")
            })?;
        }
        Err(failure) if failure.is_instance(py, &error) => {
            observed["error"] = json!(failure.value(py).str()?.extract::<String>()?);
        }
        Err(failure) => return Err(failure),
    }
    let trace: Vec<String> = calls.extract()?;
    observed["calls"] = json!(trace);
    let after: String = json_module.call_method1("dumps", (&input,))?.extract()?;
    let actual_rows = PyList::empty(py);
    for item in item_refs.iter() {
        actual_rows.append(item.getattr("__dict__")?)?;
    }
    let actual_rows: String = json_module
        .call_method1("dumps", (actual_rows,))?
        .extract()?;
    let original_rows: String = json_module.call_method1("dumps", (&rows,))?.extract()?;
    if after != unchanged || actual_rows != original_rows {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "historical ingest mutated its inputs",
        ));
    }
    Ok(observed)
}

pub fn check(source: &Source, near: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source, near)?;
    fixture_identity(fixture)?;
    let cases = crate::source_fixture::cases(fixture, FIXTURE, 8)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        for case in cases {
            let observed = observe(py, source, near, case).map_err(|error| {
                format!("ingest oracle source failed: {}: {error}", case["name"])
            })?;
            let outcome = if case.get("error").is_some() {
                "error"
            } else {
                "expected"
            };
            if observed.get(outcome) != case.get(outcome) || observed["calls"] != case["calls"] {
                return Err(format!("ingest fixture drift: {}", case["name"]));
            }
        }
        Ok(json!({
            "cases_checked":cases.len(),
            "source_runtime":"CPython3.11.9",
            "source_hash":HASH,
            "near_source_hash":near.sha256,
            "fixture_drift":false
        }))
    })
}
