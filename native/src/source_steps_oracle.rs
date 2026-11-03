//! Fixed historical plan-step computation and dependency, never native target expectations.
use crate::source_git_inventory::Source;
use crate::source_oracle_runtime::ModuleRegistration;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const PATH: &str = "tests/oracles/plan_apply.py";
const HASH: &str = "73da0b3c0e3a606f7bcdce49fcf1abddb4bc3c18fc6f59faa9344b739ccdefa3";
const FIXTURE: &str = "d173116:plan_apply.compute_steps";
const RECIPE_HASH: &str = "4edf2a8038c8b1e944ef7ca1cf979a28a9721e46b675ee90bef2366a222a5ae9";

pub fn source_identity(source: &Source, intent: &Source) -> Result<(), String> {
    let actual = Sha256::digest(source.text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if source.baseline_revision != BASELINE
        || source.path != PATH
        || source.sha256 != HASH
        || actual != HASH
    {
        return Err("steps oracle source identity drift; refusing execution".into());
    }
    crate::source_intent_oracle::source_identity(intent)
        .map_err(|error| format!("steps oracle dependency: {error}"))
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    crate::source_fixture::recipe(fixture, FIXTURE, 6, RECIPE_HASH)
}

fn namespace<'py>(
    py: Python<'py>,
    source: &Source,
    intent: &Source,
) -> PyResult<(Bound<'py, PyDict>, Bound<'py, PyAny>)> {
    let dependency = crate::source_oracle_runtime::namespace(
        py,
        intent,
        &["PlanError", "target_map"],
        &["__build_class__", "ValueError", "dict", "len"],
        &PyDict::new(py),
    )?;
    let get = |name: &str| {
        dependency.get_item(name)?.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err("missing steps oracle dependency definition")
        })
    };
    let error = get("PlanError")?;
    let dataclasses = py.import("dataclasses")?;
    let bindings = PyDict::new(py);
    bindings.set_item("target_map", get("target_map")?)?;
    bindings.set_item("dataclass", dataclasses.getattr("dataclass")?)?;
    bindings.set_item("field", dataclasses.getattr("field")?)?;
    bindings.set_item("MIXED", "mixed")?;
    bindings.set_item("PER_SET", 200)?;
    // CPython 3.11 dataclass resolves future-string annotations through
    // sys.modules[cls.__module__]. Keep the real decorator/source class intact.
    let mut registration = ModuleRegistration::new(py, &bindings)?;
    let result = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["Refusal", "Steps", "compute_steps", "render"],
        &[
            "__build_class__",
            "Exception",
            "list",
            "dict",
            "set",
            "property",
            "len",
            "sum",
            "map",
            "bool",
            "int",
            "sorted",
            "any",
            "min",
            "max",
            "str",
        ],
        &bindings,
    );
    registration.restore()?;
    Ok((result?, error))
}

fn normalized<'py>(py: Python<'py>, steps: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyDict>> {
    let observed = py
        .import("dataclasses")?
        .call_method1("asdict", (steps,))?
        .cast_into::<PyDict>()?;
    let builtins = py.import("builtins")?;
    for field in ["removals", "held_live"] {
        let rows = PyList::empty(py);
        for row in steps.getattr(field)?.try_iter()? {
            rows.append(builtins.getattr("list")?.call1((row?,))?)?;
        }
        observed.set_item(field, rows)?;
    }
    let adds = PyDict::new(py);
    for (key, value) in steps.getattr("adds")?.cast_into::<PyDict>()?.iter() {
        adds.set_item(builtins.getattr("str")?.call1((key,))?, value)?;
    }
    observed.set_item("adds", adds)?;
    observed.set_item(
        "moved_in",
        builtins
            .getattr("sorted")?
            .call1((steps.getattr("moved_in")?,))?,
    )?;
    for field in ["counted", "pending"] {
        observed.set_item(field, steps.getattr(field)?)?;
    }
    Ok(observed)
}

pub fn check(source: &Source, intent: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source, intent)?;
    fixture_identity(fixture)?;
    let cases = crate::source_fixture::cases(fixture, FIXTURE, 6)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let (globals, plan_error) = namespace(py, source, intent)
            .map_err(|error| format!("steps oracle namespace failed: {error}"))?;
        let function = globals
            .get_item("compute_steps")
            .map_err(|_| "cannot inspect steps oracle function")?
            .ok_or("missing steps oracle function")?;
        let refusal = globals
            .get_item("Refusal")
            .map_err(|_| "cannot inspect steps oracle refusal")?
            .ok_or("missing steps oracle refusal")?;
        let json_module = py
            .import("json")
            .map_err(|_| "cannot initialize oracle JSON")?;
        let mut renders = Vec::new();
        for case in cases {
            let kwargs = PyDict::new(py);
            for field in ["plan", "state", "items", "ids"] {
                let input = serde_json::to_string(&case[field])
                    .map_err(|_| "cannot encode steps oracle input")?;
                kwargs
                    .set_item(
                        field,
                        json_module
                            .call_method1("loads", (input,))
                            .map_err(|_| "cannot decode steps oracle input")?,
                    )
                    .map_err(|_| "cannot bind steps oracle input")?;
            }
            match function.call((), Some(&kwargs)) {
                Ok(steps) if case.get("error").is_none() => {
                    let ids = kwargs
                        .get_item("ids")
                        .map_err(|_| "cannot inspect render IDs")?
                        .ok_or("missing original render IDs")?;
                    let rendered: String =
                        crate::source_tooling_oracle::required(&globals, "render")
                            .and_then(|render| render.call1((&steps, ids, 20)))
                            .and_then(|text| text.extract())
                            .map_err(|error| format!("original step rendering failed: {error}"))?;
                    renders.push(json!({"name":case["name"],"text":rendered}));
                    let observed = normalized(py, &steps)
                        .map_err(|error| format!("cannot normalize source steps: {error}"))?;
                    let text: String = json_module
                        .call_method1("dumps", (observed,))
                        .and_then(|value| value.extract())
                        .map_err(|_| "cannot serialize source steps")?;
                    let observed: Value =
                        serde_json::from_str(&text).map_err(|_| "invalid source steps result")?;
                    if observed != case["expected"] {
                        return Err(format!("steps fixture drift: {}", case["name"]));
                    }
                }
                Err(error)
                    if case.get("error").is_some()
                        && (error.is_instance(py, &refusal)
                            || error.is_instance(py, &plan_error)) =>
                {
                    let text = error
                        .value(py)
                        .str()
                        .and_then(|value| value.extract::<String>())
                        .map_err(|_| "cannot format source steps refusal")?;
                    if case["error"].as_str() != Some(&text) {
                        return Err(format!("steps fixture error drift: {}", case["name"]));
                    }
                }
                Err(error) => {
                    return Err(format!(
                        "steps oracle source failed: {}: {error}",
                        case["name"]
                    ));
                }
                Ok(_) => return Err(format!("steps fixture refusal drift: {}", case["name"])),
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_runtime":"CPython3.11.9",
            "source_hash":HASH,"dependency_hash":intent.sha256,"fixture_drift":false,"renders":renders}),
        )
    })
}
