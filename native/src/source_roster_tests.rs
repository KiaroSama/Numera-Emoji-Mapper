//! Original roster assertions share one admitted corpus, private modules and delayed file cleanup.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::{PyImportError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 18] = [
    (
        "tests/reference/pack_manifest.py",
        "1bede3b165f2b0bf8b1f2c7e4b3dbc43af7de12ced7453860b8252a2b712907e",
    ),
    (
        "tests/test_pack_manifest.py",
        "fbed2a0e99384076d7375244d8a1ab562ef625327447218065ea52cffdce782d",
    ),
    (
        "tests/reference/pack_gallery.py",
        "72674ff1baafb79f28030842601bcbfeec785f5ad84555760669f291a7517ddd",
    ),
    (
        "tests/oracles/pack_rows.py",
        "1c0c0fc15ab5b6abcecb182f138f89da55c566c7210102c1eaaa5ad115343121",
    ),
    (
        "tests/reference/script_json.py",
        "b9afcea858593c9e532ee3690038433e48eefc00910af603b6b1e8b90a3e56e4",
    ),
    (
        "emojikit/operator_config.py",
        "a19f804a7bdd37baccb474eac96df6aa4d46439dd956e457c06adf3c180b9591",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
    (
        "emojikit/media_paths.py",
        "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3",
    ),
    (
        "emojikit/media.py",
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
    ),
    (
        "tests/_media_fixtures.py",
        "1c45ebbb66c1d06bab2fb897e1c24166646974cf02e6fd0d8b54c93c1df8f5e4",
    ),
    (
        "emojikit/media_lottie.py",
        "5112b55f21d4d241ffe9660d5294de20487d4ea1e9772188fb6bd125f7fef63b",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
    (
        "tests/test_pack_rows.py",
        "800c96312877a469e224798dd456885d2e3bea4e2dc5077127bd1e91f6a19503",
    ),
    (
        "tests/reference/collection_notify.py",
        "a153266b6a092b16fe28be5a2151265f4c445967330b0530c14072b824ed3b59",
    ),
    (
        "tests/reference/pack_archive.py",
        "392bd78db1321ef135706c22309facb22b417e1119a7e854717e9dde806ed0ad",
    ),
    (
        "coins/write_manifests.py",
        "64d9de164ec0bb767c745e7191f408eaab860d7a00ceb33ba453d25541e12ffa",
    ),
    (
        "tests/reference/catalog.py",
        "5dab2e475754dc219dfe3fd8e98012fffd59ab0c760f646752ff9d35f7188ee2",
    ),
    (
        "emojikit/pack_rows.py",
        "b455b1c72f2e4aefc7c42685fd027b58f3366ba19e022cb518fb9db7336158c5",
    ),
];
const CLASSES: [(&str, usize); 6] = [
    ("TheRosterDescribesTheLiveSet", 4),
    ("TheRosterRemembersRetiredIds", 5),
    ("StalenessIsNotOptimistic", 3),
    ("ThePageCarriesTheDataToo", 8),
    ("EveryFormatGetsAThumbnail", 1),
    ("AOneFamilyRefreshKeepsTheOther", 3),
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("roster/report assertions require exactly eighteen pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("roster assertion source identity drift; refusing execution".into());
        }
    }
    Ok(())
}
#[pyclass]
struct Temporary {
    name: String,
    exited: Arc<AtomicBool>,
}
#[pymethods]
impl Temporary {
    fn __enter__(&self) -> &str {
        &self.name
    }
    fn __exit__(
        &self,
        _kind: &Bound<'_, PyAny>,
        _error: &Bound<'_, PyAny>,
        _trace: &Bound<'_, PyAny>,
    ) {
        self.exited.store(true, Ordering::SeqCst);
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
        options.set_item("prefix", "original-roster-")?;
        let name: String = crate::source_oracle_runtime::host_import(py, "tempfile")?
            .call_method("mkdtemp", (), Some(&options))?
            .extract()?;
        let path = crate::tooling_environment::physical_root(Path::new(&name))
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(self.root.as_path()) {
            return Err(PyValueError::new_err("original roster temporary escaped"));
        }
        let exited = Arc::new(AtomicBool::new(false));
        self.paths
            .lock()
            .map_err(|_| PyRuntimeError::new_err("roster temporary accounting poisoned"))?
            .push((path, exited.clone()));
        Py::new(py, Temporary { name, exited })
    }
}
#[pyclass]
struct Imports {
    os: Py<PyAny>,
    fixtures: Py<PyAny>,
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
    ) -> PyResult<Bound<'py, PyAny>> {
        if level == 0 {
            if name == "tests._media_fixtures"
                && fromlist.extract::<Vec<String>>()? == ["HAS_FFMPEG", "encode_vp9", "make_png"]
            {
                return Ok(self.fixtures.bind(py).clone());
            }
            if fromlist.is_none() {
                if name == "os" {
                    return Ok(self.os.bind(py).clone());
                }
                if matches!(name, "sqlite3" | "gzip") {
                    return Ok(crate::source_oracle_runtime::host_import(py, name)?.into_any());
                }
            }
        }
        Err(PyImportError::new_err(
            "unexpected original roster assertion import",
        ))
    }
}
#[pyclass]
struct TextPath {
    source: String,
}
#[pymethods]
impl TextPath {
    fn read_text(&self, encoding: &str) -> PyResult<String> {
        if encoding != "utf-8" {
            return Err(PyValueError::new_err("roster text probe requires UTF8"));
        }
        Ok(self.source.clone())
    }
}
#[pyclass]
struct Paths {
    source: String,
}
#[pymethods]
impl Paths {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        value: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if value.extract::<String>().ok().as_deref() == Some("tests/reference/pack_gallery.py") {
            return Ok(Py::new(
                py,
                TextPath {
                    source: self.source.clone(),
                },
            )?
            .into_bound(py)
            .into_any());
        }
        crate::source_oracle_runtime::host_import(py, "pathlib")?
            .getattr("Path")?
            .call1((value,))
    }
}
#[pyclass]
struct Patch {
    environment: Py<PyAny>,
    builtins: Py<PyDict>,
}
#[pymethods]
impl Patch {
    fn __call__<'py>(&self, py: Python<'py>, target: &str) -> PyResult<Bound<'py, PyAny>> {
        let patch =
            crate::source_oracle_runtime::host_import(py, "unittest.mock")?.getattr("patch")?;
        if target == "builtins.print" {
            let values = PyDict::new(py);
            values.set_item(
                "print",
                crate::source_oracle_runtime::host_import(py, "unittest.mock")?
                    .getattr("Mock")?
                    .call0()?,
            )?;
            return patch.call_method1("dict", (self.builtins.bind(py), values));
        }
        if target == "sys.stderr" {
            return patch.call1((target,));
        }
        Err(PyValueError::new_err(
            "unexpected original roster patch target",
        ))
    }
    #[pyo3(signature = (target, values, clear=false))]
    fn dict<'py>(
        &self,
        py: Python<'py>,
        target: &Bound<'py, PyAny>,
        values: &Bound<'py, PyDict>,
        clear: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        if !target.is(self.environment.bind(py)) {
            return Err(PyValueError::new_err("foreign original roster environment"));
        }
        let options = PyDict::new(py);
        options.set_item("clear", clear)?;
        crate::source_oracle_runtime::host_import(py, "unittest.mock")?
            .getattr("patch")?
            .call_method("dict", (target, values), Some(&options))
    }
    #[getter]
    fn object<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::source_oracle_runtime::host_import(py, "unittest.mock")?
            .getattr("patch")?
            .getattr("object")
    }
    #[getter]
    fn multiple<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::source_oracle_runtime::host_import(py, "unittest.mock")?
            .getattr("patch")?
            .getattr("multiple")
    }
}

