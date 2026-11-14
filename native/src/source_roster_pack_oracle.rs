//! Original whole-roster documents and Markdown, including the first-source-result index recipe.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const HASH: &str = "f335b1e8ce4f508a40c20d13cffca0cb6c8506fbeb9b0363b53acd094d86ec13";
const ROWS_HASH: &str = "1c0c0fc15ab5b6abcecb182f138f89da55c566c7210102c1eaaa5ad115343121";
const FIXTURE: &str = "d173116:pack_manifest.build_pack";

pub fn source_identity(source: &Source, rows: &Source) -> Result<(), String> {
    for (source, path, hash) in [
        (source, "tests/oracles/pack_manifest.py", HASH),
        (rows, "tests/oracles/pack_rows.py", ROWS_HASH),
    ] {
        let actual = Sha256::digest(source.text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || actual != hash
        {
            return Err("roster pack oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}
pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    crate::source_roster_pack_recipe::identity(fixture)
}
fn required<'py>(value: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    value.get_item(name)?.ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(format!("missing roster recipe {name}"))
    })
}
fn document<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Value> {
    let text: String = py
        .import("json")?
        .call_method1("dumps", (value,))?
        .extract()?;
    serde_json::from_str(&text)
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid source roster JSON"))
}

#[pyclass]
struct Prefetched {
    pack: Py<PyAny>,
    name: String,
    calls: Arc<AtomicUsize>,
}
#[pymethods]
impl Prefetched {
    fn get_sticker_set(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        if name != self.name || self.calls.fetch_add(1, Ordering::SeqCst) != 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "roster prefetched callback drift",
            ));
        }
        Ok(self.pack.clone_ref(py))
    }
}
#[pyclass]
struct OldIndex {
    text: String,
    reads: Arc<AtomicUsize>,
}
#[pymethods]
impl OldIndex {
    fn __truediv__<'py>(slf: PyRef<'py, Self>, name: &str) -> PyResult<PyRef<'py, Self>> {
        if name != "index.json" {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "unexpected source index lookup",
            ));
        }
        Ok(slf)
    }
    fn read_text(&self, encoding: &str) -> PyResult<String> {
        if encoding != "utf-8" || self.reads.fetch_add(1, Ordering::SeqCst) != 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "source old-index callback drift",
            ));
        }
        Ok(self.text.clone())
    }
}
#[pyfunction]
fn family_inputs<'py>(py: Python<'py>, family: &str) -> PyResult<Bound<'py, PySet>> {
    if family != "general" {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "unexpected source roster family",
        ));
    }
    PySet::new(py, ["catalog.db", "publish_fixture.json"])
}

fn namespace<'py>(
    py: Python<'py>,
    source: &Source,
    rows: &Source,
    index: &Bound<'py, PyDict>,
    reads: Arc<AtomicUsize>,
) -> PyResult<Bound<'py, PyDict>> {
    let bindings = PyDict::new(py);
    bindings.set_item("EMPTY", "—")?;
    let table = crate::source_oracle_runtime::namespace(
        py,
        rows,
        &["_cell", "markdown_table"],
        &["str", "max", "len"],
        &bindings,
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("media", Py::new(py, crate::source_roster_oracle::Media)?)?;
    bindings.set_item(
        "datetime",
        py.get_type::<crate::source_intent_oracle::FixedClock>(),
    )?;
    bindings.set_item("timezone", py.import("datetime")?.getattr("timezone")?)?;
    bindings.set_item("markdown_table", required(&table, "markdown_table")?)?;
    bindings.set_item("json", py.import("json")?)?;
    let old: String = py
        .import("json")?
        .call_method1("dumps", (required(index, "old")?,))?
        .extract()?;
    bindings.set_item("OUT_DIR", Py::new(py, OldIndex { text: old, reads })?)?;
    bindings.set_item("_family_inputs", wrap_pyfunction!(family_inputs, py)?)?;
    crate::source_oracle_runtime::namespace(
        py,
        source,
        &[
            "_row",
            "build_pack",
            "_zero_note",
            "render_markdown",
            "render_index_markdown",
            "_keep_other_family",
        ],
        &[
            "str",
            "list",
            "bool",
            "enumerate",
            "len",
            "OSError",
            "ValueError",
        ],
        &bindings,
    )
}

