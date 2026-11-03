//! Original StateFileContract assertions over real disposable files, without a catalog or network.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 3] = [
    (
        "tests/reference/collection_state.py",
        "b8bf2695954cbe61a3e3cb68c8da0c583aa80a1b645abba01eabe9a28776074a",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
    (
        "tests/test_build_collection_state.py",
        "479e52e0e83c9e0e4f2edb4a29198d9360e3312eb6ce279f39d31090a8288469",
    ),
];
const METHODS: [&str; 19] = [
    "test_absent_file_uses_the_default",
    "test_truncated_state_refuses_to_start_from_scratch",
    "test_truncated_plan_refuses_to_refreeze",
    "test_plan_of_the_wrong_shape_is_rejected",
    "test_state_of_another_pack_family_is_rejected",
    "test_state_schema_is_checked",
    "test_a_failed_write_leaves_the_previous_state_intact",
    "test_a_wholly_consistent_state_is_accepted",
    "test_the_same_index_in_two_formats_is_fine",
    "test_a_negative_live_count_is_rejected",
    "test_a_live_count_above_telegrams_cap_is_rejected",
    "test_more_recorded_keys_than_live_stickers_is_rejected",
    "test_the_logo_slot_counts_towards_that_bound",
    "test_one_emoji_recorded_twice_in_a_set_is_rejected",
    "test_one_emoji_recorded_in_two_sets_is_rejected",
    "test_two_sets_sharing_a_name_are_rejected",
    "test_a_repeated_or_backwards_index_is_rejected",
    "test_malformed_field_types_are_rejected",
    "test_malformed_sent_and_skipped_entries_are_rejected",
];
const EXCLUDED: [&str; 2] = [
    "test_a_broken_state_stops_the_run_before_any_mutation",
    "test_main_stops_on_an_unreadable_plan",
];

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("collection state oracle requires exactly three pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("collection state source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

#[pyclass]
struct Fault {
    os: Py<PyAny>,
    calls: AtomicUsize,
}
#[pymethods]
impl Fault {
    #[pyo3(signature = (target, *, side_effect))]
    fn patch<'py>(
        &self,
        py: Python<'py>,
        target: &str,
        side_effect: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if target != "tests.reference.build_pack.os.replace" {
            return Err(PyValueError::new_err(
                "unexpected original collection patch target",
            ));
        }
        let options = PyDict::new(py);
        options.set_item("side_effect", side_effect)?;
        self.calls.fetch_add(1, Ordering::SeqCst);
        py.import("unittest.mock")?.getattr("patch")?.call_method(
            "object",
            (self.os.bind(py), "replace"),
            Some(&options),
        )
    }
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let path = py.import("pathlib")?.getattr("Path")?;
    let json_module = py.import("json")?;
    let contextlib = py.import("contextlib")?;
    let os = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    let actual_os = py.import("os")?;
    for field in ["name", "replace", "fdopen", "fsync", "chmod", "stat"] {
        os.setattr(field, actual_os.getattr(field)?)?;
    }
    let bindings = PyDict::new(py);
    bindings.set_item("Path", &path)?;
    bindings.set_item("os", &os)?;
    bindings.set_item("json", &json_module)?;
    bindings.set_item("contextlib", &contextlib)?;
    bindings.set_item("tempfile", py.import("tempfile")?)?;
    bindings.set_item("time", py.import("time")?)?;
    bindings.set_item("stat", py.import("stat")?)?;
    bindings.set_item("_REPLACE_DEADLINE", 1.0)?;
    let writer_globals = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["_replace", "write_json_atomic"],
        &["PermissionError", "FileNotFoundError", "min"],
        &bindings,
    )?;
    let writer = required(&writer_globals, "write_json_atomic")?;
    bindings.set_item("write_json_atomic", &writer)?;
    // These tests exercise persistence, not catalog writer ownership; no real catalog is opened.
    bindings.set_item("writer", contextlib.getattr("nullcontext")?)?;
    bindings.set_item(
        "FMT_TAG",
        crate::source_tooling_oracle::python(
            py,
            &json!({"static":"s","video":"v","animated":"a"}),
        )?,
    )?;
    bindings.set_item("MIXED", "mixed")?;
    bindings.set_item("PER_SET", 200)?;
    let globals = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &[
            "StateError",
            "_state_path",
            "_plan_path",
            "load_json",
            "save_json",
            "load_plan",
            "load_state",
            "_validate_state",
        ],
        &[
            "__build_class__",
            "RuntimeError",
            "OSError",
            "ValueError",
            "isinstance",
            "dict",
            "str",
            "list",
            "all",
            "bool",
            "int",
            "set",
            "enumerate",
            "sorted",
            "len",
        ],
        &bindings,
    )?;
    let module = PyModule::new(py, "_original_collection_state")?;
    module.dict().update(globals.as_mapping())?;
    let fault = Py::new(
        py,
        Fault {
            os: os.unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    let unittest = py.import("unittest")?;
    let tests = PyDict::new(py);
    tests.set_item("_CatalogFixture", unittest.getattr("TestCase")?)?;
    tests.set_item("cs", module)?;
    tests.set_item("json", &json_module)?;
    tests.set_item("SET", "pks1_by_YourEmojiBot")?;
    tests.set_item("SET2", "pks2_by_YourEmojiBot")?;
    tests.set_item("mock", fault.bind(py))?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["StateFileContract"],
        &["__build_class__", "list", "len", "OSError"],
        &tests,
    )?;
    let class = required(&tests, "StateFileContract")?;
    let actual: Vec<String> = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    let mut expected = METHODS.into_iter().chain(EXCLUDED).collect::<Vec<_>>();
    expected.sort_unstable();
    if actual != expected {
        return Err(PyValueError::new_err(
            "original collection state method inventory drift",
        ));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut observations = Vec::new();
    for (index, method) in METHODS.into_iter().enumerate() {
        let data = workspace.join(format!("state-{index}"));
        std::fs::create_dir(&data)?;
        let test = class.call1((method,))?;
        test.setattr("data", path.call1((&data,))?)?;
        test.setattr("keys", ["s:first", "s:second"])?;
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
                    "original collection state {method} reported {field}; evidence retained"
                )));
            }
        }
        let mut files = serde_json::Map::new();
        for (member, entry) in std::fs::read_dir(&data)?.enumerate() {
            if member >= 2 {
                return Err(PyValueError::new_err(
                    "excessive collection state evidence; retained",
                ));
            }
            let file = entry?.path();
            crate::tooling_environment::reject_links(
                &file,
                "collection state evidence cannot follow links",
            )
            .map_err(PyValueError::new_err)?;
            if !std::fs::symlink_metadata(&file)?.is_file()
                || !matches!(
                    file.file_name().and_then(|name| name.to_str()),
                    Some("publish_pk.json" | "publish_plan_pk.json")
                )
                || std::fs::metadata(&file)?.len() > 16384
            {
                return Err(PyValueError::new_err(
                    "unexpected collection state output; retained",
                ));
            }
            let options = PyDict::new(py);
            options.set_item("encoding", "utf-8")?;
            let text: String = path
                .call1((&file,))?
                .call_method("read_text", (), Some(&options))?
                .extract()?;
            files.insert(
                file.file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| PyValueError::new_err("non-UTF8 state name"))?
                    .into(),
                json!(text),
            );
        }
        observations.push(json!({"case":format!("StateFileContract.{method}"),"files":files}));
    }
    if result.getattr("testsRun")?.extract::<usize>()? != METHODS.len()
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || fault.borrow(py).calls.load(Ordering::SeqCst) != 1
    {
        return Err(PyRuntimeError::new_err(
            "original collection state execution incomplete",
        ));
    }
    directories(workspace, workspace, &mut 512, 0, &mut Vec::new())?;
    let expected = (0..METHODS.len())
        .map(|index| workspace.join(format!("state-{index}")))
        .collect::<std::collections::BTreeSet<_>>();
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
    if actual != expected {
        return Err(PyValueError::new_err(
            "unexpected collection state workspace; retained",
        ));
    }
    let report = json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":METHODS.len(),"cases":METHODS.map(|method|format!("StateFileContract.{method}")),
        "excluded":EXCLUDED.map(|method|format!("StateFileContract.{method}")),
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "fault_patches":1,"observations":observations,
        "ports":"unchanged original19state assertions/functions/atomic writer; synthetic two-key fixture instead of real image catalog; nullcontext catalog writer and private exact-target os.replace patch; two full publisher methods excluded, native command assertions separate"});
    for directory in expected {
        std::fs::remove_dir_all(directory)?;
    }
    Ok(report)
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("collection state workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect collection state workspace")?
        .next()
        .is_some()
    {
        return Err("collection state workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect collection state bytecode policy")?
        {
            return Err("collection state oracle requires disabled import bytecode writes".into());
        }
        execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original collection state replay failed: {}",
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
        "collection-state-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
