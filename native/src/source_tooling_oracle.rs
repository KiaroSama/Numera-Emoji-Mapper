//! Fixed historical tooling recipes; original functions, never native policies, generate observations.
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Component, Path};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 2] = [
    (
        "scripts/local_environment.py",
        "b568feffc0c75e27b7f08cb69aaf546be4ce544427234379316b8f708b619766",
    ),
    (
        "scripts/prebuilt_manifest.py",
        "7db57eb04bbc3640a411533a3ceffc755c79b25b2312744953fde714a0c30d0c",
    ),
];

pub fn source_identity(environment: &Source, manifest: &Source) -> Result<(), String> {
    for (source, (path, hash)) in [environment, manifest].into_iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("tooling oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub fn fixture() -> Result<Value, String> {
    let text = include_str!("../../tests/fixtures/native/tooling-source-contracts-v2.json")
        .replace("\r\n", "\n");
    if crate::codec_raster::digest_hex(text.as_bytes())
        != "86318472e82ddf4cfb6382827a4751a1d5d545f8fbaeb371da5b80c92a6cdda3"
    {
        return Err("pinned tooling fixture bytes drift".into());
    }
    serde_json::from_str(&text).map_err(|_| "invalid original tooling fixture".into())
}

fn recipe(document: &Value) -> Result<Value, String> {
    let mut value = document.clone();
    for (kind, count) in [("manifest", 24), ("environment", 2), ("confined", 4)] {
        let rows = value
            .as_object_mut()
            .and_then(|value| value.get_mut(kind))
            .and_then(Value::as_array_mut)
            .filter(|rows| rows.len() == count)
            .ok_or("tooling oracle requires its exact original recipe count")?;
        for row in rows {
            let row = row
                .as_object_mut()
                .ok_or("invalid original tooling recipe")?;
            row.remove("expected");
            if kind == "environment" {
                row.remove("original_after");
                row.remove("directories");
            }
        }
    }
    Ok(value)
}

pub fn fixture_identity(document: &Value) -> Result<(), String> {
    let mut value = recipe(document)?;
    value.sort_all_objects();
    let bytes = serde_json::to_vec(&value).map_err(|_| "cannot encode tooling recipe")?;
    if crate::codec_raster::digest_hex(&bytes)
        != "31b3e5613088ac7b80e7c94e3dc171b74cee24ea4b92021ff5dfb9c5b531f4f6"
    {
        return Err("tooling oracle recipe drift; refusing execution".into());
    }
    Ok(())
}

pub(crate) fn required<'py>(
    globals: &Bound<'py, PyDict>,
    name: &str,
) -> PyResult<Bound<'py, PyAny>> {
    globals
        .get_item(name)?
        .ok_or_else(|| PyValueError::new_err("missing original tooling function"))
}

pub(crate) fn python<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    py.import("json")?
        .call_method1("loads", (value.to_string(),))
}

pub(crate) fn observed(value: &Bound<'_, PyAny>) -> PyResult<Value> {
    let text: String = value
        .py()
        .import("json")?
        .call_method1("dumps", (value,))?
        .extract()?;
    serde_json::from_str(&text)
        .map_err(|_| PyValueError::new_err("tooling observation is not JSON"))
}

