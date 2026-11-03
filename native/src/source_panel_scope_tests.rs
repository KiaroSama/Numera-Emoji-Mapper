//! Unchanged scoped-save assertions use the original HTTP handler, catalog and plan implementation.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

pub(crate) fn threads(py: Python<'_>) -> PyResult<BTreeSet<u64>> {
    py.import("threading")?
        .call_method0("enumerate")?
        .try_iter()?
        .map(|thread| thread?.getattr("ident")?.extract())
        .collect()
}

pub(crate) fn settled_threads(py: Python<'_>, baseline: &BTreeSet<u64>) -> PyResult<()> {
    let deadline = Instant::now() + Duration::from_secs(1);
    for running in py
        .import("threading")?
        .call_method0("enumerate")?
        .try_iter()?
    {
        let running = running?;
        let identity = running.getattr("ident")?.extract::<u64>()?;
        if !baseline.contains(&identity) {
            let options = PyDict::new(py);
            options.set_item(
                "timeout",
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_secs_f64(),
            )?;
            running.call_method("join", (), Some(&options))?;
        }
    }
    if threads(py)? != *baseline {
        return Err(PyValueError::new_err(
            "original panel thread survived teardown; retained",
        ));
    }
    Ok(())
}

pub(crate) fn settled(
    py: Python<'_>,
    baseline: &BTreeSet<u64>,
    server: &Bound<'_, PyAny>,
    thread: &Bound<'_, PyAny>,
) -> PyResult<()> {
    settled_threads(py, baseline)?;
    if server
        .getattr("socket")?
        .call_method0("fileno")?
        .extract::<i64>()?
        != -1
        || thread.call_method0("is_alive")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original panel thread or socket survived teardown; retained",
        ));
    }
    Ok(())
}

pub(crate) fn handler<'py>(
    py: Python<'py>,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
    catalog: &Bound<'py, PyAny>,
    lock_busy: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyModule>, Bound<'py, PyModule>)> {
    let panel = required(bindings, "p")?;
    let values = PyDict::new(py);
    for name in ["json", "sqlite3", "secrets", "time", "threading"] {
        values.set_item(name, py.import(name)?)?;
    }
    values.set_item(
        "BaseHTTPRequestHandler",
        py.import("http.server")?
            .getattr("BaseHTTPRequestHandler")?,
    )?;
    values.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    values.set_item("unquote", py.import("urllib.parse")?.getattr("unquote")?)?;
    values.set_item("Catalog", catalog)?;
    values.set_item("build_view", panel.getattr("build_view")?)?;
    for name in [
        "PAGE",
        "ASSET_DIR",
        "ASSET_VER",
        "ICON_VER",
        "_json_for_script",
        "PER_SET",
    ] {
        values.set_item(name, panel.getattr(name)?)?;
    }
    for name in ["MAX_BODY", "_IMMUTABLE", "_MIME", "LOOPBACK_HOSTS"] {
        values.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[11], name, false)?,
        )?;
    }
    values.set_item(
        "PREVIEW_FPS",
        crate::source_logging_oracle::literal(py, sources[30], "PREVIEW_FPS", false)?,
    )?;
    let plan = crate::source_panel_plan_tests::namespace(py, sources)?;
    for name in ["PlanError", "overlay_targets", "read_plan"] {
        values.set_item(name, required(&plan, name)?)?;
    }
    values.set_item("LockBusy", lock_busy)?;
    let preview = PyDict::new(py);
    preview.set_item("parse_qs", py.import("urllib.parse")?.getattr("parse_qs")?)?;
    let preview = crate::source_oracle_runtime::namespace(
        py,
        sources[26],
        &["page_tiers", "parameters"],
        &["min", "int", "ValueError"],
        &preview,
    )?;
    values.set_item("preview_tiers", required(&preview, "page_tiers")?)?;
    values.set_item("preview_parameters", required(&preview, "parameters")?)?;
    let save = PyDict::new(py);
    save.set_item("json", py.import("json")?)?;
    save.set_item("Catalog", catalog)?;
    save.set_item("PER_SET", panel.getattr("PER_SET")?)?;
    save.set_item("build_view", panel.getattr("build_view")?)?;
    for name in ["merge_plan", "read_plan", "target_map", "write_plan"] {
        save.set_item(name, required(&plan, name)?)?;
    }
    let save = crate::source_oracle_runtime::module(
        py,
        sources[28],
        &["replace_map", "handle_save"],
        &[
            "set",
            "isinstance",
            "list",
            "all",
            "str",
            "len",
            "int",
            "bool",
            "dict",
        ],
        &save,
        "_original_panel_save",
    )?;
    for name in ["replace_map", "handle_save"] {
        values.set_item(name, save.getattr(name)?)?;
    }
    let telemetry = crate::source_panel_logging_tests::namespace(py, sources)?;
    values.set_item("ClientEventLog", required(&telemetry, "ClientEventLog")?)?;
    values.set_item(
        "log",
        py.import("logging")?
            .getattr("Logger")?
            .call1(("original-panel-scope",))?,
    )?;
    let original = crate::source_oracle_runtime::module(
        py,
        sources[11],
        &["_is_loopback", "_utc_hhmm", "make_handler"],
        &[
            "__build_class__",
            "str",
            "isinstance",
            "list",
            "dict",
            "set",
            "sorted",
            "len",
            "all",
            "int",
            "enumerate",
            "super",
            "TypeError",
            "ValueError",
            "UnicodeDecodeError",
            "OSError",
            "Exception",
            "ConnectionAbortedError",
            "ConnectionResetError",
            "BrokenPipeError",
        ],
        &values,
        "_original_panel_scope",
    )?;
    original.setattr("build_view", panel.getattr("build_view")?)?;
    Ok((original, save))
}

