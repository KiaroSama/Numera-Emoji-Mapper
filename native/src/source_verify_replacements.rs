//! Unchanged replacement/distance assertions use the admitted original client, not native answers.
use crate::source_git_inventory::Source;
use crate::source_temporary::Directories;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct Imports {
    telegram: Py<PyAny>,
    calls: AtomicUsize,
}
#[pymethods]
impl Imports {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        _globals: &Bound<'_, PyAny>,
        _locals: &Bound<'_, PyAny>,
        fromlist: Vec<String>,
        level: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        if level != 0 || name != "tests.oracles.telegram_api" || fromlist != ["Telegram"] {
            return Err(PyImportError::new_err(
                "unexpected original replacement import",
            ));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.telegram.bind(py).clone())
    }
    #[getter]
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}
#[pyclass]
struct Loader {
    path: PathBuf,
    module: Py<PyModule>,
    calls: AtomicUsize,
}
#[pymethods]
impl Loader {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        path: PathBuf,
        name: &str,
    ) -> PyResult<Bound<'py, PyModule>> {
        if path != self.path
            || !["coins_verify_logos_probe", "coins_verify_logos_distance"].contains(&name)
        {
            return Err(PyValueError::new_err(
                "unexpected original replacement module load",
            ));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.module.bind(py).clone())
    }
    #[getter]
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}
#[pyclass]
struct StageDirectories {
    ledger: Py<Directories>,
    calls: AtomicUsize,
}
#[pymethods]
impl StageDirectories {
    #[pyo3(signature = (*, prefix, dir))]
    fn __call__(&self, py: Python<'_>, prefix: &str, dir: PathBuf) -> PyResult<String> {
        let dir = crate::tooling_environment::physical_root(&dir).map_err(PyValueError::new_err)?;
        let ledger = self.ledger.borrow(py);
        let paths = ledger
            .paths
            .lock()
            .map_err(|_| PyValueError::new_err("replacement staging ledger poisoned"))?;
        if prefix != "verify-"
            || dir.file_name().is_none_or(|name| name != ".incoming")
            || !paths
                .iter()
                .any(|(path, _)| dir.parent() == Some(path.as_path()))
        {
            return Err(PyValueError::new_err(
                "original replacement staging escaped its test context",
            ));
        }
        let options = PyDict::new(py);
        options.set_item("prefix", prefix)?;
        options.set_item("dir", &dir)?;
        let name: String = py
            .import("tempfile")?
            .call_method("mkdtemp", (), Some(&options))?
            .extract()?;
        let path = crate::tooling_environment::physical_root(Path::new(&name))
            .map_err(PyValueError::new_err)?;
        if path.parent() != Some(dir.as_path()) {
            return Err(PyValueError::new_err(
                "original replacement stage parent drift",
            ));
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(name)
    }
    #[getter]
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

pub(super) fn build<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
    ledger: &Py<Directories>,
) -> PyResult<(Bound<'py, PyModule>, Bound<'py, PyDict>)> {
    let globals = bindings.copy()?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let mock = py.import("unittest.mock")?;
    let blocked = PyDict::new(py);
    blocked.set_item(
        "side_effect",
        py.import("builtins")?
            .getattr("AssertionError")?
            .call1(("unexpected original replacement effect",))?,
    )?;
    let refusal = mock.getattr("Mock")?.call((), Some(&blocked))?;
    let clock = ns.call0()?;
    clock.setattr("sleep", &refusal)?;
    clock.setattr("monotonic", py.import("time")?.getattr("monotonic")?)?;
    let bp = ns.call0()?;
    bp.setattr("time", &clock)?;
    globals.set_item("time", &clock)?;
    let os = ns.call0()?;
    os.setattr("environ", required(bindings, "command_environment")?)?;
    os.setattr("path", py.import("os.path")?)?;
    os.setattr("replace", py.import("os")?.getattr("replace")?)?;
    globals.set_item("os", &os)?;
    let log = ns.call0()?;
    for name in ["info", "warning", "error"] {
        log.setattr(name, mock.getattr("Mock")?.call0()?)?;
    }
    globals.set_item("log", log)?;
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[15],
        &["FloodWaitTooLong", "MediaError"],
        &["__build_class__", "Exception", "RuntimeError", "super"],
        &globals,
    )?;
    globals.set_item("FloodWaitTooLong", required(&errors, "FloodWaitTooLong")?)?;
    globals.set_item("Enum", py.import("enum")?.getattr("Enum")?)?;
    let protocol = crate::source_oracle_runtime::namespace(
        py,
        sources[24],
        &[
            "SetState",
            "LiveStateUnknown",
            "BotApiError",
            "AmbiguousUploadError",
        ],
        &["__build_class__", "RuntimeError"],
        &globals,
    )?;
    for name in [
        "SetState",
        "LiveStateUnknown",
        "BotApiError",
        "AmbiguousUploadError",
    ] {
        globals.set_item(name, required(&protocol, name)?)?;
    }
    let client = crate::source_telegram_namespace::namespace(py, sources[24], &globals)?;
    for name in ["_mime_for_path", "_input_sticker"] {
        globals.set_item(name, required(&client, name)?)?;
    }
    let api = ns.call0()?;
    api.setattr("Telegram", required(&client, "Telegram")?)?;
    let imports = Py::new(
        py,
        Imports {
            telegram: api.unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    let stage = Py::new(
        py,
        StageDirectories {
            ledger: ledger.clone_ref(py),
            calls: AtomicUsize::new(0),
        },
    )?;
    let temporary = ns.call0()?;
    temporary.setattr(
        "TemporaryDirectory",
        required(bindings, "tempfile")?.getattr("TemporaryDirectory")?,
    )?;
    temporary.setattr("mkdtemp", stage.bind(py))?;
    globals.set_item("tempfile", temporary)?;
    globals.set_item("shutil", py.import("shutil")?)?;
    globals.set_item("ImageOps", py.import("PIL.ImageOps")?)?;
    globals.set_item("SET_BASE", "cryptoemoji")?;
    globals.set_item(
        "PACK_LOCK",
        required(bindings, "ps")?.call_method1("pack_family_lock_path", ("cryptoemoji",))?,
    )?;
    globals.set_item(
        "SAME_IMAGE_MAX",
        crate::source_logging_oracle::literal(py, sources[21], "SAME_IMAGE_MAX", false)?,
    )?;
    for name in ["fetch_markets", "fetch_image"] {
        globals.set_item(name, &refusal)?;
    }
    let media = crate::source_static_oracle::media_namespace(py, sources[8])?;
    media.set_item("MediaError", required(&errors, "MediaError")?)?;
    let media_api = ns.call0()?;
    media_api.setattr("to_static_png", required(&media, "to_static_png")?)?;
    globals.set_item("media", media_api)?;
    let module = crate::source_oracle_runtime::module(
        py,
        sources[21],
        &[
            "dh",
            "_intent_path",
            "_staging",
            "_drop_stage",
            "_promote",
            "_run_targets",
            "logo_distance",
            "_cids",
            "cid_location",
            "_replaced_check",
            "verified_new_cid",
            "is_our_image",
            "_repoint_locked",
            "repoint",
            "reconcile_intent",
            "fix_one",
        ],
        &[
            "str",
            "int",
            "dict",
            "list",
            "len",
            "next",
            "min",
            "any",
            "enumerate",
            "zip",
            "Exception",
            "RuntimeError",
            "OSError",
            "ValueError",
            "AttributeError",
            "KeyError",
            "TypeError",
        ],
        &globals,
        "_original_coin_replacement",
    )?;
    let options = PyDict::new(py);
    options.set_item("wraps", module.getattr("is_our_image")?)?;
    let proof = mock.getattr("Mock")?.call((), Some(&options))?;
    module.setattr("is_our_image", &proof)?;
    let tests = bindings.copy()?;
    tests.set_item("mock", &mock)?;
    tests.set_item("bp", bp)?;
    tests.set_item("requests", py.import("requests")?)?;
    tests.set_item("ImageOps", py.import("PIL.ImageOps")?)?;
    tests.set_item("OLD_CID", "cid-old")?;
    tests.set_item("NEW_CID", "cid-new")?;
    tests.set_item("random", py.import("random")?)?;
    let noise = crate::source_oracle_runtime::namespace(
        py,
        sources[23],
        &["_noise_png_bytes"],
        &["range"],
        &tests,
    )?;
    tests.set_item("_noise_png_bytes", required(&noise, "_noise_png_bytes")?)?;
    let loader = Py::new(
        py,
        Loader {
            path: workspace.join("coins/verify_logos.py"),
            module: module.clone().unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    tests.set_item("_load_standalone", loader.bind(py))?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[22],
        &[
            "FakeResponse",
            "FakeDownload",
            "ReplaceSession",
            "VerifyLogosFix",
            "LogoReviewDistance",
        ],
        &[
            "__build_class__",
            "classmethod",
            "str",
            "dict",
            "list",
            "len",
            "reversed",
            "isinstance",
            "bytes",
            "RuntimeError",
            "AssertionError",
        ],
        &tests,
    )?;
    required(&tests, "__builtins__")?.set_item("__import__", imports.bind(py))?;
    for (name, value) in [
        ("replacement_imports", imports.bind(py).clone().into_any()),
        ("replacement_loader", loader.bind(py).clone().into_any()),
        ("replacement_stage", stage.bind(py).clone().into_any()),
        ("replacement_proof", proof),
        ("replacement_refusal", refusal),
        ("replacement_clock", clock),
        ("replacement_client", client.into_any()),
    ] {
        tests.set_item(name, value)?;
    }
    Ok((module, tests))
}

pub(super) fn finish(tests: &Bound<'_, PyDict>) -> PyResult<usize> {
    for (name, expected) in [
        ("replacement_imports", 24),
        ("replacement_loader", 2),
        ("replacement_stage", 16),
    ] {
        if required(tests, name)?
            .getattr("calls")?
            .extract::<usize>()?
            != expected
        {
            return Err(PyValueError::new_err(
                "original replacement port count drift",
            ));
        }
    }
    if required(tests, "replacement_refusal")?
        .getattr("call_count")?
        .extract::<usize>()?
        != 0
    {
        return Err(PyValueError::new_err(
            "original replacement reached an unpatched effect",
        ));
    }
    let client = required(tests, "replacement_client")?.cast_into::<PyDict>()?;
    let sessions = required(&client, "requests")?.getattr("Session")?;
    if sessions.getattr("call_count")?.extract::<usize>()? != 24 {
        return Err(PyValueError::new_err(
            "original replacement session count drift",
        ));
    }
    let default = sessions.getattr("return_value")?;
    for method in ["post", "get"] {
        if default
            .getattr(method)?
            .getattr("call_count")?
            .extract::<usize>()?
            != 0
        {
            return Err(PyValueError::new_err(
                "original replacement used an unpatched session",
            ));
        }
    }
    let count = required(tests, "replacement_proof")?
        .getattr("call_count")?
        .extract::<usize>()?;
    if count == 0 {
        return Err(PyValueError::new_err(
            "original replacement image proof never ran",
        ));
    }
    Ok(count)
}
