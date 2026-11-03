//! Original destination assertions and observations, after the configuration oracle's fixed admission.
use super::NumericEnvironment;
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) fn observe(py: Python<'_>, source: &Source, test: &Source) -> PyResult<Value> {
    let namespace = py.import("types")?.getattr("SimpleNamespace")?;
    let environment = PyDict::new(py);
    let os = namespace.call0()?;
    os.setattr("environ", &environment)?;
    let bindings = PyDict::new(py);
    bindings.set_item("os", os)?;
    let original = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["links_chat_id"],
        &["int"],
        &bindings,
    )?;
    let function = crate::source_logging_oracle::required(&original, "links_chat_id")?;
    let api = namespace.call0()?;
    api.setattr("links_chat_id", &function)?;
    let patch = Py::new(
        py,
        NumericEnvironment {
            values: environment.clone().unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    let mock = namespace.call0()?;
    mock.setattr("patch", patch.bind(py))?;
    let unittest = py.import("unittest")?;
    let bindings = PyDict::new(py);
    bindings.set_item("unittest", &unittest)?;
    bindings.set_item("announce", api)?;
    bindings.set_item("mock", mock)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &["LinksDestination"],
        &["__build_class__"],
        &bindings,
    )?;
    let class = crate::source_logging_oracle::required(&tests, "LinksDestination")?;
    let methods = [
        "test_unset_falls_back_to_the_owner",
        "test_numeric_channel_id_is_used_as_an_int",
        "test_at_username_is_passed_through",
        "test_surrounding_whitespace_is_tolerated",
    ];
    let deferred = "test_every_publisher_resolves_the_same_destination";
    let collected = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract::<Vec<String>>()?;
    let mut expected = methods.to_vec();
    expected.push(deferred);
    expected.sort_unstable();
    if collected != expected {
        return Err(PyValueError::new_err(
            "original destination collection drift",
        ));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    for method in methods {
        class.call1((method,))?.call_method1("run", (&result,))?;
    }
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyValueError::new_err(format!(
                "original destination assertion reported {field}"
            )));
        }
    }
    let patch_calls = patch.borrow(py).calls.load(Ordering::Relaxed);
    if result.getattr("testsRun")?.extract::<usize>()? != methods.len()
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || patch_calls != 4
        || !environment.is_empty()
    {
        return Err(PyValueError::new_err(
            "original destination execution/restoration drift",
        ));
    }
    let owner = class.getattr("OWNER")?.extract::<u64>()?;
    let int_type = py.import("builtins")?.getattr("int")?;
    let str_type = py.import("builtins")?.getattr("str")?;
    let limit = "0".repeat(4300);
    let over_limit = "0".repeat(4301);
    let mut observations = Vec::new();
    for raw in [
        None,
        Some(""),
        Some("   "),
        Some("-1001111111111"),
        Some("@packlinks"),
        Some("  -1001111111111  "),
        Some("\u{1c}-۰۱۰۰_۱۱۱_۱۱۱_۱۱۱۱\u{1f}"),
        Some("+۰۰۴۲"),
        Some("-۰"),
        Some("\u{1c}@packlinks\u{1f}"),
        Some("9223372036854775808"),
        Some("-9223372036854775809"),
        Some(limit.as_str()),
        Some(over_limit.as_str()),
        Some("_1"),
        Some("1_"),
        Some("1__0"),
        Some("0x10"),
        Some("²"),
        Some("1\0"),
        Some("+"),
        Some("- 1"),
    ] {
        environment.clear();
        if let Some(raw) = raw {
            environment.set_item("PACK_LINKS_CHAT_ID", raw)?;
        }
        let outcome = match function.call1((owner,)) {
            Ok(value) => {
                let kind = if value.get_type().is(&int_type) {
                    "int"
                } else if value.get_type().is(&str_type) {
                    "str"
                } else {
                    return Err(PyValueError::new_err(
                        "unexpected original destination type",
                    ));
                };
                json!({"kind":kind,"value":value.str()?.extract::<String>()?})
            }
            Err(error) if error.is_instance_of::<PyValueError>(py) => {
                json!({"error_kind":"ValueError","error":error.value(py).str()?.extract::<String>()?})
            }
            Err(error) => return Err(error),
        };
        observations.push(json!({"raw":raw,"owner":owner,"result":outcome}));
    }
    environment.clear();
    Ok(json!({
        "source_hash":source.sha256,"test_source_hash":test.sha256,
        "tests_run":methods.len(),"patch_calls":patch_calls,
        "original_methods":methods.map(|method|format!("LinksDestination.{method}")),
        "deferred_methods":[format!("LinksDestination.{deferred}")],
        "observations":observations,
        "ports":"private environment and literal patch target only; four pure original assertions unchanged; shared publisher import identity not executed; native String result and generic error text are adaptations"
    }))
}
