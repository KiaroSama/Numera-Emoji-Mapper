//! Regenerate the original resume fixture from only the pinned historical validation function.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const PATH: &str = "tests/oracles/collection_state.py";
const HASH: &str = "f8c6e96a1ea7c2fa34484ab5bd7c5cdcda4b7099d86f793acd4464e7284faf99";
const FIXTURE: &str = "d173116:collection_state._validate_state";
// Derived from the baseline Git fixture with only case expected/error fields removed.
const RECIPE_HASH: &str = "eef2e51afff050f934ae7fd244a5ee2c89ac482166d30723f133c185efcd15d4";

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
        return Err("resume oracle source identity drift; refusing execution".into());
    }
    Ok(())
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    crate::source_fixture::recipe(fixture, FIXTURE, 8, RECIPE_HASH)
}

fn namespace<'py>(py: Python<'py>, source: &Source) -> PyResult<Bound<'py, PyDict>> {
    let bindings = PyDict::new(py);
    let bases = PyTuple::new(py, [py.get_type::<pyo3::exceptions::PyRuntimeError>()])?;
    let error =
        py.import("builtins")?
            .getattr("type")?
            .call1(("StateError", bases, PyDict::new(py)))?;
    bindings.set_item("StateError", error)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    let formats = PyDict::new(py);
    for (format, tag) in [("static", "s"), ("video", "v"), ("animated", "a")] {
        formats.set_item(format, tag)?;
    }
    bindings.set_item("FMT_TAG", formats)?;
    bindings.set_item("MIXED", "mixed")?;
    bindings.set_item("PER_SET", 200)?;
    crate::source_oracle_runtime::namespace(
        py,
        source,
        &["_validate_state"],
        &[
            "all",
            "isinstance",
            "str",
            "dict",
            "list",
            "bool",
            "int",
            "set",
            "enumerate",
            "len",
            "sorted",
        ],
        &bindings,
    )
}

pub fn check(source: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source)?;
    fixture_identity(fixture)?;
    let cases = crate::source_fixture::cases(fixture, FIXTURE, 8)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let globals = namespace(py, source)
            .map_err(|error| format!("resume oracle namespace failed: {error}"))?;
        let function = globals
            .get_item("_validate_state")
            .map_err(|_| "cannot inspect resume oracle function")?
            .ok_or("missing resume oracle function")?;
        let error_type = globals
            .get_item("StateError")
            .map_err(|_| "cannot inspect resume oracle error")?
            .ok_or("missing resume oracle error")?;
        let json_module = py
            .import("json")
            .map_err(|_| "cannot initialize oracle JSON")?;
        let path = globals
            .get_item("Path")
            .and_then(|value| {
                value.ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing Path"))
            })
            .and_then(|path| path.call1(("fixture.json",)))
            .map_err(|_| "cannot bind resume oracle path")?;
        for case in cases {
            let input = serde_json::to_string(&case["state"])
                .map_err(|_| "cannot encode resume oracle state")?;
            // A fresh decode per case preserves the original deep-copy mutation boundary.
            let state = json_module
                .call_method1("loads", (input,))
                .map_err(|_| "cannot decode resume oracle state")?;
            match function.call1((&state, &path)) {
                Ok(value) if case.get("error").is_none() => {
                    if !value.is_none() {
                        return Err("resume oracle validation unexpectedly returned a value".into());
                    }
                    let text: String = json_module
                        .call_method1("dumps", (&state,))
                        .and_then(|value| value.extract())
                        .map_err(|_| "cannot serialize source resume state")?;
                    let result: Value =
                        serde_json::from_str(&text).map_err(|_| "invalid source resume state")?;
                    if result != case["expected"] {
                        return Err(format!("resume fixture drift: {}", case["name"]));
                    }
                }
                Err(error) if error.is_instance(py, &error_type) && case.get("error").is_some() => {
                    let text = error
                        .value(py)
                        .str()
                        .and_then(|value| value.extract::<String>())
                        .map_err(|_| "cannot format source resume refusal")?;
                    if case["error"].as_str() != Some(&text) {
                        return Err(format!("resume fixture error drift: {}", case["name"]));
                    }
                }
                Err(error) => {
                    return Err(format!(
                        "resume oracle source failed: {}: {error}",
                        case["name"]
                    ));
                }
                Ok(_) => return Err(format!("resume fixture refusal drift: {}", case["name"])),
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_runtime":"CPython3.11.9","source_hash":HASH,"fixture_drift":false}),
        )
    })
}
