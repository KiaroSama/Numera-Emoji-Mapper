//! Historical source-schema1 archive observations; synthetic payloads, never a completed app build.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{directories, observed, payload_path, required};
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 2] = [
    (
        "scripts/local_environment.py",
        "b568feffc0c75e27b7f08cb69aaf546be4ce544427234379316b8f708b619766",
    ),
    (
        "scripts/package_native.py",
        "cb504e8f0e22789a278129f893bb14cf1954483d9a3f620254e6ac5da7a326b0",
    ),
];

pub fn source_identity(environment: &Source, package: &Source) -> Result<(), String> {
    for (source, (path, hash)) in [environment, package].into_iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("package oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub fn fixture() -> Result<Value, String> {
    let text = include_str!("../../tests/fixtures/native/tooling-package-source.json")
        .replace("\r\n", "\n");
    if crate::codec_raster::digest_hex(text.as_bytes())
        != "db37b20c0ee4fe2c7ad8a503dac40c61f20fd4333c3acd91653e53b4d7f824e4"
    {
        return Err("pinned package fixture bytes drift".into());
    }
    serde_json::from_str(&text).map_err(|_| "invalid original package fixture".into())
}

pub fn fixture_identity(document: &Value) -> Result<(), String> {
    let mut value = document.clone();
    let fields = value
        .as_object_mut()
        .ok_or("package recipe must be an object")?;
    fields.remove("expected");
    value.sort_all_objects();
    let bytes = serde_json::to_vec(&value).map_err(|_| "cannot encode package recipe")?;
    if crate::codec_raster::digest_hex(&bytes)
        != "fae1903c6c552a036da85d1f226e0c2a872072a1a6eba71d8d06472880c0dd12"
    {
        return Err("package oracle recipe drift; refusing execution".into());
    }
    Ok(())
}

#[pyclass]
struct Imports {
    environment: Py<PyModule>,
}
#[pymethods]
impl Imports {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        _globals: &Bound<'_, PyAny>,
        _locals: &Bound<'_, PyAny>,
        fromlist: &Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<Bound<'py, PyModule>> {
        if name == "scripts.local_environment"
            && level == 0
            && fromlist.extract::<Vec<String>>()? == ["environment"]
        {
            return Ok(self.environment.bind(py).clone());
        }
        Err(PyImportError::new_err(
            "unexpected import in original package function",
        ))
    }
}

#[pyclass]
struct Temporary {
    output: PathBuf,
    paths: std::sync::Mutex<Vec<PathBuf>>,
}
#[pymethods]
impl Temporary {
    #[pyo3(signature = (*, prefix, dir))]
    fn directory<'py>(
        &self,
        py: Python<'py>,
        prefix: &str,
        dir: PathBuf,
    ) -> PyResult<Bound<'py, PyAny>> {
        let dir = crate::tooling_environment::physical_root(&dir).map_err(PyValueError::new_err)?;
        if dir != self.output || prefix != "package-" {
            return Err(PyValueError::new_err(
                "unexpected historical package temporary root",
            ));
        }
        let options = PyDict::new(py);
        options.set_item("dir", &dir)?;
        options.set_item("prefix", prefix)?;
        let name: String = py
            .import("tempfile")?
            .call_method("mkdtemp", (), Some(&options))?
            .extract()?;
        let path = crate::tooling_environment::physical_root(Path::new(&name))
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(self.output.as_path()) {
            return Err(PyValueError::new_err(
                "historical package temporary escaped",
            ));
        }
        self.paths
            .lock()
            .map_err(|_| PyValueError::new_err("package temporary accounting poisoned"))?
            .push(path);
        // The original with-body still runs; deletion waits until every observation passes.
        py.import("contextlib")?
            .call_method1("nullcontext", (name,))
    }
}

