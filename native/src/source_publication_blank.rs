//! Original blank-video assertions with a private subprocess patch and unchanged bounded runner.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct Patch {
    subprocess: Py<PyAny>,
    calls: AtomicUsize,
}
#[pymethods]
impl Patch {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        target: &str,
        replacement: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if target != "subprocess.Popen" || !replacement.is_callable() {
            return Err(PyValueError::new_err(
                "unexpected original blank-probe patch",
            ));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        py.import("unittest.mock")?
            .getattr("patch")?
            .call_method1("object", (self.subprocess.bind(py), "Popen", replacement))
    }
    #[getter]
    fn object<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("unittest.mock")?
            .getattr("patch")?
            .getattr("object")
    }
}

pub(super) fn run(
    py: Python<'_>,
    media_source: &Source,
    check_source: &Source,
    tests_source: &Source,
    build: &Bound<'_, PyModule>,
    static_check: &Bound<'_, PyAny>,
    result: &Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let bindings = PyDict::new(py);
    let subprocess = ns.call0()?;
    let real = py.import("subprocess")?;
    for name in [
        "PIPE",
        "TimeoutExpired",
        "CalledProcessError",
        "SubprocessError",
        "CompletedProcess",
    ] {
        subprocess.setattr(name, real.getattr(name)?)?;
    }
    let options = PyDict::new(py);
    options.set_item(
        "side_effect",
        py.import("builtins")?
            .getattr("AssertionError")?
            .call1(("unexpected original blank-probe process",))?,
    )?;
    let refusal = py
        .import("unittest.mock")?
        .getattr("Mock")?
        .call((), Some(&options))?;
    subprocess.setattr("Popen", &refusal)?;
    subprocess.setattr("run", &refusal)?;
    let os = ns.call0()?;
    os.setattr("name", py.import("os")?.getattr("name")?)?;
    let environment = PyDict::new(py);
    environment.set_item("NUMERA_CODEC_OWNED_GROUP", "1")?;
    os.setattr("environ", environment)?;
    bindings.set_item("os", os)?;
    bindings.set_item("subprocess", &subprocess)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item(
        "MediaError",
        py.import("builtins")?.getattr("RuntimeError")?,
    )?;
    bindings.set_item(
        "log",
        py.import("logging")?
            .getattr("Logger")?
            .call1(("original-blank-probe",))?,
    )?;
    bindings.set_item("signal", py.import("signal")?)?;
    bindings.set_item(
        "_KILL_GRACE",
        crate::source_logging_oracle::literal(py, media_source, "_KILL_GRACE", false)?,
    )?;
    let timeout = PyDict::new(py);
    timeout.set_item(
        "return_value",
        crate::source_logging_oracle::literal(py, media_source, "FFMPEG_TIMEOUT", false)?,
    )?;
    bindings.set_item(
        "ff_timeout",
        py.import("unittest.mock")?
            .getattr("Mock")?
            .call((), Some(&timeout))?,
    )?;
    let media = crate::source_oracle_runtime::module(
        py,
        media_source,
        &["_kill_tree", "_run"],
        &["str", "BaseException", "OSError"],
        &bindings,
        "_original_publication_probe_media",
    )?;
    media.setattr("ffmpeg_path", &refusal)?;
    for name in ["WEBM_MAX_SECONDS", "VISIBLE_ALPHA", "BLANK_MAX_VISIBLE"] {
        media.setattr(
            name,
            crate::source_logging_oracle::literal(py, media_source, name, false)?,
        )?;
    }
    bindings.set_item("media", &media)?;
    bindings.set_item("_static_is_blank", static_check)?;
    bindings.set_item("_FRAME_BYTES", 64 * 64 * 4)?;
    let check = crate::source_oracle_runtime::module(
        py,
        check_source,
        &["_video_is_blank", "_media_ok"],
        &["len", "range", "sum", "str", "Exception"],
        &bindings,
        "_original_publication_probe_check",
    )?;
    let mock = ns.call0()?;
    let patch = Py::new(
        py,
        Patch {
            subprocess: subprocess.clone().unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    mock.setattr("patch", patch.bind(py))?;
    bindings.set_item("mock", mock)?;
    bindings.set_item("unittest", py.import("unittest")?)?;
    bindings.set_item("bc", &check)?;
    bindings.set_item("collection_media_check", &check)?;
    check.setattr("_FRAME_BYTES", 64 * 64 * 4)?;
    let original = crate::source_oracle_runtime::namespace(
        py,
        tests_source,
        &["_frame", "_fake_popen", "VideoBlankCheck"],
        &["__build_class__", "bytearray", "bytes", "range", "all"],
        &bindings,
    )?;
    let class = required(&original, "VideoBlankCheck")?;
    let methods: Vec<String> = py
        .import("unittest")?
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods.len() != 6 {
        return Err(PyValueError::new_err(
            "original blank-video collection drift",
        ));
    }
    let previous = build.getattr("_media_ok")?;
    let before = result.getattr("testsRun")?.extract::<usize>()?;
    for method in &methods {
        class.call1((method,))?.call_method1("run", (result,))?;
        for field in [
            "errors",
            "failures",
            "skipped",
            "expectedFailures",
            "unexpectedSuccesses",
        ] {
            if result.getattr(field)?.len()? != 0 {
                return Err(PyValueError::new_err(format!(
                    "original VideoBlankCheck.{method} reported {field}"
                )));
            }
        }
        if !subprocess.getattr("Popen")?.is(&refusal) || !media.getattr("ffmpeg_path")?.is(&refusal)
        {
            return Err(PyValueError::new_err(
                "original blank-video patch not restored",
            ));
        }
    }
    if patch.borrow(py).calls.load(Ordering::SeqCst) != 6
        || result.getattr("testsRun")?.extract::<usize>()? != before + 6
        || refusal.getattr("call_count")?.extract::<usize>()? != 0
        || !build.getattr("_media_ok")?.is(&previous)
    {
        return Err(PyValueError::new_err(
            "original blank-video accounting drift",
        ));
    }
    Ok(methods
        .iter()
        .map(|method| format!("VideoBlankCheck.{method}"))
        .collect())
}
