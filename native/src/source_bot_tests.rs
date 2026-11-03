//! Exact original pure bot assertion classes; no transport, account or polling execution.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{observed, python, required};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::Path;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 2] = [
    (
        "tests/reference/emoji_bot.py",
        "0ae857dd8607f1e054c6825b3f1e6ecd19eaf2347cfc69a330ae0f6aefd0071f",
    ),
    (
        "tests/test_emoji_bot.py",
        "609454dce3ce7624975d4e56bfa4b05548dbc7ccfc3a2e0c48b8f87d923a5e19",
    ),
];
const CLASSES: [(&str, usize); 5] = [
    ("TestIdsReachingTheHtmlSinkAreDecimalOnly", 3),
    ("TestExtract", 6),
    ("TestBuildPayloads", 10),
    ("TestCopyButtonLimit", 5),
    ("TestIdsTypedAsText", 2),
];
const EXCLUDED: [&str; 2] = [
    "test_it_resolves_the_ids_rather_than_trusting_them",
    "test_when_nothing_resolves_it_says_so_once",
];

pub fn source_identity(source: &Source, test: &Source) -> Result<(), String> {
    for (source, (path, hash)) in [source, test].into_iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("bot assertions source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn execute(py: Python<'_>, source: &Source, test: &Source) -> PyResult<Value> {
    let original = crate::source_bot_oracle::namespace(py, source)?;
    let module = PyModule::new(py, "_original_pure_bot")?;
    module.dict().update(original.as_mapping())?;
    for (name, value) in [("COPY_MAX", 256), ("MSG_MAX", 3500)] {
        module.add(name, value)?;
    }
    let unittest = py.import("unittest")?;
    let bindings = PyDict::new(py);
    bindings.set_item("unittest", &unittest)?;
    bindings.set_item("b", module)?;
    let classes = CLASSES.map(|(name, _)| name);
    let mut definitions = vec!["_msg"];
    definitions.extend(classes);
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &definitions,
        &[
            "__build_class__",
            "enumerate",
            "len",
            "range",
            "str",
            "repr",
        ],
        &bindings,
    )?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    let mut excluded = Vec::new();
    for (name, count) in CLASSES {
        let class = required(&tests, name)?;
        let methods: Vec<String> = loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        let expected = count
            + if name == "TestIdsTypedAsText" {
                EXCLUDED.len()
            } else {
                0
            };
        if methods.len() != expected {
            return Err(PyValueError::new_err("original pure bot collection drift"));
        }
        for method in methods {
            if name == "TestIdsTypedAsText" && EXCLUDED.contains(&method.as_str()) {
                excluded.push(format!("{name}.{method}"));
                continue;
            }
            class.call1((&method,))?.call_method1("run", (&result,))?;
            for field in [
                "errors",
                "failures",
                "skipped",
                "expectedFailures",
                "unexpectedSuccesses",
            ] {
                if result.getattr(field)?.len()? != 0 {
                    return Err(PyValueError::new_err(format!(
                        "original bot {name}.{method} reported {field}"
                    )));
                }
            }
            cases.push(format!("{name}.{method}"));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 26
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || cases.len() != 26
        || excluded.len() != 2
    {
        return Err(PyValueError::new_err(
            "original pure bot assertions incomplete",
        ));
    }
    let a = "5893098741073715471";
    let unicode = "۵۸۹۳۰۹۸۷۴۱۰۷۳۷۱۵۴۷۱";
    let parse = required(&original, "parse_id_list")?;
    let mut observations = Vec::new();
    for text in [
        format!(",{a}"),
        format!(";{a}"),
        format!("{a},"),
        format!("{a};"),
        format!("{a}, \n"),
        format!("\u{1c}{unicode}\u{1f};, {a}\u{1e}"),
        "²".repeat(19),
        "9".repeat(26),
    ] {
        observations.push(json!({"text":text,"ids":observed(&parse.call1((&text,))?)?}));
    }
    let message = json!({"entities":[{"type":"custom_emoji","custom_emoji_id":unicode}]});
    let unicode_ids = observed(
        &required(&original, "extract_custom_emoji_ids")?.call1((python(py, &message)?,))?,
    )?;
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":26,"cases":cases,"excluded":excluded,"errors":0,"failures":0,"skipped":0,
        "expected_failures":0,"unexpected_successes":0,"typed_observations":observations,"unicode_ids":unicode_ids,
        "ports":"unchanged original26pure assertion methods/helpers; two typed transport methods explicitly excluded; no transport, account, environment, files or polling"}),
    )
}

pub fn observe(source: &Source, test: &Source) -> Result<Value, String> {
    source_identity(source, test)?;
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        execute(py, source, test).map_err(|error| {
            format!(
                "original pure bot replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })
}

pub async fn observe_owned(
    root: &Path,
    executable: &Path,
    source: &Source,
    test: &Source,
) -> Result<Value, String> {
    source_identity(source, test)?;
    crate::source_oracle_process::check(
        root,
        executable,
        "bot-tests",
        source,
        &[test],
        &Value::Null,
    )
    .await
}
