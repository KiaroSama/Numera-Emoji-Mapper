//! Native-backed compatibility for retained coin state, locking and atomic persistence.
use crate::{locks::Lease, logging};
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[pyclass]
pub struct NativeStateLease {
    lease: Option<Lease>,
    path: PathBuf,
}
#[pymethods]
impl NativeStateLease {
    #[new]
    fn new(py: Python<'_>, path: String, started: String) -> PyResult<Self> {
        let path = PathBuf::from(path);
        let lease=py.detach(||Lease::acquire(&path,&started)).map_err(|error| {
            let holder=std::fs::read_to_string(&path).ok().and_then(|text|serde_json::from_str::<Value>(text.trim()).ok());
            let who=holder.map(|doc|format!("record names pid {} (started {})",doc["pid"],doc["started"])).unwrap_or_else(||"another run".into());
            pyo3::exceptions::PyRuntimeError::new_err(format!("{} is held by {who}. Refusing to mutate the same pack family concurrently. [{error}]",path.file_name().unwrap_or_default().to_string_lossy()))
        })?;
        Ok(Self {
            lease: Some(lease),
            path,
        })
    }
    fn heartbeat(&self) {
        if let Some(lease) = &self.lease
            && let Ok(file) = lease.clone_file()
        {
            let _ = file
                .set_times(std::fs::FileTimes::new().set_modified(std::time::SystemTime::now()));
        }
    }
    fn descriptor(&self, py: Python<'_>) -> PyResult<i64> {
        let lease = self
            .lease
            .as_ref()
            .ok_or_else(|| pyo3::exceptions::PyRuntimeError::new_err("lease is closed"))?;
        let file = lease
            .clone_file()
            .map_err(|error| pyo3::exceptions::PyOSError::new_err(error.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::fd::IntoRawFd;
            let _ = py;
            Ok(i64::from(file.into_raw_fd()))
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let descriptor = py
                .import("msvcrt")?
                .getattr("open_osfhandle")?
                .call1((file.as_raw_handle() as usize, 0))?
                .extract::<i64>()?;
            // Python CRT owns the duplicate after successful open_osfhandle; original lease remains native.
            use std::os::windows::io::IntoRawHandle;
            let _ = file.into_raw_handle();
            Ok(descriptor)
        }
    }
    fn close(&mut self) {
        self.lease.take();
    }
    fn path(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

#[pyfunction]
pub fn state_validate_json(raw: String, base: String, per_set: i64) -> PyResult<()> {
    let state: Value = serde_json::from_str(&raw)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
    validate(&state, &base, per_set).map_err(pyo3::exceptions::PyValueError::new_err)
}
fn source_int(value: &Value) -> Option<i64> {
    match value {
        Value::Bool(value) => Some(i64::from(*value)),
        _ => value.as_i64(),
    }
}
fn validate(state: &Value, base: &str, per_set: i64) -> Result<(), String> {
    let doc = state.as_object().ok_or("state is not an object")?;
    if state["base"] != base {
        return Err(format!("state belongs to base {:?}", state["base"]));
    }
    if doc.get("done").is_some_and(|value| {
        value
            .as_array()
            .is_none_or(|items| items.iter().any(|item| !item.is_string()))
    }) {
        return Err("'done' must be a list of item keys".into());
    }
    let empty = Vec::new();
    let sets = match doc.get("sets") {
        None => &empty,
        Some(value) => value.as_array().ok_or("'sets' must be a list")?,
    };
    let mut indexes = std::collections::BTreeSet::new();
    let mut last = 0;
    for (i, set) in sets.iter().enumerate() {
        if !set.is_object() {
            return Err(format!("sets[{i}] is not an object"));
        }
        if set["name"].as_str().is_none_or(str::is_empty) {
            return Err(format!("sets[{i}] has no name"));
        }
        let index = source_int(&set["index"])
            .filter(|index| *index >= 1)
            .ok_or_else(|| format!("sets[{i}] has a bad index"))?;
        if !indexes.insert(index) {
            return Err(format!("sets[{i}] repeats index {index}"));
        }
        if index < last {
            return Err(format!("sets[{i}] index {index} goes backwards"));
        }
        last = index;
        if source_int(&set["count"]).is_none_or(|count| count < 0 || count > per_set) {
            return Err(format!("sets[{i}] count outside 0..{per_set}"));
        }
    }
    if let Some(intent) = doc.get("in_flight").filter(|value| !value.is_null()) {
        if !intent.is_object() && !intent.is_string() {
            return Err("'in_flight' must be an intent object or null".into());
        }
        if intent.is_object() {
            for field in ["key", "operation", "set_name"] {
                if !truthy(&intent[field]) {
                    return Err(format!("in_flight is missing '{field}'"));
                }
            }
            if !intent["operation"]
                .as_str()
                .is_some_and(|op| ["add", "create"].contains(&op))
            {
                return Err("in_flight operation is unknown".into());
            }
        }
    }
    Ok(())
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
#[pyfunction]
pub fn state_intent_json(raw: String) -> PyResult<String> {
    let mut value: Value = serde_json::from_str(&raw)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
    if !value.is_object() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "intent must be an object",
        ));
    }
    value["started_utc"] = json!(logging::utc());
    serde_json::to_string(&value)
        .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))
}
#[pyfunction]
pub fn state_write_bytes(py: Python<'_>, path: String, bytes: Vec<u8>) -> PyResult<()> {
    py.detach(|| crate::atomic::write_bytes(Path::new(&path), &bytes))
        .map_err(|error| pyo3::exceptions::PyOSError::new_err(error.to_string()))
}
