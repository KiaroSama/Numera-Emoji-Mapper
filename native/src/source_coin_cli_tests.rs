//! Original CLI admission assertions; imports/configuration/network ports stay private and closed.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 7] = [
    (
        "coins/fetch_paprika.py",
        "360809a22b57c19808b146016fe388769046f7337986684fd4d9c4db6f268b30",
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
        "coins/_env.py",
        "e6c2dedf03011c9b7d2c60563c924f337bd89bc9691fd4a8df5f2830ae0ba6f7",
    ),
    (
        "emojikit/operator_config.py",
        "a19f804a7bdd37baccb474eac96df6aa4d46439dd956e457c06adf3c180b9591",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
    (
        "tests/test_coin_cli_args.py",
        "a4f1201e40b9a14dac96622f8d3fac136df055a61ccdbbd47605b975f147e9f4",
    ),
];
const METHODS: [&str; 4] = [
    "test_a_misspelled_dry_flag_is_refused_by_both_providers",
    "test_an_unset_token_stops_a_live_run_before_any_download",
    "test_fetch_logos_page_count_is_not_read_at_import",
    "test_the_real_dry_flag_still_parses",
];
pub fn source_paths() -> [&'static str; 7] {
    IDENTITIES.map(|(path, _)| path)
}
pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("coin CLI replay requires exactly seven pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("coin CLI source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

#[pyfunction]
fn reject_effect() -> PyResult<()> {
    Err(PyValueError::new_err("unexpected original coin CLI effect"))
}
#[pyfunction]
fn logging_label(name: &str) -> PyResult<()> {
    if name == "fetch_logos" {
        Ok(())
    } else {
        Err(PyValueError::new_err(
            "unexpected original coin CLI log label",
        ))
    }
}
#[pyclass]
struct Imports {
    coins: Py<PyAny>,
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
        fromlist: &Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        if level != 0
            || !fromlist.is_none()
            || !matches!(name, "coins.fetch_cmc" | "coins.fetch_paprika")
        {
            return Err(PyImportError::new_err(
                "unexpected original coin CLI import",
            ));
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(self.coins.bind(py).clone())
    }
}
#[pyclass]
struct LogosLoader {
    path: PathBuf,
    module: Py<PyModule>,
    calls: AtomicUsize,
}
#[pymethods]
impl LogosLoader {
    fn __call__(&self, py: Python<'_>, path: PathBuf, name: &str) -> PyResult<Py<PyModule>> {
        if path != self.path || name != "argv_logos" {
            return Err(PyValueError::new_err("unexpected original coin CLI loader"));
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(self.module.clone_ref(py))
    }
}
#[pyclass]
struct Patch {
    http: Py<PyAny>,
    environment: Py<PyAny>,
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
        if target != "coins._http.get" {
            return Err(PyValueError::new_err("unexpected original coin CLI patch"));
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        py.import("unittest.mock")?
            .getattr("patch")?
            .call_method1("object", (self.http.bind(py), "get", replacement))
    }
    #[getter]
    fn object<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        py.import("unittest.mock")?
            .getattr("patch")?
            .getattr("object")
    }
    #[getter]
    fn dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.environment.bind(py).getattr("dict")
    }
}

