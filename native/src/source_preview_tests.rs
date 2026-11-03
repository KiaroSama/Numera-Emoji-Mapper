//! One unchanged original preview test compares twelve real native results with historical bytes.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) const FILES: &[&str] = &[
    "preview-alpha.webm",
    "static-104-True.webp",
    "static-104-False.webp",
    "static-72-True.webp",
    "static-72-False.webp",
    "animated-104-True.webp",
    "animated-104-False.webp",
    "animated-72-True.webp",
    "animated-72-False.webp",
    "video-104-True.webp",
    "video-104-False.webp",
    "video-72-True.webp",
    "video-72-False.webp",
];

#[pyclass]
struct Temporary {
    allowed: PathBuf,
    directories: Py<PyAny>,
}
#[pymethods]
impl Temporary {
    #[pyo3(signature=(*, dir))]
    fn __call__(&self, py: Python<'_>, dir: PathBuf) -> PyResult<Py<PyAny>> {
        if dir != self.allowed {
            return Err(PyValueError::new_err(
                "unexpected original preview temporary root",
            ));
        }
        Ok(self
            .directories
            .bind(py)
            .call_method0("temporary")?
            .unbind())
    }
}

#[pyclass]
struct Imports(Py<PyAny>);
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
        if level != 0
            || name != "tests._video_fixtures"
            || fromlist.extract::<Vec<String>>()? != ["BLUE", "RED", "encode"]
        {
            return Err(PyValueError::new_err("unexpected original preview import"));
        }
        Ok(self.0.bind(py).clone())
    }
}

#[pyclass]
struct Native {
    project: PathBuf,
    workspace: PathBuf,
    calls: Arc<AtomicUsize>,
}
#[pymethods]
impl Native {
    fn execute<'py>(
        &self,
        py: Python<'py>,
        request: &Bound<'py, PyDict>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let encoded: String = py
            .import("json")?
            .call_method1("dumps", (request,))?
            .extract()?;
        let request: Value = serde_json::from_str(&encoded)
            .map_err(|_| PyValueError::new_err("original preview request is not JSON"))?;
        let index = self.calls.load(Ordering::SeqCst);
        if index >= 12 {
            return Err(PyValueError::new_err(
                "original preview request count exceeded",
            ));
        }
        let format = ["static", "animated", "video"][index / 4];
        let (size, fps) = [(104, 15), (72, 10)][index % 4 / 2];
        let still = index % 2 == 0;
        let output = PathBuf::from(
            request["output"]
                .as_str()
                .ok_or_else(|| PyValueError::new_err("preview output missing"))?,
        );
        let parent = crate::tooling_environment::physical_root(
            output
                .parent()
                .ok_or_else(|| PyValueError::new_err("preview output parent missing"))?,
        )
        .map_err(PyValueError::new_err)?;
        if parent.parent() != Some(self.workspace.as_path()) {
            return Err(PyValueError::new_err(
                "original preview output escaped its context",
            ));
        }
        let source = match format {
            "static" => self.project.join("assets/numera-emoji-mapper-logo.png"),
            "animated" => self
                .project
                .join("tests/fixtures/lottie/red_circle_512.tgs"),
            _ => parent.join("preview-alpha.webm"),
        };
        let expected = json!({"operation":"preview","source":source,"format":format,
            "key":format!("fixture:{format}"),"fps":fps,"size":size,"still":still,
            "output":parent.join(format!("{format}-{size}-{}.webp",if still {"True"} else {"False"}))});
        if request != expected {
            return Err(PyValueError::new_err("original preview request drift"));
        }
        let before = std::fs::read(&source)?;
        let result = py
            .detach(|| {
                crate::codec_worker::execute_typed_at(
                    Some(&self.workspace.join("process")),
                    &request,
                )
            })
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        if std::fs::read(source)? != before {
            return Err(PyValueError::new_err(
                "native preview changed original media",
            ));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        println!("original/native preview request {} completed", index + 1);
        py.import("json")?
            .call_method1("loads", (result.to_string(),))
    }
}

