//! Reuse the caller's one historical report through actual production-registered Python callables.
//! NativeLease checks are native contracts; original Python maintenance.writer interop stays pending.
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde_json::{Value, json};
use std::path::Path;

const UTC: &str = "2026-10-07 00:00:00 UTC";

struct Handle<'py> {
    value: Bound<'py, PyAny>,
    closed: bool,
}
impl Handle<'_> {
    fn close(&mut self) -> PyResult<()> {
        if !self.closed {
            let result = self.value.call_method0("close")?;
            self.closed = true;
            if !result.is_none() {
                return Err(PyRuntimeError::new_err(
                    "registered close did not return None",
                ));
            }
        }
        Ok(())
    }
}
impl Drop for Handle<'_> {
    fn drop(&mut self) {
        if self.close().is_err() {
            eprintln!("ERROR: registered catalog/lease cleanup failed; caller workspace retained");
        }
    }
}

fn call(binding: &Bound<'_, PyAny>, mut request: Value) -> PyResult<Value> {
    request["utc"] = json!(UTC);
    let text: String = binding
        .call_method1("call", (request.to_string(),))?
        .extract()?;
    Ok(serde_json::from_str(&text).expect("registered catalog must return JSON"))
}

fn runtime_error(py: Python<'_>, error: &PyErr) {
    assert!(
        error.get_type(py).is(py.get_type::<PyRuntimeError>()),
        "expected exact Python RuntimeError, got {error}"
    );
}

fn provenance(report: &Value) {
    assert_eq!(
        report["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    for (field, expected) in [
        (
            "source_hash",
            "5dab2e475754dc219dfe3fd8e98012fffd59ab0c760f646752ff9d35f7188ee2",
        ),
        (
            "paths_source_hash",
            "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3",
        ),
        (
            "test_source_hash",
            "2680e795ff159b3fc1a61f3b5c959c689902adc62311da3d51fe518939453a96",
        ),
    ] {
        assert_eq!(report[field], expected);
    }
    assert_eq!(
        report["case"],
        "NativeCatalogParity.test_order_inclusion_signed_hash_and_family_publications_match_source"
    );
    assert_eq!(report["observation"].as_array().unwrap().len(), 7);
    assert_eq!(
        report["stub_calls"],
        json!({"require_native":1,"writer":1,"enter":1,"exit":1,"catalog_identity_keys":["a","b","c"]})
    );
}

pub fn check(root: &Path, workspace: &Path, report: &Value) {
    provenance(report);
    let root = _native::tooling_environment::physical_root(root).unwrap();
    let workspace = _native::tooling_environment::physical_root(workspace).unwrap();
    assert!(workspace != root && workspace.starts_with(&root));
    let data = workspace.join("bound");
    std::fs::create_dir(&data).expect("exclusive registered catalog workspace");
    let database = data.join("catalog.db");
    let lock = data.join(".maintenance.lock");
    let path = database.to_str().unwrap();
    let project_root = root.to_str().unwrap();
    // One attachment, no source execution or subprocess; the parent owns wall/idle limits and scratch.
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = PyModule::new(py, "_native_catalog_replay")?;
        _native::_native(&module)?;
        assert_eq!(module.getattr("API_VERSION")?.extract::<u32>()?, 1);
        let constructor = module.getattr("NativeCatalog")?;
        let lease_constructor = module.getattr("NativeLease")?;
        assert!(constructor.is_callable() && lease_constructor.is_callable());
        let mut catalog = Handle { value: constructor.call1((path, UTC, project_root))?, closed: false };
        // These are the original test's literal inputs; the report supplies only observed results.
        for (key, phash) in [("a", Some(u64::MAX)), ("b", Some(0)), ("c", None)] {
            assert_eq!(call(&catalog.value, json!({"operation":"insert","item":{
                "content_key":key,"fmt":"static","file_path":workspace.join(format!("{key}.png")),
                "emojis":["😀"],"phash":phash
            }}))?, true);
        }
        let mut actual = vec![
            call(&catalog.value, json!({"operation":"order","keys":["c","a","missing"]}))?,
            call(&catalog.value, json!({"operation":"inclusion","keys":["b"]}))?,
        ];
        for (id, name) in [(Some("111111111"), Some("fixture1")), (None, None)] {
            call(&catalog.value, json!({"operation":"uploaded","content_key":"a",
                "custom_emoji_id":id,"base":"fixture","set_name":name}))?;
        }
        for request in [
            json!({"operation":"all"}),
            json!({"operation":"pending","base":"fixture"}),
            json!({"operation":"pending","base":"other"}),
            json!({"operation":"custom_id","base":"fixture","content_key":"a"}),
            json!({"operation":"stats"}),
        ] {
            actual.push(call(&catalog.value, request)?);
        }
        assert_eq!(json!(actual), report["observation"]);

        // Unlike maintenance.writer, NativeLease bypasses same-thread writer reentrancy.
        let lock_path = lock.to_str().unwrap();
        runtime_error(py, &lease_constructor.call1((lock_path, UTC)).unwrap_err());
        catalog.close()?;
        assert!(lock.is_file(), "closing must not unlink the stable OS lock target");
        let closed = catalog.value.call_method1("call", (json!({"operation":"stats","utc":UTC}).to_string(),)).unwrap_err();
        runtime_error(py, &closed);
        assert_eq!(closed.value(py).str()?.to_str()?, "catalog is closed");
        let mut lease = Handle { value: lease_constructor.call1((lock_path, UTC))?, closed: false };
        runtime_error(py, &constructor.call1((path, UTC, project_root)).unwrap_err());
        lease.close()?;
        let mut reopened = Handle { value: constructor.call1((path, UTC, project_root))?, closed: false };
        reopened.close()?;
        assert!(lock.is_file());
        std::fs::rename(&database, data.join("closed.db")).expect("registered close must release database handles");
        Ok(())
    }).unwrap();
    // Only the outer caller removes its workspace after every source/core/binding assertion succeeds.
}