fn execute(py: Python<'_>, root: &Path, sources: &[&Source]) -> PyResult<Value> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let environment = PyDict::new(py);
    environment.set_item("COIN_PACK_BASE", "cryptoemoji")?;
    environment.set_item("COIN_PACK_TITLE", "Fixture")?;
    let before = environment.copy()?;
    let os = ns.call0()?;
    os.setattr("environ", &environment)?;
    let bindings = PyDict::new(py);
    bindings.set_item("os", &os)?;
    bindings.set_item("sys", py.import("sys")?)?;
    bindings.set_item("EXIT_USAGE", 2)?;
    let token = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["require_token"],
        &["print", "SystemExit"],
        &bindings,
    )?;
    bindings.set_item("require_token", required(&token, "require_token")?)?;
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[5],
        &["OperatorConfigMissing"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    bindings.set_item(
        "OperatorConfigMissing",
        required(&errors, "OperatorConfigMissing")?,
    )?;
    bindings.set_item(
        "WHAT",
        crate::source_logging_oracle::literal(py, sources[4], "WHAT", false)?,
    )?;
    let operator = crate::source_oracle_runtime::namespace(
        py,
        sources[4],
        &["value", "require", "stop_unless"],
        &["SystemExit"],
        &bindings,
    )?;
    let operator =
        crate::source_roster_modules::rebind(py, &operator, "_original_coin_cli_operator")?;
    bindings.set_item("operator_config", operator)?;
    bindings.set_item("argparse", py.import("argparse")?)?;
    let refusal = wrap_pyfunction!(reject_effect, py)?.into_any();
    bindings.set_item("load_env", &refusal)?;
    bindings.set_item("setup_logging", &refusal)?;
    let paprika = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &["main"],
        &["set", "len", "max", "print"],
        &bindings,
    )?;
    let paprika = crate::source_roster_modules::rebind(py, &paprika, "_original_coin_cli_paprika")?;
    let cmc = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["cmc_headers", "main"],
        &["SystemExit", "set", "len", "max", "print"],
        &bindings,
    )?;
    let cmc = crate::source_roster_modules::rebind(py, &cmc, "_original_coin_cli_cmc")?;
    bindings.set_item(
        "MAX_PAGES",
        crate::source_logging_oracle::literal(py, sources[2], "MAX_PAGES", false)?,
    )?;
    bindings.set_item(
        "PER_PAGE",
        crate::source_logging_oracle::literal(py, sources[2], "PER_PAGE", false)?,
    )?;
    bindings.set_item("setup_logging", wrap_pyfunction!(logging_label, py)?)?;
    let logos = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &["main"],
        &[
            "int",
            "len",
            "print",
            "range",
            "isinstance",
            "list",
            "RuntimeError",
            "Exception",
            "str",
            "sorted",
            "set",
        ],
        &bindings,
    )?;
    let logos = crate::source_roster_modules::rebind(py, &logos, "_original_coin_cli_logos")?;
    let http = ns.call0()?;
    http.setattr("get", &refusal)?;
    let coins = ns.call0()?;
    coins.setattr("fetch_paprika", &paprika)?;
    coins.setattr("fetch_cmc", &cmc)?;
    let imports = Py::new(
        py,
        Imports {
            coins: coins.unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    let loader = Py::new(
        py,
        LogosLoader {
            path: root.join("coins/fetch_logos.py"),
            module: logos.unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    let patch = Py::new(
        py,
        Patch {
            http: http.clone().unbind(),
            environment: crate::source_config_oracle::private_environment_patch(py, &environment)?
                .unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    let mock = ns.call0()?;
    mock.setattr("patch", &patch)?;
    let path = py.import("pathlib")?.getattr("Path")?;
    bindings.set_item("ROOT", path.call1((root,))?)?;
    bindings.set_item("_load_standalone", &loader)?;
    for (name, value) in [
        ("mock", mock),
        ("unittest", py.import("unittest")?.into_any()),
        ("contextlib", py.import("contextlib")?.into_any()),
        ("io", py.import("io")?.into_any()),
    ] {
        bindings.set_item(name, value)?;
    }
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[6],
        &[
            "_StopBeforeNetwork",
            "AnUnrecognisedArgumentCannotSelectTheLiveBranch",
        ],
        &[
            "__build_class__",
            "Exception",
            "SystemExit",
            "AssertionError",
        ],
        &bindings,
    )?;
    tests
        .get_item("__builtins__")?
        .ok_or_else(|| PyValueError::new_err("missing CLI builtins"))?
        .cast_into::<PyDict>()?
        .set_item("__import__", imports.bind(py))?;
    let class = required(&tests, "AnUnrecognisedArgumentCannotSelectTheLiveBranch")?;
    let unittest = py.import("unittest")?;
    let methods: Vec<String> = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods != METHODS {
        return Err(PyValueError::new_err("original coin CLI collection drift"));
    }
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
                    "original coin CLI {method} reported {field}"
                )));
            }
        }
        if !environment.eq(&before)?
            || !http.getattr("get")?.is(&refusal)
            || !paprika.getattr("load_env")?.is(&refusal)
            || !cmc.getattr("load_env")?.is(&refusal)
        {
            return Err(PyValueError::new_err(
                "original coin CLI patches not restored",
            ));
        }
    }
    let import_calls = imports.borrow(py).calls.load(Ordering::Relaxed);
    let loader_calls = loader.borrow(py).calls.load(Ordering::Relaxed);
    let network_patches = patch.borrow(py).calls.load(Ordering::Relaxed);
    if import_calls != 8
        || loader_calls != 4
        || network_patches != 2
        || result.getattr("testsRun")?.extract::<usize>()? != 4
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original coin CLI execution incomplete",
        ));
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":4,"import_calls":import_calls,"loader_calls":loader_calls,"network_patches":network_patches,
        "cases":METHODS.map(|method|format!("AnUnrecognisedArgumentCannotSelectTheLiveBranch.{method}")),
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "ports":"unchanged original provider/logos mains and argument assertions; original token/operator/CMC headers; private imports/configuration/network/logging and real argparse/mock; native startup/order/error differences remain"}),
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
            .map_err(|_| "cannot inspect coin CLI bytecode policy")?
        {
            return Err("coin CLI replay requires disabled import bytecode writes".into());
        }
        execute(py, &root, sources).map_err(|error| {
            format!(
                "original coin CLI replay failed: {}",
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
        "coin-cli-tests",
        sources[0],
        &sources[1..],
        &json!({}),
    )
    .await
}
