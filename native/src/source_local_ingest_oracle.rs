//! Fixed original local-ingest tests with real historical static codecs and SQLite.
use crate::source_git_inventory::Source;
use pyo3::exceptions::{PyImportError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 11] = [
    (
        "tests/reference/add_media.py",
        "8d865761219c77a459e5e37c45f15aeb05f41228a847dce6d9ab0eedc214aac6",
    ),
    (
        "tests/test_resume_safety.py",
        "0af175ef5368ed7a6613ce487f65ff5b8f42e1751fcfa9e787b17faea956b3d4",
    ),
    (
        "tests/_pack_fixtures.py",
        "bbac616f05c178647206528c015f4ba85df29a36c93fb69de52f2a0d505368d8",
    ),
    (
        "emojikit/cli_env.py",
        "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
    ),
    (
        "emojikit/media.py",
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
    ),
    (
        "emojikit/identity.py",
        "8cbefd550e306e671df1e52af2dd9fe6b4e3647c3aa1b13dc98bf35080d36e89",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
    (
        "tests/reference/catalog.py",
        "5dab2e475754dc219dfe3fd8e98012fffd59ab0c760f646752ff9d35f7188ee2",
    ),
    (
        "emojikit/media_paths.py",
        "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3",
    ),
    (
        "tests/reference/ingest.py",
        "6654f129fba59b600d8498c7d734a36973fd9761276075d1f23a28974ba69162",
    ),
    (
        "tests/_similarity_oracle.py",
        "9be48166b64ab2052798c67e2f7426c2dd0008fc27b4d668a785877fba7ac44e",
    ),
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("local ingest oracle requires exactly eleven pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("local ingest oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn required<'py>(namespace: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    namespace
        .get_item(name)?
        .ok_or_else(|| PyValueError::new_err("missing original local ingest definition"))
}
fn module<'py>(
    py: Python<'py>,
    namespace: &Bound<'py, PyDict>,
    names: &[&str],
) -> PyResult<Bound<'py, PyAny>> {
    let api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    for name in names {
        api.setattr(*name, required(namespace, name)?)?;
    }
    Ok(api)
}
fn mock<'py>(py: Python<'py>, value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let options = PyDict::new(py);
    options.set_item("return_value", value)?;
    py.import("unittest.mock")?
        .getattr("Mock")?
        .call((), Some(&options))
}
fn unexpected(py: Python<'_>) -> PyResult<Bound<'_, PyAny>> {
    let options = PyDict::new(py);
    options.set_item(
        "side_effect",
        PyRuntimeError::new_err(
            "unexpected nonstatic or collision fallback in original local ingest",
        )
        .value(py),
    )?;
    py.import("unittest.mock")?
        .getattr("Mock")?
        .call((), Some(&options))
}

#[pyclass]
struct Imports {
    tests: Py<PyAny>,
    build: Py<PyAny>,
}
#[pymethods]
impl Imports {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        globals: &Bound<'_, PyAny>,
        locals: &Bound<'_, PyAny>,
        fromlist: &Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = (globals, locals);
        let names = fromlist.extract::<Vec<String>>()?;
        if level == 0 && name == "tests.reference" && names == ["add_media"] {
            Ok(self.tests.bind(py).clone())
        } else if level == 1 && name == "build_pack" && names == ["load_env"] {
            Ok(self.build.bind(py).clone())
        } else {
            Err(PyImportError::new_err(
                "unexpected original local ingest import",
            ))
        }
    }
}