fn execute(
    py: Python<'_>,
    workspace: &Path,
    environment: &Source,
    package: &Source,
) -> PyResult<Value> {
    let original = fixture().map_err(PyValueError::new_err)?;
    let root = workspace.join("package root with spaces");
    std::fs::create_dir(&root)?;
    let inputs = original["inputs"]
        .as_object()
        .ok_or_else(|| PyValueError::new_err("missing package inputs"))?;
    for (name, text) in inputs {
        let path = payload_path(&root, name)?;
        std::fs::create_dir_all(
            path.parent()
                .ok_or_else(|| PyValueError::new_err("package payload parent missing"))?,
        )?;
        std::fs::write(
            path,
            text.as_str()
                .ok_or_else(|| PyValueError::new_err("package input must be text"))?
                .as_bytes(),
        )?;
    }
    let namespace = py.import("types")?.getattr("SimpleNamespace")?;
    let path = py.import("pathlib")?.getattr("Path")?;
    let os = namespace.call0()?;
    os.setattr("name", py.import("os")?.getattr("name")?)?;
    let private_environment = PyDict::new(py);
    os.setattr("environ", &private_environment)?;
    os.setattr("replace", py.import("os")?.getattr("replace")?)?;
    let bindings = PyDict::new(py);
    bindings.set_item("Path", &path)?;
    bindings.set_item("os", &os)?;
    bindings.set_item("stat", py.import("stat")?)?;
    let env = crate::source_oracle_runtime::namespace(
        py,
        environment,
        &["_linked", "confined_directory", "environment"],
        &["bool", "getattr", "ValueError", "dict", "str"],
        &bindings,
    )?;
    let module = PyModule::new(py, "_original_package_environment")?;
    module.add("environment", required(&env, "environment")?)?;
    let imports = Py::new(
        py,
        Imports {
            environment: module.unbind(),
        },
    )?;
    let output = root.join("dist");
    let temporary = Py::new(
        py,
        Temporary {
            output: output.clone(),
            paths: std::sync::Mutex::new(Vec::new()),
        },
    )?;
    let tempfile = namespace.call0()?;
    tempfile.setattr(
        "TemporaryDirectory",
        temporary.bind(py).getattr("directory")?,
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("Path", &path)?;
    bindings.set_item("os", &os)?;
    bindings.set_item("tempfile", tempfile)?;
    for name in [
        "json", "hashlib", "tomllib", "shutil", "zipfile", "platform",
    ] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item(
        "PLATFORMS",
        crate::source_tooling_oracle::python(
            py,
            &json!({"Windows":"windows","Linux":"linux","Darwin":"macos"}),
        )?,
    )?;
    let globals = crate::source_oracle_runtime::namespace(
        py,
        package,
        &["platform_id", "package"],
        &["ValueError", "FileExistsError", "str", "sorted"],
        &bindings,
    )?;
    required(&globals, "__builtins__")?.set_item("__import__", imports)?;
    let target: String = required(&globals, "platform_id")?.call0()?.extract()?;
    let wheel_input = original["wheel_input"]
        .as_str()
        .ok_or_else(|| PyValueError::new_err("missing original wheel input"))?;
    let revision = original["revision"]
        .as_str()
        .ok_or_else(|| PyValueError::new_err("missing original revision"))?;
    let archive: PathBuf = required(&globals, "package")?
        .call1((
            path.call1((&root,))?,
            path.call1((root.join("build/executable"),))?,
            path.call1((payload_path(&root, wheel_input)?,))?,
            path.call1((root.join("dependencies"),))?,
            path.call1((&output,))?,
            revision,
        ))?
        .extract()?;
    crate::tooling_environment::reject_links(&archive, "historical archive cannot follow links")
        .map_err(PyValueError::new_err)?;
    let stem = format!("numera-emoji-mapper-2.0.0-{target}");
    if archive != output.join(format!("{stem}.zip")) {
        return Err(PyValueError::new_err(
            "historical archive path drift; retained",
        ));
    }
    let zip = py
        .import("zipfile")?
        .getattr("ZipFile")?
        .call1((&archive,))?;
    let observed_archive = (|| -> PyResult<(Value, Vec<String>)> {
        let bytes = zip.call_method1("read", (format!("{stem}/package.json"),))?;
        let manifest = py.import("json")?.call_method1("loads", (bytes,))?;
        let names: Vec<String> = zip.call_method0("namelist")?.extract()?;
        if !zip.call_method0("testzip")?.is_none() {
            return Err(PyValueError::new_err(
                "historical archive failed CRC/header checks",
            ));
        }
        Ok((observed(&manifest)?, names))
    })();
    let closed = zip.call_method0("close");
    let (manifest, mut names) = observed_archive?;
    closed?;
    let mut expected = original["expected"].clone();
    let binary = expected["files"]
        .as_object_mut()
        .and_then(|files| files.remove("native/runtime/numera-emoji.exe"))
        .ok_or_else(|| PyValueError::new_err("missing pinned package binary"))?;
    expected["files"][if target.starts_with("windows-") {
        "native/runtime/numera-emoji.exe"
    } else {
        "native/runtime/numera-emoji"
    }] = binary;
    expected["platform"] = json!(target);
    expected["minimum_os"] = json!(if target.starts_with("windows-") {
        "Windows 10"
    } else if target.starts_with("linux-") {
        "Ubuntu 24.04 / glibc 2.39"
    } else {
        "macOS 15"
    });
    if manifest != expected {
        return Err(PyValueError::new_err(
            "original package manifest drift; retained",
        ));
    }
    let mut expected_names = manifest["files"]
        .as_object()
        .ok_or_else(|| PyValueError::new_err("missing original member inventory"))?
        .keys()
        .map(|name| format!("{stem}/{name}"))
        .chain([format!("{stem}/package.json")])
        .collect::<Vec<_>>();
    names.sort();
    expected_names.sort();
    if names != expected_names {
        return Err(PyValueError::new_err(
            "original package archive membership drift; retained",
        ));
    }
    let checksum = archive.with_extension("zip.sha256");
    let expected_checksum = format!(
        "{}  {stem}.zip\n",
        crate::tooling_manifest::digest(&archive).map_err(PyValueError::new_err)?
    );
    let text = PyDict::new(py);
    text.set_item("encoding", "utf-8")?;
    // The original writer uses the host newline; its text consumer reads universal newlines.
    let actual_checksum: String = path
        .call1((&checksum,))?
        .call_method("read_text", (), Some(&text))?
        .extract()?;
    if actual_checksum != expected_checksum {
        return Err(PyValueError::new_err(
            "original package checksum drift; retained",
        ));
    }
    for (name, text) in inputs {
        if std::fs::read(payload_path(&root, name)?)?
            != text
                .as_str()
                .ok_or_else(|| PyValueError::new_err("invalid input"))?
                .as_bytes()
        {
            return Err(PyValueError::new_err(
                "original package changed an input; retained",
            ));
        }
    }
    let owned = temporary.bind(py).borrow();
    let paths = owned
        .paths
        .lock()
        .map_err(|_| PyValueError::new_err("package temporary accounting poisoned"))?;
    if paths.len() != 1 || paths[0].parent() != Some(output.as_path()) {
        return Err(PyValueError::new_err(
            "historical temporary ownership drift; retained",
        ));
    }
    let actual = std::fs::read_dir(&output)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    let expected = BTreeSet::from([archive.clone(), checksum, paths[0].clone()]);
    if actual != expected {
        return Err(PyValueError::new_err("unexpected package output; retained"));
    }
    let members = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    if members != [root.clone()] {
        return Err(PyValueError::new_err(
            "unexpected package workspace; retained",
        ));
    }
    directories(workspace, workspace, &mut 2048, 0, &mut Vec::new())?;
    let report = json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "manifest":manifest,"archive_members":names,"inputs_checked":inputs.len(),"checksum_verified":true,
        "ports":"same-platform original platform_id; private os.environ and closed source importer; deferred original temporary deletion; synthetic archive only, no native policy expectation or completed build/signing/install"});
    drop(paths);
    drop(owned);
    std::fs::remove_dir_all(&root)?;
    Ok(report)
}

pub fn observe(
    root: &Path,
    workspace: &Path,
    environment: &Source,
    package: &Source,
    document: &Value,
) -> Result<Value, String> {
    source_identity(environment, package)?;
    fixture_identity(document)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("package oracle workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect package workspace")?
        .next()
        .is_some()
    {
        return Err("package oracle workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect package bytecode policy")?
        {
            return Err("package oracle requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, environment, package).map_err(|error| {
            format!(
                "original package replay failed: {}",
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
    package: &Source,
    document: &Value,
) -> Result<Value, String> {
    source_identity(environment, package)?;
    fixture_identity(document)?;
    crate::source_oracle_process::check(
        root,
        executable,
        "tooling-package",
        environment,
        &[package],
        &json!({"workspace":workspace,"document":document}),
    )
    .await
}
