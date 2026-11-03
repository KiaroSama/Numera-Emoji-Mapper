//! Regenerate original archive naming, live rows and both Markdown outputs from admitted source AST.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const ARCHIVE_PATH: &str = "tests/oracles/pack_archive.py";
const ARCHIVE_HASH: &str = "312e32d30bd0b8cdd905b93654a28b28131ce32f768fefd26e9d12aecd7f1c20";
const ROWS_PATH: &str = "tests/oracles/pack_rows.py";
const ROWS_HASH: &str = "1c0c0fc15ab5b6abcecb182f138f89da55c566c7210102c1eaaa5ad115343121";
const FIXTURE: &str = "d173116:pack_archive.names+rows+reports";
// Independently derived from baseline Git after removing only the original observed outputs.
const RECIPE_HASH: &str = "9e3cf45083696b5fd0bf4e073cc825f77497800d640eaba9d6632513a0f25c16";

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    let input = input_recipe(fixture)?;
    crate::source_fixture::recipe_with_observations(
        &input,
        FIXTURE,
        1,
        RECIPE_HASH,
        &["history_markdown", "manifest_markdown"],
    )
}

fn input_recipe(fixture: &Value) -> Result<Value, String> {
    crate::source_fixture::cases(fixture, FIXTURE, 1)?;
    let mut input = fixture.clone();
    input
        .as_object_mut()
        .ok_or("archive fixture requires an object")?
        .remove("archive_name");
    let folders = input
        .get_mut("folders")
        .and_then(Value::as_array_mut)
        .filter(|folders| !folders.is_empty())
        .ok_or("archive fixture requires nonempty folders")?;
    for folder in folders {
        let folder = folder
            .as_object_mut()
            .ok_or("archive folder must be an object")?;
        if folder.get("title").and_then(Value::as_str).is_none() {
            return Err("archive folder requires a title".into());
        }
        // Only this observed field is removed: title, order and unexpected metadata stay pinned.
        folder.remove("folder");
    }
    Ok(input)
}

pub fn source_identity(archive: &Source, rows: &Source) -> Result<(), String> {
    for (source, path, hash) in [
        (archive, ARCHIVE_PATH, ARCHIVE_HASH),
        (rows, ROWS_PATH, ROWS_HASH),
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
            return Err("archive oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub(crate) fn namespace<'py>(
    py: Python<'py>,
    archive: &Source,
    rows: &Source,
) -> PyResult<Bound<'py, PyDict>> {
    let bindings = PyDict::new(py);
    bindings.set_item("EMPTY", "—")?;
    let shared = crate::source_oracle_runtime::namespace(
        py,
        rows,
        &["_cell", "markdown_table", "keyword_cell"],
        &["str", "len", "max"],
        &bindings,
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("LOGO_NAME", "001_logo.png")?;
    bindings.set_item(
        "_UNSAFE",
        py.import("re")?
            .call_method1("compile", (r#"[<>:"/\\|?*\x00-\x1f]"#,))?,
    )?;
    bindings.set_item(
        "log",
        py.import("logging")?
            .call_method1("getLogger", ("pinned_archive_oracle",))?,
    )?;
    for name in ["markdown_table", "keyword_cell"] {
        bindings.set_item(
            name,
            shared.get_item(name)?.ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err("missing original archive table helper")
            })?,
        )?;
    }
    crate::source_oracle_runtime::namespace(
        py,
        archive,
        &[
            "archive_name",
            "_folder_name",
            "_rows",
            "render_history_md",
            "render_manifest_md",
        ],
        &["str", "enumerate", "len"],
        &bindings,
    )
}

fn observe_case<'py>(
    py: Python<'py>,
    globals: &Bound<'py, PyDict>,
    case: &Value,
) -> PyResult<Value> {
    let mut input = case.as_object().cloned().ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err("archive oracle input must be an object")
    })?;
    for field in ["expected", "error", "history_markdown", "manifest_markdown"] {
        input.remove(field);
    }
    let text = serde_json::to_string(&input)
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("cannot encode archive input"))?;
    let json_module = py.import("json")?;
    // Freshly decoded inputs only: source outputs never populate rows, paths or render arguments.
    let input = json_module
        .call_method1("loads", (text,))?
        .cast_into::<PyDict>()?;
    let field = |name: &str| -> PyResult<Bound<'_, PyAny>> {
        input.get_item(name)?.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("missing archive oracle {name}"))
        })
    };
    let options = PyDict::new(py);
    options.set_item("default", py.import("builtins")?.getattr("str")?)?;
    let before: String = json_module
        .getattr("dumps")?
        .call((&input,), Some(&options))?
        .extract()?;
    let items = field("items")?.cast_into::<PyDict>()?;
    let path = py.import("pathlib")?.getattr("Path")?;
    for (_, item) in items.iter() {
        let item = item.cast_into::<PyDict>()?;
        let original = item.get_item("path")?.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err("archive item has no source path")
        })?;
        item.set_item("path", path.call1((original,))?)?;
    }
    let function = |name: &str| -> PyResult<Bound<'_, PyAny>> {
        globals.get_item(name)?.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("missing original archive {name}"))
        })
    };
    let rows =
        function("_rows")?.call1((field("record")?, field("live")?, &items, field("keys")?))?;
    let history: String = function("render_history_md")?
        .call1((field("record")?, &rows))?
        .extract()?;
    let manifest: String = function("render_manifest_md")?
        .call1((field("record")?, &rows, &items))?
        .extract()?;
    let encoded: String = json_module.call_method1("dumps", (&rows,))?.extract()?;
    let expected: Value = serde_json::from_str(&encoded)
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid original archive rows"))?;
    let after: String = json_module
        .getattr("dumps")?
        .call((&input,), Some(&options))?
        .extract()?;
    if before != after {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "original archive mutated its inputs",
        ));
    }
    Ok(json!({"expected":expected,"history_markdown":history,"manifest_markdown":manifest}))
}