#[pyclass]
struct Main {
    function: Py<PyAny>,
    catalog: Py<PyAny>,
    sys: Py<PyAny>,
    output: Py<PyAny>,
    observations: Py<PyList>,
    workspace: PathBuf,
}
#[pymethods]
impl Main {
    fn __call__(&self, py: Python<'_>) -> PyResult<u8> {
        let args = self
            .sys
            .bind(py)
            .getattr("argv")?
            .extract::<Vec<String>>()?;
        if args.len() != 5
            || args[0] != "add_media.py"
            || args[1] != "--in"
            || args[3] != "--data-dir"
        {
            return Err(PyValueError::new_err("original local ingest argv drift"));
        }
        for path in [&args[2], &args[4]] {
            let path = Path::new(path);
            if !path.is_absolute() || path == self.workspace || !path.starts_with(&self.workspace) {
                return Err(PyValueError::new_err("original local ingest path escaped"));
            }
            crate::tooling_environment::reject_links(
                path,
                "original local ingest cannot follow links",
            )
            .map_err(PyValueError::new_err)?;
        }
        self.output.bind(py).call_method1("seek", (0,))?;
        self.output.bind(py).call_method1("truncate", (0,))?;
        // The unchanged test patches a PRIVATE sys.argv. Passing its tail explicitly
        // keeps the real argparse implementation from reading process-global argv.
        let code = self
            .function
            .bind(py)
            .call1((PyList::new(py, &args[1..])?,))?
            .extract::<u8>()?;
        let stdout = self
            .output
            .bind(py)
            .call_method0("getvalue")?
            .extract::<String>()?;
        let data = Path::new(&args[4]);
        let catalog = self.catalog.bind(py).call1((data.join("catalog.db"),))?;
        let rows = (|| -> PyResult<Vec<Value>> {
            let mut rows = Vec::new();
            for row in catalog.call_method0("all_items")?.try_iter()? {
                let row = row?;
                let path = PathBuf::from(row.getattr("file_path")?.extract::<String>()?);
                crate::tooling_environment::reject_links(
                    &path,
                    "original local ingest output cannot follow links",
                )
                .map_err(PyValueError::new_err)?;
                if !path.starts_with(data) {
                    return Err(PyValueError::new_err("original stored media escaped"));
                }
                let bytes = std::fs::read(path)?;
                rows.push(json!({
                    "content_key":row.getattr("content_key")?.extract::<String>()?,
                    "fmt":row.getattr("fmt")?.extract::<String>()?,
                    "phash":row.getattr("phash")?.extract::<Option<u64>>()?,
                    "emojis":row.getattr("emojis")?.extract::<Vec<String>>()?,
                    "keywords":row.getattr("keywords")?.extract::<Vec<String>>()?,
                    "sources":row.getattr("sources")?.extract::<Vec<String>>()?,
                    "uploaded":row.getattr("uploaded")?.extract::<bool>()?,
                    "included":row.getattr("included")?.extract::<bool>()?,
                    "media_bytes":bytes,
                }));
            }
            Ok(rows)
        })();
        let closed = catalog.call_method0("close");
        let rows = rows?;
        closed?;
        if std::fs::read_dir(data.join("tmp"))?.next().is_some() {
            return Err(PyValueError::new_err(
                "original local ingest left temporary files",
            ));
        }
        self.observations
            .bind(py)
            .append(py.import("json")?.call_method1(
                "loads",
                (json!({"code":code,"stdout":stdout,"items":rows}).to_string(),),
            )?)?;
        Ok(code)
    }
}

