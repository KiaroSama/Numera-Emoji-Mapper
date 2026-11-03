//! Unchanged original panel assertions in the shared historical worker, never a native-answer oracle.
use crate::source_git_inventory::Source;
use crate::source_temporary::Directories;
use crate::source_tooling_oracle::{directories, observed, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Mutex, atomic::Ordering};

use crate::source_panel_identity::{ASSETS, BASELINE, IDENTITIES, script};
pub use crate::source_panel_identity::{source_identity, source_paths};
#[path = "source_panel_entry_tests.rs"]
mod entry_tests;
const CLASSES: [(&str, usize); 5] = [
    ("BrandLogoPreview", 4),
    ("SimilarityOrder", 2),
    ("CopyTheEmojiId", 4),
    ("PublishedEmojiLeaveTheGrid", 5),
    ("OnePackCanBeUnhidden", 8),
];
fn modules<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
    environment: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyDict>> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let path = py.import("pathlib")?.getattr("Path")?;
    let images = crate::source_image_identity::build(py, sources[8], sources[7])?;
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[6],
        &["MediaError"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let greedy = crate::source_oracle_runtime::namespace(
        py,
        sources[5],
        &["greedy_indices", "near_indices"],
        &["list", "range", "len", "enumerate"],
        &PyDict::new(py),
    )?;
    let bindings = PyDict::new(py);
    bindings.set_item("Path", &path)?;
    bindings.set_item("identity", &images.identity)?;
    bindings.set_item("near_indices", required(&greedy, "near_indices")?)?;
    bindings.set_item("MediaError", required(&errors, "MediaError")?)?;
    let ingest = crate::source_oracle_runtime::namespace(
        py,
        sources[4],
        &["catalog_identity", "equivalent_files"],
        &["set", "enumerate", "len", "OSError"],
        &bindings,
    )?;
    let ports = PyDict::new(py);
    let mock = py.import("unittest.mock")?;
    let none = PyDict::new(py);
    none.set_item("return_value", py.None())?;
    ports.set_item(
        "require_native",
        mock.getattr("Mock")?.call((), Some(&none))?,
    )?;
    let context = PyDict::new(py);
    context.set_item(
        "return_value",
        py.import("contextlib")?.getattr("nullcontext")?.call0()?,
    )?;
    ports.set_item("writer", mock.getattr("Mock")?.call((), Some(&context))?)?;
    ports.set_item("catalog_identity", required(&ingest, "catalog_identity")?)?;
    ports.set_item("log", &images.log)?;
    let catalog = crate::source_catalog_oracle::namespace_with_ports(
        py, sources[2], sources[3], workspace, &ports,
    )?;
    let os = ns.call0()?;
    os.setattr("environ", environment)?;
    bindings.set_item("os", &os)?;
    bindings.set_item("ROOT", path.call1((workspace,))?)?;
    let operator = crate::source_oracle_runtime::namespace(
        py,
        sources[9],
        &["value", "brand_logo_bots", "brand_logo_path"],
        &["frozenset"],
        &bindings,
    )?;
    let operator_api = ns.call0()?;
    for name in ["brand_logo_bots", "brand_logo_path"] {
        operator_api.setattr(name, required(&operator, name)?)?;
    }
    let view_bindings = PyDict::new(py);
    view_bindings.set_item("Path", &path)?;
    view_bindings.set_item("json", py.import("json")?)?;
    view_bindings.set_item("operator_config", operator_api)?;
    view_bindings.set_item("greedy_indices", required(&greedy, "greedy_indices")?)?;
    view_bindings.set_item("log", &images.log)?;
    for name in ["FMT_ORDER", "LOGO_KEY"] {
        view_bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[1], name, false)?,
        )?;
    }
    let pattern = crate::source_logging_oracle::literal(py, sources[1], "_PREMIUM_ID", true)?;
    view_bindings.set_item(
        "_PREMIUM_ID",
        py.import("re")?.call_method1("compile", (pattern,))?,
    )?;
    let view = crate::source_oracle_runtime::module(
        py,
        sources[1],
        &[
            "order_by_similarity",
            "copy_id_for",
            "packs_named",
            "build_view",
        ],
        &["sorted", "set", "len", "int", "OSError", "ValueError"],
        &view_bindings,
        "_original_panel_view",
    )?;
    let output = PyDict::new(py);
    output.set_item("pv", &view)?;
    output.set_item("Catalog", required(&catalog, "Catalog")?)?;
    output.set_item("hamming", required(&images.globals, "hamming")?)?;
    output.set_item("os", os)?;
    let helper = PyDict::new(py);
    helper.set_item("Image", py.import("PIL.Image")?)?;
    let helper =
        crate::source_oracle_runtime::namespace(py, sources[10], &["_make_png"], &[], &helper)?;
    output.set_item("_make_png", required(&helper, "_make_png")?)?;
    Ok(output)
}
fn random_orders(py: Python<'_>, tests: &Bound<'_, PyDict>) -> PyResult<Value> {
    let class = required(tests, "SimilarityOrder")?;
    let instance = class.call0()?;
    let output = PyList::empty(py);
    for seed in [1, 1234, 99999] {
        let items = instance
            .call_method1("_items", (seed,))?
            .cast_into::<PyList>()?;
        let input = PyList::empty(py);
        for (index, item) in items.iter().enumerate() {
            let row = PyDict::new(py);
            row.set_item("fmt", item.getattr("fmt")?)?;
            row.set_item("phash", item.getattr("phash")?)?;
            row.set_item("identity", index)?;
            input.append(row)?;
        }
        let original = required(tests, "_reference_order")?.call1((&items,))?;
        let expected = PyList::empty(py);
        for item in original.try_iter()? {
            let item = item?;
            let index = items
                .iter()
                .position(|candidate| candidate.is(&item))
                .ok_or_else(|| {
                    PyValueError::new_err("original ordering fabricated an item identity")
                })?;
            expected.append(index)?;
        }
        let row = PyDict::new(py);
        row.set_item("seed", seed)?;
        row.set_item("items", input)?;
        row.set_item("expected", expected)?;
        output.append(row)?;
    }
    observed(output.as_any())
}
fn execute(
    py: Python<'_>,
    root: &Path,
    workspace: &Path,
    sources: &[&Source],
    scripts: &str,
) -> PyResult<Value> {
    let environment = PyDict::new(py);
    let bindings = modules(py, workspace, sources, &environment)?;
    let ledger = Py::new(
        py,
        Directories {
            root: workspace.into(),
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let tempfile = ns.call0()?;
    tempfile.setattr("TemporaryDirectory", ledger.bind(py).getattr("temporary")?)?;
    bindings.set_item("tempfile", tempfile)?;
    for name in ["unittest", "json", "random"] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item("mock", py.import("unittest.mock")?)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("dataclass", py.import("dataclasses")?.getattr("dataclass")?)?;
    let panel = ns.call0()?;
    let pv = required(&bindings, "pv")?;
    for name in ["build_view", "packs_named"] {
        panel.setattr(name, pv.getattr(name)?)?;
    }
    let files: Vec<String> =
        crate::source_logging_oracle::literal(py, sources[11], "SCRIPT_FILES", false)?.extract()?;
    if files != ASSETS.map(|(name, _)| name) {
        return Err(PyValueError::new_err("original panel SCRIPT_FILES drift"));
    }
    panel.setattr("SCRIPT", scripts)?;
    panel.setattr(
        "SCRIPT_FILES",
        crate::source_logging_oracle::literal(py, sources[11], "SCRIPT_FILES", false)?,
    )?;
    let assets = root.join("assets");
    panel.setattr(
        "ASSET_DIR",
        py.import("pathlib")?.getattr("Path")?.call1((&assets,))?,
    )?;
    panel.setattr(
        "PAGE",
        std::fs::read_to_string(assets.join("panel.html"))?
            .replace("\r\n", "\n")
            .replace('\r', "\n"),
    )?;
    let hashlib = py.import("hashlib")?;
    let digest = |bytes: &[u8]| -> PyResult<String> {
        hashlib
            .call_method1("sha1", (PyBytes::new(py, bytes),))?
            .call_method0("hexdigest")?
            .extract()
    };
    panel.setattr("ASSET_VER", &digest(scripts.as_bytes())?[..12])?;
    panel.setattr(
        "ICON_VER",
        &digest(&std::fs::read(assets.join("logo-128.png"))?)?[..12],
    )?;
    bindings.set_item("hashlib", hashlib)?;
    bindings.set_item(
        "ROOT",
        py.import("pathlib")?.getattr("Path")?.call1((root,))?,
    )?;
    bindings.set_item("p", panel)?;
    let mut definitions = vec!["_Fake", "_reference_order"];
    definitions.extend(CLASSES.map(|(name, _)| name));
    let mut registration = crate::source_oracle_runtime::ModuleRegistration::new(py, &bindings)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &definitions,
        &[
            "__build_class__",
            "range",
            "str",
            "len",
            "all",
            "sorted",
            "set",
            "min",
            "id",
            "enumerate",
        ],
        &bindings,
    );
    registration.restore()?;
    let tests = tests?;
    let unittest = py.import("unittest")?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    for (name, count) in CLASSES {
        let class = required(&tests, name)?;
        let methods: Vec<String> = loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract()?;
        if methods.len() != count {
            return Err(PyValueError::new_err(
                "original panel view collection drift",
            ));
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
                        "original panel {name}.{method} reported {field}; retained"
                    )));
                }
            }
            if !environment.is_empty() {
                return Err(PyValueError::new_err(
                    "original panel environment patch was not restored",
                ));
            }
            cases.push(format!("{name}.{method}"));
        }
    }
    let asset_tests = crate::source_oracle_runtime::namespace(
        py,
        sources[12],
        &["ThePanelScriptsShip"],
        &["__build_class__", "str", "zip"],
        &bindings,
    )?;
    let class = required(&asset_tests, "ThePanelScriptsShip")?;
    let methods: Vec<String> = loader
        .call_method1("getTestCaseNames", (&class,))?
        .extract()?;
    if methods.len() != 5 {
        return Err(PyValueError::new_err(
            "original panel asset collection drift",
        ));
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
                    "original panel assets {method} reported {field}; retained"
                )));
            }
        }
        cases.push(format!("ThePanelScriptsShip.{method}"));
    }
    cases.extend(crate::source_panel_plan_tests::run(
        py, sources, &bindings, &result,
    )?);
    cases.extend(crate::source_panel_page_tests::run(
        py, sources, &bindings, &result,
    )?);
    cases.extend(crate::source_panel_logging_tests::run(
        py, sources, &bindings, &result,
    )?);
    cases.extend(crate::source_panel_request_tests::run(
        py, workspace, sources, &bindings, &result,
    )?);
    cases.extend(crate::source_panel_scope_tests::run(
        py, sources, &bindings, &result,
    )?);
    cases.extend(crate::source_panel_intent_tests::run(
        py, sources, &bindings, &result,
    )?);
    cases.extend(crate::source_panel_concurrency_tests::run(
        py, sources, &bindings, &result,
    )?);
    cases.extend(entry_tests::run(
        py,
        workspace,
        sources,
        ledger.bind(py).as_any(),
        &result,
    )?);
    if result.getattr("testsRun")?.extract::<usize>()? != 146
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original panel assertions incomplete",
        ));
    }
    let orders = random_orders(py, &tests)?;
    let copy_labels = [
        "premium-id:۱۲۳۴۵",
        "premium-id:１２３４５",
        "premium-id:123\n",
        "premium-id:123\r\n",
        "premium-id:²",
        "premium-id:123\n\n",
    ];
    let mut copies = Vec::new();
    for label in copy_labels {
        let id: String = pv.call_method1("copy_id_for", (label,))?.extract()?;
        copies.push(json!({"label":label,"expected":id}));
    }
    let ledger = ledger.borrow(py);
    let paths = ledger
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("panel temporary accounting poisoned"))?;
    if paths.len() != 52
        || paths
            .iter()
            .any(|(_, cleaned)| !cleaned.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original panel teardown incomplete; retained",
        ));
    }
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    let expected = paths
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
    if actual != expected || crate::process_owner::cleanup_pending(workspace) {
        return Err(PyValueError::new_err(
            "unexpected original panel scratch; retained",
        ));
    }
    let mut kinds = [0; 10];
    for (path, _) in paths.iter() {
        directories(workspace, path, &mut 64, 0, &mut Vec::new())?;
        let names = |directory: &Path| -> PyResult<BTreeSet<String>> {
            std::fs::read_dir(directory)?
                .map(|entry| {
                    entry?.file_name().into_string().map_err(|_| {
                        PyValueError::new_err("original panel fixture name is not UTF-8")
                    })
                })
                .collect()
        };
        let top = names(path)?;
        if top.is_empty() {
            kinds[9] += 1;
            continue;
        }
        if top == BTreeSet::from(["catalog.db".into()]) {
            if !path.join("catalog.db").is_file() {
                return Err(PyValueError::new_err(
                    "original connection catalog missing; retained",
                ));
            }
            kinds[7] += 1;
            continue;
        }
        if top.contains(".maintenance.lock") {
            let mut expected = ["catalog.db", ".maintenance.lock"]
                .map(str::to_owned)
                .into_iter()
                .collect::<BTreeSet<_>>();
            expected.extend((0..5).map(|i| format!("{i}.png")));
            if top.contains("pack_plan.json") {
                expected.insert("pack_plan.json".into());
            }
            if top != expected || expected.iter().any(|name| !path.join(name).is_file()) {
                return Err(PyValueError::new_err(
                    "unexpected original panel intent fixture members; retained",
                ));
            }
            kinds[6] += 1;
            continue;
        }
        if top == BTreeSet::from(["pack_plan.json".into()]) {
            if !path.join("pack_plan.json").is_file() {
                return Err(PyValueError::new_err(
                    "original panel plan file absent; retained",
                ));
            }
            kinds[5] += 1;
            continue;
        }
        let mut expected = ["catalog.db", "media"]
            .map(str::to_owned)
            .into_iter()
            .collect::<BTreeSet<_>>();
        let (kind, images) = if top.contains("logo.png") {
            expected.insert("logo.png".into());
            (0, (0..3).map(|i| format!("i{i}.png")).collect())
        } else if top.contains("brand.png") {
            expected.extend(["brand.png".into(), "publish_pk.json".into()]);
            (1, (0..6).map(|i| format!("i{i}.png")).collect())
        } else {
            let images = names(&path.join("media/static"))?;
            let kind = match images.len() {
                2 => 2,
                3 => 3,
                4 => 8,
                5 => 4,
                _ => {
                    return Err(PyValueError::new_err(
                        "original panel image count drift; retained",
                    ));
                }
            };
            let expected_images = if kind == 2 {
                ["a.png", "b.png"].map(str::to_owned).into_iter().collect()
            } else {
                (0..images.len()).map(|i| format!("i{i}.png")).collect()
            };
            (kind, expected_images)
        };
        if top != expected
            || names(&path.join("media"))? != BTreeSet::from(["static".into()])
            || names(&path.join("media/static"))? != images
            || !path.join("catalog.db").is_file()
        {
            return Err(PyValueError::new_err(
                "unexpected original panel fixture members; retained",
            ));
        }
        kinds[kind] += 1;
    }
    if kinds != [3, 8, 1, 9, 5, 1, 17, 4, 2, 2] {
        return Err(PyValueError::new_err(
            "original panel context inventory drift; retained",
        ));
    }
    for (path, _) in paths.iter() {
        std::fs::remove_dir_all(path)?;
    }
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":"12.3.0",
        "source_hashes":IDENTITIES.into_iter().collect::<BTreeMap<_,_>>(),"tests_run":146,"temporary_calls":52,
        "cases":cases,"orders":orders,"copies":copies,"errors":0,"failures":0,"skipped":0,"expected_failures":0,"unexpected_successes":0,
        "excluded":["PanelIntentPersistence.test_actual_video_plan_survives_migration_reload_and_restore"],
        "ports":"unchanged original23view+5asset+12plan+59served-text+2telemetry+5pure-request+4port-diagnostic+1preview-tier+8real-HTTP-scope+4real-HTTP-connection+17real-HTTP-intent+2event-pinned-reorder+1original-shared-map-reader+1source-only-reload+2unchanged in-process main/signature methods/setup/teardown/reference walk; main uses private no-op dotenv/logging ports and exact import facade, no Python signature claim for Rust; real original catalog/ingest/identity/greedy/Pillow/hashlib/plan/atomic writer; intent cohort real original writer/maintenance ownership and logical signatures; private operator environment; no-op native prerequisite and other cohorts catalog writer, delayed existing temporary ledger; pinned unchanged scripts/page/icon; served-text is not browser acceptance; original video migration replay excluded explicitly; no native result oracle"}),
    )
}
pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("panel view workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect panel view workspace")?
        .next()
        .is_some()
    {
        return Err("panel view workspace must be empty".into());
    }
    let scripts = script(&root)?;
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect panel view bytecode policy")?
        {
            return Err("panel view replay requires disabled import bytecode writes".into());
        }
        execute(py, &root, &workspace, sources, &scripts).map_err(|error| {
            format!(
                "original panel view replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })
}
pub async fn observe_owned(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    sources: &[&Source],
) -> Result<Value, String> {
    source_identity(sources)?;
    script(root)?;
    crate::source_oracle_process::check(
        root,
        executable,
        "panel-view-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}
