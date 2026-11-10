//! Fixed loaded and real-file observations from original validation, not a general codec oracle.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const SOURCE_HASH: &str = "5112b55f21d4d241ffe9660d5294de20487d4ea1e9772188fb6bd125f7fef63b";
const ERRORS_HASH: &str = "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026";
const FIXTURE_HASH: &str = "bfce4eafbb267ca24bcd5c02faac7a1ef4474edb5b164cc15dc12337b072bf40";

pub(crate) fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn source_identity(source: &Source, errors: &Source) -> Result<(), String> {
    for (source, path, expected) in [
        (source, "emojikit/media_lottie.py", SOURCE_HASH),
        (errors, "emojikit/errors.py", ERRORS_HASH),
    ] {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != expected
            || hash(source.text.as_bytes()) != expected
        {
            return Err("Lottie oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn inputs(fixture: &Value) -> Result<Value, String> {
    let mut recipe = fixture.clone();
    let cases = recipe["cases"]
        .as_array_mut()
        .filter(|cases| cases.len() == 84)
        .ok_or("Lottie oracle requires84 original observations")?;
    for case in cases {
        let case = case.as_object_mut().ok_or("invalid Lottie oracle case")?;
        if case.remove("expected").is_none() || case.remove("source_error_kind").is_none() {
            return Err("Lottie oracle case requires result and class observations".into());
        }
    }
    Ok(recipe)
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    // Git text checkouts may translate newlines, never number tokens or Unicode.
    let original =
        include_str!("../../tests/fixtures/native/lottie-validation.json").replace("\r\n", "\n");
    if hash(original.as_bytes()) != FIXTURE_HASH {
        return Err("pinned original Lottie fixture bytes drift".into());
    }
    let original: Value =
        serde_json::from_str(&original).map_err(|_| "invalid pinned original Lottie fixture")?;
    if inputs(fixture)? != inputs(&original)? {
        return Err("Lottie oracle recipe drift; refusing execution".into());
    }
    Ok(())
}

fn file_inputs(fixture: &Value) -> Result<Value, String> {
    let mut input = fixture.clone();
    let cases = input["validations"]
        .as_array_mut()
        .filter(|cases| cases.len() == 7)
        .ok_or("Lottie file oracle requires7 original observations")?;
    for case in cases {
        let case = case
            .as_object_mut()
            .ok_or("invalid Lottie file observation")?;
        for field in ["expected", "error_kind", "error"] {
            case.remove(field);
        }
    }
    Ok(input)
}

pub fn file_fixture_identity(fixture: &Value) -> Result<(), String> {
    let original =
        include_str!("../../tests/fixtures/native/lottie-file.json").replace("\r\n", "\n");
    if hash(original.as_bytes())
        != "d6393b50d2f893e5e7733cc21162369d75665ebe0235a4da9a86ec59f9722356"
    {
        return Err("pinned original Lottie file fixture bytes drift".into());
    }
    let original: Value =
        serde_json::from_str(&original).map_err(|_| "invalid original Lottie file fixture")?;
    if file_inputs(fixture)? != file_inputs(&original)? {
        return Err("Lottie file oracle recipe drift; refusing execution".into());
    }
    Ok(())
}

#[derive(Default)]
struct Calls(Mutex<Vec<&'static str>>);
impl Calls {
    fn record(&self, event: &'static str) -> PyResult<()> {
        self.0
            .lock()
            .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("oracle port lock poisoned"))?
            .push(event);
        Ok(())
    }
}
#[pyclass]
struct InputPath {
    name: String,
    size: u64,
    calls: Arc<Calls>,
}
#[pymethods]
impl InputPath {
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }
    fn stat(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.calls.record("stat")?;
        let options = PyDict::new(py);
        options.set_item("st_size", self.size)?;
        Ok(crate::source_oracle_runtime::host_import(py, "types")?
            .getattr("SimpleNamespace")?
            .call((), Some(&options))?
            .unbind())
    }
    fn read_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.calls.record("magic")?;
        Ok(PyBytes::new(py, &[0x1f, 0x8b]))
    }
}
#[pyclass]
struct LoadDocument {
    document: Py<PyAny>,
    calls: Arc<Calls>,
}
#[pymethods]
impl LoadDocument {
    fn __call__(&self, py: Python<'_>, _path: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        self.calls.record("load")?;
        Ok(self.document.clone_ref(py))
    }
}

pub(crate) fn namespace<'py>(
    py: Python<'py>,
    source: &Source,
    errors: &Source,
    definitions: &[&str],
) -> PyResult<Bound<'py, PyDict>> {
    let error_namespace = crate::source_oracle_runtime::namespace(
        py,
        errors,
        &["MediaError"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item(
        "MediaError",
        error_namespace
            .get_item("MediaError")?
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing source MediaError"))?,
    )?;
    for name in ["math", "gzip", "io", "json", "os"] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("PREVIEW_SIZE", 104)?;
    bindings.set_item("PREVIEW_FPS", 15)?;
    bindings.set_item("PREVIEW_QUALITY", 60)?;
    bindings.set_item("TGS_MAX_UNPACKED", 8_u64 * 1024 * 1024)?;
    bindings.set_item("TGS_SIZE", 512)?;
    bindings.set_item("TGS_MAX_BYTES", 65536)?;
    bindings.set_item("TGS_FPS", 60)?;
    bindings.set_item("TGS_MAX_SECONDS", 3.0)?;
    bindings.set_item("_GZIP_MAGIC", PyBytes::new(py, &[0x1f, 0x8b]))?;
    crate::source_oracle_runtime::namespace(
        py,
        source,
        definitions,
        &[
            "list",
            "dict",
            "len",
            "type",
            "isinstance",
            "float",
            "TypeError",
            "ValueError",
            "OSError",
            "Exception",
            "str",
        ],
        &bindings,
    )
}

fn observe(py: Python<'_>, globals: &Bound<'_, PyDict>, case: &Value) -> PyResult<Value> {
    let calls = Arc::new(Calls::default());
    let document = py
        .import("json")?
        .call_method1("loads", (case["document"].to_string(),))?;
    globals.set_item(
        "_load_lottie",
        Py::new(
            py,
            LoadDocument {
                document: document.unbind(),
                calls: calls.clone(),
            },
        )?,
    )?;
    let filename = case["filename"]
        .as_str()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid original filename"))?;
    let size = case["compressed_size"].as_u64().ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err("invalid original compressed size")
    })?;
    let path = Py::new(
        py,
        InputPath {
            name: filename.into(),
            size,
            calls: calls.clone(),
        },
    )?;
    let result = globals
        .get_item("validate_tgs")?
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing original validate_tgs"))?
        .call1((path,));
    let observation = match result {
        Ok(value) if value.is_none() => json!({"expected":null,"source_error_kind":null}),
        Ok(_) => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "source validation returned a value",
            ));
        }
        Err(error) => json!({"expected":error.value(py).str()?.to_string(),
            "source_error_kind":error.get_type(py).name()?.to_string()}),
    };
    let actual = calls
        .0
        .lock()
        .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("oracle port lock poisoned"))?;
    let expected: &[&str] = if size > 65536 {
        &["stat"]
    } else {
        &["stat", "magic", "load"]
    };
    if actual.as_slice() != expected {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "Lottie oracle port order drift",
        ));
    }
    Ok(observation)
}

