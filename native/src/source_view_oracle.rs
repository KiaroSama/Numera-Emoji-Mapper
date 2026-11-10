//! Replay the original unbranded, non-seeding view fixture without consulting the native model.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const PATH: &str = "tests/oracles/panel_view.py";
const HASH: &str = "8935c34372dea19ec47d586369344390eecd44f0fc9b65a24f9581f13be5cd27";
const FIXTURE: &str = "d173116:panel_view.build_view";
const RECIPE_HASH: &str = "bfb1d18cfc11a82ec8b35dc454c318b84a14517871e7a8e56fb920e60609d3c2";

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
        return Err("view oracle source identity drift; refusing execution".into());
    }
    Ok(())
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    crate::source_fixture::recipe(fixture, FIXTURE, 3, RECIPE_HASH)
}

#[pyclass]
struct Configuration;
#[pymethods]
impl Configuration {
    #[pyo3(signature=(**_kwargs))]
    fn brand_logo_path(&self, _kwargs: Option<&Bound<'_, PyDict>>) -> Option<String> {
        None
    }
    #[pyo3(signature=(**_kwargs))]
    fn brand_logo_bots<'py>(
        &self,
        py: Python<'py>,
        _kwargs: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        crate::source_oracle_runtime::host_import(py, "builtins")?
            .getattr("frozenset")?
            .call0()
    }
}

#[pyclass]
struct Catalog {
    request: Py<PyDict>,
    calls: Arc<Mutex<Vec<&'static str>>>,
}
impl Catalog {
    fn record(&self, name: &'static str) -> PyResult<()> {
        self.calls
            .lock()
            .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("view call recording failed"))?
            .push(name);
        Ok(())
    }
    fn field<'py>(&self, py: Python<'py>, name: &str) -> PyResult<Bound<'py, PyAny>> {
        self.request
            .bind(py)
            .get_item(name)?
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing view request field"))
    }
}
#[pymethods]
impl Catalog {
    fn all_items<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        self.record("all_items")?;
        let namespace =
            crate::source_oracle_runtime::host_import(py, "types")?.getattr("SimpleNamespace")?;
        let items = self.field(py, "items")?.cast_into::<PyList>()?;
        let output = PyList::empty(py);
        for item in items.iter() {
            let item = item.cast_into::<PyDict>()?;
            output.append(namespace.call((), Some(&item))?)?;
        }
        Ok(output)
    }
    fn published_keys<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PySet>> {
        self.record("published_keys")?;
        let keys = self.field(py, "published")?.cast_into::<PyList>()?;
        PySet::new(py, keys.iter())
    }
    fn published_set_names<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.record("published_set_names")?;
        self.field(py, "published_sets")
    }
}

fn observe<'py>(py: Python<'py>, globals: &Bound<'py, PyDict>, case: &Value) -> PyResult<Value> {
    let json_module = py.import("json")?;
    let request = serde_json::to_string(&case["request"])
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid view request"))?;
    let request = json_module
        .call_method1("loads", (request,))?
        .cast_into::<PyDict>()?;
    let field = |name: &str| {
        request
            .get_item(name)?
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing view request field"))
    };
    if field("branded")?.is_truthy()? || field("logo_path")?.is_truthy()? {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "Branding requires an explicit filesystem/source-oracle fixture",
        ));
    }
    let show_published = field("show_published")?;
    let keep_sets = field("keep_sets")?;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let catalog = Py::new(
        py,
        Catalog {
            request: request.clone().unbind(),
            calls: calls.clone(),
        },
    )?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("seed_order", false)?;
    let result = globals
        .get_item("build_view")?
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing view oracle"))?
        .call((catalog, "", &show_published, &keep_sets), Some(&kwargs))?;
    let mut expected_calls = vec!["all_items"];
    if !show_published.is_truthy()? {
        expected_calls.push("published_keys");
        if keep_sets.is_truthy()? {
            expected_calls.push("published_set_names");
        }
    }
    let actual_calls = calls
        .lock()
        .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("view call recording failed"))?;
    if *actual_calls != expected_calls {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "view oracle callback drift",
        ));
    }
    let by_key = result.get_item(1)?.cast_into::<PyDict>()?;
    let paths = PyDict::new(py);
    for (key, path) in by_key.iter() {
        paths.set_item(key, path.str()?)?;
    }
    let output = PyDict::new(py);
    output.set_item("view", result.get_item(0)?)?;
    output.set_item("by_key", paths)?;
    output.set_item("hidden", result.get_item(2)?)?;
    let text: String = json_module.call_method1("dumps", (output,))?.extract()?;
    serde_json::from_str(&text)
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid source view result"))
}

pub fn check(source: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source)?;
    fixture_identity(fixture)?;
    let cases = crate::source_fixture::cases(fixture, FIXTURE, 3)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let observed = (|| -> PyResult<Vec<Value>> {
            let bindings = PyDict::new(py);
            bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
            bindings.set_item(
                "_PREMIUM_ID",
                py.import("re")?
                    .call_method1("compile", (r"^premium-id:(\d+)$",))?,
            )?;
            bindings.set_item("operator_config", Py::new(py, Configuration)?)?;
            bindings.set_item("LOGO_KEY", "__brand_logo__")?;
            let globals = crate::source_oracle_runtime::namespace(
                py,
                source,
                &["copy_id_for", "build_view"],
                &["len", "set"],
                &bindings,
            )?;
            cases
                .iter()
                .map(|case| observe(py, &globals, case))
                .collect()
        })()
        .map_err(|error| format!("view oracle source failed: {error}"))?;
        for (case, result) in cases.iter().zip(observed) {
            if case.get("error").is_some() || result != case["expected"] {
                return Err(format!("view fixture drift: {}", case["name"]));
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_runtime":"CPython3.11.9","source_hash":HASH,"fixture_drift":false}),
        )
    })
}
