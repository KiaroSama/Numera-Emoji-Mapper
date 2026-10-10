#![deny(unsafe_code)]

use pyo3::prelude::*;

pub mod announce;
pub mod archive_catalog;
pub mod archive_check;
pub mod archive_cli;
pub mod archive_move;
pub mod archive_rows;
pub mod archive_sync;
pub mod asset_version;
pub mod atomic;
pub mod bot_payloads;
pub mod brand_logo;
pub mod browser_open;
pub mod catalog;
pub mod catalog_read;
pub mod collection_manifest;
pub mod collection_names;
pub mod collection_notify;
pub mod collector;
pub mod config;
pub mod convert_cli;
pub mod emoji_bot;
pub mod emoji_ids;
pub mod fetch_cli;
pub mod fetch_ids_cli;
pub mod identity_cli;
pub mod identity_invariants;
pub mod identity_references;
pub mod identity_survey;
pub mod ingest;
pub mod intent;
pub mod local_cli;
pub mod locks;
pub mod logging;
pub mod media_adapter;
pub mod media_store;
pub mod migration_apply;
pub mod migration_bundle;
pub mod migration_database;
pub mod migration_files;
pub mod migration_provenance;
pub mod migration_restore;
pub mod migration_signature;
pub mod ownership;
pub mod pack_export;
pub mod pack_gallery;
pub mod pack_rows;
pub mod panel_events;
pub mod panel_http;
pub mod panel_preview;
pub mod panel_save;
pub mod panel_session;
pub mod panel_view;
pub mod paths;
pub mod plan_apply;
pub mod plan_remove;
pub mod plan_status;
pub mod plan_steps;
pub mod process_owner;
pub mod publication;
pub mod publication_recovery;
pub mod publish_cli;
pub mod publish_collection;
pub mod python_repr;
pub mod reconcile;
pub mod resume;
pub mod roster;
pub mod roster_cli;
pub mod roster_index;
pub mod roster_markdown;
pub mod roster_provenance;
pub mod sandbox_lease;
pub mod sqlite_snapshot;
pub mod state_artifacts;
pub mod status;
pub mod sync_order;
pub mod telegram;
pub mod ticker_build;
pub mod ticker_keywords;
pub mod ticker_recovery;
pub mod ticker_state;
pub mod upload_effect;

#[pyclass]
struct NativeTelegram {
    client: telegram::Telegram,
}

