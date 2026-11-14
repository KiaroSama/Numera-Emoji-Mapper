//! Original cache assertions retain real image/filesystem operations and delayed checked teardown.
use crate::source_git_inventory::Source;
use crate::source_temporary::Directories;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 3] = [
    (
        "coins/_keywords.py",
        "7c157ebff8fd5dbb8ec28c87ce8e85b70559601a8671b01c2510fe4e9f70f75f",
    ),
    (
        "coins/fetch_logos.py",
        "a42706ef2afb8efc880e7d695199a8f3b1a8d3acc9f17e625f60b49e40a36181",
    ),
    (
        "tests/test_coin_logo_cache.py",
        "fbe79fb393c4895294c288e7500b121e9dbe2395d6769bf95630eea569878be5",
    ),
];
const METHODS: [&str; 5] = [
    "test_bad_download_never_reaches_the_destination",
    "test_corrupt_cached_png_is_redownloaded",
    "test_corrupt_leftover_png_is_not_advertised",
    "test_good_cached_png_is_not_redownloaded",
    "test_valid_image_rejects_error_bodies_and_truncation",
];

pub fn source_paths() -> [&'static str; 3] {
    IDENTITIES.map(|(path, _)| path)
}
pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("logo cache replay requires exactly three pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("logo cache source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (_url, *, binary=false, retries=6))]
fn reject_get(_url: &str, binary: bool, retries: u32) -> PyResult<()> {
    let _ = (binary, retries);
    Err(PyValueError::new_err(
        "unpatched original cache GET refused",
    ))
}
#[pyfunction]
fn reject_sleep(duration: f64) -> PyResult<()> {
    if duration == 0.0 {
        Ok(())
    } else {
        Err(PyValueError::new_err(
            "unpatched original cache sleep refused",
        ))
    }
}
#[pyfunction]
fn reject_logging(_name: &str) -> PyResult<()> {
    Err(PyValueError::new_err(
        "unpatched original cache logging refused",
    ))
}
#[pyclass]
struct CacheLoader {
    path: PathBuf,
    module: Py<PyModule>,
}
#[pymethods]
impl CacheLoader {
    fn __call__(&self, py: Python<'_>, path: PathBuf, name: &str) -> PyResult<Py<PyModule>> {
        if path != self.path || name != "coins_fetch_logos" {
            return Err(PyValueError::new_err("unexpected original cache import"));
        }
        Ok(self.module.clone_ref(py))
    }
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let image = py.import("PIL.Image")?;
    if image.getattr("__version__")?.extract::<String>()? != "12.3.0" {
        return Err(PyValueError::new_err(
            "logo cache replay requires Pillow12.3.0",
        ));
    }
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let path = py.import("pathlib")?.getattr("Path")?;
    let bindings = PyDict::new(py);
    let os = ns.call0()?;
    os.setattr("replace", py.import("os")?.getattr("replace")?)?;
    for (name, value) in [
        ("Path", path.clone()),
        ("os", os),
        ("Image", image.into_any()),
        ("csv", py.import("csv")?.into_any()),
        ("io", py.import("io")?.into_any()),
    ] {
        bindings.set_item(name, value)?;
    }
    bindings.set_item(
        "HEADER",
        crate::source_logging_oracle::literal(py, sources[0], "HEADER", false)?,
    )?;
    let keywords = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &["_valid_image", "_old_names", "write_keywords_csv"],
        &["open", "sorted", "print", "Exception"],
        &bindings,
    )?;
    for name in ["_valid_image", "write_keywords_csv"] {
        bindings.set_item(name, required(&keywords, name)?)?;
    }
    let get = wrap_pyfunction!(reject_get, py)?.into_any();
    let sleep = wrap_pyfunction!(reject_sleep, py)?.into_any();
    let logging = wrap_pyfunction!(reject_logging, py)?.into_any();
    let time = ns.call0()?;
    time.setattr("sleep", &sleep)?;
    bindings.set_item("time", &time)?;
    bindings.set_item("_get", &get)?;
    bindings.set_item("setup_logging", &logging)?;
    bindings.set_item("argparse", py.import("argparse")?)?;
    bindings.set_item("EXIT_PARTIAL", 3)?;
    for name in ["API", "PER_PAGE", "MAX_PAGES", "IMG_DELAY"] {
        bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[1], name, false)?,
        )?;
    }
    bindings.set_item("PAGE_DELAY", 2.0)?;
    bindings.set_item(
        "_SAFE_RE",
        crate::source_logging_oracle::literal(py, sources[1], "_SAFE_RE", true)?,
    )?;
    // literal() returns the admitted pattern, not a compiled regular expression.
    bindings.set_item(
        "_SAFE_RE",
        py.import("re")?
            .call_method1("compile", (required(&bindings, "_SAFE_RE")?,))?,
    )?;
    for (name, relative) in [
        ("PNG_DIR", "png"),
        ("SVG_DIR", "svg"),
        ("KEYWORDS_CSV", "keywords.csv"),
    ] {
        bindings.set_item(name, path.call1((workspace.join(relative),))?)?;
    }
    let globals = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["safe_ticker", "fetch_logo", "existing_svg_tickers", "main"],
        &[
            "str",
            "int",
            "len",
            "list",
            "range",
            "sorted",
            "set",
            "isinstance",
            "print",
            "RuntimeError",
            "Exception",
        ],
        &bindings,
    )?;
    let module = crate::source_roster_modules::rebind(py, &globals, "_original_cache_fetcher")?;
    let ledger = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let tempfile = ns.call0()?;
    tempfile.setattr("mkdtemp", ledger.bind(py).getattr("mkdtemp")?)?;
    let shutil = ns.call0()?;
    shutil.setattr("rmtree", ledger.bind(py).getattr("rmtree")?)?;
    bindings.set_item("ROOT", path.call1((workspace,))?)?;
    bindings.set_item(
        "_load_standalone",
        Py::new(
            py,
            CacheLoader {
                path: workspace.join("coins/fetch_logos.py"),
                module: module.clone().unbind(),
            },
        )?,
    )?;
    bindings.set_item(
        "HTML_ERROR",
        crate::source_logging_oracle::literal(py, sources[2], "HTML_ERROR", false)?,
    )?;
    for (name, value) in [
        ("tempfile", tempfile),
        ("shutil", shutil),
        ("unittest", py.import("unittest")?.into_any()),
        ("mock", py.import("unittest.mock")?.into_any()),
        ("contextlib", py.import("contextlib")?.into_any()),
    ] {
        bindings.set_item(name, value)?;
    }
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["_png_bytes", "FetchLogosCacheValidation"],
        &["__build_class__", "classmethod", "open", "list", "dict"],
        &bindings,
    )?;
    let class = required(&tests, "FetchLogosCacheValidation")?;
    let unittest = py.import("unittest")?;
    let methods: Vec<String> = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods != METHODS {
        return Err(PyValueError::new_err(
            "original logo cache collection drift",
        ));
    }
    class.call_method0("setUpClass")?;
    let result = unittest.getattr("TestResult")?.call0()?;
    for method in METHODS {
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
                    "original logo cache {method} reported {field}: {}; retained",
                    crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                )));
            }
        }
        for (name, value) in [("_get", &get), ("setup_logging", &logging)] {
            if !module.getattr(name)?.is(value) {
                return Err(PyValueError::new_err(
                    "original logo cache patch not restored",
                ));
            }
        }
        if !time.getattr("sleep")?.is(&sleep) {
            return Err(PyValueError::new_err("original logo cache clock changed"));
        }
    }
    class.call_method0("tearDownClass")?;
    if result.getattr("testsRun")?.extract::<usize>()? != 5
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original logo cache execution incomplete",
        ));
    }
    let ledger = ledger.borrow(py);
    let paths = ledger
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("cache temporary accounting poisoned"))?;
    if paths.len() != 5
        || paths
            .iter()
            .any(|(_, cleaned)| !cleaned.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original logo cache teardown incomplete; retained",
        ));
    }
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    if actual
        != paths
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<BTreeSet<_>>()
        || crate::process_owner::cleanup_pending(workspace)
    {
        return Err(PyValueError::new_err(
            "unexpected logo cache scratch; retained",
        ));
    }
    for (index, (path, _)) in paths.iter().enumerate() {
        directories(workspace, path, &mut 32, 0, &mut Vec::new())?;
        let members = std::fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        let expected: &[&str] = match index {
            0 => &["png"],
            1..=3 => &["png", "keywords.csv"],
            4 => &["png", "good.png", "html.png", "cut.png", "blank.png"],
            _ => return Err(PyValueError::new_err("unexpected logo cache context")),
        };
        if members
            != expected
                .iter()
                .map(|name| std::ffi::OsString::from(*name))
                .collect::<BTreeSet<_>>()
        {
            return Err(PyValueError::new_err(
                "unexpected original logo cache files; retained",
            ));
        }
        let members = std::fs::read_dir(path.join("png"))?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        let expected: &[&str] = match index {
            1 | 3 => &["btc.png"],
            2 => &["junk.png", "cut.png", "ok.png"],
            _ => &[],
        };
        if members
            != expected
                .iter()
                .map(|name| std::ffi::OsString::from(*name))
                .collect::<BTreeSet<_>>()
        {
            return Err(PyValueError::new_err(
                "unexpected original logo cache image/staging; retained",
            ));
        }
    }
    for (path, _) in paths.iter() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":"12.3.0",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":5,"temporary_calls":5,"cases":METHODS.map(|method|format!("FetchLogosCacheValidation.{method}")),
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"unchanged original cache/image/CSV/functions/classes/args/assertions; real Pillow/pathlib/csv/argparse/os.replace; private module loader/clock/HTTP/logging and delayed owned directory cleanup; native path/temp/error differences remain"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("logo cache workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect logo cache workspace")?
        .next()
        .is_some()
    {
        return Err("logo cache workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect cache bytecode policy")?
        {
            return Err("logo cache replay requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original logo cache replay failed: {}",
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
        "logo-cache-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
