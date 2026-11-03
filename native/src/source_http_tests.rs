//! Fixed original HTTP assertions; only external requests, sleep, imports and environment are private.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 5] = [
    (
        "coins/_http.py",
        "c3ca151103971504e4eefdf5c44b7bb50858e0936fa6cd806982c8bede3540db",
    ),
    (
        "coins/_paprika_api.py",
        "34742583025adaad3c7513ce0031c13d32759e29cfa8076609150a0badeb1acc",
    ),
    (
        "coins/fetch_cmc.py",
        "74a5a49bc7bbe91d3718d27886ce97f55d3479c3cd78d89a8a711389d1bbe5e9",
    ),
    (
        "coins/fetch_logos.py",
        "a42706ef2afb8efc880e7d695199a8f3b1a8d3acc9f17e625f60b49e40a36181",
    ),
    (
        "tests/test_coin_http.py",
        "f48e4093f91e5608e71ffff2b44000bcf18247c77c16670b8fcbc8a8bc64d512",
    ),
];
const SHARED: [&str; 10] = [
    "test_400_and_404_are_no_match_and_are_never_retried",
    "test_402_is_a_quota_answer_and_is_never_retried",
    "test_a_rejected_cmc_key_stops_the_run_and_names_the_key",
    "test_a_success_is_decoded_and_costs_one_call",
    "test_a_transient_failure_is_retried_and_then_reported_as_TRANSIENT",
    "test_a_transport_exception_is_retried_like_a_bad_status",
    "test_an_absurd_retry_after_is_capped",
    "test_an_unanswered_cmc_lookup_is_unknown_not_no_match",
    "test_fetch_logos_still_raises_after_its_retries",
    "test_retry_after_beats_the_backoff_ladder",
];
const PAGING: [&str; 3] = [
    "test_a_typo_falls_back_instead_of_crashing_at_import",
    "test_the_default_is_short",
    "test_the_env_var_overrides_it",
];

