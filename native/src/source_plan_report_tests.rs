//! Original plan-report assertions share the status cohort's private modules and temporary ledger.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{observed, required};
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PySet};
use serde_json::{Value, json};
use std::path::Path;

const METHODS: [&str; 5] = [
    "test_a_plan_that_matches_the_packs_is_not_pending",
    "test_moves_held_and_candidates_are_counted_per_pack",
    "test_over_capacity_is_pending",
    "test_the_command_reports_the_ids_it_would_retire_and_exits_3",
    "test_no_saved_plan_is_nothing_pending",
];
#[pyclass]
struct Imports {
    artifacts: Py<PyAny>,
    json: Py<PyAny>,
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
        if level == 0 {
            if name == "emojikit.state_artifacts"
                && fromlist.extract::<Vec<String>>()? == ["plan_keys"]
            {
                return Ok(self.artifacts.bind(py).clone());
            }
            if name == "json"
                && (fromlist.is_none()
                    || (fromlist.is_instance_of::<pyo3::types::PyTuple>()
                        && fromlist.is_empty()?))
            {
                return Ok(self.json.bind(py).clone());
            }
        }
        Err(PyImportError::new_err(
            "unexpected original plan reader import",
        ))
    }
}

fn native_literals<'py>(py: Python<'py>, source: &Source) -> PyResult<Bound<'py, PyDict>> {
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (&source.text, &source.path))?;
    let values = PyDict::new(py);
    for node in crate::source_test_inventory::walk(&ast, &tree)? {
        if crate::source_test_inventory::kind(&node)? != "Assign" {
            continue;
        }
        let targets = node.getattr("targets")?;
        if targets.len()? != 1 {
            continue;
        }
        let target = targets.get_item(0)?;
        if crate::source_test_inventory::kind(&target)? != "Name" {
            continue;
        }
        let name: String = target.getattr("id")?.extract()?;
        if !["state", "plan", "ids"].contains(&name.as_str()) {
            continue;
        }
        if values.contains(&name)? {
            return Err(PyValueError::new_err("duplicate original plan CLI literal"));
        }
        values.set_item(
            name,
            ast.call_method1("literal_eval", (node.getattr("value")?,))?,
        )?;
    }
    if values.len() != 3 {
        return Err(PyValueError::new_err("original plan CLI literals missing"));
    }
    Ok(values)
}

pub(crate) fn reader<'py>(
    py: Python<'py>,
    panel_source: &Source,
    artifact_source: &Source,
) -> PyResult<Bound<'py, PyDict>> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let artifacts = crate::source_oracle_runtime::namespace(
        py,
        artifact_source,
        &["_plan_refs", "plan_keys"],
        &[
            "isinstance",
            "dict",
            "list",
            "str",
            "all",
            "range",
            "len",
            "ValueError",
        ],
        &PyDict::new(py),
    )?;
    let api = ns.call0()?;
    api.setattr("plan_keys", required(&artifacts, "plan_keys")?)?;
    let bindings = PyDict::new(py);
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item(
        "PLAN_NAME",
        crate::source_logging_oracle::literal(py, panel_source, "PLAN_NAME", false)?,
    )?;
    let panel = crate::source_oracle_runtime::namespace(
        py,
        panel_source,
        &["PlanError", "read_plan", "target_map"],
        &[
            "__build_class__",
            "ValueError",
            "OSError",
            "KeyError",
            "TypeError",
            "dict",
            "len",
            "any",
            "isinstance",
            "int",
            "bool",
        ],
        &bindings,
    )?;
    required(&panel, "__builtins__")?.set_item(
        "__import__",
        Py::new(
            py,
            Imports {
                artifacts: api.unbind(),
                json: py.import("json")?.into_any().unbind(),
            },
        )?,
    )?;
    Ok(panel)
}

