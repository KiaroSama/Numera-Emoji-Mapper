//! Partial native-boundary mapping of NativeStateCompatibility's single original method.
//! Baseline 1a36a968; tests/test_native_state_compat.py SHA256
//! f656a4216951aa4da735c8550e5448fc31383bd7d1d03b8289fc6e11c8b65b2e.
//! Python wrapper exception subclasses, callbacks/finally, unsupported-object serialization
//! and original-source lock interoperability remain pending; no wrapper is recreated here.
use _native::state_bindings::{
    NativeStateLease, state_intent_json, state_validate_json, state_write_bytes,
};
use pyo3::exceptions::{PyOSError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
        loop {
            let path = parent.join(format!(
                "state compatibility {} {}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("cannot create state compatibility scratch: {error}"),
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
            eprintln!("State compatibility scratch retained: {error}");
        }
    }
}

struct Descriptor {
    os: Py<PyModule>,
    fd: Option<i64>,
}
impl Descriptor {
    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(fd) = self.fd.take() {
            self.os.bind(py).call_method1("close", (fd,))?;
        }
        Ok(())
    }
}
impl Drop for Descriptor {
    fn drop(&mut self) {
        Python::attach(|py| {
            if let Err(error) = self.close(py) {
                eprintln!("State compatibility descriptor cleanup failed: {error}");
            }
        });
    }
}

fn lease_boundary(py: Python<'_>, root: &Path) -> PyResult<()> {
    let path = root.join("family.lock");
    let name = path.to_str().unwrap();
    let lease_type = py.get_type::<NativeStateLease>();
    let lease = lease_type.call1((name, "fixture started UTC"))?;
    let mut descriptor = Descriptor {
        os: py.import("os")?.unbind(),
        fd: Some(lease.call_method0("descriptor")?.extract()?),
    };
    descriptor
        .os
        .bind(py)
        .call_method1("fstat", (descriptor.fd.unwrap(),))?;
    let record: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(record["pid"], std::process::id());
    assert!(record.get("started").is_some());
    let heartbeat = lease.getattr("heartbeat")?;
    assert!(heartbeat.is_callable());
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(10_000);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(old))
        .unwrap();
    assert!(
        std::fs::metadata(&path).unwrap().modified().unwrap()
            < old + std::time::Duration::from_secs(1000)
    );
    heartbeat.call0()?;
    assert!(
        std::fs::metadata(&path).unwrap().modified().unwrap()
            > old + std::time::Duration::from_secs(1000)
    );
    let busy = lease_type.call1((name, "second writer")).unwrap_err();
    assert!(busy.is_instance_of::<PyRuntimeError>(py));
    assert!(!busy.is_instance_of::<PyOSError>(py));
    let message = busy.value(py).str()?.to_str()?.to_owned();
    assert!(message.contains(&format!("record names pid {} ", std::process::id())));
    assert!(message.contains("Refusing to mutate"));
    assert!(message.contains("present when checked"));
    assert!(_native::locks::Lease::acquire(&path, "native writer").is_err());
    let stale = br#"{"pid":-1,"started":"stale diagnostic"}"#;
    std::fs::write(&path, stale).unwrap();
    let busy = lease_type
        .call1((name, "stale record must still refuse"))
        .unwrap_err();
    assert!(busy.is_instance_of::<PyRuntimeError>(py));
    let message = busy.value(py).str()?.to_str()?.to_owned();
    assert!(message.contains("is held by another run."));
    assert_eq!(std::fs::read(&path).unwrap(), stale);
    assert!(_native::locks::Lease::acquire(&path, "record cannot release lease").is_err());

    // The retained Python context manager owns callback-descriptor closing.
    // Close this test-owned duplicate explicitly, without claiming to test that wrapper.
    descriptor.close(py)?;
    lease.call_method0("close")?;
    assert!(path.exists(), "stable lock target must never be deleted");
    let closed = lease.call_method0("descriptor").unwrap_err();
    assert!(closed.is_instance_of::<PyRuntimeError>(py));
    {
        let _held = _native::locks::Lease::acquire(&path, "typed writer").unwrap();
        let busy = lease_type
            .call1((name, "competing binding writer"))
            .unwrap_err();
        assert!(busy.is_instance_of::<PyRuntimeError>(py));
    }
    let next = lease_type.call1((name, "reacquired"))?;
    next.call_method0("close")?;
    Ok(())
}