fn execute(
    py: Python<'_>,
    workspace: &Path,
    sources: &[&Source],
    artwork: &Path,
) -> PyResult<Value> {
    let image = py.import("PIL.Image")?;
    if image.getattr("__version__")?.extract::<String>()? != "12.3.0" {
        return Err(PyRuntimeError::new_err(
            "original local ingest requires Pillow12.3.0",
        ));
    }
    let path_type = py.import("pathlib")?.getattr("Path")?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let log = py
        .import("logging")?
        .getattr("Logger")?
        .call1(("original_local_ingest",))?;
    log.setattr("disabled", true)?;
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[6],
        &["MediaError"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("Image", &image)?;
    bindings.set_item("Path", &path_type)?;
    bindings.set_item("io", py.import("io")?)?;
    bindings.set_item("log", &log)?;
    for name in ["SIZE", "VISIBLE_ALPHA", "BLANK_MAX_VISIBLE"] {
        bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[4], name, false)?,
        )?;
    }
    bindings.set_item("MediaError", required(&errors, "MediaError")?)?;
    let media = crate::source_oracle_runtime::namespace(
        py,
        sources[4],
        &[
            "_trim",
            "fit_100",
            "render_svg",
            "_load_image",
            "is_blank_image",
            "to_static_png",
            "ext_for_format",
        ],
        &["min", "max", "round", "sum", "bytes", "str", "Exception"],
        &bindings,
    )?;
    let media_api = module(py, &media, &["to_static_png", "ext_for_format"])?;
    media_api.setattr("MediaError", required(&errors, "MediaError")?)?;
    bindings.set_item("hashlib", py.import("hashlib")?)?;
    bindings.set_item("ImageChops", py.import("PIL.ImageChops")?)?;
    bindings.set_item("media", &media_api)?;
    let identity = crate::source_oracle_runtime::namespace(
        py,
        sources[5],
        &[
            "_norm_pixels",
            "_dhash",
            "fingerprint",
            "content_key",
            "perceptual_hash",
            "collision_key",
        ],
        &["range", "Exception", "OSError", "ValueError"],
        &bindings,
    )?;
    let identity_api = module(py, &identity, &["fingerprint", "collision_key"])?;
    let fallback = unexpected(py)?;
    identity_api.setattr("same_image", &fallback)?;
    let near = crate::source_oracle_runtime::namespace(
        py,
        sources[10],
        &["near_indices"],
        &["enumerate"],
        &PyDict::new(py),
    )?;
    let context = py.import("contextlib")?.getattr("nullcontext")?.call0()?;
    let writer = mock(py, &context)?;
    let prerequisite = mock(py, &py.None().into_bound(py))?;
    let load_env = mock(py, &py.None().into_bound(py))?;
    let setup_logging = mock(py, &py.None().into_bound(py))?;
    let temporary = py.import("functools")?.getattr("partial")?;
    let options = PyDict::new(py);
    options.set_item("dir", workspace)?;
    let temporary = temporary.call(
        (py.import("tempfile")?.getattr("TemporaryDirectory")?,),
        Some(&options),
    )?;
    let temporary = mock_wrapped(py, &temporary)?;
    let tempfile = ns.call0()?;
    tempfile.setattr("TemporaryDirectory", &temporary)?;
    let ingest_bindings = PyDict::new(py);
    for (key, value) in [
        ("Path", path_type.clone()),
        ("identity", identity_api.clone()),
        ("MediaError", required(&errors, "MediaError")?),
        ("near_indices", required(&near, "near_indices")?),
        ("os", py.import("os")?.into_any()),
        ("hashlib", py.import("hashlib")?.into_any()),
        ("shutil", py.import("shutil")?.into_any()),
        ("tempfile", tempfile.clone()),
        ("write_json_atomic", fallback.clone()),
    ] {
        ingest_bindings.set_item(key, value)?;
    }
    let ingest = crate::source_oracle_runtime::namespace(
        py,
        sources[9],
        &[
            "equivalent_files",
            "catalog_identity",
            "_retain_refused",
            "store_media",
        ],
        &[
            "set",
            "enumerate",
            "len",
            "OSError",
            "FileExistsError",
            "BaseException",
        ],
        &ingest_bindings,
    )?;
    let ports = PyDict::new(py);
    ports.set_item("require_native", &prerequisite)?;
    ports.set_item("writer", &writer)?;
    ports.set_item("catalog_identity", required(&ingest, "catalog_identity")?)?;
    ports.set_item("log", &log)?;
    let catalog = crate::source_catalog_oracle::namespace_with_ports(
        py, sources[7], sources[8], workspace, &ports,
    )?;
    let catalog_type = required(&catalog, "Catalog")?;
    let exit_bindings = PyDict::new(py);
    let bp = ns.call0()?;
    for name in ["EXIT_OK", "EXIT_USAGE", "EXIT_PARTIAL", "EXIT_FAILED"] {
        let value = crate::source_logging_oracle::literal(py, sources[3], name, false)?;
        exit_bindings.set_item(name, &value)?;
        bp.setattr(name, value)?;
    }
    let exits = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["ingest_exit_code"],
        &[],
        &exit_bindings,
    )?;
    let output = py.import("io")?.getattr("StringIO")?.call0()?;
    let print_options = PyDict::new(py);
    print_options.set_item("file", &output)?;
    let print = py.import("functools")?.getattr("partial")?.call(
        (py.import("builtins")?.getattr("print")?,),
        Some(&print_options),
    )?;
    let bindings = PyDict::new(py);
    for (key, value) in [
        ("Path", path_type.clone()),
        ("Image", image.into_any()),
        ("media", media_api),
        ("identity", identity_api),
        ("Catalog", catalog_type.clone()),
        ("ingest_exit_code", required(&exits, "ingest_exit_code")?),
        ("store_media", required(&ingest, "store_media")?),
        ("log", log),
        ("tempfile", tempfile.clone()),
        ("os", py.import("os")?.into_any()),
        ("argparse", py.import("argparse")?.into_any()),
        ("setup_logging", setup_logging.clone()),
        ("print", print),
        ("ROOT", path_type.call1((workspace,))?),
        (
            "phash_threshold_arg",
            required(&catalog, "check_phash_threshold")?,
        ),
    ] {
        bindings.set_item(key, value)?;
    }
    bindings.set_item("DEFAULT_PHASH_THRESHOLD", -1)?;
    for name in ["_STILL_EXTS", "_LOTTIE_EXTS", "_VIDEO_EXTS"] {
        bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[0], name, false)?,
        )?;
    }
    let main = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &[
            "_is_animated_gif",
            "choose_format",
            "convert",
            "_media_path",
            "iter_sources",
            "main",
        ],
        &["getattr", "Exception", "sorted", "str", "list"],
        &bindings,
    )?;
    let sys = ns.call0()?;
    let sentinel = PyList::empty(py);
    sys.setattr("argv", &sentinel)?;
    let observations = PyList::empty(py);
    let api = ns.call0()?;
    api.setattr(
        "main",
        Py::new(
            py,
            Main {
                function: required(&main, "main")?.unbind(),
                catalog: catalog_type.unbind(),
                sys: sys.clone().unbind(),
                output: output.clone().unbind(),
                observations: observations.clone().unbind(),
                workspace: workspace.into(),
            },
        )?,
    )?;
    let tests_package = ns.call0()?;
    tests_package.setattr("add_media", api)?;
    let build = ns.call0()?;
    build.setattr("load_env", &load_env)?;
    let imports = Py::new(
        py,
        Imports {
            tests: tests_package.unbind(),
            build: build.unbind(),
        },
    )?;
    required(&main, "__builtins__")?
        .cast_into::<PyDict>()?
        .set_item("__import__", &imports)?;
    let fixture_bindings = PyDict::new(py);
    fixture_bindings.set_item("Image", py.import("PIL.Image")?)?;
    let fixture =
        crate::source_oracle_runtime::namespace(py, sources[2], &["_png"], &[], &fixture_bindings)?;
    let bindings = PyDict::new(py);
    let unittest = py.import("unittest")?;
    for (key, value) in [
        ("unittest", unittest.clone().into_any()),
        ("mock", py.import("unittest.mock")?.into_any()),
        ("tempfile", tempfile),
        ("Path", path_type),
        ("sys", sys.clone()),
        ("bp", bp),
        ("_png", required(&fixture, "_png")?),
    ] {
        bindings.set_item(key, value)?;
    }
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["AddMediaExitCode"],
        &["__build_class__", "str"],
        &bindings,
    )?;
    required(&tests, "__builtins__")?
        .cast_into::<PyDict>()?
        .set_item("__import__", &imports)?;
    let class = required(&tests, "AddMediaExitCode")?;
    let methods = [
        "test_all_inputs_unusable_exits_failed",
        "test_some_inputs_usable_exits_partial",
        "test_all_inputs_usable_exits_ok",
    ];
    let mut expected = methods.to_vec();
    expected.sort_unstable();
    let collected = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract::<Vec<String>>()?;
    if collected != expected {
        return Err(PyValueError::new_err(
            "original local ingest collection drift",
        ));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    for method in methods {
        let before = observations.len();
        class.call1((method,))?.call_method1("run", (&result,))?;
        if observations.len() != before + 1 {
            return Err(PyRuntimeError::new_err(
                "original local ingest did not yield one observation",
            ));
        }
        observations
            .get_item(before)?
            .set_item("source_case", format!("AddMediaExitCode.{method}"))?;
    }
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyRuntimeError::new_err(format!(
                "original local ingest reported {field}"
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 3
        || observations.len() != 3
        || !sys.getattr("argv")?.is(&sentinel)
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyRuntimeError::new_err(
            "original local ingest execution/restoration drift",
        ));
    }
    for (port, count) in [
        (&prerequisite, 6),
        (&writer, 6),
        (&load_env, 3),
        (&setup_logging, 3),
        (&temporary, 6),
    ] {
        if port.getattr("call_count")?.extract::<usize>()? != count {
            return Err(PyRuntimeError::new_err(
                "original local ingest boundary accounting drift",
            ));
        }
    }
    fallback.call_method0("assert_not_called")?;
    let encoded = py
        .import("json")?
        .call_method1("dumps", (&observations,))?
        .extract::<String>()?;
    let observations: Value = serde_json::from_str(&encoded)
        .map_err(|_| PyValueError::new_err("invalid original local ingest observations"))?;
    output.call_method0("close")?;
    let artwork_output = workspace.join("original-artwork.png");
    required(&media, "to_static_png")?.call1((artwork, &artwork_output))?;
    let (key, phash) = required(&identity, "fingerprint")?
        .call1((&artwork_output, "static"))?
        .extract::<(String, Option<u64>)>()?;
    if !key.starts_with("s:") || phash.is_none() {
        return Err(PyRuntimeError::new_err(
            "original artwork fingerprint incomplete",
        ));
    }
    let image = crate::codec_raster::OpenImage(
        py.import("PIL.Image")?
            .call_method1("open", (&artwork_output,))?,
    );
    let size = image.0.getattr("size")?.extract::<(u32, u32)>()?;
    let mode = image.0.getattr("mode")?.extract::<String>()?;
    let pixels = image.0.call_method0("tobytes")?.cast_into::<PyBytes>()?;
    let rgba_sha256 = crate::codec_raster::digest_hex(pixels.as_bytes());
    image.close()?;
    let bytes = std::fs::read(&artwork_output)?;
    let artwork = json!({"input_sha256":ARTWORK_HASH,"content_key":key,"phash":phash,
        "size":[size.0,size.1],"mode":mode,"rgba_sha256":rgba_sha256,"media_bytes":bytes});
    std::fs::remove_file(artwork_output)?;
    Ok(
        json!({"baseline_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":"12.3.0",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":3,"original_methods":methods.map(|name|format!("AddMediaExitCode.{name}")),
        "observations":observations,"artwork":artwork,
        "ports":"private argv/imports and explicit real argparse args; logging/environment/native prerequisite/ownership are no-op ports; original static conversion/fingerprint/SQLite/store/test bodies retained; nonstatic and ambiguous collision fallback excluded"}),
    )
}
fn mock_wrapped<'py>(py: Python<'py>, function: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let options = PyDict::new(py);
    options.set_item("wraps", function)?;
    py.import("unittest.mock")?
        .getattr("Mock")?
        .call((), Some(&options))
}

