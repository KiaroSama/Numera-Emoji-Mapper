//! The seven original shared-report assertions reuse the roster corpus, workspace and teardown ledger.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct Identity {
    workspace: PathBuf,
    calls: Arc<AtomicUsize>,
}
#[pymethods]
impl Identity {
    fn __call__(
        &self,
        _catalog: &Bound<'_, PyAny>,
        key: &str,
        format: &str,
        path: PathBuf,
        phash: Option<u64>,
    ) -> PyResult<(String, bool)> {
        crate::tooling_environment::reject_links(&path, "linked original report image")
            .map_err(PyValueError::new_err)?;
        let parent = crate::tooling_environment::physical_root(
            path.parent()
                .ok_or_else(|| PyValueError::new_err("report image has no parent"))?,
        )
        .map_err(PyValueError::new_err)?;
        if key != "s:a"
            || format != "static"
            || phash.is_some()
            || !parent.starts_with(&self.workspace)
            || !path.ends_with("media/a.png")
            || !path.is_file()
            || path.metadata()?.len() > 65536
            || self.calls.fetch_add(1, Ordering::SeqCst) != 0
        {
            return Err(PyValueError::new_err(
                "unexpected original report catalog identity",
            ));
        }
        Ok((key.into(), true))
    }
}
#[pyclass]
struct Imports {
    native: Py<PyAny>,
    source: Py<PyAny>,
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
            if name == "json" && fromlist.is_none() {
                return Ok(crate::source_oracle_runtime::host_import(py, "json")?.into_any());
            }
            if name == "emojikit" && fromlist.extract::<Vec<String>>()? == ["_native"] {
                return Ok(self.native.bind(py).clone());
            }
            if name == "tests.oracles" && fromlist.extract::<Vec<String>>()? == ["pack_rows"] {
                return Ok(self.source.bind(py).clone());
            }
        }
        Err(PyImportError::new_err(
            "unexpected original shared-report import",
        ))
    }
}