pub(crate) fn run(
    py: Python<'_>,
    workspace: &Path,
    sources: &[&Source],
    temporary: &Bound<'_, PyAny>,
    operator: &Bound<'_, PyAny>,
) -> PyResult<Value> {
    let panel = reader(py, sources[6], sources[7])?;
    let bindings = PyDict::new(py);
    for name in ["PlanError", "read_plan", "target_map"] {
        bindings.set_item(name, required(&panel, name)?)?;
    }
    for name in ["argparse", "json", "sqlite3"] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item(
        "ROOT",
        py.import("pathlib")?
            .getattr("Path")?
            .call1((workspace.to_path_buf(),))?,
    )?;
    bindings.set_item("operator_config", operator)?;
    bindings.set_item("EXIT_OK", 0)?;
    bindings.set_item("EXIT_USAGE", 2)?;
    bindings.set_item("EXIT_PENDING", 3)?;
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (&sources[4].text, &sources[4].path))?;
    bindings.set_item("__doc__", ast.call_method1("get_docstring", (tree,))?)?;
    let mock = py.import("unittest.mock")?;
    for name in ["setup_logging", "load_env"] {
        let options = PyDict::new(py);
        options.set_item(
            "side_effect",
            PyValueError::new_err("unpatched original plan setup").value(py),
        )?;
        bindings.set_item(name, mock.getattr("Mock")?.call((), Some(&options))?)?;
    }
    let globals = crate::source_oracle_runtime::namespace(
        py,
        sources[4],
        &[
            "_live_pack_of",
            "_catalog_ids",
            "summarize",
            "render",
            "main",
        ],
        &[
            "dict",
            "int",
            "str",
            "set",
            "sorted",
            "len",
            "any",
            "print",
            "Exception",
        ],
        &bindings,
    )?;
    let module = crate::source_roster_modules::rebind(py, &globals, "_original_plan_report")?;
    let tests = PyDict::new(py);
    tests.set_item(
        "STATE",
        crate::source_logging_oracle::literal(py, sources[5], "STATE", false)?,
    )?;
    tests.set_item("plan_status", &module)?;
    tests.set_item("tempfile", temporary)?;
    for name in ["unittest", "contextlib", "io", "json"] {
        tests.set_item(name, py.import(name)?)?;
    }
    tests.set_item("mock", &mock)?;
    tests.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[5],
        &["_plan", "WhatThePlanWouldChange"],
        &["__build_class__", "sorted", "str"],
        &tests,
    )?;
    let unittest = py.import("unittest")?;
    let class = required(&tests, "WhatThePlanWouldChange")?;
    let names: Vec<String> = unittest
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    let mut expected = METHODS.to_vec();
    expected.sort_unstable();
    if names != expected {
        return Err(PyValueError::new_err(
            "original plan report collection drift",
        ));
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
                    "original plan report {method} reported {field}; retained"
                )));
            }
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 5
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original plan report execution incomplete",
        ));
    }
    // Exact original CLI literals supply both source-rendered outputs for the native tracer.
    let inputs = native_literals(py, sources[8])?;
    let summary = module
        .getattr("summarize")?
        .call1((required(&inputs, "plan")?, required(&inputs, "state")?))?;
    let ids = required(&inputs, "ids")?;
    let offline: String = module
        .getattr("render")?
        .call1((&summary, &ids, py.None()))?
        .extract()?;
    let live = PySet::new(py, [ids.get_item("s:a")?])?;
    let online: String = module
        .getattr("render")?
        .call1((&summary, &ids, live))?
        .extract()?;
    Ok(
        json!({"tests_run":5,"cases":METHODS.map(|name|format!("WhatThePlanWouldChange.{name}")),
        "errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "native_inputs":observed(&inputs)?,"native_summary":observed(&summary)?,"offline":offline,"live":online,
        "ports":"five unchanged original assertions/private patchable module/original plan reader and validator/real files; mock logging/environment/catalog-ID ports; shared delayed temporary custody; native CLI literals AST-admitted from exact original source, no target-derived output"}),
    )
}
