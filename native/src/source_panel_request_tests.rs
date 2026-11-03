//! Original pure read/host assertions reuse the admitted panel and shared historical result.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::prelude::*;
use pyo3::types::PyDict;

pub(crate) fn run<'py>(
    py: Python<'py>,
    workspace: &std::path::Path,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
    result: &Bound<'py, PyAny>,
) -> PyResult<Vec<String>> {
    let panel = required(bindings, "p")?;
    let safe = PyDict::new(py);
    safe.set_item("json", py.import("json")?)?;
    let safe =
        crate::source_oracle_runtime::namespace(py, sources[24], &["json_for_script"], &[], &safe)?;
    panel.setattr("_json_for_script", required(&safe, "json_for_script")?)?;
    let host = PyDict::new(py);
    host.set_item(
        "LOOPBACK_HOSTS",
        crate::source_logging_oracle::literal(py, sources[11], "LOOPBACK_HOSTS", false)?,
    )?;
    let host =
        crate::source_oracle_runtime::namespace(py, sources[11], &["_is_loopback"], &[], &host)?;
    panel.setattr("_is_loopback", required(&host, "_is_loopback")?)?;
    let values = PyDict::new(py);
    values.set_item("p", panel)?;
    values.set_item("json", py.import("json")?)?;
    values.set_item("unittest", py.import("unittest")?)?;
    let loader = py.import("unittest")?.getattr("TestLoader")?.call0()?;
    let mut cases = Vec::new();
    for (source, name, count) in [
        (sources[22], "InertItemJson", 3),
        (sources[23], "LoopbackCheck", 2),
    ] {
        let tests = crate::source_oracle_runtime::namespace(
            py,
            source,
            &[name],
            &["__build_class__"],
            &values,
        )?;
        let class = required(&tests, name)?;
        let methods: Vec<String> = loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        if methods.len() != count {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "original panel request collection drift",
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
                        "original panel request {name}.{method} reported {field}; retained"
                    )));
                }
            }
            cases.push(format!("{name}.{method}"));
        }
    }
    let preview = PyDict::new(py);
    let preview = crate::source_oracle_runtime::namespace(
        py,
        sources[26],
        &["page_tiers"],
        &["min"],
        &preview,
    )?;
    let api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    api.setattr("page_tiers", required(&preview, "page_tiers")?)?;
    values.set_item("panel_preview", api)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[25],
        &["TheWarmUpRendersBeforeTheScroll"],
        &["__build_class__"],
        &values,
    )?;
    // Only the pure tier method belongs here; real renderer/cancellation methods remain separate.
    let name = "TheWarmUpRendersBeforeTheScroll";
    let method = "test_the_tiers_follow_the_frame_rate_cap";
    required(&tests, name)?
        .call1((method,))?
        .call_method1("run", (result,))?;
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "original panel preview {name}.{method} reported {field}; retained"
            )));
        }
    }
    cases.push(format!("{name}.{method}"));

    let subprocess = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    let original_subprocess = py.import("subprocess")?;
    let options = PyDict::new(py);
    options.set_item(
        "side_effect",
        pyo3::exceptions::PyRuntimeError::new_err("unexpected original port diagnostic command")
            .value(py),
    )?;
    subprocess.setattr(
        "run",
        py.import("unittest.mock")?
            .getattr("Mock")?
            .call((), Some(&options))?,
    )?;
    subprocess.setattr(
        "SubprocessError",
        original_subprocess.getattr("SubprocessError")?,
    )?;
    let ports = PyDict::new(py);
    ports.set_item("subprocess", &subprocess)?;
    let diagnostic = crate::source_oracle_runtime::namespace(
        py,
        sources[11],
        &["_port_holder"],
        &["len", "int", "OSError", "ValueError"],
        &ports,
    )?;
    let panel = required(bindings, "p")?;
    panel.setattr("subprocess", &subprocess)?;
    panel.setattr("_port_holder", required(&diagnostic, "_port_holder")?)?;
    values.set_item("p", panel)?;
    values.set_item("mock", py.import("unittest.mock")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[23],
        &["TheBusyPortMessageNamesTheProcess"],
        &["__build_class__", "chr", "type", "OSError"],
        &values,
    )?;
    let class = required(&tests, "TheBusyPortMessageNamesTheProcess")?;
    let methods: Vec<String> = loader
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods.len() != 4 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "original panel port diagnostic collection drift",
        ));
    }
    let original_run = subprocess.getattr("run")?;
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
                    "original port diagnostic {method} reported {field}; retained"
                )));
            }
        }
        if !subprocess.getattr("run")?.is(&original_run) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "original diagnostic process patch not restored",
            ));
        }
        cases.push(format!("TheBusyPortMessageNamesTheProcess.{method}"));
    }
    let source_path = workspace.join("panel-source.txt");
    let mut staged = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&source_path)?;
    std::io::Write::write_all(&mut staged, sources[11].text.as_bytes())?;
    staged.sync_all()?;
    drop(staged);
    let module = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    module.setattr("__file__", &source_path)?;
    let syntax = PyDict::new(py);
    syntax.set_item("p_mod", &module)?;
    syntax.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    // This method reads source only; the inherited HTTP fixture belongs to other observations.
    syntax.set_item("MutationGuard", py.import("unittest")?.getattr("TestCase")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[34],
        &["RefreshMustActuallyRefresh"],
        &["__build_class__"],
        &syntax,
    )?;
    let class = required(&tests, "RefreshMustActuallyRefresh")?;
    let methods: Vec<String> = loader
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods
        != [
            "test_a_reload_sees_a_change_another_process_made",
            "test_the_reload_replaces_the_shared_view_in_place",
        ]
    {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "original reload source collection drift",
        ));
    }
    let method = "test_the_reload_replaces_the_shared_view_in_place";
    class.call1((method,))?.call_method1("run", (result,))?;
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "original reload source assertion reported {field}; retained"
            )));
        }
    }
    let metadata = std::fs::symlink_metadata(&source_path)?;
    if !metadata.is_file()
        || crate::tooling_environment::reject_links(&source_path, "linked panel source").is_err()
        || metadata.len() != sources[11].text.len() as u64
        || std::fs::read(&source_path)? != sources[11].text.as_bytes()
        || module
            .getattr("__file__")?
            .extract::<std::path::PathBuf>()?
            != source_path
    {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "original reload source bytes drift; retained",
        ));
    }
    std::fs::remove_file(source_path)?;
    cases.push(format!("RefreshMustActuallyRefresh.{method}"));
    Ok(cases)
}
