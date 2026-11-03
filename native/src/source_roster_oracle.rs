//! Original roster-row fixture replay uses only the admitted historical source and stub contract.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const PATH: &str = "tests/oracles/pack_manifest.py";
const HASH: &str = "f335b1e8ce4f508a40c20d13cffca0cb6c8506fbeb9b0363b53acd094d86ec13";
const FIXTURE: &str = "d173116:pack_manifest._row";
const RECIPE_HASH: &str = "4cb0be295bc5ae632482ed9dfb41172498aee0afeeb6deda52fa48815c0d2a51";

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
        return Err("roster oracle source identity drift; refusing execution".into());
    }
    Ok(())
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    crate::source_fixture::recipe(fixture, FIXTURE, 4, RECIPE_HASH)
}

#[pyclass]
pub(crate) struct Media;
#[pymethods]
impl Media {
    fn telegram_sticker_format(&self, sticker: &Bound<'_, PyDict>) -> PyResult<&'static str> {
        for (flag, format) in [("is_animated", "animated"), ("is_video", "video")] {
            if let Some(value) = sticker.get_item(flag)?
                && value.is_truthy()?
            {
                return Ok(format);
            }
        }
        Ok("static")
    }
}

pub fn check(source: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source)?;
    fixture_identity(fixture)?;
    let cases = crate::source_fixture::cases(fixture, FIXTURE, 4)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let bindings = PyDict::new(py);
        bindings
            .set_item(
                "media",
                Py::new(py, Media).map_err(|_| "cannot bind roster media stub")?,
            )
            .map_err(|_| "cannot bind roster media stub")?;
        let globals = crate::source_oracle_runtime::namespace(
            py,
            source,
            &["_row"],
            &["str", "list"],
            &bindings,
        )
        .map_err(|error| format!("roster oracle namespace failed: {error}"))?;
        let function = globals
            .get_item("_row")
            .map_err(|_| "cannot inspect roster oracle function")?
            .ok_or("missing roster oracle function")?;
        let json_module = py
            .import("json")
            .map_err(|_| "cannot initialize oracle JSON")?;
        for case in cases {
            let input =
                serde_json::to_string(case).map_err(|_| "cannot encode roster oracle input")?;
            let input = json_module
                .call_method1("loads", (input,))
                .and_then(|value| value.cast_into::<PyDict>().map_err(Into::into))
                .map_err(|_| "cannot decode roster oracle input")?;
            let field = |name: &str| -> Result<Bound<'_, PyAny>, String> {
                input
                    .get_item(name)
                    .map_err(|_| "cannot inspect roster oracle input")?
                    .ok_or_else(|| format!("missing roster oracle {name}"))
            };
            let kwargs = PyDict::new(py);
            for (name, original) in [
                ("is_logo", "is_logo"),
                ("name", "label"),
                ("sources", "sources"),
                ("key", "key"),
                ("prior", "prior"),
            ] {
                kwargs
                    .set_item(name, field(original)?)
                    .map_err(|_| "cannot bind roster oracle input")?;
            }
            let observed = function
                .call((field("pos")?, field("st")?), Some(&kwargs))
                .map_err(|error| {
                    format!("roster oracle source failed: {}: {error}", case["name"])
                })?;
            let text: String = json_module
                .call_method1("dumps", (observed,))
                .and_then(|value| value.extract())
                .map_err(|_| "cannot serialize source roster result")?;
            let result: Value =
                serde_json::from_str(&text).map_err(|_| "invalid source roster result")?;
            if case.get("error").is_some() || result != case["expected"] {
                return Err(format!("roster fixture drift: {}", case["name"]));
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_runtime":"CPython3.11.9","source_hash":HASH,"fixture_drift":false}),
        )
    })
}
