//! Original NativeIngestParity through the production-registered NativeCatalog type.
//! Historical fixture regeneration remains a separate worker gate, not performed here.
use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyModule};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new(project: &Path) -> Self {
        let parent =
            crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        loop {
            let path = parent.join(format!(
                "binding ingest {} {}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create binding ingest scratch: {error}"),
            }
        }
    }
    fn clean(&self) -> std::io::Result<()> {
        match std::fs::remove_dir_all(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if let Err(error) = self.clean() {
            eprintln!("Binding ingest scratch retained: {error}");
        }
    }
}

struct CatalogOwner {
    object: Py<PyAny>,
    closed: bool,
}
impl CatalogOwner {
    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if !self.closed {
            self.object.bind(py).call_method0("close")?;
            self.closed = true;
        }
        Ok(())
    }
}
impl Drop for CatalogOwner {
    fn drop(&mut self) {
        if !self.closed {
            Python::attach(|py| {
                if let Err(error) = self.close(py) {
                    eprintln!("Binding ingest catalog cleanup failed: {error}");
                }
            });
        }
    }
}

#[pyclass]
struct Compare {
    names: BTreeMap<String, String>,
    evidence: Py<PyDict>,
    calls: Py<PyList>,
}
#[pymethods]
impl Compare {
    fn __call__(
        &self,
        py: Python<'_>,
        source: &str,
        _incoming: &str,
        _format: &str,
    ) -> PyResult<Py<PyAny>> {
        let filename = Path::new(source)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| PyKeyError::new_err("comparison path has no rival filename"))?;
        let key = self
            .names
            .get(filename)
            .ok_or_else(|| PyKeyError::new_err("unexpected rival filename"))?;
        self.calls.bind(py).append(key)?;
        // The supplied evidence's Python bool/None object must survive unchanged.
        self.evidence
            .bind(py)
            .get_item(key)?
            .map(|value| value.unbind())
            .ok_or_else(|| PyKeyError::new_err("missing comparison evidence"))
    }
}

#[pyclass]
struct Collision {
    value: String,
}
#[pymethods]
impl Collision {
    fn __call__(&self, _path: &str, _key: &str) -> String {
        self.value.clone()
    }
}

fn fixture() -> Value {
    let bytes = include_bytes!("../../tests/fixtures/backend/ingest.json");
    let hash = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        hash,
        "2ec808a2a1210be99d3d161f3a14df5c8a3ab2b0b6895571cd44ec8fec646818"
    );
    let document: Value = serde_json::from_slice(bytes).unwrap();
    crate::source_fixture::cases(&document, "d173116:ingest.catalog_identity", 8).unwrap();
    crate::source_ingest_oracle::fixture_identity(&document).unwrap();
    let inventory: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    assert_eq!(
        inventory["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    for (path, expected) in [
        (
            "tests/test_native_ingest.py",
            "2c8ed0057135b280012708727d47d0f53a182ecdce0e307ba8605dbcad10c35a",
        ),
        (
            "tests/oracles/ingest.py",
            "d2e7740602bd6993e51ab6fd0c0244764ca71e47b9dbafa2c6bdd5895050d0b6",
        ),
        (
            "tests/_similarity_oracle.py",
            "9be48166b64ab2052798c67e2f7426c2dd0008fc27b4d668a785877fba7ac44e",
        ),
    ] {
        let records = inventory["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|record| record["source"] == path)
            .collect::<Vec<_>>();
        assert_eq!(records.len(), 1, "source provenance for {path}");
        assert_eq!(
            records[0]["sha256"], expected,
            "source provenance for {path}"
        );
    }
    document
}

fn observe(
    py: Python<'_>,
    class: &Bound<'_, PyAny>,
    json_module: &Bound<'_, PyModule>,
    project: &Path,
    scratch: &Path,
    index: usize,
    case: &Value,
) -> PyResult<()> {
    let database = scratch.join(index.to_string()).join("catalog.db");
    let mut catalog = CatalogOwner {
        object: class
            .call1((
                database.to_str().unwrap(),
                "fixture UTC",
                project.to_str().unwrap(),
            ))?
            .unbind(),
        closed: false,
    };
    let mut names = BTreeMap::new();
    for (rival, original) in case["items"].as_array().unwrap().iter().enumerate() {
        let filename = format!("rival-{rival}");
        names.insert(
            filename.clone(),
            original["content_key"].as_str().unwrap().to_owned(),
        );
        let mut item = original.clone();
        item["file_path"] = json!(scratch.join(filename));
        let request = json!({"operation":"insert", "item":item, "utc":"fixture UTC"});
        catalog
            .object
            .bind(py)
            .call_method1("call", (request.to_string(),))?;
    }
    let calls = PyList::empty(py);
    let comparison = json_module
        .call_method1("loads", (case["comparison"].to_string(),))?
        .cast_into::<PyDict>()?;
    let compare = Py::new(
        py,
        Compare {
            names,
            evidence: comparison.unbind(),
            calls: calls.clone().unbind(),
        },
    )?;
    let collision = Py::new(
        py,
        Collision {
            value: case["collision"].as_str().unwrap().into(),
        },
    )?;
    let request = json!({"incoming":case["incoming"],"threshold":case["threshold"]});
    let result = catalog
        .object
        .bind(py)
        .call_method1("identify", (request.to_string(), compare, collision));
    let name = case["name"].as_str().unwrap();
    if let Some(expected) = case.get("error") {
        let error = result.unwrap_err();
        assert!(
            error.get_type(py).is(py.get_type::<PyValueError>()),
            "{name}: {error}"
        );
        assert_eq!(
            error.value(py).str()?.to_str()?,
            expected.as_str().unwrap(),
            "{name}"
        );
    } else {
        let value: String = result?.extract()?;
        assert_eq!(
            serde_json::from_str::<Value>(&value).unwrap(),
            case["expected"],
            "{name}"
        );
    }
    assert_eq!(
        json!(calls.extract::<Vec<String>>()?),
        case["calls"],
        "{name}"
    );
    catalog.close(py)?;
    Ok(())
}

#[test]
fn registered_catalog_nominations_unknown_ambiguity_and_collision_match_source() {
    let document = fixture();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let scratch = Scratch::new(project);
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = PyModule::new(py, "_native_ingest_test")?;
        crate::_native(&module)?;
        let class = module.getattr("NativeCatalog")?;
        let json_module = py.import("json")?;
        assert!(class.is_callable());
        for (index, case) in document["cases"].as_array().unwrap().iter().enumerate() {
            observe(py, &class, &json_module, project, &scratch.0, index, case)?;
        }
        Ok(())
    })
    .unwrap();
    scratch.clean().unwrap();
}