fn index_result<'py>(
    py: Python<'py>,
    globals: &Bound<'py, PyDict>,
    recipe: &Bound<'py, PyDict>,
    first: &Bound<'py, PyDict>,
) -> PyResult<Value> {
    let summary = PyDict::new(py);
    for key in ["set_name", "title", "family", "pack_index", "link", "count"] {
        summary.set_item(key, required(first, key)?)?;
    }
    let packs = PyList::new(py, [summary])?;
    let current = PyDict::new(py);
    let foreign = PyDict::new(py);
    let retired = PyDict::new(py);
    for row in required(first, "emoji")?.cast_into::<PyList>()?.iter() {
        let row = row.cast_into::<PyDict>()?;
        let cid = required(&row, "custom_emoji_id")?;
        let position = PyDict::new(py);
        position.set_item("set", required(first, "set_name")?)?;
        for field in ["slot", "index"] {
            position.set_item(field, required(&row, field)?)?;
        }
        current.set_item(&cid, position)?;
        for value in required(&row, "source_emoji_ids")?
            .cast_into::<PyList>()?
            .iter()
        {
            foreign.set_item(value, &cid)?;
        }
        for value in required(&row, "previous_custom_emoji_ids")?
            .cast_into::<PyList>()?
            .iter()
        {
            retired.set_item(value, &cid)?;
        }
    }
    let clock_options = PyDict::new(py);
    clock_options.set_item(
        "tz",
        py.import("datetime")?.getattr("timezone")?.getattr("utc")?,
    )?;
    let captured_utc: String = py
        .get_type::<crate::source_intent_oracle::FixedClock>()
        .call_method("now", (), Some(&clock_options))?
        .call_method1("strftime", ("%Y-%m-%d %H:%M:%S UTC",))?
        .extract()?;
    let inputs = required(globals, "_keep_other_family")?.call1((
        required(recipe, "family")?,
        &packs,
        &current,
        &foreign,
        &retired,
        required(recipe, "inputs")?,
    ))?;
    let result = PyDict::new(py);
    result.set_item("captured_utc", captured_utc)?;
    result.set_item("pack_count", packs.len())?;
    let mut count = 0_u64;
    for pack in packs.iter() {
        let pack = pack.cast_into::<PyDict>()?;
        count = count
            .checked_add(required(&pack, "count")?.extract::<u64>()?)
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("roster count overflow"))?;
    }
    result.set_item("emoji_count", count)?;
    result.set_item("packs", &packs)?;
    result.set_item("by_current_id", current)?;
    result.set_item("by_source_id", foreign)?;
    result.set_item("id_changes", retired.len())?;
    result.set_item("by_previous_id", retired)?;
    result.set_item("inputs", inputs)?;
    let markdown: String = required(globals, "render_index_markdown")?
        .call1((&result,))?
        .extract()?;
    Ok(
        json!({"documents":[document(py, first.as_any())?], "expected":document(py, result.as_any())?, "markdown":markdown}),
    )
}

pub fn check(source: &Source, rows: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source, rows)?;
    fixture_identity(fixture)?;
    let recipe = crate::source_roster_pack_recipe::inputs(fixture)?;
    let cases = crate::source_fixture::cases(fixture, FIXTURE, 4)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let observed = (|| -> PyResult<Value> {
            let text = serde_json::to_string(&recipe).map_err(|_| {
                pyo3::exceptions::PyValueError::new_err("invalid roster input recipe")
            })?;
            let input = py
                .import("json")?
                .call_method1("loads", (text,))?
                .cast_into::<PyDict>()?;
            let index = required(&input, "index_case")?.cast_into::<PyDict>()?;
            let reads = Arc::new(AtomicUsize::new(0));
            let globals = namespace(py, source, rows, &index, reads.clone())?;
            let mut first = None;
            let mut observed = Vec::new();
            for case in required(&input, "cases")?.cast_into::<PyList>()?.iter() {
                let case = case.cast_into::<PyDict>()?;
                let record = required(&case, "record")?.cast_into::<PyDict>()?;
                let calls = Arc::new(AtomicUsize::new(0));
                let api = Py::new(
                    py,
                    Prefetched {
                        pack: required(&case, "pack")?.unbind(),
                        name: required(&record, "name")?.extract()?,
                        calls: calls.clone(),
                    },
                )?;
                let result = required(&globals, "build_pack")?
                    .call1((
                        api,
                        &record,
                        required(&case, "family")?,
                        required(&case, "provenance")?,
                        PySet::empty(py)?,
                        required(&case, "prior")?,
                    ))?
                    .cast_into::<PyDict>()?;
                if calls.load(Ordering::SeqCst) != 1 {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "source pack callback was not exercised",
                    ));
                }
                let markdown: String = required(&globals, "render_markdown")?
                    .call1((&result,))?
                    .extract()?;
                observed
                    .push(json!({"expected":document(py, result.as_any())?,"markdown":markdown}));
                if first.is_none() {
                    first = Some(result);
                }
            }
            let first = first.ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err("missing first source roster")
            })?;
            let index = index_result(py, &globals, &index, &first)?;
            if reads.load(Ordering::SeqCst) != 1 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "source old index was not read",
                ));
            }
            Ok(json!({"cases":observed,"index_case":index}))
        })()
        .map_err(|error| format!("roster pack oracle source failed: {error}"))?;
        for (case, result) in cases.iter().zip(
            observed["cases"]
                .as_array()
                .ok_or("invalid source roster results")?,
        ) {
            if case.get("error").is_some()
                || case["expected"] != result["expected"]
                || case["markdown"] != result["markdown"]
            {
                return Err(format!("roster pack fixture drift: {}", case["name"]));
            }
        }
        for field in ["documents", "expected", "markdown"] {
            if fixture["index_case"][field] != observed["index_case"][field] {
                return Err(format!("roster index fixture drift: {field}"));
            }
        }
        Ok(
            json!({"cases_checked":4,"index_cases_checked":1,"source_runtime":"CPython3.11.9","source_hash":HASH,"rows_source_hash":ROWS_HASH,"fixture_drift":false}),
        )
    })
}
