//! Regenerate bot payload fixtures from the fixed historical pure helpers, never the target.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const PATH: &str = "tests/oracles/emoji_bot.py";
const HASH: &str = "e7ac5c65647d1e3e7b74e7fa8f7b027814439733aef1e1d0baab5033f53b175c";
const FIXTURE: &str = "d173116:emoji_bot.pure_payloads";
const RECIPE_HASH: &str = "fa6b256a78abe5255867d5960d6c9233f7265c99426edca96c988c355d3241d9";
const FUNCTIONS: &[&str] = &[
    "parse_id_list",
    "extract_custom_emoji_ids",
    "_fallback_char",
    "_emoji_span",
    "_batch_ids",
    "_render_message",
    "_copy_text",
    "_chunk_for_copy",
    "_copy_keyboard",
    "build_payloads",
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
        return Err("bot oracle source identity drift; refusing execution".into());
    }
    Ok(())
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    crate::source_fixture::recipe(fixture, FIXTURE, 6, RECIPE_HASH)
}

pub(crate) fn namespace<'py>(py: Python<'py>, source: &Source) -> PyResult<Bound<'py, PyDict>> {
    let bindings = PyDict::new(py);
    bindings.set_item("html", py.import("html")?)?;
    bindings.set_item(
        "log",
        py.import("logging")?
            .call_method1("getLogger", ("bot-oracle",))?,
    )?;
    bindings.set_item("DEFAULT_FALLBACK", "⭐")?;
    for (name, value) in [("PER_ID_COST", 110), ("COPY_MAX", 256), ("MSG_MAX", 3500)] {
        bindings.set_item(name, value)?;
    }
    let regex = py.import("re")?;
    for (name, pattern) in [
        ("_ID_LIST_RE", r"^\d{15,25}(?:[\s,;]+\d{15,25})*$"),
        ("_ID_SEP_RE", r"[\s,;]+"),
        ("_CUSTOM_EMOJI_ID", r"\d{1,25}"),
    ] {
        bindings.set_item(name, regex.call_method1("compile", (pattern,))?)?;
    }
    crate::source_oracle_runtime::namespace(
        py,
        source,
        FUNCTIONS,
        &["set", "str", "max", "range", "len", "enumerate"],
        &bindings,
    )
}

pub fn check(source: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source)?;
    fixture_identity(fixture)?;
    let cases = crate::source_fixture::cases(fixture, FIXTURE, 6)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let globals = namespace(py, source)
            .map_err(|error| format!("bot oracle namespace failed: {error}"))?;
        let json_module = py
            .import("json")
            .map_err(|_| "cannot initialize oracle JSON")?;
        for case in cases {
            let input =
                serde_json::to_string(case).map_err(|_| "cannot encode bot oracle input")?;
            let input = json_module
                .call_method1("loads", (input,))
                .and_then(|value| value.cast_into::<PyDict>().map_err(Into::into))
                .map_err(|_| "cannot decode bot oracle input")?;
            let field = |name: &str| -> Result<Bound<'_, PyAny>, String> {
                input
                    .get_item(name)
                    .map_err(|_| "cannot inspect bot oracle input")?
                    .ok_or_else(|| format!("missing bot oracle {name}"))
            };
            let operation = case["operation"]
                .as_str()
                .ok_or("missing bot oracle operation")?;
            let name = match operation {
                "payloads" => "build_payloads",
                "typed" => "parse_id_list",
                "extract" => "extract_custom_emoji_ids",
                _ => return Err("unsupported bot oracle operation".into()),
            };
            let function = globals
                .get_item(name)
                .map_err(|_| "cannot inspect bot oracle function")?
                .ok_or("missing bot oracle function")?;
            let observed = match operation {
                "payloads" => function.call1((field("ids")?, field("labels")?, field("rich")?)),
                "typed" => function.call1((field("text")?,)),
                "extract" => function.call1((field("message")?,)),
                _ => unreachable!(),
            }
            .map_err(|error| format!("bot oracle source failed: {}: {error}", case["name"]))?;
            let text: String = json_module
                .call_method1("dumps", (observed,))
                .and_then(|value| value.extract())
                .map_err(|_| "cannot serialize source bot result")?;
            let result: Value =
                serde_json::from_str(&text).map_err(|_| "invalid source bot result")?;
            if case.get("error").is_some() || result != case["expected"] {
                return Err(format!("bot fixture drift: {}", case["name"]));
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_runtime":"CPython3.11.9","source_hash":HASH,"fixture_drift":false}),
        )
    })
}
