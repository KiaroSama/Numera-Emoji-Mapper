//! Original ZIP assertions execute against private historical modules and real disposable archives.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 9] = [
    (
        "tests/reference/pack_export.py",
        "5db69b593bb9b5577c4c4490f4efe65ace9a085e0142eda21936999de67c28d4",
    ),
    (
        "tests/test_pack_export.py",
        "38d5af4cfdbdeca5751733e0eb9de91dfe622b6c708191a1ea03648b1ca5c0e7",
    ),
    (
        "tests/_media_fixtures.py",
        "1c45ebbb66c1d06bab2fb897e1c24166646974cf02e6fd0d8b54c93c1df8f5e4",
    ),
    (
        "tests/reference/pack_archive.py",
        "392bd78db1321ef135706c22309facb22b417e1119a7e854717e9dde806ed0ad",
    ),
    (
        "tests/oracles/pack_rows.py",
        "1c0c0fc15ab5b6abcecb182f138f89da55c566c7210102c1eaaa5ad115343121",
    ),
    (
        "emojikit/media_paths.py",
        "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3",
    ),
    (
        "emojikit/operator_config.py",
        "a19f804a7bdd37baccb474eac96df6aa4d46439dd956e457c06adf3c180b9591",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
];
const METHODS: [&str; 4] = [
    "test_files_are_in_slot_order_with_the_manifest_and_sources_untouched",
    "test_the_set_name_works_as_well_as_the_number",
    "test_a_stale_roster_is_refused",
    "test_a_missing_file_refuses_instead_of_a_partial_zip",
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("export tests require exactly nine pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("export tests source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

#[pyclass]
struct Temporary {
    name: String,
    cleaned: Arc<AtomicBool>,
}
#[pymethods]
impl Temporary {
    #[getter]
    fn name(&self) -> &str {
        &self.name
    }
    fn cleanup(&self) {
        self.cleaned.store(true, Ordering::SeqCst);
    }
}
#[pyclass]
struct Directories {
    root: PathBuf,
    paths: Mutex<Vec<(PathBuf, Arc<AtomicBool>)>>,
}
#[pymethods]
impl Directories {
    fn temporary(&self, py: Python<'_>) -> PyResult<Py<Temporary>> {
        let options = PyDict::new(py);
        options.set_item("dir", &self.root)?;
        options.set_item("prefix", "original-export-")?;
        let name: String = crate::source_oracle_runtime::host_import(py, "tempfile")?
            .call_method("mkdtemp", (), Some(&options))?
            .extract()?;
        let path = crate::tooling_environment::physical_root(Path::new(&name))
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(self.root.as_path()) {
            return Err(PyValueError::new_err("original export temporary escaped"));
        }
        let cleaned = Arc::new(AtomicBool::new(false));
        self.paths
            .lock()
            .map_err(|_| PyRuntimeError::new_err("export temporary accounting poisoned"))?
            .push((path, cleaned.clone()));
        Py::new(py, Temporary { name, cleaned })
    }
}
#[pyclass]
struct Export {
    function: Py<PyAny>,
    codes: Mutex<Vec<u8>>,
}
#[pymethods]
impl Export {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        pack: &str,
        zip_path: PathBuf,
    ) -> PyResult<Bound<'py, PyAny>> {
        let result = self.function.bind(py).call1((pack, zip_path))?;
        self.codes
            .lock()
            .map_err(|_| PyRuntimeError::new_err("export outcome accounting poisoned"))?
            .push(result.extract()?);
        Ok(result)
    }
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    if py
        .import("PIL")?
        .getattr("__version__")?
        .extract::<String>()?
        != "12.3.0"
    {
        return Err(PyValueError::new_err(
            "original export requires Pillow12.3.0",
        ));
    }
    let archive = crate::source_archive_tests::module(
        py,
        workspace,
        [
            sources[3], sources[4], sources[5], sources[6], sources[7], sources[8],
        ],
    )?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let roster = ns.call0()?;
    roster.setattr(
        "OUT_DIR",
        py.import("pathlib")?
            .getattr("Path")?
            .call1((workspace.join("packs"),))?,
    )?;
    let options = PyDict::new(py);
    options.set_item(
        "side_effect",
        PyRuntimeError::new_err("unpatched original roster freshness").value(py),
    )?;
    roster.setattr(
        "check_stale",
        py.import("unittest.mock")?
            .getattr("Mock")?
            .call((), Some(&options))?,
    )?;
    let environment = PyDict::new(py);
    let os = ns.call0()?;
    os.setattr("environ", &environment)?;
    os.setattr("replace", py.import("os")?.getattr("replace")?)?;
    // The original test patches os.environ; share that same private dictionary with its operator functions.
    let value = archive.getattr("operator_config")?.getattr("value")?;
    value
        .getattr("__globals__")?
        .get_item("os")?
        .setattr("environ", &environment)?;
    let bindings = PyDict::new(py);
    for (name, value) in [
        ("Path", py.import("pathlib")?.getattr("Path")?),
        ("json", py.import("json")?.into_any()),
        ("zipfile", py.import("zipfile")?.into_any()),
        ("os", os.clone()),
        ("operator_config", archive.getattr("operator_config")?),
        ("pack_archive", archive.clone().into_any()),
        ("pack_manifest", roster.clone()),
    ] {
        bindings.set_item(name, value)?;
    }
    for (name, code) in [("EXIT_OK", 0), ("EXIT_FAILED", 1), ("EXIT_STALE", 3)] {
        bindings.set_item(name, code)?;
    }
    let functions = ["find_roster", "plan_entries", "export"];
    let original = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &functions,
        &["str", "len", "sorted", "print", "OSError", "ValueError"],
        &bindings,
    )?;
    let export = PyModule::new(py, "_original_pack_export")?;
    export.dict().update(original.as_mapping())?;
    for name in functions {
        let function = required(&original, name)?;
        let rebound = py.import("types")?.getattr("FunctionType")?.call1((
            function.getattr("__code__")?,
            export.dict(),
            function.getattr("__name__")?,
            function.getattr("__defaults__")?,
            function.getattr("__closure__")?,
        ))?;
        for attribute in ["__kwdefaults__", "__annotations__"] {
            rebound.setattr(attribute, function.getattr(attribute)?)?;
        }
        export.add(name, rebound)?;
    }
    let capture = Py::new(
        py,
        Export {
            function: export.getattr("export")?.unbind(),
            codes: Mutex::new(vec![]),
        },
    )?;
    export.add("export", capture.bind(py))?;
    let images = PyDict::new(py);
    images.set_item("Image", py.import("PIL.Image")?)?;
    let images =
        crate::source_oracle_runtime::namespace(py, sources[2], &["make_png"], &[], &images)?;
    let temporary = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(vec![]),
        },
    )?;
    let tempfile = ns.call0()?;
    tempfile.setattr(
        "TemporaryDirectory",
        temporary.bind(py).getattr("temporary")?,
    )?;
    let unittest = py.import("unittest")?;
    let tests = PyDict::new(py);
    for (name, value) in [
        ("pack_export", export.into_any()),
        ("pack_archive", archive.into_any()),
        ("pack_manifest", roster),
        ("make_png", required(&images, "make_png")?),
        ("tempfile", tempfile),
        ("unittest", unittest.clone().into_any()),
        ("mock", py.import("unittest.mock")?.into_any()),
        ("Path", py.import("pathlib")?.getattr("Path")?),
        ("contextlib", py.import("contextlib")?.into_any()),
        ("hashlib", py.import("hashlib")?.into_any()),
        ("io", py.import("io")?.into_any()),
        ("json", py.import("json")?.into_any()),
        ("zipfile", py.import("zipfile")?.into_any()),
        ("os", os),
    ] {
        tests.set_item(name, value)?;
    }
    tests.set_item("KA", format!("s:{}", "a".repeat(32)))?;
    tests.set_item("KB", format!("v:{}", "b".repeat(32)))?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["APackExportsAsAZip"],
        &["__build_class__", "str"],
        &tests,
    )?;
    let class = required(&tests, "APackExportsAsAZip")?;
    let actual: Vec<String> = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    let mut expected = METHODS.to_vec();
    expected.sort_unstable();
    if actual != expected {
        return Err(PyValueError::new_err("original export collection drift"));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut outputs = Vec::new();
    for method in METHODS {
        let test = class.call1((method,))?;
        test.call_method1("run", (&result,))?;
        for field in [
            "errors",
            "failures",
            "skipped",
            "expectedFailures",
            "unexpectedSuccesses",
        ] {
            if result.getattr(field)?.len()? != 0 {
                return Err(PyValueError::new_err(format!(
                    "original export {method} reported {field}: {}; retained",
                    crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                )));
            }
        }
        if !environment.is_empty() {
            return Err(PyValueError::new_err(
                "original export environment not restored",
            ));
        }
        let path: PathBuf = test.getattr("dir")?.extract()?;
        if path.parent() != Some(workspace) {
            return Err(PyValueError::new_err("original export evidence escaped"));
        }
        let file = path.join("out.zip");
        let mut entries = Vec::new();
        if file.is_file() {
            let zip = py.import("zipfile")?.getattr("ZipFile")?.call1((&file,))?;
            let reading = (|| -> PyResult<()> {
                let names: Vec<String> = zip.call_method0("namelist")?.extract()?;
                if names.len() != 4 {
                    return Err(PyValueError::new_err("original export member count drift"));
                }
                for name in names {
                    let bytes: Vec<u8> = zip.call_method1("read", (&name,))?.extract()?;
                    if bytes.len() > 16384 {
                        return Err(PyValueError::new_err("excessive original ZIP member"));
                    }
                    entries.push(json!({"name":name,"sha256":crate::codec_raster::digest_hex(&bytes),
                        "text":if name=="_manifest.md" { Some(String::from_utf8(bytes).map_err(|_|PyValueError::new_err("nonUTF8 original manifest"))?) } else { None }}));
                }
                Ok(())
            })();
            let closed = zip.call_method0("close");
            reading?;
            closed?;
        }
        outputs.push(json!({"case":format!("APackExportsAsAZip.{method}"),"entries":entries}));
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 4
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original export execution incomplete",
        ));
    }
    let codes = capture
        .borrow(py)
        .codes
        .lock()
        .map_err(|_| PyRuntimeError::new_err("export outcomes poisoned"))?
        .clone();
    if codes != [0, 0, 3, 1] {
        return Err(PyValueError::new_err("original export outcome drift"));
    }
    let temporary = temporary.borrow(py);
    let paths = temporary
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("export cleanup accounting poisoned"))?;
    if paths.len() != 4
        || paths
            .iter()
            .any(|(_, closed)| !closed.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err("original export cleanup incomplete"));
    }
    directories(workspace, workspace, &mut 64, 0, &mut vec![])?;
    let expected = paths
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    if actual != expected {
        return Err(PyValueError::new_err(
            "unexpected original export workspace; retained",
        ));
    }
    let allowed = [
        "packs",
        "packs/mine2_by_bot.json",
        "logo.png",
        "media",
        "media/a.webp",
        "media/b.webm",
        "out.zip",
    ];
    let mut members = Vec::new();
    for root in expected {
        let mut pending = vec![root.clone()];
        while let Some(path) = pending.pop() {
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| PyValueError::new_err("foreign original export evidence"))?
                .to_str()
                .ok_or_else(|| PyValueError::new_err("nonUTF8 export member"))?
                .replace('\\', "/");
            if path != root && !allowed.contains(&relative.as_str()) {
                return Err(PyValueError::new_err(
                    "unexpected original export member; retained",
                ));
            }
            if path.is_dir() {
                pending.extend(
                    std::fs::read_dir(&path)?
                        .map(|entry| entry.map(|entry| entry.path()))
                        .collect::<std::io::Result<Vec<_>>>()?,
                );
            } else if std::fs::metadata(&path)?.len() > 65536 {
                return Err(PyValueError::new_err(
                    "excessive original export evidence; retained",
                ));
            }
            members.push(path);
        }
    }
    members.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in members {
        if path.is_dir() {
            std::fs::remove_dir(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":"12.3.0",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":4,"cases":METHODS.map(|name|format!("APackExportsAsAZip.{name}")),"temporary_calls":4,"codes":codes,"outputs":outputs,
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"unchanged four original methods/make_png/external Pillow/zipfile/private patchable export+archive+roster/environment; original patched freshness/catalog ports do not prove native freshness; ordered decoded members not compressed ZIP byte equivalence; delayed success-only cleanup"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("export workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect export workspace")?
        .next()
        .is_some()
    {
        return Err("export workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect export bytecode policy")?
        {
            return Err("export tests require disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original export tests failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })
}
pub async fn observe_owned(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    sources: &[&Source],
) -> Result<Value, String> {
    source_identity(sources)?;
    crate::source_oracle_process::check(
        root,
        executable,
        "export-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