pub(crate) fn run<'py>(
    py: Python<'py>,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
    result: &Bound<'py, PyAny>,
) -> PyResult<Vec<String>> {
    let state = crate::source_reference_state::namespace(py, sources[15], None)?;
    let (original, _) = handler(
        py,
        sources,
        bindings,
        &required(bindings, "Catalog")?,
        &required(&state, "LockBusy")?,
    )?;
    let tests = PyDict::new(py);
    for name in [
        "tempfile",
        "Path",
        "unittest",
        "json",
        "Catalog",
        "_make_png",
    ] {
        tests.set_item(name, required(bindings, name)?)?;
    }
    for name in ["sqlite3", "threading"] {
        tests.set_item(name, py.import(name)?)?;
    }
    tests.set_item("closing", py.import("contextlib")?.getattr("closing")?)?;
    tests.set_item(
        "ThreadingHTTPServer",
        py.import("http.server")?.getattr("ThreadingHTTPServer")?,
    )?;
    let urllib = py.import("urllib")?;
    py.import("urllib.request")?;
    tests.set_item("request", urllib.getattr("request")?)?;
    tests.set_item("error", py.import("urllib.error")?)?;
    tests.set_item("p", original)?;
    tests.set_item(
        "TOKEN",
        crate::source_logging_oracle::literal(py, sources[27], "TOKEN", false)?,
    )?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[27],
        &["SaveScope"],
        &["__build_class__", "range", "staticmethod", "set"],
        &tests,
    )?;
    let class = required(&tests, "SaveScope")?;
    let methods: Vec<String> = py
        .import("unittest")?
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods.len() != 8 {
        return Err(PyValueError::new_err(
            "original panel scope collection drift",
        ));
    }
    let baseline = threads(py)?;
    let mut cases = Vec::new();
    for method in methods {
        let instance = class.call1((&method,))?;
        instance.call_method1("run", (result,))?;
        for field in [
            "errors",
            "failures",
            "skipped",
            "expectedFailures",
            "unexpectedSuccesses",
        ] {
            if result.getattr(field)?.len()? != 0 {
                return Err(PyValueError::new_err(format!(
                    "original SaveScope.{method} reported {field}; retained"
                )));
            }
        }
        settled(
            py,
            &baseline,
            &instance.getattr("httpd")?,
            &instance.getattr("thread")?,
        )?;
        cases.push(format!("SaveScope.{method}"));
    }
    // This class imports only stdlib inside its unchanged setup; the admitted handler stays private.
    let tests = PyDict::new(py);
    for name in ["tempfile", "Path", "unittest"] {
        tests.set_item(name, required(bindings, name)?)?;
    }
    tests.set_item("Catalog", required(bindings, "Catalog")?)?;
    let (panel, _) = handler(
        py,
        sources,
        bindings,
        &required(bindings, "Catalog")?,
        &required(&state, "LockBusy")?,
    )?;
    panel.setattr("ASSET_VER", required(bindings, "p")?.getattr("ASSET_VER")?)?;
    tests.set_item("p", panel)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[23],
        &["ConnectionsAreReused"],
        &["__build_class__", "super", "str", "range", "len"],
        &tests,
    )?;
    let class = required(&tests, "ConnectionsAreReused")?;
    let methods: Vec<String> = py
        .import("unittest")?
        .getattr("TestLoader")?
        .call0()?
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods.len() != 4 {
        return Err(PyValueError::new_err(
            "original panel connection collection drift",
        ));
    }
    for method in methods {
        let instance = class.call1((&method,))?;
        instance.call_method1("run", (result,))?;
        for field in [
            "errors",
            "failures",
            "skipped",
            "expectedFailures",
            "unexpectedSuccesses",
        ] {
            if result.getattr(field)?.len()? != 0 {
                return Err(PyValueError::new_err(format!(
                    "original ConnectionsAreReused.{method} reported {field}; retained"
                )));
            }
        }
        settled_threads(py, &baseline)?;
        let connection = instance.getattr("conn")?;
        if !connection.getattr("sock")?.is_none() {
            return Err(PyValueError::new_err(
                "original panel connection teardown incomplete; retained",
            ));
        }
        let port = connection.getattr("port")?.extract::<u16>()?;
        if std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(100),
        )
        .is_ok()
        {
            return Err(PyValueError::new_err(
                "original panel connection port survived; retained",
            ));
        }
        cases.push(format!("ConnectionsAreReused.{method}"));
    }
    Ok(cases)
}