pub(super) fn admit(project: &Path) -> Result<(), String> {
    for (relative, hash) in [
        (
            "assets/numera-emoji-mapper-logo.png",
            "6f85376de3893dfa1e78045ce264103b0055acec400dd8a35222405d5c4f6951",
        ),
        (
            "tests/fixtures/lottie/red_circle_512.tgs",
            "6bf94cd15f126d10d45da557627a375da84c81f1b18d2affbf4b21263d09a991",
        ),
    ] {
        let path = project.join(relative);
        crate::tooling_environment::reject_links(
            &path,
            "original preview input cannot follow links",
        )?;
        if !crate::migration_bundle::resolved(&path)?.starts_with(project)
            || crate::tooling_manifest::digest(&path)? != hash
        {
            return Err("original preview media identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub(super) fn run<'py>(
    py: Python<'py>,
    project: &Path,
    workspace: &Path,
    sources: [&Source; 3],
    media: &Bound<'py, PyAny>,
    lottie: &Bound<'py, PyDict>,
    directories: &Bound<'py, PyAny>,
) -> PyResult<Vec<String>> {
    for name in ["lottie_still_webp", "lottie_preview_webp"] {
        media.setattr(name, required(lottie, name)?)?;
    }
    media.setattr("PREVIEW_QUALITY", 60)?;
    let values = PyDict::new(py);
    for name in ["hashlib", "io", "threading", "os"] {
        values.set_item(name, py.import(name)?)?;
    }
    values.set_item("Image", py.import("PIL.Image")?)?;
    values.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    values.set_item("media", media)?;
    values.set_item("video_decode", media.getattr("video_decode")?)?;
    let preview = crate::source_oracle_runtime::module(
        py,
        sources[1],
        &[
            "_check_key",
            "_dest",
            "cached_path",
            "preview_bytes",
            "_write",
            "_video_animation",
        ],
        &["str", "len", "range", "round", "max", "min", "ValueError"],
        &values,
        "_original_complete_preview",
    )?;
    preview.setattr("_locks", PyDict::new(py))?;
    for name in ["_guard", "_renders", "_MAX_VIDEO_FRAMES"] {
        crate::source_oracle_runtime::assignment(py, sources[1], name, &preview.dict())?;
    }
    let video = crate::source_oracle_runtime::module(
        py,
        sources[2],
        &["encode"],
        &["str", "list", "bytes"],
        &values,
        "_original_preview_video",
    )?;
    for name in ["RED", "BLUE"] {
        crate::source_oracle_runtime::assignment(py, sources[2], name, &video.dict())?;
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let bridge = Py::new(
        py,
        Native {
            project: project.into(),
            workspace: workspace.into(),
            calls: calls.clone(),
        },
    )?;
    let temporary = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    temporary.setattr(
        "TemporaryDirectory",
        Py::new(
            py,
            Temporary {
                allowed: project.join("logs"),
                directories: directories.clone().unbind(),
            },
        )?,
    )?;
    values.set_item("tempfile", temporary)?;
    values.set_item(
        "ROOT",
        py.import("pathlib")?.getattr("Path")?.call1((project,))?,
    )?;
    values.set_item("unittest", py.import("unittest")?)?;
    values.set_item("media_bridge", bridge)?;
    values.set_item("panel_preview", &preview)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &["NativePreviewCodec"],
        &["__build_class__", "str", "len"],
        &values,
    )?;
    required(&tests, "__builtins__")?.set_item(
        "__import__",
        Py::new(py, Imports(video.into_any().unbind()))?,
    )?;
    let class = required(&tests, "NativePreviewCodec")?;
    let method = "test_static_and_lottie_preview_bytes_match_source";
    let methods: Vec<String> = py
        .import("unittest")?
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods != [method] {
        return Err(PyValueError::new_err(
            "original complete preview collection drift",
        ));
    }
    let result = py.import("unittest")?.getattr("TestResult")?.call0()?;
    class.call1((method,))?.call_method1("run", (&result,))?;
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyValueError::new_err(format!(
                "original preview reported {field}; retained"
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 1 || calls.load(Ordering::SeqCst) != 12 {
        return Err(PyValueError::new_err(
            "original complete preview execution incomplete",
        ));
    }
    let native = py.import("pathlib")?.getattr("Path")?;
    let paths = preview.getattr("_locks")?.cast_into::<PyDict>()?;
    if paths.len() != 12 {
        return Err(PyValueError::new_err(
            "original preview cache inventory drift",
        ));
    }
    let mut caches = BTreeSet::new();
    for (destination, _) in paths.iter() {
        let path: PathBuf = native.call1((destination,))?.extract()?;
        let parent = path
            .parent()
            .ok_or_else(|| PyValueError::new_err("preview cache parent missing"))?;
        if parent.file_name().and_then(|value| value.to_str()) != Some("preview")
            || parent.parent().and_then(Path::parent) != Some(workspace)
            || !path.is_file()
            || path.extension().and_then(|v| v.to_str()) != Some("webp")
        {
            return Err(PyValueError::new_err(
                "original preview cache escaped; retained",
            ));
        }
        caches.insert(parent.to_path_buf());
    }
    if caches.len() != 1 {
        return Err(PyValueError::new_err(
            "original preview context inventory drift",
        ));
    }
    let cache = caches.into_iter().next().unwrap();
    crate::source_tooling_oracle::directories(workspace, &cache, &mut 16, 0, &mut Vec::new())?;
    let files = std::fs::read_dir(&cache)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    let expected = paths
        .keys()
        .iter()
        .map(|key| key.extract::<PathBuf>())
        .collect::<PyResult<BTreeSet<_>>>()?;
    if files != expected
        || cache.parent().unwrap().join("catalog.db").exists()
        || crate::process_owner::cleanup_pending(workspace)
    {
        return Err(PyValueError::new_err(
            "original preview cache or custody differs; retained",
        ));
    }
    admit(project).map_err(PyValueError::new_err)?;
    // The shared ledger inventories the thirteen remaining regular files before final teardown.
    for path in files {
        std::fs::remove_file(path)?;
    }
    std::fs::remove_dir(cache)?;
    Ok(vec![format!("NativePreviewCodec.{method}")])
}
