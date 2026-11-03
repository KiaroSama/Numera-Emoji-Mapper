//! Historical gallery HTML and real static thumbnails; native rendering never supplies expectations.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const HASH: &str = "3a1460e846edb4bb75675f6d4d78d9e7d05ab75c7c61ec287d810a57cf5c9158";
const SCRIPT_HASH: &str = "b9afcea858593c9e532ee3690038433e48eefc00910af603b6b1e8b90a3e56e4";
const ART: &str = "assets/numera-emoji-mapper-logo.png";
const ART_HASH: &str = "6f85376de3893dfa1e78045ce264103b0055acec400dd8a35222405d5c4f6951";
const FIXTURE: &str = "d173116:pack_gallery.render+script_json.json_for_script";
const RECIPE_HASH: &str = "800cb1b1cf7836c0c73b35e49fb2975dc76e9085a146ab5a8081d3b8c5c62e61";
static NEXT: AtomicU64 = AtomicU64::new(0);

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn source_identity(source: &Source, script: &Source) -> Result<(), String> {
    for (source, path, hash) in [
        (source, "tests/oracles/pack_gallery.py", HASH),
        (script, "tests/oracles/script_json.py", SCRIPT_HASH),
    ] {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || digest(source.text.as_bytes()) != hash
        {
            return Err("gallery oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    crate::source_fixture::cases(fixture, FIXTURE, 2)?;
    let mut recipe = fixture.clone();
    if let Some(real) = recipe["real_static"].as_object_mut() {
        real.remove("expected");
    }
    crate::source_fixture::recipe(&recipe, FIXTURE, 2, RECIPE_HASH)
}

struct Scratch(PathBuf);
impl Scratch {
    fn new(root: &Path) -> Result<Self, String> {
        let parent = crate::tooling_environment::confined_directory(root, "logs/tmp", true)?;
        loop {
            let directory = parent.join(format!(
                "source-gallery-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&directory) {
                Ok(()) => return Ok(Self(directory)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err("cannot create historical gallery scratch".into()),
            }
        }
    }
    fn clean(&self) -> Result<(), String> {
        if crate::process_owner::cleanup_pending(&self.0) {
            return Err("historical gallery ownership unresolved; scratch retained".into());
        }
        match std::fs::remove_dir_all(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("cannot clean historical gallery scratch".into()),
        }
    }
}

#[pyclass]
struct MediaOf {
    path: Option<Py<PyAny>>,
}
#[pymethods]
impl MediaOf {
    fn __call__(&self, py: Python<'_>, _entry: &Bound<'_, PyAny>) -> Py<PyAny> {
        self.path
            .as_ref()
            .map_or_else(|| py.None(), |path| path.clone_ref(py))
    }
}
#[pyclass]
struct Thumbnails {
    values: Py<PyDict>,
}
#[pymethods]
impl Thumbnails {
    fn __call__(
        &self,
        py: Python<'_>,
        cid: &str,
        _source: &Bound<'_, PyAny>,
        _format: &str,
        _cache: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        Ok(self
            .values
            .bind(py)
            .get_item(cid)?
            .map_or_else(|| py.None(), |value| value.unbind()))
    }
}

pub(crate) fn bindings<'py>(
    py: Python<'py>,
    source: &Source,
    script: &Source,
) -> PyResult<Bound<'py, PyDict>> {
    let safe = crate::source_oracle_runtime::namespace(py, script, &["json_for_script"], &[], &{
        let bindings = PyDict::new(py);
        bindings.set_item("json", py.import("json")?)?;
        bindings
    })?;
    let bindings = PyDict::new(py);
    bindings.set_item(
        "json_for_script",
        safe.get_item("json_for_script")?
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing safe script oracle"))?,
    )?;
    bindings.set_item("html", py.import("html")?)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (&source.text, &source.path))?;
    let mut found = std::collections::BTreeSet::new();
    for node in crate::source_test_inventory::body(&tree)? {
        if crate::source_test_inventory::kind(&node)? != "Assign" {
            continue;
        }
        let target = node.getattr("targets")?.get_item(0)?;
        if crate::source_test_inventory::kind(&target)? != "Name" {
            continue;
        }
        let name: String = target.getattr("id")?.extract()?;
        if matches!(name.as_str(), "_CSS" | "_JS") {
            if !found.insert(name.clone()) {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "duplicate gallery asset",
                ));
            }
            let value: String = ast
                .call_method1("literal_eval", (node.getattr("value")?,))?
                .extract()?;
            bindings.set_item(name, value)?;
        }
    }
    if found.len() != 2 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "missing gallery assets",
        ));
    }
    Ok(bindings)
}