fn atomic_bytes_boundary(py: Python<'_>, root: &Path) -> PyResult<()> {
    let native_path = root.join("native.json");
    let value = PyDict::new(py);
    value.set_item("label", "é 😀")?;
    value.set_item("real", f64::INFINITY)?;
    value.set_item("keys", ["a", "b"])?;
    let options = PyDict::new(py);
    options.set_item("ensure_ascii", false)?;
    options.set_item("indent", 1)?;
    let text: String = py
        .import("json")?
        .getattr("dumps")?
        .call((&value,), Some(&options))?
        .extract()?;
    // Fixed original input and documented Python formatting, not native output.
    let expected =
        "{\n \"label\": \"é 😀\",\n \"real\": Infinity,\n \"keys\": [\n  \"a\",\n  \"b\"\n ]\n}";
    assert_eq!(text, expected);
    let newline = if cfg!(windows) { "\r\n" } else { "\n" };
    let bytes = text.replace('\n', newline).into_bytes();
    state_write_bytes(py, native_path.to_str().unwrap().into(), bytes)?;
    let before = std::fs::read(&native_path).unwrap();
    assert_eq!(before, expected.replace('\n', newline).as_bytes());
    let blocked = root.join("blocked.json");
    std::fs::create_dir(&blocked).unwrap();
    std::fs::write(blocked.join("keep"), b"retained").unwrap();
    let error = state_write_bytes(
        py,
        blocked.to_str().unwrap().into(),
        b"{\"new\":true}".to_vec(),
    )
    .unwrap_err();
    assert!(error.is_instance_of::<PyOSError>(py));
    assert_eq!(std::fs::read(blocked.join("keep")).unwrap(), b"retained");
    assert_eq!(std::fs::read(&native_path).unwrap(), before);
    assert_eq!(std::fs::read_dir(&blocked).unwrap().count(), 1);
    assert!(
        std::fs::read_dir(root).unwrap().all(|entry| {
            entry
                .unwrap()
                .path()
                .extension()
                .is_none_or(|extension| extension != "tmp")
        }),
        "atomic write left a sibling temporary file"
    );
    Ok(())
}

fn schema_and_intent_boundary(py: Python<'_>) -> PyResult<()> {
    let valid = json!({
        "base":"fixture", "done":[],
        "sets":[{"name":"fixture1","index":1,"count":1}]
    });
    let mut booleans = valid.clone();
    booleans["sets"] = json!([{"name":"fixture1","index":true,"count":false}]);
    for state in [&valid, &booleans] {
        state_validate_json(state.to_string(), "fixture".into(), 200)?;
    }
    let mut null_done = valid.clone();
    null_done["done"] = Value::Null;
    let mut zero_index = valid.clone();
    zero_index["sets"] = json!([{"name":"fixture1","index":0,"count":1}]);
    let mut replace_intent = valid;
    replace_intent["in_flight"] = json!({
        "key":"a", "operation":"replace", "set_name":"fixture"
    });
    for (case, state) in [
        ("null done", null_done),
        ("zero index", zero_index),
        ("replace resume operation", replace_intent),
    ] {
        let error = state_validate_json(state.to_string(), "fixture".into(), 200).unwrap_err();
        // This is the actual PyO3 error, not proof of retained StateInvalid mapping.
        assert!(error.is_instance_of::<PyValueError>(py), "{case}: {error}");
    }
    let fields = concat!(
        "{\"key\":\"a\",\"operation\":\"replace\",\"set_name\":\"fixture\",",
        "\"set_index\":1,\"expected_before\":1,\"title\":\"\",\"format\":\"static\"}"
    );
    let actual = state_intent_json(fields.into())?;
    let mut intent: Value = serde_json::from_str(&actual).unwrap();
    assert_eq!(
        intent["operation"], "replace",
        "intent builder must not impose publisher-only operation validation"
    );
    assert!(intent["started_utc"].as_str().unwrap().ends_with(" UTC"));
    intent.as_object_mut().unwrap().remove("started_utc");
    assert_eq!(intent, serde_json::from_str::<Value>(fields).unwrap());
    Ok(())
}

#[test]
fn original_state_shape_cases_preserve_every_acceptance_and_refusal_without_mutation() {
    let valid = json!({
        "base":"t", "per_set":200, "done":["a"], "sets":[{
            "name":"t1_by_bot", "title":"T 1", "count":3, "index":1
        }], "in_flight":null
    });
    let mut cases = vec![(valid.clone(), true)];
    let mut wrong_base = valid.clone();
    wrong_base["base"] = json!("other");
    cases.push((wrong_base, false));
    for count in [-1, 201] {
        let mut state = valid.clone();
        state["sets"] = json!([{"name":"n","title":"","count":count,"index":1}]);
        cases.push((state, false));
    }
    for first in [1, 2] {
        let mut state = valid.clone();
        state["sets"] = json!([
            {"name":"a","title":"","count":1,"index":first},
            {"name":"b","title":"","count":1,"index":1}
        ]);
        cases.push((state, false));
    }
    for intent in [
        json!({"key":"a"}),
        json!({"key":"a","operation":"add"}),
        json!({"key":"a","operation":"wat","set_name":"s"}),
    ] {
        let mut state = valid.clone();
        state["in_flight"] = intent;
        cases.push((state, false));
    }
    let mut invalid_done = valid;
    invalid_done["done"] = json!([1, 2]);
    cases.push((invalid_done, false));
    assert_eq!(cases.len(), 10);
    Python::initialize();
    Python::attach(|py| {
        for (state, accepted) in cases {
            let original = state.clone();
            let raw = state.to_string();
            let result = state_validate_json(raw.clone(), "t".into(), 200);
            if accepted {
                result.unwrap();
            } else {
                let error = result.unwrap_err();
                assert!(error.is_instance_of::<PyValueError>(py));
                assert!(!error.is_instance_of::<PyOSError>(py));
            }
            assert_eq!(state, original);
            assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), original);
        }
    });
}

#[test]
fn test_exported_state_lease_callback_and_atomic_bytes_native_boundary_subset() {
    let scratch = Scratch::new();
    Python::initialize();
    Python::attach(|py| {
        lease_boundary(py, &scratch.0)?;
        atomic_bytes_boundary(py, &scratch.0)?;
        schema_and_intent_boundary(py)
    })
    .unwrap();
    scratch.clean().unwrap();
}
