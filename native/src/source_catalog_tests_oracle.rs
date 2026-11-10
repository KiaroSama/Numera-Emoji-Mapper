//! Replay only the admitted original catalog test classes; native ownership/install are explicit ports.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
#[path = "source_real_catalog_tests.rs"]
mod real_catalog;
#[path = "source_similarity_prerequisite.rs"]
mod source_similarity_prerequisite;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 15] = [
    (
        "tests/reference/catalog.py",
        "5dab2e475754dc219dfe3fd8e98012fffd59ab0c760f646752ff9d35f7188ee2",
    ),
    (
        "emojikit/media_paths.py",
        "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3",
    ),
    (
        "tests/test_catalog.py",
        "33391dab27bc8d005844839047b8683a14212c910cf0240f76f7c08fb26ea062",
    ),
    (
        "tests/reference/ingest.py",
        "6654f129fba59b600d8498c7d734a36973fd9761276075d1f23a28974ba69162",
    ),
    (
        "tests/_similarity_oracle.py",
        "9be48166b64ab2052798c67e2f7426c2dd0008fc27b4d668a785877fba7ac44e",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
    (
        "tests/test_catalog_order.py",
        "4cf89c5ae515a92b7bbdc397642a691ff0f6879e64169fd6b798a8ef607010b0",
    ),
    (
        "tests/test_media_paths.py",
        "dda79d4365c8864be58bf82900ad20b78e292502d71896fd44e26c0203f17bd0",
    ),
    (
        "emojikit/similarity.py",
        "7f849f7bebc5837b100de00afe7eedbe275045359377087fa0187445a4587892",
    ),
    (
        "tests/test_native_similarity.py",
        "1ab3bc95d93fa181d7214b65ff83cdf9efa89bc00b08aa15761987248cbe3b7e",
    ),
    (
        "tests/test_native_catalog.py",
        "2680e795ff159b3fc1a61f3b5c959c689902adc62311da3d51fe518939453a96",
    ),
    (
        "tests/reference/maintenance.py",
        "d145a381f6da57a03551eeec76d1812e5e521af707e6dc7e7925dc0852b86278",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
    (
        "tests/test_native_real_catalog.py",
        "62a4a58199f9204deae3455fc7e4cf31ab1c01a9f7a6b3572c711c14023aa72b",
    ),
    (
        "emojikit/sqlite_snapshot.py",
        "8e3dd41f6607f1f1e4da4b777192088b9b1b370cf429774968a0771a1f78ca4d",
    ),
];
const CLASSES: [&str; 6] = [
    "TestCatalogIntegrity",
    "TestCatalogDurabilityAndScale",
    "TestCatalog",
    "OrderTests",
    "MediaPathTests",
    "LegacyConversionTests",
];
const CASES: [&str; 35] = [
    "TestCatalogIntegrity.test_fuid_is_not_silently_reassigned",
    "TestCatalogIntegrity.test_same_fuid_same_key_is_idempotent",
    "TestCatalogIntegrity.test_merged_duplicate_leaves_no_orphan_file",
    "TestCatalogIntegrity.test_readding_the_canonical_file_does_not_delete_it",
    "TestCatalogIntegrity.test_same_catalog_publishes_to_two_bases",
    "TestCatalogIntegrity.test_publication_records_are_per_base",
    "TestCatalogIntegrity.test_forgetting_a_deleted_pack_allows_republishing",
    "TestCatalogIntegrity.test_legacy_upload_state_is_adopted_once",
    "TestCatalogIntegrity.test_excluded_items_are_not_counted_as_pending",
    "TestCatalogDurabilityAndScale.test_wal_and_relaxed_sync",
    "TestCatalogDurabilityAndScale.test_failed_init_does_not_leak_the_connection",
    "TestCatalogDurabilityAndScale.test_set_order_survives_a_key_list_past_the_variable_limit",
    "TestCatalogDurabilityAndScale.test_set_order_pushes_unlisted_items_after",
    "TestCatalogDurabilityAndScale.test_empty_order_writes_nothing",
    "TestCatalog.test_add_new_and_exact_dedup",
    "TestCatalog.test_near_duplicate_merge_by_phash",
    "TestCatalog.test_phash_beyond_threshold_is_new",
    "TestCatalog.test_seen_file_unique_id_skips",
    "TestCatalog.test_pending_and_mark_uploaded",
    "TestCatalog.test_stats",
    "TestCatalog.test_large_phash_64bit",
    "TestCatalog.test_persistence_across_reopen",
    "OrderTests.test_default_order_is_insertion",
    "OrderTests.test_set_order_reorders",
    "OrderTests.test_order_persists_across_reopen_and_new_items_go_last",
    "OrderTests.test_meta_roundtrip",
    "MediaPathTests.test_inside_the_data_folder_is_stored_relative",
    "MediaPathTests.test_outside_the_data_folder_stays_absolute",
    "MediaPathTests.test_a_moved_data_folder_still_finds_every_file",
    "MediaPathTests.test_a_raw_reader_resolves_like_the_catalog",
    "MediaPathTests.test_windows_case_does_not_make_a_path_outside",
    "LegacyConversionTests.test_opening_converts_once_and_keeps_every_file",
    "LegacyConversionTests.test_a_converted_catalog_is_not_converted_again",
    "LegacyConversionTests.test_a_catalog_marked_with_the_old_value_is_healed_on_open",
    "LegacyConversionTests.test_a_legacy_relative_row_means_the_project_even_without_meta",
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("catalog test oracle requires exactly fifteen pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        let actual = Sha256::digest(source.text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || actual != hash
        {
            return Err("catalog test oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

#[derive(Default)]
struct Calls {
    prerequisite: usize,
    writer: usize,
    enter: usize,
    exit: usize,
    temporary: usize,
}
struct State {
    workspace: PathBuf,
    calls: Mutex<Calls>,
}
impl State {
    fn calls(&self) -> PyResult<std::sync::MutexGuard<'_, Calls>> {
        self.calls.lock().map_err(|_| {
            pyo3::exceptions::PyRuntimeError::new_err("catalog test port accounting failed")
        })
    }
    fn temporary(&self, path: &Path) -> PyResult<()> {
        let physical = crate::tooling_environment::physical_root(path)
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        if physical == self.workspace || !physical.starts_with(&self.workspace) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "catalog test temporary directory escaped",
            ));
        }
        self.calls()?.temporary += 1;
        Ok(())
    }
}
#[pyclass]
struct Context(Arc<State>);
#[pymethods]
impl Context {
    fn __enter__(&self) -> PyResult<()> {
        self.0.calls()?.enter += 1;
        Ok(())
    }
    fn __exit__(
        &self,
        _kind: &Bound<'_, PyAny>,
        _error: &Bound<'_, PyAny>,
        _trace: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        self.0.calls()?.exit += 1;
        Ok(())
    }
}
#[pyclass]
struct Boundary(Arc<State>);
#[pymethods]
impl Boundary {
    fn require_native(&self) -> PyResult<()> {
        self.0.calls()?.prerequisite += 1;
        Ok(())
    }
    fn writer(&self, py: Python<'_>, path: PathBuf) -> PyResult<Py<Context>> {
        let path = crate::tooling_environment::physical_root(&path)
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        if path == self.0.workspace || !path.starts_with(&self.0.workspace) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "catalog test writer escaped its workspace",
            ));
        }
        self.0.calls()?.writer += 1;
        Py::new(py, Context(self.0.clone()))
    }
    fn mkdtemp(&self, py: Python<'_>) -> PyResult<String> {
        let kwargs = PyDict::new(py);
        kwargs.set_item("dir", &self.0.workspace)?;
        let path: String = crate::source_oracle_runtime::host_import(py, "tempfile")?
            .call_method("mkdtemp", (), Some(&kwargs))?
            .extract()?;
        self.0.temporary(Path::new(&path))?;
        Ok(path)
    }
    fn temporary_directory<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let kwargs = PyDict::new(py);
        kwargs.set_item("dir", &self.0.workspace)?;
        let temporary = crate::source_oracle_runtime::host_import(py, "tempfile")?.call_method(
            "TemporaryDirectory",
            (),
            Some(&kwargs),
        )?;
        let path: String = temporary.getattr("name")?.extract()?;
        self.0.temporary(Path::new(&path))?;
        Ok(temporary)
    }
    fn same_image(
        &self,
        _a: &Bound<'_, PyAny>,
        _b: &Bound<'_, PyAny>,
        _format: &str,
    ) -> PyResult<bool> {
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "unexpected codec fallback in original storage-only tests",
        ))
    }
    fn collision_key(&self, _path: &Bound<'_, PyAny>, _key: &str) -> PyResult<String> {
        Err(pyo3::exceptions::PyRuntimeError::new_err(
            "unexpected collision fallback in original storage-only tests",
        ))
    }
}

