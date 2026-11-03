//! Original publication and applier graph; admission belongs to the reconciliation cohort.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::path::Path;

#[path = "source_publication_state.rs"]
mod persistent;

pub(crate) struct Modules<'py> {
    pub apply: Bound<'py, PyModule>,
    pub build: Bound<'py, PyModule>,
    pub order: Bound<'py, PyModule>,
    pub state: Bound<'py, PyDict>,
    pub locks: Bound<'py, PyDict>,
    pub panel: Bound<'py, PyDict>,
    pub errors: Bound<'py, PyDict>,
    pub telegram: Bound<'py, PyDict>,
    pub environment: Bound<'py, PyDict>,
    pub os: Bound<'py, PyAny>,
}

pub(crate) fn build<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
    ports: &Bound<'py, PyDict>,
) -> PyResult<Modules<'py>> {
    let persistent::Persistent { state, locks } = persistent::build(py, sources, ports)?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let path = py.import("pathlib")?.getattr("Path")?;
    let mock = py.import("unittest.mock")?.getattr("Mock")?;
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[13],
        &["FloodWaitTooLong", "OperatorConfigMissing"],
        &["__build_class__", "Exception", "RuntimeError", "super"],
        &PyDict::new(py),
    )?;
    let media_bindings = PyDict::new(py);
    media_bindings.set_item("_static_is_blank", required(&state, "_static_is_blank")?)?;
    let media_check = crate::source_oracle_runtime::namespace(
        py,
        sources[23],
        &["_media_ok"],
        &["Exception"],
        &media_bindings,
    )?;
    let names_bindings = PyDict::new(py);
    names_bindings.set_item("FMT_TAG", required(&state, "FMT_TAG")?)?;
    names_bindings.set_item(
        "_BASE_RE",
        py.import("re")?.call_method1(
            "compile",
            (crate::source_logging_oracle::literal(
                py,
                sources[24],
                "_BASE_RE",
                true,
            )?,),
        )?,
    )?;
    names_bindings.set_item(
        "_NAME_MAX",
        crate::source_logging_oracle::literal(py, sources[24], "_NAME_MAX", false)?,
    )?;
    let names = crate::source_oracle_runtime::namespace(
        py,
        sources[24],
        &["check_name_length", "valid_base", "parse_formats"],
        &["len", "SystemExit", "any", "set", "ValueError"],
        &names_bindings,
    )?;
    let environment = PyDict::new(py);
    environment.set_item("BRAND_LOGO_BOTS", "")?;
    let private_os = ns.call0()?;
    private_os.setattr("environ", &environment)?;
    let private_path = ns.call0()?;
    private_path.setattr("isabs", py.import("os.path")?.getattr("isabs")?)?;
    private_os.setattr("path", private_path)?;
    let operator_bindings = PyDict::new(py);
    operator_bindings.set_item("os", &private_os)?;
    operator_bindings.set_item("Path", &path)?;
    operator_bindings.set_item("ROOT", path.call1((workspace,))?)?;
    operator_bindings.set_item(
        "WHAT",
        crate::source_logging_oracle::literal(py, sources[25], "WHAT", false)?,
    )?;
    operator_bindings.set_item(
        "OperatorConfigMissing",
        required(&errors, "OperatorConfigMissing")?,
    )?;
    let operator = crate::source_oracle_runtime::namespace(
        py,
        sources[25],
        &[
            "value",
            "require",
            "brand_logo_keywords",
            "brand_logo_bots",
            "brand_logo_path",
            "brand_logo",
        ],
        &["frozenset", "str"],
        &operator_bindings,
    )?;
    let operator = crate::source_roster_modules::rebind(py, &operator, "_original_apply_operator")?;
    let clock = ns.call0()?;
    clock.setattr("sleep", mock.call0()?)?;
    clock.setattr("monotonic", py.import("time")?.getattr("monotonic")?)?;
    let protocol = required(ports, "protocol")?.cast_into::<PyDict>()?;
    let telegram_bindings = PyDict::new(py);
    telegram_bindings.set_item("os", &private_os)?;
    telegram_bindings.set_item("Path", &path)?;
    telegram_bindings.set_item("tempfile", py.import("tempfile")?)?;
    telegram_bindings.set_item("time", &clock)?;
    telegram_bindings.set_item("log", required(ports, "log")?)?;
    telegram_bindings.set_item("FloodWaitTooLong", required(&errors, "FloodWaitTooLong")?)?;
    for name in [
        "SetState",
        "LiveStateUnknown",
        "BotApiError",
        "AmbiguousUploadError",
    ] {
        telegram_bindings.set_item(name, required(&protocol, name)?)?;
    }
    let telegram =
        crate::source_telegram_namespace::namespace(py, sources[22], &telegram_bindings)?;
    let blocked = PyDict::new(py);
    blocked.set_item(
        "side_effect",
        PyRuntimeError::new_err("unpatched original publication effect").value(py),
    )?;
    let common = PyDict::new(py);
    for (name, value) in [
        ("os", private_os.clone()),
        ("Path", path.clone()),
        ("operator_config", operator.into_any()),
        ("Catalog", required(ports, "Catalog")?),
        ("log", required(ports, "log")?),
        ("Telegram", required(&telegram, "Telegram")?),
    ] {
        common.set_item(name, value)?;
    }
    for name in [
        "StateError",
        "SetDrift",
        "_state_path",
        "_lock_path",
        "load_json",
        "load_state",
        "save_json",
        "BrandLogo",
        "DEFAULT_EMOJI",
        "MIXED",
        "PER_SET",
        "FMT_TAG",
        "BRAND_LOGO_EMOJI",
    ] {
        common.set_item(name, required(&state, name)?)?;
    }
    for name in ["FloodWaitTooLong", "OperatorConfigMissing"] {
        common.set_item(name, required(&errors, name)?)?;
    }
    for name in ["LockBusy", "exclusive_lock"] {
        common.set_item(name, required(&locks, name)?)?;
    }
    for name in [
        "BotApiError",
        "LiveStateUnknown",
        "SetState",
        "AmbiguousUploadError",
    ] {
        common.set_item(name, required(&protocol, name)?)?;
    }
    for name in ["check_name_length", "valid_base", "parse_formats"] {
        common.set_item(name, required(&names, name)?)?;
    }
    common.set_item("argparse", py.import("argparse")?)?;
    common.set_item("writer", py.import("contextlib")?.getattr("nullcontext")?)?;
    common.set_item("ROOT", path.call1((workspace,))?)?;
    common.set_item("redact", py.import("builtins")?.getattr("str")?)?;
    for (name, value) in [
        ("EXIT_OK", 0),
        ("EXIT_USAGE", 2),
        ("EXIT_PENDING", 3),
        ("EXIT_PARTIAL", 3),
        ("EXIT_FAILED", 4),
    ] {
        common.set_item(name, value)?;
    }
    let config_bindings = PyDict::new(py);
    config_bindings.set_item("os", &private_os)?;
    config_bindings.set_item("sys", py.import("sys")?)?;
    for name in ["EXIT_OK", "EXIT_PARTIAL", "EXIT_FAILED"] {
        config_bindings.set_item(name, required(&common, name)?)?;
    }
    let config = crate::source_oracle_runtime::namespace(
        py,
        sources[26],
        &["safe_int_env", "ingest_exit_code"],
        &["int", "ValueError", "print"],
        &config_bindings,
    )?;
    for name in ["safe_int_env", "ingest_exit_code"] {
        common.set_item(name, required(&config, name)?)?;
    }
    for name in ["load_env", "setup_logging"] {
        common.set_item(name, mock.call((), Some(&blocked))?)?;
    }
    common.set_item("register_secret", mock.call0()?)?;
    let build_bindings = common.copy()?;
    let reconcile = required(ports, "cr")?;
    for name in [
        "_probe",
        "_set_is_open",
        "reconcile_set",
        "_live_index",
        "_confirm_new_upload",
        "_file_is_permanently_rejected",
        "_manifest_mismatch",
    ] {
        build_bindings.set_item(name, reconcile.getattr(name)?)?;
    }
    for name in ["load_plan", "freeze_plan", "_all_items"] {
        build_bindings.set_item(name, required(&state, name)?)?;
    }
    build_bindings.set_item("time", &clock)?;
    build_bindings.set_item("_media_ok", required(&media_check, "_media_ok")?)?;
    for name in ["notify", "write_manifest", "collection_preflight"] {
        build_bindings.set_item(name, mock.call0()?)?;
    }
    let original = crate::source_oracle_runtime::namespace(
        py,
        sources[11],
        &[
            "_by_position",
            "publish_format",
            "_record_cids",
            "pending_keys",
            "main",
            "_publish",
        ],
        &[
            "enumerate",
            "sorted",
            "len",
            "set",
            "next",
            "max",
            "RuntimeError",
            "isinstance",
            "str",
            "bool",
            "int",
            "ValueError",
            "Exception",
            "print",
        ],
        &build_bindings,
    )?;
    let build = crate::source_roster_modules::rebind(py, &original, "_original_apply_publisher")?;
    let order_bindings = PyDict::new(py);
    order_bindings.set_item("log", required(ports, "log")?)?;
    order_bindings.set_item("time", &clock)?;
    order_bindings.set_item(
        "MOVE_DELAY",
        crate::source_logging_oracle::literal(py, sources[19], "MOVE_DELAY", false)?,
    )?;
    let original = crate::source_oracle_runtime::namespace(
        py,
        sources[19],
        &["desired_order", "plan_moves", "sync_set"],
        &["list", "enumerate", "next", "len"],
        &order_bindings,
    )?;
    let order = crate::source_roster_modules::rebind(py, &original, "_original_apply_order")?;
    let panel = crate::source_plan_report_tests::reader(py, sources[18], sources[21])?;
    let apply_bindings = common;
    apply_bindings.set_item("build_collection", &build)?;
    apply_bindings.set_item("sync_order", &order)?;
    apply_bindings.set_item("_probe", reconcile.getattr("_probe")?)?;
    for name in ["PlanError", "read_plan", "target_map"] {
        apply_bindings.set_item(name, required(&panel, name)?)?;
    }
    for name in ["sqlite3", "re"] {
        apply_bindings.set_item(name, py.import(name)?)?;
    }
    apply_bindings.set_item("_dt", py.import("datetime")?)?;
    apply_bindings.set_item("dataclass", py.import("dataclasses")?.getattr("dataclass")?)?;
    apply_bindings.set_item("field", py.import("dataclasses")?.getattr("field")?)?;
    for name in ["MAX_CHANGES", "MAX_WAIT"] {
        apply_bindings.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[17], name, false)?,
        )?;
    }
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (&sources[17].text, &sources[17].path))?;
    apply_bindings.set_item("__doc__", ast.call_method1("get_docstring", (tree,))?)?;
    let mut registration =
        crate::source_oracle_runtime::ModuleRegistration::new(py, &apply_bindings)?;
    let original = crate::source_oracle_runtime::namespace(
        py,
        sources[17],
        &[
            "Refusal",
            "Steps",
            "read_catalog",
            "compute_steps",
            "render",
            "_journal_path",
            "load_journal",
            "_now",
            "_live_stickers",
            "_finish_removal",
            "settle_intent",
            "remove_one",
            "apply_run",
            "_run",
            "main",
            "_apply",
        ],
        &[
            "__build_class__",
            "Exception",
            "RuntimeError",
            "property",
            "list",
            "bool",
            "sum",
            "map",
            "len",
            "str",
            "enumerate",
            "int",
            "sorted",
            "any",
            "set",
            "max",
            "min",
            "isinstance",
            "dict",
            "next",
            "all",
            "print",
        ],
        &apply_bindings,
    );
    registration.restore()?;
    let apply = crate::source_roster_modules::rebind(py, &original?, "_original_plan_apply")?;
    Ok(Modules {
        apply,
        build,
        order,
        state,
        locks,
        panel,
        errors,
        telegram,
        environment,
        os: private_os,
    })
}
