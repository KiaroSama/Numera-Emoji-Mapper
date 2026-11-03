//! Exact original alias/enhance module loader; shared original inventory and writer remain patchable.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct Loader {
    root: PathBuf,
    alias: Py<PyModule>,
    enhance: Py<PyModule>,
    remap: Py<PyModule>,
    calls: AtomicUsize,
}
#[pymethods]
impl Loader {
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        path: PathBuf,
        name: &str,
    ) -> PyResult<Bound<'py, PyModule>> {
        let module = if path == self.root.join("coins/alias_map.py")
            && matches!(
                name,
                "coins_alias_map" | "alias_map_lock" | "alias_map_busy"
            ) {
            &self.alias
        } else if path == self.root.join("coins/enhance_map.py")
            && matches!(name, "enhance_map_lock" | "enhance_map_busy")
        {
            &self.enhance
        } else if path == self.root.join("coins/remap_ids.py") && name == "remap_ids_lock" {
            &self.remap
        } else {
            return Err(PyValueError::new_err("unexpected original map module load"));
        };
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(module.bind(py).clone())
    }
    #[getter]
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

pub(super) fn build<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
    bindings: &Bound<'py, PyDict>,
    remap: &Bound<'py, PyModule>,
) -> PyResult<Bound<'py, PyDict>> {
    let globals = bindings.copy()?;
    globals.set_item("re", py.import("re")?)?;
    globals.set_item("EXIT_FAILED", 4)?;
    globals.set_item("EXIT_OK", 0)?;
    globals.set_item("sys", py.import("sys")?)?;
    globals.set_item("os", required(bindings, "fp")?.getattr("os")?)?;
    globals.set_item("mock", py.import("unittest.mock")?)?;
    let helpers = bindings.copy()?;
    helpers.set_item("random", py.import("random")?)?;
    let helper = crate::source_oracle_runtime::namespace(
        py,
        sources[23],
        &["_noise_png_bytes"],
        &["range"],
        &helpers,
    )?;
    globals.set_item("_noise_png_bytes", required(&helper, "_noise_png_bytes")?)?;
    let stub = crate::source_oracle_runtime::namespace(
        py,
        sources[16],
        &["StubBot"],
        &["__build_class__"],
        &globals,
    )?;
    globals.set_item("StubBot", required(&stub, "StubBot")?)?;
    let path = required(bindings, "Path")?;
    globals.set_item("ROOT", path.call1((workspace,))?)?;
    globals.set_item(
        "INV",
        path.call1((workspace.join("unpatched-map-inv.md"),))?,
    )?;
    globals.set_item(
        "OUT_INV",
        path.call1((workspace.join("unpatched-map-out.md"),))?,
    )?;
    globals.set_item(
        "KEYWORDS_CSV",
        path.call1((workspace.join("unpatched-map-keywords.csv"),))?,
    )?;
    let inventory = required(bindings, "_inventory")?;
    for name in ["parse_missing", "refill_inventory", "base_ticker"] {
        globals.set_item(name, inventory.getattr(name)?)?;
    }
    globals.set_item(
        "DROP",
        crate::source_logging_oracle::literal(py, sources[17], "DROP", false)?,
    )?;
    let alias = crate::source_oracle_runtime::namespace(
        py,
        sources[17],
        &["norm", "main"],
        &["open", "set", "sorted", "len", "print"],
        &globals,
    )?;
    let alias = crate::source_roster_modules::rebind(py, &alias, "_original_alias_map")?;
    let enhance = crate::source_oracle_runtime::namespace(
        py,
        sources[18],
        &["main"],
        &["set", "sorted", "print"],
        &globals,
    )?;
    let enhance = crate::source_roster_modules::rebind(py, &enhance, "_original_enhance_map")?;
    globals.set_item("map_alias", &alias)?;
    globals.set_item("map_enhance", &enhance)?;
    let loader = Py::new(
        py,
        Loader {
            root: workspace.into(),
            alias: alias.unbind(),
            enhance: enhance.unbind(),
            remap: remap.clone().unbind(),
            calls: AtomicUsize::new(0),
        },
    )?;
    globals.set_item("_load_standalone", loader.bind(py))?;
    globals.set_item("map_loader", loader.bind(py))?;
    globals.set_item(
        "INVENTORY",
        crate::source_logging_oracle::literal(py, sources[16], "INVENTORY", false)?,
    )?;
    Ok(globals)
}