const ARTWORK_HASH: &str = "6f85376de3893dfa1e78045ce264103b0055acec400dd8a35222405d5c4f6951";
fn artwork_path(root: &Path) -> Result<PathBuf, String> {
    use std::io::Read;
    let path = root.join("assets/numera-emoji-mapper-logo.png");
    crate::tooling_environment::reject_links(&path, "original artwork cannot follow links")?;
    let file = std::fs::File::open(&path).map_err(|_| "cannot open original artwork")?;
    let metadata = file
        .metadata()
        .map_err(|_| "cannot inspect original artwork")?;
    if !metadata.is_file() || metadata.len() != 277314 {
        return Err("original artwork input identity drift".into());
    }
    let mut bytes = Vec::new();
    file.take(277315)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read original artwork")?;
    if crate::codec_raster::digest_hex(&bytes) != ARTWORK_HASH {
        return Err("original artwork input identity drift".into());
    }
    Ok(path)
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root || !workspace.starts_with(&root) {
        return Err("local ingest oracle requires its project-contained workspace".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect local ingest workspace")?
        .next()
        .is_some()
    {
        return Err("local ingest workspace must be empty".into());
    }
    let artwork = artwork_path(&root)?;
    Python::initialize();
    let report = Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        execute(py, &workspace, sources, &artwork)
            .map_err(|_| "original local ingest replay failed; workspace retained".to_owned())
    })?;
    artwork_path(&root)?;
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot verify local ingest cleanup")?
        .next()
        .is_some()
    {
        return Err("original local ingest left scratch; retained".into());
    }
    Ok(report)
}