#[pymethods]
impl NativeTelegram {
    #[new]
    fn new(token: String, base: String) -> PyResult<Self> {
        telegram::Telegram::new(token, base)
            .map(|client| Self { client })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
    fn read_call(&self, py: Python<'_>, method: String, data: String) -> PyResult<String> {
        let data: std::collections::BTreeMap<String, String> = serde_json::from_str(&data)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        py.detach(|| {
            let value = self
                .client
                .read_call(&method, &data)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            serde_json::to_string(&value)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
        })
    }
    fn probe(&self, py: Python<'_>, name: String) -> PyResult<String> {
        py.detach(|| {
            let (state, value) = self.client.probe(&name);
            let state = match state {
                telegram::SetState::Exists => "exists",
                telegram::SetState::Missing => "missing",
                telegram::SetState::Unknown => "unknown",
            };
            serde_json::to_string(&serde_json::json!([state, value]))
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
        })
    }
}

#[pyfunction]
fn roster_row_json(request: String) -> PyResult<String> {
    let request = serde_json::from_str(&request)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let value = roster::row(&request).map_err(pyo3::exceptions::PyValueError::new_err)?;
    serde_json::to_string(&value)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn bot_json(request: String) -> PyResult<String> {
    let request = serde_json::from_str(&request)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let value = bot_payloads::execute(&request).map_err(pyo3::exceptions::PyValueError::new_err)?;
    serde_json::to_string(&value)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn panel_view_json(request: String) -> PyResult<String> {
    let value = serde_json::from_str(&request)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let output = panel_view::build(value).map_err(pyo3::exceptions::PyValueError::new_err)?;
    serde_json::to_string(&output)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn near_catalog_json(py: Python<'_>, request: String, compare: Py<PyAny>) -> PyResult<String> {
    let request: serde_json::Value = serde_json::from_str(&request)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let items = request["items"]
        .as_array()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing items"))?;
    let fmt = request["fmt"]
        .as_str()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing format"))?;
    let lookup = request["lookup"]
        .as_str()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing lookup"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
    let result = runtime
        .block_on(reconcile::near_catalog(
            items,
            fmt,
            request["probe"].as_u64(),
            lookup,
            async |item: &serde_json::Value| {
                let key = item["content_key"].as_str().ok_or("missing key")?;
                compare
                    .call1(py, (key,))
                    .and_then(|v| v.extract::<Option<bool>>(py))
                    .map_err(|e| e.to_string())
            },
        ))
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    let result = match result {
        reconcile::NearMatch::Missing => serde_json::Value::Null,
        reconcile::NearMatch::Unique(key) => serde_json::json!(key),
        reconcile::NearMatch::Ambiguous => serde_json::json!("ambiguous"),
        reconcile::NearMatch::Undecidable => serde_json::json!("undecidable"),
    };
    serde_json::to_string(&result)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyfunction]
fn plan_steps_json(request: String) -> PyResult<String> {
    let value = serde_json::from_str(&request)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let output = plan_steps::compute(&value).map_err(pyo3::exceptions::PyValueError::new_err)?;
    serde_json::to_string(&output)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pyclass]
struct NativeCatalog {
    catalog: std::sync::Mutex<Option<catalog::Catalog>>,
}

#[pymethods]
impl NativeCatalog {
    #[new]
    fn new(path: String, utc: String, project_root: String) -> PyResult<Self> {
        catalog::Catalog::open(
            std::path::Path::new(&path),
            &utc,
            std::path::Path::new(&project_root),
        )
        .map(|catalog| Self {
            catalog: std::sync::Mutex::new(Some(catalog)),
        })
        .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }
    fn call(&mut self, request: String) -> PyResult<String> {
        let value = serde_json::from_str(&request)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let mut guard = self
            .catalog
            .lock()
            .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("catalog lock is poisoned"))?;
        let catalog = guard
            .as_mut()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("catalog is closed"))?;
        let result = catalog
            .dispatch(value)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
        serde_json::to_string(&result)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
    fn identify(
        &self,
        py: Python<'_>,
        request: String,
        compare: Py<PyAny>,
        collision: Py<PyAny>,
    ) -> PyResult<String> {
        let request: serde_json::Value = serde_json::from_str(&request)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let incoming = request
            .get("incoming")
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing incoming item"))?;
        let threshold = request
            .get("threshold")
            .and_then(serde_json::Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid phash threshold"))?;
        let guard = self
            .catalog
            .lock()
            .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("catalog lock is poisoned"))?;
        let catalog = guard
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("catalog is closed"))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let output = runtime
            .block_on(ingest::identify(
                catalog,
                incoming,
                threshold,
                async |a: &std::path::Path, b: &std::path::Path, fmt: &str| {
                    let a = a.to_str().ok_or("media path is not UTF-8")?;
                    let b = b.to_str().ok_or("media path is not UTF-8")?;
                    let result = compare.call1(py, (a, b, fmt)).map_err(|e| e.to_string())?;
                    result
                        .extract::<Option<bool>>(py)
                        .map_err(|e| e.to_string())
                },
                async |path: &std::path::Path, key: &str| {
                    let path = path.to_str().ok_or("media path is not UTF-8")?;
                    collision
                        .call1(py, (path, key))
                        .and_then(|v| v.extract::<String>(py))
                        .map_err(|e| e.to_string())
                },
            ))
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        serde_json::to_string(&output)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
    fn close(&mut self) -> PyResult<()> {
        self.catalog
            .lock()
            .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("catalog lock is poisoned"))?
            .take();
        Ok(())
    }
}

#[pyclass]
struct NativeLease {
    lease: Option<locks::Lease>,
}

#[pymethods]
impl NativeLease {
    #[new]
    fn new(path: String, started: String) -> PyResult<Self> {
        locks::Lease::acquire(std::path::Path::new(&path), &started)
            .map(|lease| Self { lease: Some(lease) })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
    fn close(&mut self) {
        self.lease.take();
    }
}

#[pyfunction]
fn write_state_json(path: String, contents: String) -> PyResult<()> {
    let value = serde_json::from_str(&contents)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    atomic::write_json(std::path::Path::new(&path), &value)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

#[pyfunction]
fn resume_json(py: Python<'_>, request: String) -> PyResult<String> {
    py.detach(move || {
        let value = serde_json::from_str(&request)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let output = resume::execute(value).map_err(pyo3::exceptions::PyValueError::new_err)?;
        serde_json::to_string(&output)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    })
}

#[pyfunction]
fn intent_json(py: Python<'_>, request: String) -> PyResult<String> {
    py.detach(move || {
        let value = serde_json::from_str(&request)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let output = intent::execute(value).map_err(pyo3::exceptions::PyValueError::new_err)?;
        serde_json::to_string(&output)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    })
}

fn greedy(hashes: &[u64]) -> Vec<usize> {
    if hashes.is_empty() {
        return Vec::new();
    }
    let mut remaining: Vec<usize> = (1..hashes.len()).collect();
    let mut ordered = Vec::with_capacity(hashes.len());
    ordered.push(0);
    let mut last = hashes[0];
    while !remaining.is_empty() {
        let (mut best, mut distance) = (0, 65);
        for (i, &index) in remaining.iter().enumerate() {
            let d = (hashes[index] ^ last).count_ones();
            if d < distance {
                best = i;
                distance = d;
                if d == 0 {
                    break;
                }
            }
        }
        // swap_remove changes first-minimum ties and therefore the pack order.
        let index = remaining.remove(best);
        ordered.push(index);
        last = hashes[index];
    }
    ordered
}

#[pyfunction]
fn greedy_indices(py: Python<'_>, hashes: Vec<u64>) -> Vec<usize> {
    py.detach(move || greedy(&hashes))
}

#[pyfunction]
fn near_indices(
    py: Python<'_>,
    hashes: Vec<Option<u64>>,
    query: u64,
    threshold: i32,
) -> PyResult<Vec<usize>> {
    if threshold != -1 && !(0..=16).contains(&threshold) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "phash threshold must be -1 or 0..16",
        ));
    }
    Ok(py.detach(move || {
        hashes
            .iter()
            .enumerate()
            .filter_map(|(i, hash)| {
                hash.filter(|h| threshold >= 0 && (h ^ query).count_ones() <= threshold as u32)
                    .map(|_| i)
            })
            .collect()
    }))
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("API_VERSION", 1)?;
    m.add_function(wrap_pyfunction!(bot_json, m)?)?;
    m.add_function(wrap_pyfunction!(roster_row_json, m)?)?;
    m.add_class::<NativeLease>()?;
    m.add_class::<NativeCatalog>()?;
    m.add_class::<NativeTelegram>()?;
    m.add_function(wrap_pyfunction!(panel_view_json, m)?)?;
    m.add_function(wrap_pyfunction!(near_catalog_json, m)?)?;
    m.add_function(wrap_pyfunction!(plan_steps_json, m)?)?;
    m.add_function(wrap_pyfunction!(write_state_json, m)?)?;
    m.add_function(wrap_pyfunction!(greedy_indices, m)?)?;
    m.add_function(wrap_pyfunction!(near_indices, m)?)?;
    m.add_function(wrap_pyfunction!(intent_json, m)?)?;
    m.add_function(wrap_pyfunction!(resume_json, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::greedy;

    #[test]
    fn stable_ties_and_empty_input() {
        assert!(greedy(&[]).is_empty());
        assert_eq!(greedy(&[0, 1, 2, 3]), [0, 1, 3, 2]);
        assert_eq!(greedy(&[u64::MAX, u64::MAX, 0]), [0, 1, 2]);
    }
}
