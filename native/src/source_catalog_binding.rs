//! Unchanged combined catalog assertion; private imports/patch targets never reach installed applications.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct Ports {
    package: Py<PyModule>,
    globals: Py<PyDict>,
    imports: AtomicUsize,
    patches: AtomicUsize,
}
#[pymethods]
impl Ports {
    fn import_module<'py>(
        slf: PyRef<'py, Self>,
        py: Python<'py>,
        name: &str,
        _globals: &Bound<'_, PyAny>,
        _locals: &Bound<'_, PyAny>,
        fromlist: &Bound<'_, PyAny>,
        level: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let names = fromlist.extract::<Vec<String>>()?;
        if level == 0 && name == "emojikit" && names == ["_native"] {
            slf.imports.fetch_add(1, Ordering::Relaxed);
            return Ok(slf.package.bind(py).clone().into_any());
        }
        if level == 0 && name == "unittest" && names == ["mock"] {
            let mock = crate::source_oracle_runtime::host_import(py, "types")?
                .getattr("SimpleNamespace")?
                .call0()?;
            mock.setattr("patch", slf.into_pyobject(py)?)?;
            let module = PyModule::new(py, "_catalog_binding_unittest")?;
            module.add("mock", mock)?;
            return Ok(module.into_any());
        }
        Err(PyImportError::new_err(
            "unexpected original catalog binding import",
        ))
    }

    #[pyo3(signature = (target, *, side_effect))]
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        target: &str,
        side_effect: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        if target != "tests.reference.catalog.catalog_identity" || !side_effect.is_callable() {
            return Err(PyValueError::new_err("unexpected original catalog patch"));
        }
        self.patches.fetch_add(1, Ordering::Relaxed);
        let options = PyDict::new(py);
        options.set_item("side_effect", side_effect)?;
        let mock = crate::source_oracle_runtime::host_import(py, "unittest.mock")?;
        let values = PyDict::new(py);
        values.set_item(
            "catalog_identity",
            mock.getattr("Mock")?.call((), Some(&options))?,
        )?;
        mock.getattr("patch")?
            .call_method1("dict", (self.globals.bind(py), values))
    }
}

pub(crate) fn check(
    py: Python<'_>,
    project: &Path,
    sources: &[&Source],
    original: &Bound<'_, PyDict>,
) -> PyResult<Value> {
    let native = PyModule::new(py, "_native_catalog_combined")?;
    crate::_native(&native)?;
    let package = PyModule::new(py, "_catalog_binding_package")?;
    package.add("_native", native)?;
    let ports = Py::new(
        py,
        Ports {
            package: package.unbind(),
            globals: PyDict::new(py).unbind(),
            imports: AtomicUsize::new(0),
            patches: AtomicUsize::new(0),
        },
    )?;
    let ports = ports.bind(py);
    let state = crate::source_reference_state::namespace(py, sources[12], None)?;
    let ownership = crate::source_reference_state::ownership(
        py,
        sources[11],
        &state,
        "_catalog_binding_ownership",
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("writer", ownership.getattr("writer")?)?;
    bindings.set_item("require_native", required(original, "require_native")?)?;
    let identity = required(original, "catalog_identity")?;
    bindings.set_item("catalog_identity", &identity)?;
    let catalog = crate::source_catalog_oracle::namespace_with_ports(
        py, sources[0], sources[1], project, &bindings,
    )?;
    // Methods close over this dictionary. Patching a copied module would not intercept them.
    ports.borrow_mut().globals = catalog.clone().unbind();
    let tests = PyDict::new(py);
    for name in ["dataclasses", "json", "tempfile", "unittest"] {
        tests.set_item(name, py.import(name)?)?;
    }
    let path = py.import("pathlib")?.getattr("Path")?;
    tests.set_item("Path", &path)?;
    tests.set_item("ROOT", path.call1((project,))?)?;
    tests.set_item("UTC", "2026-10-07 00:00:00 UTC")?;
    tests.set_item("Catalog", required(&catalog, "Catalog")?)?;
    tests.set_item("writer", ownership.getattr("writer")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[10],
        &["NativeCatalogParity"],
        &["__build_class__", "RuntimeError", "str", "list"],
        &tests,
    )?;
    required(&tests, "__builtins__")?
        .cast_into::<PyDict>()?
        .set_item("__import__", ports.getattr("import_module")?)?;
    let unittest = py.import("unittest")?;
    let suite = unittest.getattr("TestLoader")?.call0()?.call_method1(
        "loadTestsFromTestCase",
        (required(&tests, "NativeCatalogParity")?,),
    )?;
    let expected = "test_order_inclusion_signed_hash_and_family_publications_match_source";
    let methods = suite
        .try_iter()?
        .map(|test| test?.getattr("_testMethodName")?.extract::<String>())
        .collect::<PyResult<Vec<_>>>()?;
    if methods != [expected] {
        return Err(PyValueError::new_err(
            "original combined catalog collection drift",
        ));
    }
    let result = unittest.getattr("TestResult")?.call0()?;
    suite.call_method1("run", (&result,))?;
    for field in [
        "errors",
        "failures",
        "skipped",
        "expectedFailures",
        "unexpectedSuccesses",
    ] {
        if result.getattr(field)?.len()? != 0 {
            return Err(PyRuntimeError::new_err(format!(
                "original combined catalog reported {field}"
            )));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 1
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        || !required(&catalog, "catalog_identity")?.is(&identity)
        || !required(&catalog, "writer")?.is(&ownership.getattr("writer")?)
        || ownership.getattr("_owners")?.getattr("held")?.len()? != 0
    {
        return Err(PyRuntimeError::new_err(
            "original combined catalog restoration drift",
        ));
    }
    let calls = ports.borrow();
    if calls.imports.load(Ordering::Relaxed) != 1 || calls.patches.load(Ordering::Relaxed) != 1 {
        return Err(PyRuntimeError::new_err(
            "original combined catalog boundary accounting differs",
        ));
    }
    Ok(
        json!({"cases":[format!("NativeCatalogParity.{expected}")],"tests_run":1,
        "errors":0,"failures":0,"skipped":0,"native_imports":1,"identity_patches":1,
        "ports":"unchanged original class, original Catalog/maintenance writer/OS lock and registered NativeCatalog; private emojikit/unittest import, identity patch and ROOT; prerequisite is a recording port, not installation proof"}),
    )
}