fn file_bytes(root: &std::path::Path, relative: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let path = std::path::Path::new(relative);
    if !relative.starts_with("tests/fixtures/native/")
        || path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err("Lottie oracle input must be a confined native fixture".into());
    }
    let path = root.join(path);
    crate::tooling_environment::reject_links(&path, "Lottie oracle fixture cannot follow links")?;
    if !crate::migration_bundle::resolved(&path)
        .map_err(|_| "cannot resolve Lottie fixture")?
        .starts_with(root)
    {
        return Err("Lottie oracle fixture escaped project root".into());
    }
    let file = std::fs::File::open(&path).map_err(|_| "cannot open Lottie fixture")?;
    if !file
        .metadata()
        .map_err(|_| "cannot inspect Lottie fixture")?
        .is_file()
    {
        return Err("Lottie oracle fixture must be regular".into());
    }
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read Lottie fixture")?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Lottie oracle fixture exceeds2MiB".into());
    }
    Ok(bytes)
}

struct FileInput {
    path: std::path::PathBuf,
    bytes: Vec<u8>,
    temporary: bool,
}

fn file_inputs_at(
    root: &std::path::Path,
    workspace: &std::path::Path,
    fixture: &Value,
) -> Result<Vec<FileInput>, String> {
    let cases = fixture["validations"]
        .as_array()
        .ok_or("missing file observations")?;
    let mut inputs = Vec::new();
    for case in cases {
        let temporary = case["input_base64"].is_string();
        let (path, bytes) = if let Some(relative) = case["input"].as_str() {
            (root.join(relative), file_bytes(root, relative)?)
        } else {
            use base64::Engine;
            use std::io::Write;
            let relative = case["input_base64"]
                .as_str()
                .ok_or("missing original input")?;
            let encoded = file_bytes(root, relative)?;
            let encoded =
                std::str::from_utf8(&encoded).map_err(|_| "invalid fixture base64 text")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded.trim())
                .map_err(|_| "invalid original fixture base64")?;
            let name = case["name"]
                .as_str()
                .filter(|name| {
                    !name.is_empty()
                        && !name.contains(['/', '\\', ':'])
                        && !matches!(*name, "." | "..")
                })
                .ok_or("invalid original fixture name")?;
            let path = workspace.join(name);
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|_| "cannot create original file fixture")?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|_| "cannot persist original file fixture")?;
            (path, bytes)
        };
        if case["input_sha256"].as_str() != Some(hash(&bytes).as_str()) {
            return Err("Lottie oracle input hash drift".into());
        }
        inputs.push(FileInput {
            path,
            bytes,
            temporary,
        });
    }
    Ok(inputs)
}

