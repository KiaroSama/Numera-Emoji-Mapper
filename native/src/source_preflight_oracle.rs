//! Pinned preflight reports and unchanged outcome assertions; source admission precedes execution.
use crate::source_git_inventory::Source;
use crate::source_temporary::Directories;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const SOURCE: &str = "tests/oracles/collection_preflight.py";
const HASH: &str = "2821e30d219dbf4cb4df8ff08f97f697e6f89bf79cca1d5a2ae572000dc5d965";
const TEST: &str = "tests/test_native_publish_cli.py";
const TEST_HASH: &str = "95b8036314ecd15ed82734e8ec7845f6aa7c8227cd8db781d8a2cdfed90c2bc5";

pub fn source_identity(source: &Source, test: &Source) -> Result<(), String> {
    for (source, path, hash) in [(source, SOURCE, HASH), (test, TEST, TEST_HASH)] {
        let actual = Sha256::digest(source.text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || actual != hash
        {
            return Err("preflight oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub fn source_paths() -> [&'static str; 11] {
    [
        SOURCE,
        TEST,
        "tests/reference/collection_preflight.py",
        "tests/test_preflight_outcomes.py",
        "tests/reference/catalog.py",
        "emojikit/media_paths.py",
        "tests/_panel_fixtures.py",
        "tests/reference/build_collection.py",
        "tests/reference/ingest.py",
        "emojikit/errors.py",
        "emojikit/telegram_api.py",
    ]
}

pub fn outcome_identity(sources: &[&Source]) -> Result<(), String> {
    let hashes = [
        HASH,
        TEST_HASH,
        "082c0c8aacee2b1b702bc51e69d8916de3efe98d4983d7b9cb904be2fff16daa",
        "09a8c2929a6ef1c03091a196a4eda7d0a91c096570506ebd1d9475f61c707d4c",
        "5dab2e475754dc219dfe3fd8e98012fffd59ab0c760f646752ff9d35f7188ee2",
        "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3",
        "15f432b0d9063e0d83b7ea3bbd90f913e14c2c74818b7ba8ef92af50398d4039",
        "e651186017023c8adf7a4a5525717f29e0ca78842a91c08ec25d0230b2c3a62e",
        "6654f129fba59b600d8498c7d734a36973fd9761276075d1f23a28974ba69162",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
        "712554abbfeb63f72f38c4a27261b973bc81a7497991615223df44bec29265e8",
    ];
    if sources.len() != hashes.len() {
        return Err("preflight outcomes require eleven pinned sources".into());
    }
    for ((source, path), hash) in sources.iter().zip(source_paths()).zip(hashes) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("preflight outcome source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub async fn observe_owned(
    root: &std::path::Path,
    executable: &std::path::Path,
    workspace: &std::path::Path,
    sources: &[&Source],
) -> Result<Value, String> {
    outcome_identity(sources)?;
    crate::source_oracle_process::check(
        root,
        executable,
        "preflight-outcomes",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}

const OUTCOMES: [&str; 8] = [
    "test_a_missing_file_fails_and_is_never_sent",
    "test_a_partial_transport_failure_is_not_success_either",
    "test_a_refusal_outranks_an_unknown",
    "test_a_telegram_refusal_fails",
    "test_a_total_transport_failure_is_never_success",
    "test_an_empty_queue_is_not_a_validation",
    "test_every_file_accepted_is_the_only_success",
    "test_it_publishes_nothing_whatever_the_outcome",
];
const SUMMARIES: [&str; 3] = [
    "test_an_unknown_never_prints_an_acceptance_claim",
    "test_each_category_is_reported_separately",
    "test_success_names_the_number_actually_accepted",
];

#[pyclass]
struct Clock(AtomicUsize);
#[pymethods]
impl Clock {
    fn sleep(&self, duration: f64) -> PyResult<()> {
        if duration != 0.0 || self.0.fetch_add(1, Ordering::SeqCst) >= 26 {
            return Err(PyValueError::new_err(
                "unexpected original preflight pacing",
            ));
        }
        Ok(())
    }
}

fn execute(py: Python<'_>, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let image = py.import("PIL.Image")?;
    if image.getattr("__version__")?.extract::<String>()? != "12.3.0" {
        return Err(PyValueError::new_err(
            "preflight assertions require Pillow12.3.0",
        ));
    }
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let path = py.import("pathlib")?.getattr("Path")?;
    let builtins = py.import("builtins")?;
    let mock = py.import("unittest.mock")?;
    let rejected = PyDict::new(py);
    rejected.set_item(
        "side_effect",
        builtins
            .getattr("RuntimeError")?
            .call1(("unexpected preflight identity comparison",))?,
    )?;
    let reject = mock.getattr("Mock")?.call((), Some(&rejected))?;
    let ingest_bindings = PyDict::new(py);
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[9],
        &["MediaError"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let identity = ns.call0()?;
    identity.setattr("collision_key", &reject)?;
    for (name, value) in [
        ("Path", path.clone()),
        ("identity", identity),
        ("near_indices", reject.clone()),
        ("equivalent_files", reject.clone()),
        ("MediaError", required(&errors, "MediaError")?),
    ] {
        ingest_bindings.set_item(name, value)?;
    }
    let ingest = crate::source_oracle_runtime::namespace(
        py,
        sources[8],
        &["catalog_identity"],
        &["set", "enumerate", "len"],
        &ingest_bindings,
    )?;
    let options = PyDict::new(py);
    options.set_item("return_value", py.None())?;
    let prerequisite = mock.getattr("Mock")?.call((), Some(&options))?;
    options.set_item(
        "return_value",
        py.import("contextlib")?.getattr("nullcontext")?.call0()?,
    )?;
    let writer = mock.getattr("Mock")?.call((), Some(&options))?;
    let ports = PyDict::new(py);
    ports.set_item("require_native", &prerequisite)?;
    ports.set_item("writer", &writer)?;
    ports.set_item("catalog_identity", required(&ingest, "catalog_identity")?)?;
    let logger = py
        .import("logging")?
        .getattr("Logger")?
        .call1(("original_preflight", 100))?;
    ports.set_item("log", &logger)?;
    let catalog = crate::source_catalog_oracle::namespace_with_ports(
        py, sources[4], sources[5], workspace, &ports,
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("Catalog", required(&catalog, "Catalog")?)?;
    let queue = crate::source_oracle_runtime::namespace(
        py,
        sources[7],
        &["_by_position", "pending_keys"],
        &["enumerate", "sorted", "len"],
        &bindings,
    )?;
    let protocol = crate::source_oracle_runtime::namespace(
        py,
        sources[10],
        &["BotApiError"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let clock = Py::new(py, Clock(AtomicUsize::new(0)))?;
    let time = ns.call0()?;
    let sleep = clock.bind(py).getattr("sleep")?;
    time.setattr("sleep", &sleep)?;
    bindings.set_item("time", &time)?;
    bindings.set_item("Path", &path)?;
    bindings.set_item("BotApiError", required(&protocol, "BotApiError")?)?;
    bindings.set_item("log", &logger)?;
    bindings.set_item("redact", builtins.getattr("str")?)?;
    for (name, value) in [("EXIT_OK", 0), ("EXIT_PARTIAL", 3), ("EXIT_FAILED", 4)] {
        bindings.set_item(name, value)?;
    }
    let delay = crate::source_logging_oracle::literal(py, sources[2], "PREFLIGHT_DELAY", false)?;
    bindings.set_item("PREFLIGHT_DELAY", &delay)?;
    let preflight = crate::source_oracle_runtime::module(
        py,
        sources[2],
        &["preflight", "_report"],
        &["print", "len", "str", "RuntimeError"],
        &bindings,
        "_original_preflight",
    )?;
    let png_bindings = PyDict::new(py);
    png_bindings.set_item("Image", image)?;
    let png = crate::source_oracle_runtime::namespace(
        py,
        sources[6],
        &["_make_png"],
        &[],
        &png_bindings,
    )?;
    let ledger = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let tempfile = ns.call0()?;
    tempfile.setattr("TemporaryDirectory", ledger.bind(py).getattr("temporary")?)?;
    bindings.set_item("tempfile", tempfile)?;
    bindings.set_item("unittest", py.import("unittest")?)?;
    bindings.set_item("pf", &preflight)?;
    bindings.set_item("pending_keys", required(&queue, "pending_keys")?)?;
    bindings.set_item("_make_png", required(&png, "_make_png")?)?;
    bindings.set_item(
        "BASE",
        crate::source_logging_oracle::literal(py, sources[3], "BASE", false)?,
    )?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &[
            "_StubTelegram",
            "PreflightOutcomes",
            "TheSummaryMatchesTheOutcome",
        ],
        &[
            "__build_class__",
            "range",
            "len",
            "set",
            "list",
            "isinstance",
            "Exception",
            "RuntimeError",
        ],
        &bindings,
    )?;
    let unittest = py.import("unittest")?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let capture = py.import("io")?.getattr("StringIO")?.call0()?;
    let redirect = py
        .import("contextlib")?
        .getattr("redirect_stdout")?
        .call1((&capture,))?;
    redirect.call_method0("__enter__")?;
    let observed = (|| -> PyResult<Vec<String>> {
        let mut cases = Vec::new();
        for (class_name, methods) in [
            ("PreflightOutcomes", OUTCOMES.as_slice()),
            ("TheSummaryMatchesTheOutcome", SUMMARIES.as_slice()),
        ] {
            let class = required(&tests, class_name)?;
            if loader
                .call_method1("getTestCaseNames", (&class,))?
                .extract::<Vec<String>>()?
                != methods
            {
                return Err(PyValueError::new_err(
                    "original preflight assertion collection drift",
                ));
            }
            for method in methods {
                class.call1((*method,))?.call_method1("run", (&result,))?;
                for field in [
                    "errors",
                    "failures",
                    "skipped",
                    "expectedFailures",
                    "unexpectedSuccesses",
                ] {
                    if result.getattr(field)?.len()? != 0 {
                        return Err(PyValueError::new_err(format!(
                            "original preflight {class_name}.{method} reported {field}: {}; retained",
                            crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                        )));
                    }
                }
                if !preflight.getattr("PREFLIGHT_DELAY")?.is(&delay)
                    || !time.getattr("sleep")?.is(&sleep)
                {
                    return Err(PyValueError::new_err(
                        "original preflight pacing was not restored",
                    ));
                }
                cases.push(format!("{class_name}.{method}"));
            }
        }
        Ok(cases)
    })();
    let restored = redirect.call_method1("__exit__", (py.None(), py.None(), py.None()));
    let closed = capture.call_method0("close");
    restored?;
    closed?;
    let cases = observed?;
    if result.getattr("testsRun")?.extract::<usize>()? != 11
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || clock.borrow(py).0.load(Ordering::SeqCst) != 26
        || reject.getattr("call_count")?.extract::<usize>()? != 0
        || prerequisite.getattr("call_count")?.extract::<usize>()? != 8
        || writer.getattr("call_count")?.extract::<usize>()? != 8
    {
        return Err(PyValueError::new_err(
            "original preflight execution accounting incomplete",
        ));
    }
    let ledger = ledger.borrow(py);
    let paths = ledger
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("preflight temporary accounting poisoned"))?;
    if paths.len() != 8
        || paths
            .iter()
            .any(|(_, cleaned)| !cleaned.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original preflight teardown incomplete; retained",
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
            "unexpected preflight scratch; retained",
        ));
    }
    for (index, (path, _)) in paths.iter().enumerate() {
        directories(workspace, path, &mut 16, 0, &mut Vec::new())?;
        let members = std::fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        if members
            != ["catalog.db", "media"]
                .map(std::ffi::OsString::from)
                .into_iter()
                .collect()
        {
            return Err(PyValueError::new_err(
                "unexpected original preflight files; retained",
            ));
        }
        let images = std::fs::read_dir(path.join("media"))?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        let expected = if index == 0 {
            vec!["i1.png", "i2.png"]
        } else {
            vec!["i0.png", "i1.png", "i2.png"]
        };
        if images != expected.into_iter().map(std::ffi::OsString::from).collect() {
            return Err(PyValueError::new_err(
                "unexpected original preflight artwork; retained",
            ));
        }
    }
    for (path, _) in paths.iter() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(
        json!({"tests_run":11,"cases":cases,"temporary_calls":8,"sleep_calls":26,
        "identity_comparisons":0,"prerequisite_calls":8,"writer_calls":8,
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"unchanged original preflight/report/queue/catalog/identity/PNG/test bodies; real SQLite/Pillow/stdlib capture; private zero-delay clock, prerequisite/ownership no-ops and rejecting unused comparison callbacks; native missing-result transport adaptation is not socket failure equivalence"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    outcome_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("preflight assertion workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect preflight assertion workspace")?
        .next()
        .is_some()
    {
        return Err("preflight assertion workspace must be empty".into());
    }
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect preflight bytecode policy")?
        {
            return Err("preflight replay requires disabled import bytecode writes".into());
        }
        let reports = generate(sources[0], sources[1])?;
        let mut result = execute(py, &workspace, sources).map_err(|error| {
            format!(
                "original preflight replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })?;
        result["source_revision"] = json!(BASELINE);
        result["source_runtime"] = json!("CPython3.11.9");
        result["Pillow"] = json!("12.3.0");
        result["source_hashes"] = json!(
            sources
                .iter()
                .map(|source| (source.path.as_str(), source.sha256.as_str()))
                .collect::<std::collections::BTreeMap<_, _>>()
        );
        result["report_observations"] = reports;
        Ok(result)
    })
}

pub fn generate(source: &Source, test: &Source) -> Result<Value, String> {
    source_identity(source, test)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let capture = py
            .import("io")
            .and_then(|module| module.getattr("StringIO"))
            .and_then(|class| class.call0())
            .map_err(|_| "cannot initialize preflight report capture")?;
        let observed = (|| -> PyResult<Vec<Value>> {
            let bindings = PyDict::new(py);
            let options = PyDict::new(py);
            options.set_item("file", &capture)?;
            let print = py
                .import("functools")?
                .getattr("partial")?
                .call((py.import("builtins")?.getattr("print")?,), Some(&options))?;
            bindings.set_item("print", print)?;
            // An isolated logger avoids mutating global handlers or emitting source diagnostics.
            bindings.set_item(
                "log",
                py.import("logging")?
                    .getattr("Logger")?
                    .call1(("preflight-oracle", 100))?,
            )?;
            for (name, code) in [("EXIT_OK", 0), ("EXIT_FAILED", 4), ("EXIT_PARTIAL", 3)] {
                bindings.set_item(name, code)?;
            }
            let globals = crate::source_oracle_runtime::namespace(
                py,
                source,
                &["_report"],
                &["len"],
                &bindings,
            )?;
            let report = globals.get_item("_report")?.ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err("preflight report definition is missing")
            })?;
            let json = py.import("json")?;
            let mut results = Vec::new();
            // These are the original test's five successive _report calls, not native outputs.
            for (name, args) in [
                (
                    "missing",
                    json!([0, [], [["s:missing", "missing.png"]], []]),
                ),
                ("empty", json!([0, [], [], []])),
                ("accepted", json!([1, [], [], []])),
                (
                    "refused",
                    json!([
                        0,
                        [[
                            "s:missing",
                            "missing.png",
                            "uploadStickerFile failed: Bad Request: wrong file type"
                        ]],
                        [],
                        []
                    ]),
                ),
                (
                    "unknown",
                    json!([
                        0,
                        [],
                        [],
                        [["s:missing", "missing.png", "Bot API success has no result"]]
                    ]),
                ),
            ] {
                capture.call_method1("seek", (0,))?;
                capture.call_method1("truncate", (0,))?;
                let encoded = serde_json::to_string(&args).map_err(|_| {
                    pyo3::exceptions::PyValueError::new_err("cannot encode preflight report recipe")
                })?;
                let args = json.call_method1("loads", (encoded,))?;
                let code: u8 = report
                    .call1((
                        args.get_item(0)?,
                        args.get_item(1)?,
                        args.get_item(2)?,
                        args.get_item(3)?,
                    ))?
                    .extract()?;
                let stdout: String = capture.call_method0("getvalue")?.extract()?;
                results.push(json!({"name":name,"code":code,"stdout":stdout}));
            }
            Ok(results)
        })();
        capture
            .call_method0("close")
            .map_err(|_| "cannot close preflight report capture")?;
        let reports =
            observed.map_err(|error| format!("preflight oracle source failed: {error}"))?;
        Ok(
            json!({"cases_checked":reports.len(),"source_runtime":"CPython3.11.9",
            "source_hash":HASH,"test_source_hash":TEST_HASH,"reports":reports}),
        )
    })
}
