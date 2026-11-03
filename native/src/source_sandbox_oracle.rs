//! Fixed original sandbox policy observations; never creates a clone or starts a panel.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{observed, python, required};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::path::Path;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const SOURCE_HASH: &str = "a37b7609521a78ebca9a6d0a2d1dae7b70fccce2fa50af8d5a16f2d60dfef2ae";

pub fn source_identity(source: &Source) -> Result<(), String> {
    if source.baseline_revision != BASELINE
        || source.path != "scripts/panel_sandbox.py"
        || source.sha256 != SOURCE_HASH
        || crate::codec_raster::digest_hex(source.text.as_bytes()) != SOURCE_HASH
    {
        return Err("sandbox oracle source identity drift; refusing execution".into());
    }
    Ok(())
}

pub fn fixture() -> Result<Value, String> {
    let text = include_str!("../../tests/fixtures/native/tooling-sandbox-source.json")
        .replace("\r\n", "\n");
    if crate::codec_raster::digest_hex(text.as_bytes())
        != "bbc3989cf96b497cc878c3161c2d97b58758c989dfb8a1362288e0b3d2be7e72"
    {
        return Err("pinned sandbox fixture bytes drift".into());
    }
    serde_json::from_str(&text).map_err(|_| "invalid original sandbox fixture".into())
}

fn recipe(document: &Value) -> Result<Value, String> {
    let mut value = document.clone();
    let fields = value
        .as_object_mut()
        .ok_or("sandbox recipe must be an object")?;
    fields.remove("scrubbed");
    let cases = fields
        .get_mut("arguments")
        .and_then(Value::as_array_mut)
        .filter(|cases| cases.len() == 5)
        .ok_or("sandbox recipe requires five argument cases")?;
    for case in cases {
        let fields = case
            .as_object_mut()
            .ok_or("invalid sandbox argument recipe")?;
        for field in ["port", "all", "with_pack", "bot_username", "expected"] {
            fields.remove(field);
        }
    }
    Ok(value)
}

pub fn fixture_identity(document: &Value) -> Result<(), String> {
    let mut value = recipe(document)?;
    value.sort_all_objects();
    let bytes = serde_json::to_vec(&value).map_err(|_| "cannot encode sandbox recipe")?;
    if crate::codec_raster::digest_hex(&bytes)
        != "c2895f4945b7c02d5f797611d22187491bfd358741005c24e2b0cb73dd5a2ce6"
    {
        return Err("sandbox oracle recipe drift; refusing execution".into());
    }
    Ok(())
}

fn execute(py: Python<'_>, workspace: &Path, source: &Source) -> PyResult<Value> {
    let original = fixture().map_err(PyValueError::new_err)?;
    let mut output = recipe(&original).map_err(PyValueError::new_err)?;
    let template = original["template"]
        .as_str()
        .ok_or_else(|| PyValueError::new_err("missing original sandbox template"))?;
    let file = workspace.join(".env.example");
    std::fs::write(&file, template.as_bytes())?;
    let path = py.import("pathlib")?.getattr("Path")?;
    let os = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    let environment = python(py, &original["original"])?;
    os.setattr("environ", &environment)?;
    let re = py.import("re")?;
    let secretish = re.call_method1(
        "compile",
        (
            "TOKEN|SECRET|PASSWORD|_KEY$|^OWNER_ID$",
            re.getattr("IGNORECASE")?,
        ),
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("Path", &path)?;
    bindings.set_item("ROOT", path.call1((workspace,))?)?;
    bindings.set_item("os", os)?;
    bindings.set_item("argparse", py.import("argparse")?)?;
    bindings.set_item("DEFAULT_PORT", 9451)?;
    bindings.set_item("_SECRETISH", secretish)?;
    let globals = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["scrubbed_environment", "build_parser", "panel_arguments"],
        &["set", "str", "int"],
        &bindings,
    )?;
    output["scrubbed"] = observed(&required(&globals, "scrubbed_environment")?.call0()?)?;
    if observed(&environment)? != original["original"] {
        return Err(PyValueError::new_err(
            "original sandbox scrub mutated its input; retained",
        ));
    }
    for case in output["arguments"]
        .as_array_mut()
        .ok_or_else(|| PyValueError::new_err("missing sandbox cases"))?
    {
        let parser = required(&globals, "build_parser")?.call0()?;
        let args = parser.call_method1("parse_args", (python(py, &case["input"])?,))?;
        for field in ["port", "all", "with_pack", "bot_username"] {
            case[field] = observed(&args.getattr(field)?)?;
        }
        case["expected"] = observed(
            &required(&globals, "panel_arguments")?.call1((args, path.call1(("$CLONE",))?))?,
        )?;
    }
    if output != original {
        return Err(PyValueError::new_err(
            "original sandbox policy observation drift; retained",
        ));
    }
    let members = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    if members != [file.clone()] {
        return Err(PyValueError::new_err(
            "unexpected sandbox policy evidence; retained",
        ));
    }
    crate::tooling_environment::reject_links(&file, "sandbox template cannot follow links")
        .map_err(PyValueError::new_err)?;
    if !std::fs::symlink_metadata(&file)?.is_file()
        || std::fs::read(&file)? != template.as_bytes()
        || crate::process_owner::cleanup_pending(workspace)
    {
        return Err(PyValueError::new_err(
            "sandbox policy cleanup is unverified; retained",
        ));
    }
    let report = json!({"source_revision":BASELINE,"source_hash":SOURCE_HASH,
        "source_runtime":"CPython3.11.9","argument_cases":5,"scrub_cases":1,"observations":output,
        "ports":"private synthetic os.environ; actual template/re/argparse and original three policy functions; no clone, panel, account or target policy execution"});
    std::fs::remove_file(file)?;
    Ok(report)
}

pub fn observe(
    root: &Path,
    workspace: &Path,
    source: &Source,
    document: &Value,
) -> Result<Value, String> {
    source_identity(source)?;
    fixture_identity(document)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("sandbox oracle workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect sandbox workspace")?
        .next()
        .is_some()
    {
        return Err("sandbox oracle workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect sandbox bytecode policy")?
        {
            return Err("sandbox oracle requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, source).map_err(|error| {
            format!(
                "original sandbox replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })
}

pub async fn observe_owned(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    source: &Source,
    document: &Value,
) -> Result<Value, String> {
    source_identity(source)?;
    fixture_identity(document)?;
    crate::source_oracle_process::check(
        root,
        executable,
        "tooling-sandbox",
        source,
        &[],
        &json!({"workspace":workspace,"document":document}),
    )
    .await
}
