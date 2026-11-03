//! Original plan assertions share the panel historical namespace, result and delayed scratch ledger.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::prelude::*;
use pyo3::types::PyDict;

pub(crate) fn namespace<'py>(py: Python<'py>, sources: &[&Source]) -> PyResult<Bound<'py, PyDict>> {
    let state = crate::source_reference_state::namespace(py, sources[15], None)?;
    let options = PyDict::new(py);
    options.set_item("_dt", py.import("datetime")?)?;
    options.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    options.set_item(
        "PLAN_NAME",
        crate::source_logging_oracle::literal(py, sources[14], "PLAN_NAME", false)?,
    )?;
    options.set_item("write_json_atomic", required(&state, "write_json_atomic")?)?;
    let plan = crate::source_oracle_runtime::namespace(
        py,
        sources[14],
        &[
            "PlanError",
            "build_plan",
            "write_plan",
            "target_map",
            "merge_plan",
        ],
        &[
            "__build_class__",
            "ValueError",
            "dict",
            "len",
            "set",
            "sorted",
            "str",
        ],
        &options,
    )?;
    let reader = crate::source_plan_report_tests::reader(py, sources[14], sources[29])?;
    plan.set_item("PlanError", required(&reader, "PlanError")?)?;
    plan.set_item("read_plan", required(&reader, "read_plan")?)?;
    let overlay = crate::source_oracle_runtime::namespace(
        py,
        sources[14],
        &["overlay_targets"],
        &["dict"],
        &plan,
    )?;
    plan.set_item("overlay_targets", required(&overlay, "overlay_targets")?)?;
    Ok(plan)
}

pub(crate) fn run<'py>(
    py: Python<'py>,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
    result: &Bound<'py, PyAny>,
) -> PyResult<Vec<String>> {
    let plan = namespace(py, sources)?;
    let test_bindings = PyDict::new(py);
    for name in ["tempfile", "Path", "unittest", "json"] {
        test_bindings.set_item(name, required(bindings, name)?)?;
    }
    test_bindings.set_item("PLAN_NAME", required(&plan, "PLAN_NAME")?)?;
    for name in ["build_plan", "merge_plan", "write_plan"] {
        test_bindings.set_item(name, required(&plan, name)?)?;
    }
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[13],
        &[
            "card",
            "ThePlanSaysWhatTheOwnerDecided",
            "TheMergeDropsDecisionsAboutEmojiThatAreGone",
        ],
        &["__build_class__", "str", "range", "dict", "set"],
        &test_bindings,
    )?;
    let loader = py.import("unittest")?.getattr("TestLoader")?.call0()?;
    let mut cases = Vec::new();
    for (name, count) in [
        ("ThePlanSaysWhatTheOwnerDecided", 7),
        ("TheMergeDropsDecisionsAboutEmojiThatAreGone", 5),
    ] {
        let class = required(&tests, name)?;
        let methods: Vec<String> = loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        if methods.len() != count {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "original panel plan collection drift",
            ));
        }
        for method in methods {
            class.call1((&method,))?.call_method1("run", (result,))?;
            for field in [
                "errors",
                "failures",
                "skipped",
                "expectedFailures",
                "unexpectedSuccesses",
            ] {
                if result.getattr(field)?.len()? != 0 {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!(
                        "original panel plan {name}.{method} reported {field}; retained"
                    )));
                }
            }
            cases.push(format!("{name}.{method}"));
        }
    }
    Ok(cases)
}
