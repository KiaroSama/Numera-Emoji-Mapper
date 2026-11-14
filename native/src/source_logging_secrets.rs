//! Fixed original committed-secret assertion on synthetic files, never operator configuration.
use crate::source_git_inventory::Source;
use crate::source_logging_oracle::{literal, required, source_identity};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::path::Path;

fn observe(py: Python<'_>, workspace: &Path, source: &Source, test: &Source) -> PyResult<Value> {
    let unittest = py.import("unittest")?;
    let mock = py.import("unittest.mock")?.getattr("Mock")?;
    let namespace = py.import("types")?.getattr("SimpleNamespace")?;
    let keys = literal(py, source, "SECRET_ENV_KEYS", false)?;
    let api = namespace.call0()?;
    api.setattr("SECRET_ENV_KEYS", &keys)?;
    let keys: Vec<String> = keys.extract()?;
    let mut observations = Vec::new();
    for (index, (name, configuration, leaking)) in [
        ("clean", "TOKEN=synthetic-never-issued-credential\n".to_owned(), false),
        ("leak", "TOKEN=synthetic-never-issued-credential\n".to_owned(), true),
        ("unicode-duplicate", "TOKEN=synthetic-replaced-value\u{85}TOKEN=\"synthetic-never-issued-credential\"\nAPI_KEY=éééééééééééé\nPUBLIC_CONFIG=synthetic-never-issued-credential\n".to_owned(), true),
    ].into_iter().enumerate() {
        let directory = workspace.join(format!("case-{index}"));
        std::fs::create_dir(&directory)?;
        std::fs::write(directory.join(".env"), configuration.as_bytes())?;
        let mut bytes = if leaking {
            b"before\xffsynthetic-never-issued-credential after".to_vec()
        } else {
            b"ordinary public text".to_vec()
        };
        if index == 2 {
            bytes.extend_from_slice(" éééééééééééé".as_bytes());
        }
        std::fs::write(directory.join("tracked é.txt"), &bytes)?;
        std::fs::write(directory.join("untracked.txt"), b"synthetic-never-issued-credential")?;
        let large = std::fs::File::create(directory.join("large.txt"))?;
        use std::io::Write;
        (&large).write_all(b"synthetic-never-issued-credential")?;
        large.set_len(2 * 1024 * 1024 + 1)?;
        drop(large);
        std::fs::create_dir(directory.join("directory.txt"))?;
        let tracked = ["tracked é.txt", "large.txt", "missing.txt", "directory.txt"];
        let response = namespace.call0()?;
        response.setattr("stdout", pyo3::types::PyBytes::new(py, (tracked.join("\0") + "\0").as_bytes()))?;
        let options = PyDict::new(py);
        options.set_item("return_value", response)?;
        let run = mock.call((), Some(&options))?;
        let subprocess = namespace.call0()?;
        subprocess.setattr("run", &run)?;
        subprocess.setattr("SubprocessError", py.import("subprocess")?.getattr("SubprocessError")?)?;
        let bindings = PyDict::new(py);
        bindings.set_item("unittest", &unittest)?;
        bindings.set_item("L", &api)?;
        bindings.set_item("ROOT", py.import("pathlib")?.getattr("Path")?.call1((&directory,))?)?;
        bindings.set_item("subprocess", subprocess)?;
        let globals = crate::source_oracle_runtime::namespace(
            py, test, &["TestNoCommittedSecrets"], &["__build_class__", "len", "any", "OSError"], &bindings,
        )?;
        let case = required(&globals, "TestNoCommittedSecrets")?.call1(("test_no_real_secret_is_committed",))?;
        let options = PyDict::new(py);
        // wraps records the original computed list and still executes its original failing assertion.
        options.set_item("wraps", case.getattr("assertEqual")?)?;
        let assertion = mock.call((), Some(&options))?;
        case.setattr("assertEqual", &assertion)?;
        let result = unittest.getattr("TestResult")?.call0()?;
        case.call_method1("run", (&result,))?;
        for field in ["errors", "skipped", "expectedFailures", "unexpectedSuccesses"] {
            if result.getattr(field)?.len()? != 0 {
                return Err(PyValueError::new_err(format!("original committed-secret assertion reported {field}")));
            }
        }
        if result.getattr("testsRun")?.extract::<usize>()? != 1
            || assertion.getattr("call_count")?.extract::<usize>()? != 1
            || run.getattr("call_count")?.extract::<usize>()? != 1
        {
            return Err(PyValueError::new_err("original committed-secret assertion did not execute"));
        }
        let args = assertion.getattr("call_args")?.getattr("args")?;
        let leaks: Vec<String> = args.get_item(0)?.extract()?;
        if args.get_item(1)?.len()? != 0
            || result.getattr("failures")?.len()? != usize::from(!leaks.is_empty())
            || result.call_method0("wasSuccessful")?.extract::<bool>()? != leaks.is_empty()
        {
            return Err(PyValueError::new_err("original committed-secret failure outcome drift"));
        }
        let failures = usize::from(!leaks.is_empty());
        observations.push(json!({"name":name,"configuration":configuration,"tracked":tracked,
            "bytes":bytes,"leaks":leaks,"tests_run":1,"assertions":1,
            "deliberate_failures":failures}));
        // Only the owned synthetic case is removed, after its original assertions were observed.
        std::fs::remove_dir_all(directory)?;
    }
    Ok(
        json!({"source_runtime":"CPython3.11.9","source_hash":source.sha256,
        "test_source_hash":test.sha256,"secret_keys":keys,
        "original_method":"TestNoCommittedSecrets.test_no_real_secret_is_committed",
        "observations":observations,
        "ports":"fixed synthetic tracked-list response; real original parsing, file reads and assertions; no operator configuration or real Git subprocess in source replay"}),
    )
}

pub fn generate(
    root: &Path,
    workspace: &Path,
    source: &Source,
    test: &Source,
) -> Result<Value, String> {
    source_identity(source, test)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root || !workspace.starts_with(&root) {
        return Err("committed-secret replay requires its project-contained workspace".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect secret-scan workspace")?
        .next()
        .is_some()
    {
        return Err("committed-secret replay workspace must be empty".into());
    }
    Python::initialize();
    let report = Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        observe(py, &workspace, source, test).map_err(|_| {
            "original committed-secret assertion failed; synthetic workspace retained".to_owned()
        })
    })?;
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot verify secret-scan cleanup")?
        .next()
        .is_some()
    {
        return Err("committed-secret replay left scratch; retained for diagnosis".into());
    }
    Ok(report)
}