pub fn check(archive: &Source, rows: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(archive, rows)?;
    fixture_identity(fixture)?;
    let cases = crate::source_fixture::cases(fixture, FIXTURE, 1)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let globals = namespace(py, archive, rows)
            .map_err(|error| format!("archive oracle namespace failed: {error}"))?;
        for case in cases {
            let observed = observe_case(py, &globals, case).map_err(|error| {
                format!("archive oracle source failed: {}: {error}", case["name"])
            })?;
            if case.get("error").is_some()
                || ["expected", "history_markdown", "manifest_markdown"]
                    .iter()
                    .any(|field| observed.get(*field) != case.get(*field))
            {
                return Err(format!("archive fixture drift: {}", case["name"]));
            }
        }
        let folder_function = globals
            .get_item("_folder_name")
            .map_err(|_| "cannot inspect original folder oracle")?
            .ok_or("missing original folder oracle")?;
        let folders = fixture["folders"]
            .as_array()
            .ok_or("archive fixture requires folders")?;
        for folder in folders {
            let title = folder["title"]
                .as_str()
                .ok_or("archive folder requires title")?;
            let observed = folder_function
                .call1((title,))
                .and_then(|value| value.extract::<String>())
                .map_err(|error| format!("archive folder oracle failed: {error}"))?;
            if folder["folder"].as_str() != Some(&observed) {
                return Err(format!("archive folder fixture drift: {title}"));
            }
        }
        let filename = globals
            .get_item("archive_name")
            .map_err(|_| "cannot inspect original archive name oracle")?
            .ok_or("missing original archive name oracle")?;
        // Fixed call from the original generator; archive_name itself remains historical source.
        let observed = filename
            .call1((7, "animated", format!("a:{}", "b".repeat(32)), ".tgs"))
            .and_then(|value| value.extract::<String>())
            .map_err(|error| format!("archive name oracle failed: {error}"))?;
        if fixture["archive_name"].as_str() != Some(&observed) {
            return Err("archive name fixture drift".into());
        }
        Ok(
            json!({"cases_checked":cases.len(),"folders_checked":folders.len(),
            "source_runtime":"CPython3.11.9","source_hash":ARCHIVE_HASH,
            "rows_source_hash":ROWS_HASH,"fixture_drift":false}),
        )
    })
}
