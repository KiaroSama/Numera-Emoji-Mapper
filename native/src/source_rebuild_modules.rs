//! Private unchanged rebuild modules; the fixed cohort admits each source before assembly.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct Imports {
    package: Py<PyAny>,
    coins: Option<Py<PyAny>>,
    calls: AtomicUsize,
    maps: AtomicUsize,
    refused: AtomicUsize,
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
        let requested: Vec<String> = fromlist.extract()?;
        if level == 0 && name == "emojikit" && requested == ["identity", "media"] {
            self.calls.fetch_add(1, Ordering::SeqCst);
            return Ok(self.package.bind(py).clone());
        }
        if level == 0
            && name == "coins"
            && requested == ["_dedup_map"]
            && let Some(coins) = &self.coins
        {
            self.maps.fetch_add(1, Ordering::SeqCst);
            return Ok(coins.bind(py).clone());
        }
        self.refused.fetch_add(1, Ordering::SeqCst);
        Err(pyo3::exceptions::PyImportError::new_err(
            "unexpected original rebuild application import",
        ))
    }
    #[getter]
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
    #[getter]
    fn maps(&self) -> usize {
        self.maps.load(Ordering::SeqCst)
    }
    #[getter]
    fn refused(&self) -> usize {
        self.refused.load(Ordering::SeqCst)
    }
}

pub(super) struct Modules<'py> {
    pub cfg: Bound<'py, PyModule>,
    pub rebuild: Bound<'py, PyModule>,
    pub map: Bound<'py, PyModule>,
    pub state: Bound<'py, PyModule>,
    pub locks: Bound<'py, PyDict>,
    pub bindings: Bound<'py, PyDict>,
    pub clock: Bound<'py, PyAny>,
    pub refusal: Bound<'py, PyAny>,
    pub imports: Bound<'py, PyAny>,
}