pub fn source_paths() -> [&'static str; 5] {
    IDENTITIES.map(|(path, _)| path)
}
pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("HTTP replay requires exactly five pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("HTTP source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

#[pyfunction]
fn reject_sleep(_duration: f64) -> PyResult<()> {
    Err(PyValueError::new_err(
        "unpatched original HTTP sleep refused",
    ))
}

#[pyclass]
struct LogosLoader {
    path: PathBuf,
    module: Py<PyModule>,
}
#[pymethods]
impl LogosLoader {
    fn __call__(&self, py: Python<'_>, path: PathBuf, name: &str) -> PyResult<Py<PyModule>> {
        if path != self.path || name != "http_fetch_logos" {
            return Err(PyValueError::new_err("unexpected original HTTP import"));
        }
        Ok(self.module.clone_ref(py))
    }
}

fn execute(py: Python<'_>, root: &Path, sources: &[&Source]) -> PyResult<Value> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let bindings = PyDict::new(py);
    let environment = PyDict::new(py);
    let os = ns.call0()?;
    os.setattr("environ", &environment)?;
    let time = ns.call0()?;
    // Every retrying original test patches this exact private attribute first.
    let unexpected_sleep = wrap_pyfunction!(reject_sleep, py)?.into_any();
    time.setattr("sleep", &unexpected_sleep)?;
    let requests = ns.call0()?;
    requests.setattr(
        "ConnectionError",
        py.import("requests")?.getattr("ConnectionError")?,
    )?;
    bindings.set_item("os", &os)?;
    bindings.set_item("time", &time)?;
    bindings.set_item("requests", &requests)?;
    bindings.set_item("SESSION", py.None())?;
    for name in ["RETRY_AFTER_MAX", "PAGE_DELAY_DEFAULT", "PAGE_DELAY_ENV"] {
        bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[0], name, false)?,
        )?;
    }
    let globals = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &["page_delay", "_retry_after", "get"],
        &[
            "max",
            "min",
            "float",
            "range",
            "str",
            "Exception",
            "ValueError",
            "print",
        ],
        &bindings,
    )?;
    let http = crate::source_roster_modules::rebind(py, &globals, "_original_coin_http")?;
    bindings.set_item("_http", &http)?;
    let object = py.import("builtins")?.getattr("object")?;
    for name in ["QUOTA_EXHAUSTED", "TRANSIENT"] {
        bindings.set_item(name, object.call0()?)?;
    }
    bindings.set_item(
        "HEADERS",
        crate::source_logging_oracle::literal(py, sources[1], "HEADERS", false)?,
    )?;
    let paprika =
        crate::source_oracle_runtime::namespace(py, sources[1], &["http_json"], &[], &bindings)?;
    let paprika = crate::source_roster_modules::rebind(py, &paprika, "_original_http_paprika")?;
    let cmc = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["cmc_json"],
        &["SystemExit"],
        &bindings,
    )?;
    let cmc = crate::source_roster_modules::rebind(py, &cmc, "_original_http_cmc")?;
    bindings.set_item(
        "HEADERS",
        crate::source_logging_oracle::literal(py, sources[3], "HEADERS", false)?,
    )?;
    let logos = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["_get"],
        &["RuntimeError"],
        &bindings,
    )?;
    let logos = crate::source_roster_modules::rebind(py, &logos, "_original_http_logos")?;
    let path = py.import("pathlib")?.getattr("Path")?;
    bindings.set_item("ROOT", path.call1((root,))?)?;
    bindings.set_item(
        "_load_standalone",
        Py::new(
            py,
            LogosLoader {
                path: root.join("coins/fetch_logos.py"),
                module: logos.unbind(),
            },
        )?,
    )?;
    for (name, value) in [
        ("coins_fp", paprika.into_any()),
        ("coins_cmc", cmc.into_any()),
        ("unittest", py.import("unittest")?.into_any()),
        ("mock", py.import("unittest.mock")?.into_any()),
        ("contextlib", py.import("contextlib")?.into_any()),
        ("io", py.import("io")?.into_any()),
    ] {
        bindings.set_item(name, value)?;
    }
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[4],
        &[
            "StubResponse",
            "StubSession",
            "SharedHttpClient",
            "PagingDelay",
        ],
        &[
            "__build_class__",
            "classmethod",
            "property",
            "list",
            "len",
            "isinstance",
            "Exception",
            "reversed",
            "str",
            "SystemExit",
            "RuntimeError",
        ],
        &bindings,
    )?;
    let unittest = py.import("unittest")?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    for (name, expected) in [
        ("SharedHttpClient", SHARED.as_slice()),
        ("PagingDelay", PAGING.as_slice()),
    ] {
        let class = required(&tests, name)?;
        let collected: Vec<String> = loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        let selected = collected
            .iter()
            .filter(|method| method.as_str() != "test_no_coin_script_hand_rolls_urlopen_again")
            .map(String::as_str)
            .collect::<Vec<_>>();
        if selected != expected
            || collected.len() != expected.len() + usize::from(name == "SharedHttpClient")
        {
            return Err(PyValueError::new_err("original HTTP collection drift"));
        }
        class.call_method0("setUpClass")?;
        for method in expected {
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
                        "original HTTP {name}.{method} reported {field}"
                    )));
                }
            }
            if !http.getattr("SESSION")?.is_none()
                || !time.getattr("sleep")?.is(&unexpected_sleep)
                || !environment.is_empty()
            {
                return Err(PyValueError::new_err(
                    "original HTTP patches were not restored",
                ));
            }
            cases.push(format!("{name}.{method}"));
        }
        class.call_method0("tearDownClass")?;
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 13
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err("original HTTP execution incomplete"));
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":13,"cases":cases,"errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"unchanged original HTTP/wrappers/stubs/classes/setup/teardown and assertions; private environment/time/requests/module import; architecture scanner excluded; no transport/pooling/native exception/stdout equivalence"}),
    )
}

pub fn observe(root: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect HTTP bytecode policy")?
        {
            return Err("HTTP replay requires disabled import bytecode writes".into());
        }
        execute(py, &root, sources).map_err(|error| {
            format!(
                "original HTTP replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })
}

pub async fn observe_owned(
    root: &Path,
    executable: &Path,
    sources: &[&Source],
) -> Result<Value, String> {
    source_identity(sources)?;
    crate::source_oracle_process::check(
        root,
        executable,
        "http-tests",
        sources[0],
        &sources[1..],
        &json!({}),
    )
    .await
}