fn execute(
    py: Python<'_>,
    root: &Path,
    workspace: &Path,
    sources: &[&Source],
    input: &(PathBuf, Vec<u8>),
) -> PyResult<Value> {
    let environment = PyDict::new(py);
    let (fixtures, media, encoder_calls) =
        crate::source_roster_media::build(py, workspace, sources)?;
    let (roster, gallery) =
        crate::source_roster_modules::build(py, workspace, sources, &environment, &media)?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let os = ns.call0()?;
    os.setattr("environ", &environment)?;
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
    let builtins = roster
        .dict()
        .get_item("__builtins__")?
        .ok_or_else(|| PyValueError::new_err("missing roster builtins"))?
        .cast_into::<PyDict>()?;
    let patch = Py::new(
        py,
        Patch {
            environment: environment.clone().unbind().into_any(),
            builtins: builtins.unbind(),
        },
    )?;
    let bindings = PyDict::new(py);
    for (name, value) in [
        ("pm", roster.clone().into_any()),
        ("pack_gallery", gallery.into_any()),
        ("tempfile", tempfile),
        ("unittest", py.import("unittest")?.into_any()),
        ("json", py.import("json")?.into_any()),
        ("patch", patch.into_bound(py).into_any()),
        (
            "Path",
            Py::new(
                py,
                Paths {
                    source: sources[2].text.clone(),
                },
            )?
            .into_bound(py)
            .into_any(),
        ),
    ] {
        bindings.set_item(name, value)?;
    }
    bindings.set_item("__file__", root.join("tests/test_pack_manifest.py"))?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &[
            "_sticker",
            "_FakeTG",
            "TheRosterDescribesTheLiveSet",
            "TheRosterRemembersRetiredIds",
            "StalenessIsNotOptimistic",
            "ThePageCarriesTheDataToo",
            "EveryFormatGetsAThumbnail",
            "AOneFamilyRefreshKeepsTheOther",
        ],
        &[
            "__build_class__",
            "set",
            "sorted",
            "dict",
            "SystemExit",
            "str",
            "open",
            "range",
        ],
        &bindings,
    )?;
    tests
        .get_item("__builtins__")?
        .ok_or_else(|| PyValueError::new_err("missing roster test builtins"))?
        .set_item(
            "__import__",
            Py::new(
                py,
                Imports {
                    os: os.unbind(),
                    fixtures: fixtures.unbind(),
                },
            )?,
        )?;
    let unittest = py.import("unittest")?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    for (name, count) in CLASSES {
        let class = required(&tests, name)?;
        let names: Vec<String> = unittest
            .getattr("TestLoader")?
            .call0()?
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        if names.len() != count {
            return Err(PyValueError::new_err(
                "original roster method collection drift",
            ));
        }
        for method in names {
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
                        "original roster {name}.{method} reported {field}: {}; retained",
                        crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                    )));
                }
            }
            if !environment.is_empty() {
                return Err(PyValueError::new_err(
                    "original roster environment not restored",
                ));
            }
            cases.push(format!("{name}.{method}"));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 24
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original roster execution incomplete",
        ));
    }
    let reports = crate::source_report_tests::run(
        py,
        workspace,
        sources,
        &roster,
        &required(&bindings, "tempfile")?,
    )?;
    let temporary = temporary.borrow(py);
    let paths = temporary
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("roster cleanup accounting poisoned"))?;
    // Roster15 + the original publisher report's one catalog/image context.
    if paths.len() != 16
        || paths
            .iter()
            .any(|(_, exited)| !exited.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original roster context exit incomplete",
        ));
    }
    directories(workspace, workspace, &mut 128, 0, &mut vec![])?;
    let expected = paths
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
    let actual = std::fs::read_dir(workspace)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    if actual != expected {
        return Err(PyValueError::new_err(
            "unexpected original roster workspace; retained",
        ));
    }
    let allowed = [
        "catalog.db",
        "t.json",
        "packs",
        "packs/index.json",
        "index.json",
        "coin1_by_bot.json",
        "coin1_by_bot.md",
        "coin1_by_bot.html",
        "README.md",
        "logs",
        "logs/tmp",
        "static.png",
        "anim.tgs",
        "clip.webm",
        "frames",
        "frames/000.png",
        "frames/001.png",
        "frames/002.png",
        "thumbs",
        "thumbs/111.webp",
        "thumbs/222.webp",
        "thumbs/222_still.webp",
        "thumbs/333.webm",
        "media",
        "media/a.png",
        "manifests",
        "manifests/pk1_by_bot.md",
    ];
    let mut members = Vec::new();
    let mut indexes = Vec::new();
    let mut thumbnails = serde_json::Map::new();
    for root in expected {
        let mut pending = vec![root.clone()];
        while let Some(path) = pending.pop() {
            let relative = path
                .strip_prefix(&root)
                .map_err(|_| PyValueError::new_err("foreign roster evidence"))?
                .to_str()
                .ok_or_else(|| PyValueError::new_err("nonUTF8 roster member"))?
                .replace('\\', "/");
            if path != root && !allowed.contains(&relative.as_str()) {
                return Err(PyValueError::new_err(
                    "unexpected original roster member; retained",
                ));
            }
            if path.is_dir() {
                pending.extend(
                    std::fs::read_dir(&path)?
                        .map(|e| e.map(|e| e.path()))
                        .collect::<std::io::Result<Vec<_>>>()?,
                );
            } else {
                if std::fs::metadata(&path)?.len() > 65536 {
                    return Err(PyValueError::new_err("excessive original roster evidence"));
                }
                if relative == "index.json" {
                    indexes.push(
                        serde_json::from_slice::<Value>(&std::fs::read(&path)?)
                            .map_err(|_| PyValueError::new_err("invalid original family index"))?,
                    );
                }
                if relative.starts_with("thumbs/") {
                    let bytes = std::fs::read(&path)?;
                    if bytes.is_empty() || thumbnails.insert(relative, json!({
                        "bytes":bytes.len(), "sha256":crate::codec_raster::digest_hex(&bytes),
                    })).is_some() {
                        return Err(PyValueError::new_err("empty or repeated original thumbnail"));
                    }
                }
            }
            members.push(path);
        }
    }
    if indexes.len() != 2 {
        return Err(PyValueError::new_err(
            "original family index observation count drift",
        ));
    }
    let expected_thumbnails = [
        "thumbs/111.webp",
        "thumbs/222.webp",
        "thumbs/222_still.webp",
        "thumbs/333.webm",
    ]
    .into_iter()
    .map(String::from)
    .collect::<BTreeSet<_>>();
    if thumbnails.keys().cloned().collect::<BTreeSet<_>>() != expected_thumbnails
        || encoder_calls.load(Ordering::SeqCst) != 1
        || std::fs::read(&input.0)? != input.1
    {
        return Err(PyValueError::new_err(
            "original all-format generation incomplete; retained",
        ));
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
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":24,"cases":cases,"reports":reports,"temporary_calls":16,"family_indexes":indexes,"errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "excluded":[],"thumbnails":thumbnails,"encoder_calls":1,"lottie_input_sha256":crate::codec_raster::digest_hex(&input.1),
        "ports":"24 unchanged roster methods plus7 original report assertions/builders/Lottie/real SQLite/files/writer/gallery/assets; fixed clock/private patchable modules/environment; pinned gallery source-text probe; original mocked family collaborators retained; one exact FFmpeg argv via existing owned5wall3idle capture inside outer session; no real-browser or abrupt-owner-death claim; success-only checked cleanup"}),
    )
}
pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("roster workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect roster workspace")?
        .next()
        .is_some()
    {
        return Err("roster workspace must be empty".into());
    }
    let input = crate::source_roster_media::fixture(&root)?;
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|s| s.getattr("dont_write_bytecode"))
            .and_then(|v| v.extract::<bool>())
            .map_err(|_| "cannot inspect roster bytecode policy")?
        {
            return Err("roster assertions require disabled import bytecode writes".into());
        }
        execute(py, &root, &workspace, sources, &input).map_err(|e| {
            format!(
                "original roster assertions failed: {}",
                crate::logging::redact(&e.to_string(), &[])
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
        "roster-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