pub fn check_files(
    root: &std::path::Path,
    workspace: &std::path::Path,
    source: &Source,
    errors: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    source_identity(source, errors)?;
    file_fixture_identity(fixture)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("Lottie oracle workspace must stay in project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect Lottie workspace")?
        .next()
        .is_some()
    {
        return Err("Lottie oracle workspace must be empty".into());
    }
    // Admit every physical input before any historical definition can execute.
    let inputs = file_inputs_at(&root, &workspace, fixture)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let globals = namespace(
            py,
            source,
            errors,
            &["_load_lottie", "_all_layers", "validate_tgs"],
        )
        .map_err(|error| format!("Lottie file oracle namespace failed: {error}"))?;
        let function = globals
            .get_item("validate_tgs")
            .map_err(|_| "cannot inspect original file validator")?
            .ok_or("missing original file validator")?;
        let cases = fixture["validations"]
            .as_array()
            .ok_or("missing file observations")?;
        for (index, (case, input)) in cases.iter().zip(&inputs).enumerate() {
            let path = &input.path;
            let input_path = py
                .import("pathlib")
                .and_then(|module| module.getattr("Path"))
                .and_then(|class| class.call1((path.clone(),)))
                .map_err(|error| format!("cannot bind original fixture path: {error}"))?;
            let result = function.call1((input_path,));
            let actual = match result {
                Ok(value) if value.is_none() => json!({"expected":true}),
                Ok(_) => return Err("original file validator returned a value".into()),
                Err(error) => json!({"error_kind":error.get_type(py).name()
                    .map_err(|_| "cannot inspect original file error")?.to_string(),
                    "error":error.value(py).str().map_err(|_| "cannot format original file error")?.to_string()}),
            };
            let expected = case
                .as_object()
                .ok_or("invalid original file observation")?
                .iter()
                .filter(|(name, _)| matches!(name.as_str(), "expected" | "error_kind" | "error"))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<serde_json::Map<_, _>>();
            if actual != Value::Object(expected) {
                return Err(format!("Lottie file fixture observation drift at{index}"));
            }
            if std::fs::read(path).map_err(|_| "cannot verify unchanged fixture")? != input.bytes {
                return Err("Lottie file oracle changed input bytes".into());
            }
            if input.temporary {
                std::fs::remove_file(path).map_err(|_| "cannot clean original file fixture")?;
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_runtime":"CPython3.11.9",
            "source_hash":SOURCE_HASH,"dependency_source_hash":ERRORS_HASH,"fixture_drift":false,
            "supplied_ports":[],"file_codec_coverage":"seven fixed validation observations"}),
        )
    })
}

pub fn check(source: &Source, errors: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source, errors)?;
    fixture_identity(fixture)?;
    let cases = fixture["cases"].as_array().ok_or("missing Lottie cases")?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let globals = namespace(
            py,
            source,
            errors,
            &["_load_lottie", "_all_layers", "validate_tgs"],
        )
        .map_err(|error| format!("Lottie oracle namespace failed: {error}"))?;
        for (index, case) in cases.iter().enumerate() {
            let actual = observe(py, &globals, case)
                .map_err(|error| format!("Lottie oracle source failed at{index}: {error}"))?;
            if actual["expected"] != case["expected"]
                || actual["source_error_kind"] != case["source_error_kind"]
            {
                return Err(format!("Lottie fixture observation drift at{index}"));
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_runtime":"CPython3.11.9",
            "source_hash":SOURCE_HASH,"dependency_source_hash":ERRORS_HASH,"fixture_drift":false,
            "supplied_ports":["stat","magic","load"],"file_codec_coverage":false}),
        )
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_admission_compares_the_same_physical_path_spelling() {
        let root = crate::tooling_environment::physical_root(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap(),
        )
        .unwrap();
        let bytes = super::file_bytes(&root, "tests/fixtures/native/tgs-load/valid.tgs").unwrap();
        assert_eq!(
            super::hash(&bytes),
            "7eab5f062ab6ff2b69178b0086356f9fa361ac0f755b4448e05c97c713376135"
        );
        assert!(super::file_bytes(&root, "tests/fixtures/native/../outside").is_err());
    }
}