fn required<'py>(globals: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    globals.get_item(name)?.ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(format!(
            "missing original catalog test definition: {name}"
        ))
    })
}

fn execute(
    py: Python<'_>,
    root: &Path,
    workspace: &Path,
    sources: &[&Source],
    selected: Option<&Path>,
) -> PyResult<Value> {
    // Original legacy-path fixtures write beneath ROOT/tests; never bind ROOT to the checkout.
    let project = workspace.join("project");
    std::fs::create_dir(&project)?;
    std::fs::create_dir(project.join("tests"))?;
    let state = Arc::new(State {
        workspace: workspace.into(),
        calls: Mutex::new(Calls::default()),
    });
    let boundary = Py::new(py, Boundary(state.clone()))?;
    let boundary = boundary.bind(py);
    let near = crate::source_oracle_runtime::namespace(
        py,
        sources[4],
        &["near_indices"],
        &["enumerate"],
        &PyDict::new(py),
    )?;
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[5],
        &["MediaError"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("near_indices", required(&near, "near_indices")?)?;
    bindings.set_item("MediaError", required(&errors, "MediaError")?)?;
    bindings.set_item("identity", boundary)?;
    let identity = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["catalog_identity", "equivalent_files"],
        &["set", "enumerate", "len", "OSError"],
        &bindings,
    )?;
    // The original failed-init test patches its sqlite3.connect attribute. A private module
    // forwards real SQLite APIs without replacing the interpreter's shared stdlib attribute.
    let sqlite = pyo3::types::PyModule::new(py, "_catalog_test_sqlite")?;
    sqlite
        .dict()
        .update(py.import("sqlite3")?.dict().as_mapping())?;
    let ports = PyDict::new(py);
    ports.set_item("sqlite3", &sqlite)?;
    ports.set_item("require_native", boundary.getattr("require_native")?)?;
    ports.set_item("writer", boundary.getattr("writer")?)?;
    ports.set_item("catalog_identity", required(&identity, "catalog_identity")?)?;
    let catalog = crate::source_catalog_oracle::namespace_with_ports(
        py, sources[0], sources[1], &project, &ports,
    )?;
    let tempfile = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    tempfile.setattr("mkdtemp", boundary.getattr("mkdtemp")?)?;
    tempfile.setattr(
        "TemporaryDirectory",
        boundary.getattr("temporary_directory")?,
    )?;
    let test_bindings = PyDict::new(py);
    test_bindings.set_item("Catalog", required(&catalog, "Catalog")?)?;
    test_bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    test_bindings.set_item("shutil", py.import("shutil")?)?;
    test_bindings.set_item("sqlite3", &sqlite)?;
    test_bindings.set_item("tempfile", tempfile)?;
    let unittest = py.import("unittest")?;
    test_bindings.set_item("unittest", &unittest)?;
    test_bindings.set_item("mock", py.import("unittest.mock")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &CLASSES[..3],
        &["__build_class__", "len", "bytes", "range", "set"],
        &test_bindings,
    )?;
    let image = py.import("PIL.Image")?;
    let pillow: String = image.getattr("__version__")?.extract()?;
    if pillow != "12.3.0" {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "original catalog order tests require Pillow12.3.0",
        ));
    }
    test_bindings.set_item("Image", image)?;
    let order = crate::source_oracle_runtime::namespace(
        py,
        sources[6],
        &["_png", "OrderTests"],
        &["__build_class__", "range", "list", "reversed"],
        &test_bindings,
    )?;
    tests.set_item("OrderTests", required(&order, "OrderTests")?)?;
    let sys = py.import("sys")?;
    let platform: String = sys.getattr("platform")?.extract()?;
    test_bindings.set_item("sys", sys)?;
    test_bindings.set_item(
        "ROOT",
        py.import("pathlib")?.getattr("Path")?.call1((&project,))?,
    )?;
    test_bindings.set_item("MEDIA_PATHS_DONE", "data-relative-2")?;
    test_bindings.set_item("media_paths", required(&catalog, "media_paths")?)?;
    let media_paths = crate::source_oracle_runtime::namespace(
        py,
        sources[7],
        &CLASSES[4..],
        &["__build_class__", "dict", "str"],
        &test_bindings,
    )?;
    for class in &CLASSES[4..] {
        tests.set_item(class, required(&media_paths, class)?)?;
    }
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let loaded = PyList::empty(py);
    let mut names = Vec::new();
    for class in CLASSES {
        let suite = loader.call_method1("loadTestsFromTestCase", (required(&tests, class)?,))?;
        for test in suite.try_iter()? {
            let test = test?;
            let method: String = test.getattr("_testMethodName")?.extract()?;
            names.push(format!("{class}.{method}"));
            loaded.append(test)?;
        }
    }
    names.sort();
    let mut expected = CASES.to_vec();
    expected.sort();
    if names != expected {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "original catalog test collection differs from all35fixed cases",
        ));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    unittest
        .getattr("TestSuite")?
        .call1((&loaded,))?
        .call_method1("run", (&result,))?;
    let count: usize = result.getattr("testsRun")?.extract()?;
    for field in [
        "errors",
        "failures",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        let entries = result.getattr(field)?;
        if entries.len()? != 0 {
            let first = entries.get_item(0)?;
            let failed = if field == "unexpectedSuccesses" {
                first
            } else {
                first.get_item(0)?
            };
            let method: String = failed.getattr("_testMethodName")?.extract()?;
            return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "original catalog tests reported {} {field}; first method={method}; scratch retained",
                entries.len()?
            )));
        }
    }
    if count != CASES.len() || !result.call_method0("wasSuccessful")?.extract::<bool>()? {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "original catalog test execution incomplete",
        ));
    }
    let mut skipped = Vec::new();
    for entry in result.getattr("skipped")?.try_iter()? {
        let entry = entry?;
        let test = entry.get_item(0)?;
        let class: String = test.getattr("__class__")?.getattr("__name__")?.extract()?;
        let method: String = test.getattr("_testMethodName")?.extract()?;
        let reason: String = entry.get_item(1)?.extract()?;
        skipped.push(json!({"source_case":format!("{class}.{method}"),"reason":reason}));
    }
    let expected_skips = if platform == "win32" {
        Vec::new()
    } else {
        vec![
            json!({"source_case":"MediaPathTests.test_windows_case_does_not_make_a_path_outside",
            "reason":"case-insensitive paths are a Windows property"}),
        ]
    };
    if skipped != expected_skips {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "original catalog tests have unexpected platform skips",
        ));
    }
    let prerequisite =
        source_similarity_prerequisite::check(py, workspace, &catalog, sources[8], sources[9])?;
    let calls = state.calls()?;
    if calls.prerequisite != 47
        || calls.writer != 47
        || calls.enter != 47
        || calls.exit != 47
        || calls.temporary != 35
    {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
            "original catalog test boundary accounting differs: prerequisite={} writer={} enter={} exit={} temporary={}",
            calls.prerequisite, calls.writer, calls.enter, calls.exit, calls.temporary
        )));
    }
    drop(calls);
    std::fs::create_dir(project.join("logs"))?;
    let combined = crate::source_catalog_binding::check(py, &project, sources, &catalog)?;
    let real = real_catalog::run(py, root, &project, sources, &catalog, selected)?;
    std::fs::remove_dir(project.join("logs"))?;
    let calls = state.calls()?;
    if calls.prerequisite != 48
        || calls.writer != 47
        || calls.enter != 47
        || calls.exit != 47
        || calls.temporary != 35
    {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "combined catalog changed unrelated boundary accounting",
        ));
    }
    // Original cleanups own all content. Nonempty parents must fail, never be recursively erased.
    std::fs::remove_dir(project.join("tests"))?;
    std::fs::remove_dir(project)?;
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":pillow,
        "source_platform":platform,"source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "cases":names,"tests_run":count,"errors":0,"failures":0,"skipped":skipped.len(),"skipped_cases":skipped,
        "boundary_calls":{"prerequisite":calls.prerequisite,"writer":calls.writer,"enter":calls.enter,"exit":calls.exit,"temporary":calls.temporary},
        "missing_native":prerequisite,"combined_binding":combined,"real_catalog":real,
        "coverage":{"assertions":"original35catalog/order/path test methods and setup/teardown; exact platform skip reported separately","storage":"original Catalog and external sqlite3","identity":"original reference ingest and near_indices; no decoder fallback","ownership":"supplied no-op context, not native OS locking","native_prerequisite":"supplied no-op, not installation acceptance"}}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root || !workspace.starts_with(&root) {
        return Err("catalog test oracle requires its project-contained workspace".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect catalog test workspace")?
        .next()
        .is_some()
    {
        return Err("catalog test oracle workspace must be empty".into());
    }
    let requested = std::env::var_os("NUMERA_REAL_CATALOG_FIXTURE").map(PathBuf::from);
    let selected = real_catalog::admit(&root, requested.as_deref())?;
    Python::initialize();
    let report = Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        execute(py, &root, &workspace, sources, selected.as_deref()).map_err(|error| {
            if selected.is_some() {
                "original catalog replay with private fixture failed; local evidence retained"
                    .into()
            } else {
                format!(
                    "original catalog test replay failed: {}",
                    crate::logging::redact(&error.to_string(), &[])
                )
            }
        })
    })?;
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot verify catalog test cleanup")?
        .next()
        .is_some()
    {
        return Err("original catalog tests left scratch; retained for diagnosis".into());
    }
    Ok(report)
}