pub(crate) fn run(
    py: Python<'_>,
    workspace: &Path,
    sources: &[&Source],
    roster: &Bound<'_, PyModule>,
    temporary: &Bound<'_, PyAny>,
) -> PyResult<Value> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let bindings = PyDict::new(py);
    bindings.set_item("EMPTY", "—")?;
    let rows = crate::source_oracle_runtime::namespace(
        py,
        sources[3],
        &["_cell", "markdown_table", "keyword_cell"],
        &["str", "len", "max"],
        &bindings,
    )?;
    let source = ns.call0()?;
    source.setattr("pack_rows", ns.call0()?)?;
    for name in ["markdown_table", "keyword_cell"] {
        source
            .getattr("pack_rows")?
            .setattr(name, required(&rows, name)?)?;
    }
    let module = PyModule::new(py, "_report_original_registered_native")?;
    crate::_native(&module)?;
    let native = ns.call0()?;
    native.setattr("_native", module)?;
    let imports = Py::new(
        py,
        Imports {
            native: native.unbind(),
            source: source.unbind(),
        },
    )?;
    let wrapper = crate::source_oracle_runtime::namespace(
        py,
        sources[17],
        &["markdown_table", "keyword_cell"],
        &["str", "len"],
        &PyDict::new(py),
    )?;
    required(&wrapper, "__builtins__")?.set_item("__import__", &imports)?;
    let pack_rows = ns.call0()?;
    for name in ["markdown_table", "keyword_cell"] {
        pack_rows.setattr(name, required(&wrapper, name)?)?;
        bindings.set_item(name, required(&rows, name)?)?;
    }
    bindings.set_item(
        "log",
        py.import("logging")?
            .call_method1("getLogger", ("original_reports",))?,
    )?;
    let publisher = crate::source_oracle_runtime::namespace(
        py,
        sources[13],
        &["write_manifest"],
        &["enumerate", "len"],
        &bindings,
    )?;
    let publisher_api = ns.call0()?;
    publisher_api.setattr("write_manifest", required(&publisher, "write_manifest")?)?;
    let archive = crate::source_archive_oracle::namespace(py, sources[14], sources[3])?;
    let archive_api = ns.call0()?;
    archive_api.setattr(
        "render_manifest_md",
        required(&archive, "render_manifest_md")?,
    )?;
    let coin = crate::source_oracle_runtime::namespace(
        py,
        sources[15],
        &["render_pack_md"],
        &["enumerate", "str", "sorted", "len"],
        &bindings,
    )?;
    let coin_api = ns.call0()?;
    coin_api.setattr("render_pack_md", required(&coin, "render_pack_md")?)?;
    let calls = Arc::new(AtomicUsize::new(0));
    let prerequisite = py.import("unittest.mock")?.getattr("Mock")?.call0()?;
    let ports = PyDict::new(py);
    ports.set_item("require_native", &prerequisite)?;
    ports.set_item("writer", py.import("contextlib")?.getattr("nullcontext")?)?;
    ports.set_item(
        "catalog_identity",
        Py::new(
            py,
            Identity {
                workspace: workspace.into(),
                calls: calls.clone(),
            },
        )?,
    )?;
    let catalog = crate::source_catalog_oracle::namespace_with_ports(
        py,
        sources[16],
        sources[7],
        workspace,
        &ports,
    )?;
    let image_bindings = PyDict::new(py);
    image_bindings.set_item("Image", py.import("PIL.Image")?)?;
    let image = crate::source_oracle_runtime::namespace(
        py,
        sources[9],
        &["make_png"],
        &[],
        &image_bindings,
    )?;
    let tests_bindings = PyDict::new(py);
    tests_bindings.set_item("HOSTILE", "left|right\nnext line")?;
    tests_bindings.set_item("ESCAPED", "left\\|right next line")?;
    tests_bindings.set_item("unittest", py.import("unittest")?)?;
    tests_bindings.set_item("tempfile", temporary)?;
    tests_bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    tests_bindings.set_item("pack_rows", pack_rows)?;
    tests_bindings.set_item("collection_notify", publisher_api)?;
    tests_bindings.set_item("pack_archive", archive_api)?;
    tests_bindings.set_item("pack_manifest", roster)?;
    tests_bindings.set_item("write_manifests", coin_api)?;
    tests_bindings.set_item("Catalog", required(&catalog, "Catalog")?)?;
    tests_bindings.set_item("make_png", required(&image, "make_png")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[12],
        &["TheSharedRenderer", "EveryWriterUsesIt"],
        &["__build_class__", "set"],
        &tests_bindings,
    )?;
    required(&tests, "__builtins__")?.set_item("__import__", imports)?;
    let unittest = py.import("unittest")?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    for (name, count) in [("TheSharedRenderer", 3), ("EveryWriterUsesIt", 4)] {
        let class = required(&tests, name)?;
        let methods: Vec<String> = unittest
            .getattr("TestLoader")?
            .call0()?
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        if methods.len() != count {
            return Err(PyValueError::new_err("original report collection drift"));
        }
        for method in methods {
            class.call1((&method,))?.call_method1("run", (&result,))?;
            for field in [
                "errors",
                "failures",
                "skipped",
                "expectedFailures",
                "unexpectedSuccesses",
            ] {
                if result.getattr(field)?.len()? != 0 {
                    return Err(PyValueError::new_err(format!(
                        "original report {name}.{method} reported {field}: {}; retained",
                        crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                    )));
                }
            }
            cases.push(format!("{name}.{method}"));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 7
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || prerequisite.getattr("call_count")?.extract::<usize>()? != 1
        || calls.load(Ordering::SeqCst) != 1
    {
        return Err(PyValueError::new_err(
            "original report execution incomplete; retained",
        ));
    }
    Ok(
        json!({"tests_run":7,"cases":cases,"errors":0,"failures":0,"skipped":0,
        "expected_failures":0,"unexpected_successes":0,"identity_calls":1,
        "ports":"original seven assertions/wrappers/registered binding/four writer bodies/real SQLite/Pillow; catalog prerequisite and lease are no-op, one exact single-insert identity port, no dedup/locking proof; roster owns delayed checked cleanup"}),
    )
}