fn normalize(value: &mut Value, root: &Path) -> PyResult<()> {
    match value {
        Value::String(text) => {
            let root = root
                .to_str()
                .ok_or_else(|| PyValueError::new_err("tooling root is not UTF-8"))?;
            *text = text.replace(root, "$ROOT").replace('\\', "/");
        }
        Value::Array(values) => {
            for value in values {
                normalize(value, root)?;
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                normalize(value, root)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn payload_path(root: &Path, name: &str) -> PyResult<std::path::PathBuf> {
    if name.is_empty()
        || name.contains(['\\', ':'])
        || Path::new(name)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(PyValueError::new_err(
            "tooling payload must stay inside its fixture",
        ));
    }
    Ok(root.join(name))
}

pub(crate) fn directories(
    root: &Path,
    path: &Path,
    left: &mut usize,
    depth: usize,
    found: &mut Vec<String>,
) -> PyResult<()> {
    if depth > 16 || *left == 0 {
        return Err(PyValueError::new_err(
            "tooling observation inventory exceeded its bound",
        ));
    }
    *left -= 1;
    crate::tooling_environment::reject_links(path, "tooling evidence cannot follow links")
        .map_err(PyValueError::new_err)?;
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.is_file() {
        return Ok(());
    }
    if !metadata.is_dir() || crate::process_owner::cleanup_pending(path) {
        return Err(PyValueError::new_err(
            "tooling evidence custody is unresolved",
        ));
    }
    if path != root {
        found.push(
            path.strip_prefix(root)
                .map_err(|_| PyValueError::new_err("foreign tooling evidence"))?
                .to_str()
                .ok_or_else(|| PyValueError::new_err("tooling evidence path is not UTF-8"))?
                .replace('\\', "/"),
        );
    }
    for entry in std::fs::read_dir(path)? {
        directories(root, &entry?.path(), left, depth + 1, found)?;
    }
    Ok(())
}

fn execute(
    py: Python<'_>,
    workspace: &Path,
    environment: &Source,
    manifest: &Source,
) -> PyResult<Value> {
    let types = py.import("types")?;
    let namespace = types.getattr("SimpleNamespace")?;
    let path_module = py.import("pathlib")?;
    let path = path_module.getattr("Path")?;
    let os = namespace.call0()?;
    os.setattr("environ", PyDict::new(py))?;
    let bindings = PyDict::new(py);
    bindings.set_item("os", &os)?;
    bindings.set_item("Path", &path)?;
    bindings.set_item("stat", py.import("stat")?)?;
    let env = crate::source_oracle_runtime::namespace(
        py,
        environment,
        &["_linked", "confined_directory", "environment"],
        &["bool", "getattr", "ValueError", "dict", "str"],
        &bindings,
    )?;
    let original = fixture().map_err(PyValueError::new_err)?;
    let mut output = recipe(&original).map_err(PyValueError::new_err)?;
    let mock = py.import("unittest.mock")?.getattr("Mock")?;
    for (index, case) in output["manifest"]
        .as_array_mut()
        .ok_or_else(|| PyValueError::new_err("missing tooling cases"))?
        .iter_mut()
        .enumerate()
    {
        let root = workspace.join(format!("manifest-{index}"));
        std::fs::create_dir(&root)?;
        for (name, text) in case["payload"]
            .as_object()
            .ok_or_else(|| PyValueError::new_err("invalid tooling payload"))?
        {
            let target = payload_path(&root, name)?;
            std::fs::create_dir_all(
                target
                    .parent()
                    .ok_or_else(|| PyValueError::new_err("payload parent missing"))?,
            )?;
            std::fs::write(
                target,
                text.as_str()
                    .ok_or_else(|| PyValueError::new_err("payload is not text"))?
                    .as_bytes(),
            )?;
        }
        if let Some(name) = case["missing"].as_str() {
            std::fs::remove_file(payload_path(&root, name)?)?;
        }
        if let Some(name) = case["changed"].as_str() {
            std::fs::write(payload_path(&root, name)?, b"changed")?;
        }
        std::fs::write(
            root.join("package.json"),
            case["manifest"].to_string().as_bytes(),
        )?;
        let platform = namespace.call0()?;
        for (name, value) in [
            ("system", case["host"]["system"].clone()),
            ("machine", case["host"]["machine"].clone()),
            (
                "libc_ver",
                json!([case["host"]["libc"], case["host"]["libc_version"]]),
            ),
            ("mac_ver", json!([case["host"]["macos"], [], ""])),
        ] {
            let options = PyDict::new(py);
            options.set_item("return_value", python(py, &value)?)?;
            platform.setattr(name, mock.call((), Some(&options))?)?;
        }
        let sys = namespace.call0()?;
        let mut version: Vec<u32> = python(py, &case["host"]["python"])?.extract()?;
        version.push(0);
        sys.setattr("version_info", PyTuple::new(py, version)?)?;
        let bindings = PyDict::new(py);
        for name in ["json", "hashlib", "tomllib"] {
            bindings.set_item(name, py.import(name)?)?;
        }
        bindings.set_item("Path", &path)?;
        bindings.set_item("PurePosixPath", path_module.getattr("PurePosixPath")?)?;
        bindings.set_item("platform", platform)?;
        bindings.set_item("sys", sys)?;
        let globals = crate::source_oracle_runtime::namespace(
            py,
            manifest,
            &["verify"],
            &[
                "isinstance",
                "dict",
                "any",
                "int",
                "tuple",
                "map",
                "ValueError",
                "str",
            ],
            &bindings,
        )?;
        let result = match required(&globals, "verify")?.call1((path.call1((&root,))?,)) {
            Ok(value) => json!({"manifest":observed(&value)?}),
            Err(error) if error.is_instance_of::<PyValueError>(py) => json!({
                "error":error.value(py).str()?.to_str()?, "error_type":"ValueError"}),
            Err(error) => return Err(error),
        };
        case["expected"] = result;
        normalize(&mut case["expected"], &root)?;
        if case["expected"] != original["manifest"][index]["expected"] {
            return Err(PyValueError::new_err(format!(
                "original tooling manifest observation drift at {index}; retained"
            )));
        }
    }
    for (index, case) in output["environment"]
        .as_array_mut()
        .ok_or_else(|| PyValueError::new_err("missing environment cases"))?
        .iter_mut()
        .enumerate()
    {
        let root = workspace.join(format!("environment-{index}"));
        std::fs::create_dir(&root)?;
        let input = python(py, &case["original"])?;
        let options = PyDict::new(py);
        options.set_item(
            "build",
            case["build"]
                .as_bool()
                .ok_or_else(|| PyValueError::new_err("invalid build recipe"))?,
        )?;
        let actual =
            required(&env, "environment")?.call((path.call1((&root,))?, &input), Some(&options))?;
        let mut actual = observed(&actual)?;
        normalize(&mut actual, &root)?;
        case["expected"] = actual;
        case["original_after"] = observed(&input)?;
        let mut found = Vec::new();
        directories(&root, &root, &mut 2048, 0, &mut found)?;
        found.sort();
        case["directories"] = json!(found);
        if *case != original["environment"][index] {
            return Err(PyValueError::new_err(
                "original tooling environment drift; retained",
            ));
        }
    }
    let confined = workspace.join("confined");
    std::fs::create_dir(&confined)?;
    for (index, case) in output["confined"]
        .as_array_mut()
        .ok_or_else(|| PyValueError::new_err("missing confinement cases"))?
        .iter_mut()
        .enumerate()
    {
        let options = PyDict::new(py);
        options.set_item("create", false)?;
        let relative = case["relative"]
            .as_str()
            .ok_or_else(|| PyValueError::new_err("invalid confined recipe"))?;
        case["expected"] = match required(&env, "confined_directory")?
            .call((path.call1((&confined,))?, relative), Some(&options))
        {
            Ok(value) => json!({"path":value.str()?.to_str()?}),
            Err(error) if error.is_instance_of::<PyValueError>(py) => {
                json!({"error":error.value(py).str()?.to_str()?})
            }
            Err(error) => return Err(error),
        };
        normalize(&mut case["expected"], &confined)?;
        if *case != original["confined"][index] {
            return Err(PyValueError::new_err(
                "original tooling confinement drift; retained",
            ));
        }
    }
    // Only complete source observations authorize removal; no deleting Drop on failure.
    let expected = (0..24)
        .map(|index| format!("manifest-{index}"))
        .chain((0..2).map(|index| format!("environment-{index}")))
        .chain(["confined".to_owned()])
        .collect::<BTreeSet<_>>();
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| {
            let name = entry?
                .file_name()
                .into_string()
                .map_err(|_| std::io::Error::other("non-UTF8 tooling member"))?;
            Ok(name)
        })
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    if actual != expected {
        return Err(PyValueError::new_err(
            "unexpected tooling evidence; retained",
        ));
    }
    directories(workspace, workspace, &mut 2048, 0, &mut Vec::new())?;
    for name in expected {
        std::fs::remove_dir_all(workspace.join(name))?;
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "manifest_cases":24,"environment_cases":2,"confined_cases":4,"observations":output,
        "ports":"fixed recipe host facts in private sys/platform namespaces; real original source/pathlib/hashlib/json/tomllib and files; no successful install/signing or native policy used for expected values"}),
    )
}

pub fn observe(
    root: &Path,
    workspace: &Path,
    environment: &Source,
    manifest: &Source,
    document: &Value,
) -> Result<Value, String> {
    source_identity(environment, manifest)?;
    fixture_identity(document)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("tooling oracle workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect tooling workspace")?
        .next()
        .is_some()
    {
        return Err("tooling oracle workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect tooling bytecode policy")?
        {
            return Err("tooling oracle requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, environment, manifest).map_err(|error| {
            format!(
                "original tooling replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })
}

pub async fn observe_owned(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    environment: &Source,
    manifest: &Source,
    document: &Value,
) -> Result<Value, String> {
    source_identity(environment, manifest)?;
    fixture_identity(document)?;
    crate::source_oracle_process::check(
        root,
        executable,
        "tooling-contracts",
        environment,
        &[manifest],
        &json!({"workspace":workspace,"document":document}),
    )
    .await
}
