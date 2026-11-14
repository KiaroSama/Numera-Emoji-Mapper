//! Historical pure intent source runs only as an independently hash-pinned development oracle.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PySet};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const PATH: &str = "tests/oracles/panel_plan.py";
const HASH: &str = "4c6b43a35ea5fa5907cc1933524b1a5c79214d12bb13cccc269e34d7ae38b1e4";
// Baseline Git fixture without case outputs: independent of the editable replay document.
const RECIPE_HASH: &str = "01939b9f49d2bda93bf39b38ad2fd897973674fd712fa2cb9d8d2c732df15882";
const FUNCTIONS: &[&str] = &[
    "PlanError",
    "build_plan",
    "target_map",
    "overlay_targets",
    "merge_plan",
];

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
        return Err("intent oracle source identity drift; refusing execution".into());
    }
    Ok(())
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    crate::source_fixture::recipe(fixture, "d173116:emojikit/panel_plan.py", 8, RECIPE_HASH)
}

#[pyclass]
pub(crate) struct FixedClock;
#[pymethods]
impl FixedClock {
    #[staticmethod]
    #[pyo3(signature = (tz=None))]
    fn now<'py>(py: Python<'py>, tz: Option<&Bound<'py, PyAny>>) -> PyResult<Bound<'py, PyAny>> {
        let options = PyDict::new(py);
        options.set_item("tzinfo", tz)?;
        crate::source_oracle_runtime::host_import(py, "datetime")?
            .getattr("datetime")?
            .call((2026, 10, 7), Some(&options))
    }
}

fn namespace<'py>(py: Python<'py>, source: &Source) -> PyResult<Bound<'py, PyDict>> {
    let bindings = PyDict::new(py);
    let datetime = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    datetime.setattr("datetime", py.get_type::<FixedClock>())?;
    datetime.setattr("timezone", py.import("datetime")?.getattr("timezone")?)?;
    bindings.set_item("_dt", datetime)?;
    crate::source_oracle_runtime::namespace(
        py,
        source,
        FUNCTIONS,
        &[
            "__build_class__",
            "ValueError",
            "dict",
            "len",
            "set",
            "sorted",
            "str",
        ],
        &bindings,
    )
}

pub fn check(source: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source)?;
    fixture_identity(fixture)?;
    let cases = crate::source_fixture::cases(fixture, "d173116:emojikit/panel_plan.py", 8)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let globals = namespace(py, source)
            .map_err(|error| format!("intent oracle namespace failed: {error}"))?;
        let json_module = py
            .import("json")
            .map_err(|_| "cannot initialize oracle JSON")?;
        let error_type = globals
            .get_item("PlanError")
            .and_then(|value| {
                value.ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing PlanError"))
            })
            .map_err(|_| "missing oracle error class")?;
        for case in cases {
            let operation = case["operation"]
                .as_str()
                .ok_or("missing intent oracle operation")?;
            let function = match operation {
                "build" => "build_plan",
                "merge" => "merge_plan",
                "targets" => "target_map",
                "overlay" => "overlay_targets",
                _ => return Err("unsupported intent oracle operation".into()),
            };
            let input = case
                .as_object()
                .ok_or("invalid intent case")?
                .iter()
                .filter(|(key, _)| {
                    !matches!(key.as_str(), "name" | "operation" | "expected" | "error")
                })
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<serde_json::Map<_, _>>();
            let input = serde_json::to_string(&input).map_err(|_| "cannot encode oracle input")?;
            let kwargs = json_module
                .call_method1("loads", (input,))
                .and_then(|value| value.cast_into::<PyDict>().map_err(Into::into))
                .map_err(|_| "cannot decode oracle input")?;
            if operation == "merge" {
                for field in ["scope", "live"] {
                    if let Some(value) = kwargs
                        .get_item(field)
                        .map_err(|_| "cannot inspect merge input")?
                        && !value.is_none()
                    {
                        let list = value
                            .cast_into::<PyList>()
                            .map_err(|_| "oracle merge set must be a list")?;
                        kwargs
                            .set_item(
                                field,
                                PySet::new(py, list.iter()).map_err(|_| "invalid oracle set")?,
                            )
                            .map_err(|_| "cannot bind oracle set")?;
                    }
                }
            }
            let function = globals
                .get_item(function)
                .map_err(|_| "cannot inspect oracle function")?
                .ok_or("oracle function missing")?;
            let observed = if operation == "targets" {
                function.call1((kwargs
                    .get_item("previous")
                    .map_err(|_| "missing previous")?
                    .ok_or("missing previous")?,))
            } else if operation == "overlay" {
                function.call1((
                    kwargs
                        .get_item("view")
                        .map_err(|_| "missing view")?
                        .ok_or("missing view")?,
                    kwargs
                        .get_item("previous")
                        .map_err(|_| "missing previous")?
                        .ok_or("missing previous")?,
                ))
            } else {
                function.call((), Some(&kwargs))
            };
            match observed {
                Ok(value) if case.get("error").is_none() => {
                    let text: String = json_module
                        .call_method1("dumps", (value,))
                        .and_then(|value| value.extract())
                        .map_err(|_| "cannot serialize source result")?;
                    let result: Value =
                        serde_json::from_str(&text).map_err(|_| "invalid source result")?;
                    if result != case["expected"] {
                        return Err(format!("intent fixture drift: {}", case["name"]));
                    }
                }
                Err(error) if error.is_instance(py, &error_type) && case.get("error").is_some() => {
                    let text = error
                        .value(py)
                        .str()
                        .and_then(|value| value.extract::<String>())
                        .map_err(|_| "cannot format source refusal")?;
                    if case["error"].as_str() != Some(&text) {
                        return Err(format!("intent fixture error drift: {}", case["name"]));
                    }
                }
                Err(error) => {
                    return Err(format!(
                        "intent oracle source failed: {}: {error}",
                        case["name"]
                    ));
                }
                Ok(_) => return Err(format!("intent fixture refusal drift: {}", case["name"])),
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_runtime":"CPython3.11.9","source_hash":HASH,"fixture_drift":false}),
        )
    })
}
