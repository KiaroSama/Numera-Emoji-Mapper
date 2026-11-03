//! Replay the original structured-plan and generic-publisher rewrites from two exact source slices.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const HASH: &str = "3b0faf872a3d7ee9ebfd8324060e7c2ff7541ead4b743fa30e5ebd9939248001";
const GENERIC_HASH: &str = "4cb0211b690af6960a59e6c5ee851f64dc376aa611712fb938f6a87ed8ec8a68";
const FIXTURE: &str = "d173116:state_artifacts.rewrite";
const RECIPE_HASH: &str = "c7490526a6582235e386b7b72d4d81fd01e920e40d718ab8eff25210a212c939";

pub fn source_identity(source: &Source, generic: &Source) -> Result<(), String> {
    for (source, path, hash) in [
        (source, "tests/oracles/state_artifacts.py", HASH),
        (generic, "tests/oracles/collection_migrate.py", GENERIC_HASH),
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
            return Err("state artifacts oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    crate::source_fixture::recipe(fixture, FIXTURE, 3, RECIPE_HASH)
}

pub fn check(source: &Source, generic: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source, generic)?;
    fixture_identity(fixture)?;
    let cases = crate::source_fixture::cases(fixture, FIXTURE, 3)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let observed = (|| -> PyResult<Vec<Value>> {
            let bindings = PyDict::new(py);
            bindings.set_item("copy", py.import("copy")?)?;
            let plan = crate::source_oracle_runtime::namespace(
                py,
                source,
                &["_plan_refs", "remap_plan"],
                &[
                    "ValueError",
                    "isinstance",
                    "dict",
                    "list",
                    "str",
                    "all",
                    "range",
                    "len",
                ],
                &bindings,
            )?;
            let generic = crate::source_oracle_runtime::namespace(
                py,
                generic,
                &["_remap"],
                &["isinstance", "dict", "list", "str"],
                &PyDict::new(py),
            )?;
            let json_module = py.import("json")?;
            cases
                .iter()
                .map(|case| {
                    let text = serde_json::to_string(case).map_err(|_| {
                        pyo3::exceptions::PyValueError::new_err("invalid state artifact case")
                    })?;
                    let input = json_module
                        .call_method1("loads", (text,))?
                        .cast_into::<PyDict>()?;
                    let field = |name: &str| {
                        input.get_item(name)?.ok_or_else(|| {
                            pyo3::exceptions::PyValueError::new_err("missing state artifact input")
                        })
                    };
                    let (namespace, function) = if case["name"] == "pack_plan.json" {
                        (&plan, "remap_plan")
                    } else {
                        (&generic, "_remap")
                    };
                    let result = namespace
                        .get_item(function)?
                        .ok_or_else(|| {
                            pyo3::exceptions::PyValueError::new_err("missing state artifact oracle")
                        })?
                        .call1((field("document")?, field("mapping")?))?;
                    let text: String = json_module.call_method1("dumps", (result,))?.extract()?;
                    serde_json::from_str(&text).map_err(|_| {
                        pyo3::exceptions::PyValueError::new_err("invalid state artifact result")
                    })
                })
                .collect()
        })()
        .map_err(|error| format!("state artifacts oracle source failed: {error}"))?;
        for (case, result) in cases.iter().zip(observed) {
            if case.get("error").is_some() || result != case["expected"] {
                return Err(format!("state artifacts fixture drift: {}", case["name"]));
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_runtime":"CPython3.11.9","source_hash":HASH,"dependency_source_hash":GENERIC_HASH,"fixture_drift":false}),
        )
    })
}