pub(super) fn build<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
    temporary: &Bound<'py, PyAny>,
) -> PyResult<Modules<'py>> {
    let bindings = PyDict::new(py);
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let path = py.import("pathlib")?.getattr("Path")?;
    let image = py.import("PIL.Image")?;
    if image.getattr("__version__")?.extract::<String>()? != "12.3.0" {
        return Err(PyValueError::new_err(
            "original rebuild assertions require Pillow12.3.0",
        ));
    }
    for name in ["json", "csv", "hashlib", "re", "contextlib", "io", "random"] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item("Path", &path)?;
    bindings.set_item("Image", image)?;
    bindings.set_item("ImageChops", py.import("PIL.ImageChops")?)?;
    bindings.set_item(
        "defaultdict",
        py.import("collections")?.getattr("defaultdict")?,
    )?;
    bindings.set_item("tempfile", temporary)?;
    bindings.set_item("unittest", py.import("unittest")?)?;
    bindings.set_item("mock", py.import("unittest.mock")?)?;
    bindings.set_item("importlib", py.import("importlib")?)?;
    bindings.set_item("ROOT", path.call1((workspace,))?)?;
    let locks =
        crate::source_reference_state::namespace(py, sources[2], Some(&workspace.join(".locks")))?;
    let state = crate::source_roster_modules::rebind(py, &locks, "_original_rebuild_state")?;
    bindings.set_item("ps", &state)?;
    for name in [
        "exclusive_lock",
        "pack_family_lock_path",
        "write_json_atomic",
    ] {
        bindings.set_item(name, state.getattr(name)?)?;
    }
    let blocked = PyDict::new(py);
    blocked.set_item(
        "side_effect",
        py.import("builtins")?
            .getattr("AssertionError")?
            .call1(("unpatched original rebuild effect",))?,
    )?;
    let refusal = py
        .import("unittest.mock")?
        .getattr("Mock")?
        .call((), Some(&blocked))?;
    let clock = ns.call0()?;
    clock.setattr("sleep", &refusal)?;
    clock.setattr("monotonic", py.import("time")?.getattr("monotonic")?)?;
    bindings.set_item("time", &clock)?;
    bindings.set_item("redact", py.import("builtins")?.getattr("str")?)?;
    bindings.set_item("EXIT_OK", 0)?;
    bindings.set_item("EXIT_PARTIAL", 3)?;
    let images = crate::source_image_identity::build(py, sources[3], sources[4])?;
    for name in ["_dhash", "hamming"] {
        bindings.set_item(name, required(&images.globals, name)?)?;
    }
    bindings.set_item("media", &images.media)?;
    let package = ns.call0()?;
    package.setattr("identity", images.identity)?;
    package.setattr("media", images.media)?;
    bindings.set_item("Enum", py.import("enum")?.getattr("Enum")?)?;
    let protocol = crate::source_oracle_runtime::namespace(
        py,
        sources[5],
        &[
            "SetState",
            "LiveStateUnknown",
            "BotApiError",
            "AmbiguousUploadError",
        ],
        &["__build_class__", "RuntimeError", "super"],
        &bindings,
    )?;
    for name in [
        "SetState",
        "LiveStateUnknown",
        "BotApiError",
        "AmbiguousUploadError",
    ] {
        bindings.set_item(name, required(&protocol, name)?)?;
    }
    let log = ns.call0()?;
    for name in ["info", "warning", "error", "debug"] {
        log.setattr(name, &refusal)?;
    }
    bindings.set_item("log", log)?;
    let os = ns.call0()?;
    os.setattr("environ", PyDict::new(py))?;
    bindings.set_item("os", os)?;
    bindings.set_item(
        "FloodWaitTooLong",
        py.import("builtins")?.getattr("RuntimeError")?,
    )?;
    let client = crate::source_telegram_namespace::namespace(py, sources[5], &bindings)?;
    let imports = Py::new(
        py,
        Imports {
            package: package.clone().unbind(),
            coins: None,
            calls: AtomicUsize::new(0),
            maps: AtomicUsize::new(0),
            refused: AtomicUsize::new(0),
        },
    )?
    .into_bound(py)
    .into_any();
    required(&client, "__builtins__")?.set_item("__import__", &imports)?;
    let tg_api = ns.call0()?;
    for name in [
        "SetState",
        "LiveStateUnknown",
        "BotApiError",
        "AmbiguousUploadError",
        "Telegram",
    ] {
        tg_api.setattr(name, required(&client, name)?)?;
    }
    bindings.set_item("tg_api", &tg_api)?;
    bindings.set_item("SourceTelegram", required(&client, "Telegram")?)?;
    let announce = crate::source_oracle_runtime::module(
        py,
        sources[6],
        &[
            "links_chat_id",
            "worker_publish_url",
            "announce_via_worker",
            "announce_packs",
        ],
        &["RuntimeError", "str", "int"],
        &bindings,
        "_original_rebuild_announce",
    )?;
    let requests = ns.call0()?;
    requests.setattr("post", &refusal)?;
    announce.setattr("requests", requests)?;
    bindings.set_item("source_announce", &announce)?;
    bindings.set_item("BASE", "cryptoemoji")?;
    bindings.set_item("TITLE", "Fixture")?;
    bindings.set_item("USER_ID", 0)?;
    for name in [
        "EMOJI_CHAR",
        "PER_SET",
        "SAME_IMAGE_MAX",
        "PERMANENT_MEDIA_ERRORS",
        "SHARED_GROUP_LIMIT",
    ] {
        bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[1], name, false)?,
        )?;
    }
    for name in [
        "EMOJI",
        "STATE",
        "PLAN",
        "OLD_STATE",
        "GROUPS_REPORT",
        "INV",
        "OUT_INV",
        "TICKER_IDS",
        "KEYWORDS_CSV",
    ] {
        bindings.set_item(
            name,
            path.call1((workspace.join(format!("unpatched-{name}")),))?,
        )?;
    }
    bindings.set_item(
        "LOCK",
        state.call_method1("pack_family_lock_path", ("cryptoemoji",))?,
    )?;
    let cfg = crate::source_oracle_runtime::module(
        py,
        sources[1],
        &[
            "img_hash",
            "is_blank",
            "load_keywords",
            "inventory_tickers",
            "build_plan",
            "_plan_is_sound",
            "load_plan",
            "_count",
            "_order_walks_plan",
            "_state_problem",
            "load_state",
            "save_state",
            "_stop_retryable",
        ],
        &[
            "str",
            "int",
            "bool",
            "list",
            "dict",
            "set",
            "isinstance",
            "all",
            "sorted",
            "len",
            "enumerate",
            "iter",
            "next",
            "sum",
            "open",
            "print",
            "Exception",
            "OSError",
            "ValueError",
            "SystemExit",
        ],
        &bindings,
        "_original_rebuild_config",
    )?;
    bindings.set_item("cfg", &cfg)?;
    for name in [
        "load_plan",
        "load_state",
        "save_state",
        "_stop_retryable",
        "is_blank",
    ] {
        bindings.set_item(name, cfg.getattr(name)?)?;
    }
    bindings.set_item("announce_packs", &refusal)?;
    let rebuild = crate::source_oracle_runtime::module(
        py,
        sources[0],
        &[
            "_is_permanent_media_error",
            "_mark_in_flight",
            "notify",
            "delete_old_packs",
            "_marked_add_landed",
            "_reconcile_in_flight",
            "build",
            "_build",
            "build_command",
        ],
        &[
            "str",
            "int",
            "len",
            "set",
            "sorted",
            "max",
            "range",
            "any",
            "print",
            "Exception",
            "RuntimeError",
            "SystemExit",
        ],
        &bindings,
        "_original_rebuild_engine",
    )?;
    bindings.set_item("rd", &rebuild)?;
    let bp = ns.call0()?;
    bp.setattr("EXIT_OK", 0)?;
    bp.setattr("EXIT_PARTIAL", 3)?;
    bindings.set_item("bp", bp)?;
    let fakes = crate::source_oracle_runtime::namespace(
        py,
        sources[7],
        &[
            "_image",
            "_png_bytes",
            "_png",
            "FakeTelegram",
            "RebuildCase",
        ],
        &[
            "__build_class__",
            "range",
            "len",
            "str",
            "int",
            "dict",
            "set",
            "property",
            "RuntimeError",
            "AssertionError",
            "SystemExit",
        ],
        &bindings,
    )?;
    for name in [
        "_image",
        "_png_bytes",
        "_png",
        "FakeTelegram",
        "RebuildCase",
    ] {
        bindings.set_item(name, required(&fakes, name)?)?;
    }
    let inventory_bindings = bindings.copy()?;
    for name in ["_TICKER_RE", "_PREMIUM_RE"] {
        inventory_bindings.set_item(
            name,
            py.import("re")?.call_method1(
                "compile",
                (crate::source_logging_oracle::literal(
                    py,
                    sources[13],
                    name,
                    true,
                )?,),
            )?,
        )?;
    }
    let inventory = crate::source_oracle_runtime::namespace(
        py,
        sources[13],
        &["refill_inventory"],
        &["enumerate"],
        &inventory_bindings,
    )?;
    bindings.set_item(
        "refill_inventory",
        required(&inventory, "refill_inventory")?,
    )?;
    bindings.set_item("canonical_map_lock", state.getattr("canonical_map_lock")?)?;
    let map = crate::source_oracle_runtime::module(
        py,
        sources[12],
        &[
            "reapply_aliases",
            "approved_shared_tickers",
            "unapproved_shared_groups",
            "MapIdentityUnproven",
            "resolve_by_image",
            "map_and_fill",
            "_map_and_fill",
            "fill_inventory",
        ],
        &[
            "__build_class__",
            "RuntimeError",
            "str",
            "set",
            "sorted",
            "len",
            "list",
            "dict",
            "max",
            "print",
            "OSError",
            "ValueError",
            "Exception",
            "SystemExit",
        ],
        &bindings,
        "_original_rebuild_map",
    )?;
    bindings.set_item("dmap", &map)?;
    let coins = ns.call0()?;
    coins.setattr("_dedup_map", &map)?;
    let map_imports = Py::new(
        py,
        Imports {
            package: package.unbind(),
            coins: Some(coins.unbind()),
            calls: AtomicUsize::new(0),
            maps: AtomicUsize::new(0),
            refused: AtomicUsize::new(0),
        },
    )?;
    bindings.set_item("map_imports", map_imports)?;
    Ok(Modules {
        cfg,
        rebuild,
        map,
        state,
        locks,
        bindings,
        clock,
        refusal,
        imports,
    })
}