fn decode<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    let text = serde_json::to_string(value)
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid gallery input"))?;
    py.import("json")?.call_method1("loads", (text,))
}

fn observe(
    py: Python<'_>,
    root: &Path,
    scratch: &Path,
    source: &Source,
    script: &Source,
    fixture: &Value,
) -> PyResult<()> {
    let bindings = bindings(py, source, script)?;
    let path = py.import("pathlib")?.getattr("Path")?;
    for case in fixture["cases"]
        .as_array()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing gallery cases"))?
    {
        let thumbnails = decode(py, &case["thumbnails"])?.cast_into::<PyDict>()?;
        bindings.set_item(
            "thumb_uri",
            Py::new(
                py,
                Thumbnails {
                    values: thumbnails.unbind(),
                },
            )?,
        )?;
        let globals =
            crate::source_oracle_runtime::namespace(py, source, &["render"], &["chr"], &bindings)?;
        let actual: String = globals
            .get_item("render")?
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing gallery renderer"))?
            .call1((
                decode(py, &case["document"])?,
                Py::new(py, MediaOf { path: None })?,
                path.call1(("unused-codec-cache",))?,
            ))?
            .extract()?;
        if case.get("error").is_some() || case["expected"].as_str() != Some(&actual) {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "gallery fixture drift: {}",
                case["name"]
            )));
        }
    }
    bindings.del_item("thumb_uri")?;
    for (name, value) in [("THUMB", 88), ("THUMB_FPS", 9), ("THUMB_QUALITY", 65)] {
        bindings.set_item(name, value)?;
    }
    bindings.set_item("Image", py.import("PIL.Image")?)?;
    bindings.set_item("base64", py.import("base64")?)?;
    bindings.set_item(
        "log",
        py.import("logging")?
            .call_method1("getLogger", ("gallery-oracle",))?,
    )?;
    let mime = PyDict::new(py);
    for (extension, kind) in [
        (".webp", "image/webp"),
        (".webm", "video/webm"),
        (".png", "image/png"),
    ] {
        mime.set_item(extension, kind)?;
    }
    bindings.set_item("_MIME", mime)?;
    let globals = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["_thumb_file", "_still_file", "_uri", "thumb_uri", "render"],
        &["chr", "Exception"],
        &bindings,
    )?;
    let actual: String = globals
        .get_item("render")?
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing real gallery renderer"))?
        .call1((
            decode(py, &fixture["real_static"]["document"])?,
            Py::new(
                py,
                MediaOf {
                    path: Some(path.call1((root.join(ART),))?.unbind()),
                },
            )?,
            path.call1((scratch.to_path_buf(),))?,
        ))?
        .extract()?;
    if fixture["real_static"]["expected"].as_str() != Some(&actual) {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "gallery real static fixture drift",
        ));
    }
    Ok(())
}

pub fn check(
    root: &Path,
    source: &Source,
    script: &Source,
    fixture: &Value,
) -> Result<Value, String> {
    source_identity(source, script)?;
    fixture_identity(fixture)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let art = root.join(ART);
    crate::tooling_environment::reject_links(
        &art,
        "historical gallery artwork cannot follow links",
    )?;
    let original = std::fs::read(&art).map_err(|_| "cannot read historical gallery artwork")?;
    if digest(&original) != ART_HASH {
        return Err("historical gallery artwork identity drift".into());
    }
    let scratch = Scratch::new(&root)?;
    Python::initialize();
    let result = Python::attach(|py| -> Result<(), String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        observe(py, &root, &scratch.0, source, script, fixture)
            .map_err(|error| format!("gallery oracle source failed: {error}"))?;
        if std::fs::read(&art).map_err(|_| "cannot verify historical gallery artwork")? != original
        {
            return Err("historical gallery artwork changed".into());
        }
        Ok(())
    });
    result.map_err(|error| {
        format!(
            "{error}; historical gallery evidence retained at {}",
            scratch.0.display()
        )
    })?;
    scratch.clean()?;
    Ok(
        json!({"cases_checked":3,"source_runtime":"CPython3.11.9","source_hash":HASH,"dependency_hash":SCRIPT_HASH,"fixture_drift":false}),
    )
}
