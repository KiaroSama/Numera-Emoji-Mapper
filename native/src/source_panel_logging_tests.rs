//! Original closed-schema telemetry assertions in the existing historical panel worker.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::prelude::*;
use pyo3::types::PyDict;

pub(crate) fn namespace<'py>(py: Python<'py>, sources: &[&Source]) -> PyResult<Bound<'py, PyDict>> {
    let telemetry = PyDict::new(py);
    for name in ["logging", "math", "re", "threading", "time"] {
        telemetry.set_item(name, py.import(name)?)?;
    }
    telemetry.set_item("deque", py.import("collections")?.getattr("deque")?)?;
    let original = crate::source_oracle_runtime::namespace(
        py,
        sources[21],
        &["ClientEventLog"],
        &[
            "__build_class__",
            "frozenset",
            "set",
            "isinstance",
            "list",
            "len",
            "dict",
            "str",
            "type",
            "int",
            "float",
            "ValueError",
            "sorted",
        ],
        &telemetry,
    )?;
    for name in ["EVENTS", "NUMBERS", "SOURCES"] {
        crate::source_oracle_runtime::assignment(py, sources[21], name, &original)?;
    }
    Ok(original)
}

pub(crate) fn run<'py>(
    py: Python<'py>,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
    result: &Bound<'py, PyAny>,
) -> PyResult<Vec<String>> {
    let original = namespace(py, sources)?;
    let api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    api.setattr("SOURCES", required(&original, "SOURCES")?)?;
    let tests = PyDict::new(py);
    for name in ["io", "logging", "unittest"] {
        tests.set_item(name, py.import(name)?)?;
    }
    tests.set_item("panel", required(bindings, "p")?)?;
    tests.set_item("panel_logging", api)?;
    tests.set_item("ClientEventLog", required(&original, "ClientEventLog")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[20],
        &["UILogContract", "EveryScriptMayReportErrors"],
        &["__build_class__", "float", "repr", "ValueError", "set"],
        &tests,
    )?;
    let loader = py.import("unittest")?.getattr("TestLoader")?.call0()?;
    let mut cases = Vec::new();
    for name in ["UILogContract", "EveryScriptMayReportErrors"] {
        let class = required(&tests, name)?;
        let methods: Vec<String> = loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        if methods.len() != 1 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "original panel telemetry collection drift",
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
                        "original panel telemetry {name}.{method} reported {field}; retained"
                    )));
                }
            }
            cases.push(format!("{name}.{method}"));
        }
    }
    Ok(cases)
}
