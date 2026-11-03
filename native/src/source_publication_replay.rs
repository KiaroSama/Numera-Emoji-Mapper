//! Unchanged original publisher tests with real original output functions and scoped diagnostics.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::Path;

#[path = "source_publication_blank.rs"]
mod blank;
#[path = "source_verified_retry.rs"]
mod verified_retry;

fn functions<'py>(
    py: Python<'py>,
    source: &Source,
    definitions: &[&str],
    builtins: &[&str],
    bindings: &Bound<'py, PyDict>,
    name: &str,
) -> PyResult<Bound<'py, pyo3::types::PyModule>> {
    let original =
        crate::source_oracle_runtime::namespace(py, source, definitions, builtins, bindings)?;
    crate::source_roster_modules::rebind(py, &original, name)
}

pub(super) fn run(
    py: Python<'_>,
    workspace: &Path,
    sources: &[&Source],
    ports: &Bound<'_, PyDict>,
    temporary: &Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    let modules = crate::source_plan_apply_modules::build(py, workspace, sources, ports)?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let real_mock = py.import("unittest.mock")?;
    let logging = py.import("logging")?;
    let registry = logging
        .getattr("Logger")?
        .getattr("manager")?
        .getattr("loggerDict")?
        .cast_into::<PyDict>()?;
    let previous = registry.get_item("build_collection")?;
    let previous_log = modules.build.getattr("log")?;
    let stdout = py.import("sys")?.getattr("stdout")?;
    let log = logging.getattr("Logger")?.call1(("build_collection",))?;
    log.setattr("propagate", false)?;
    log.call_method1("setLevel", (logging.getattr("INFO")?,))?;
    let handler = logging.getattr("NullHandler")?.call0()?;
    log.call_method1("addHandler", (&handler,))?;
    let stack = py.import("contextlib")?.getattr("ExitStack")?.call0()?;
    let outcome = (|| -> PyResult<Vec<String>> {
        let build_globals = modules.build.dict();
        for (dictionary, key) in [(&registry, "build_collection"), (&build_globals, "log")] {
            let values = PyDict::new(py);
            values.set_item(key, &log)?;
            let guard = real_mock
                .getattr("patch")?
                .call_method1("dict", (dictionary, values))?;
            stack.call_method1("enter_context", (guard,))?;
        }
        let bindings = PyDict::new(py);
        for name in ["Path", "str", "int"] {
            bindings.set_item(
                name,
                if name == "Path" {
                    py.import("pathlib")?.getattr(name)?
                } else {
                    py.import("builtins")?.getattr(name)?
                },
            )?;
        }
        bindings.set_item("log", &log)?;
        bindings.set_item("os", &modules.os)?;
        bindings.set_item("redact", py.import("builtins")?.getattr("str")?)?;
        bindings.set_item("_state_path", required(&modules.state, "_state_path")?)?;
        bindings.set_item("save_json", required(&modules.state, "save_json")?)?;
        let blocked = PyDict::new(py);
        blocked.set_item(
            "side_effect",
            PyRuntimeError::new_err("unpatched original announcement HTTP").value(py),
        )?;
        let requests = ns.call0()?;
        requests.setattr("post", real_mock.getattr("Mock")?.call((), Some(&blocked))?)?;
        bindings.set_item("requests", requests)?;
        let announce = functions(
            py,
            sources[31],
            &[
                "links_chat_id",
                "worker_publish_url",
                "announce_via_worker",
                "announce_packs",
            ],
            &["RuntimeError", "str", "int"],
            &bindings,
            "_original_publication_announce",
        )?;
        bindings.set_item("announce_packs", announce.getattr("announce_packs")?)?;
        let rows = crate::source_oracle_runtime::namespace(
            py,
            sources[33],
            &["_cell", "markdown_table", "keyword_cell"],
            &["str", "len", "max"],
            &PyDict::new(py),
        )?;
        rows.set_item(
            "EMPTY",
            crate::source_logging_oracle::literal(py, sources[33], "EMPTY", false)?,
        )?;
        for name in ["markdown_table", "keyword_cell"] {
            bindings.set_item(name, required(&rows, name)?)?;
        }
        let notify = functions(
            py,
            sources[30],
            &["notify", "write_manifest"],
            &["Exception", "enumerate", "len"],
            &bindings,
            "_original_publication_notify",
        )?;
        for (name, code) in [("EXIT_OK", 0), ("EXIT_FAILED", 4), ("EXIT_PARTIAL", 3)] {
            bindings.set_item(name, code)?;
        }
        let clock = ns.call0()?;
        clock.setattr("sleep", real_mock.getattr("Mock")?.call0()?)?;
        bindings.set_item("time", clock)?;
        bindings.set_item(
            "BotApiError",
            required(ports, "protocol")?.get_item("BotApiError")?,
        )?;
        bindings.set_item(
            "PREFLIGHT_DELAY",
            crate::source_logging_oracle::literal(py, sources[32], "PREFLIGHT_DELAY", false)?,
        )?;
        let preflight = functions(
            py,
            sources[32],
            &["preflight", "_report"],
            &["len", "print", "str", "RuntimeError"],
            &bindings,
            "_original_publication_preflight",
        )?;
        modules.build.setattr("notify", notify.getattr("notify")?)?;
        modules
            .build
            .setattr("write_manifest", notify.getattr("write_manifest")?)?;
        modules.build.setattr("collection_preflight", preflight)?;
        let source_path = workspace.join("publisher-source.txt");
        let mut staged = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&source_path)?;
        std::io::Write::write_all(&mut staged, sources[11].text.as_bytes())?;
        staged.sync_all()?;
        drop(staged);
        modules.build.setattr("__file__", &source_path)?;
        let helper = PyDict::new(py);
        helper.set_item("bc", &modules.build)?;
        helper.set_item("mock", &real_mock)?;
        let helper = crate::source_oracle_runtime::namespace(
            py,
            sources[2],
            &["_main"],
            &["list"],
            &helper,
        )?;
        let tests = PyDict::new(py);
        tests.set_item("_main", required(&helper, "_main")?)?;
        tests.set_item("_CatalogFixture", required(ports, "_CatalogFixture")?)?;
        tests.set_item("bc", &modules.build)?;
        tests.set_item("announce", &announce)?;
        tests.set_item("collection_notify", &notify)?;
        tests.set_item("announce_packs", announce.getattr("announce_packs")?)?;
        tests.set_item(
            "cs",
            crate::source_roster_modules::rebind(
                py,
                &modules.state,
                "_original_publication_state",
            )?,
        )?;
        tests.set_item("os", &modules.os)?;
        tests.set_item("mock", &real_mock)?;
        tests.set_item("tempfile", temporary)?;
        tests.set_item("unittest", py.import("unittest")?)?;
        tests.set_item("Image", py.import("PIL.Image")?)?;
        tests.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
        tests.set_item("io", py.import("io")?)?;
        tests.set_item("json", py.import("json")?)?;
        tests.set_item("identity", required(ports, "identity")?)?;
        tests.set_item("Catalog", required(ports, "Catalog")?)?;
        tests.set_item(
            "exclusive_lock",
            required(&modules.locks, "exclusive_lock")?,
        )?;
        tests.set_item("MIXED", required(&modules.state, "MIXED")?)?;
        tests.set_item("SetDrift", required(&modules.state, "SetDrift")?)?;
        let protocol = required(ports, "protocol")?.cast_into::<PyDict>()?;
        tests.set_item(
            "tg_api",
            crate::source_roster_modules::rebind(py, &protocol, "_original_publication_protocol")?,
        )?;
        tests.set_item(
            "AmbiguousUploadError",
            required(ports, "protocol")?.get_item("AmbiguousUploadError")?,
        )?;
        tests.set_item(
            "redirect_stdout",
            py.import("contextlib")?.getattr("redirect_stdout")?,
        )?;
        tests.set_item("SET", "pks1_by_YourEmojiBot")?;
        tests.set_item("SET2", "pks2_by_YourEmojiBot")?;
        for (name, code) in [
            ("EXIT_OK", 0),
            ("EXIT_USAGE", 2),
            ("EXIT_PARTIAL", 3),
            ("EXIT_FAILED", 4),
        ] {
            tests.set_item(name, code)?;
        }
        let fakes = required(ports, "fakes")?.cast_into::<PyDict>()?;
        for name in ["FakeTG", "_sticker"] {
            tests.set_item(name, required(&fakes, name)?)?;
        }
        tests.set_item(
            "_make_png",
            required(
                &required(ports, "fixture_bindings")?.cast_into::<PyDict>()?,
                "_make_png",
            )?,
        )?;
        let cli = crate::source_oracle_runtime::namespace(
            py,
            sources[27],
            &["CliContract", "OccupiedNameTG", "PublishThroughMain"],
            &[
                "__build_class__",
                "str",
                "int",
                "range",
                "all",
                "len",
                "list",
                "set",
                "sorted",
                "RuntimeError",
                "super",
            ],
            &tests,
        )?;
        let dedup = crate::source_oracle_runtime::namespace(
            py,
            sources[28],
            &["FakeTelegram", "PublishDedupTest"],
            &[
                "__build_class__",
                "range",
                "str",
                "dict",
                "list",
                "set",
                "bytes",
                "len",
                "RuntimeError",
                "super",
            ],
            &tests,
        )?;
        let invariants = crate::source_oracle_runtime::namespace(
            py,
            sources[29],
            &[
                "AClosedSetIsNeverAppendedTo",
                "MixedFamiliesStillCheckEachItemsOwnFormat",
            ],
            &["__build_class__", "range", "str", "len", "sum"],
            &tests,
        )?;
        let lottie = crate::source_lottie_oracle::namespace(
            py,
            sources[36],
            sources[13],
            &["_load_lottie"],
        )?;
        let identity = required(ports, "identity")?;
        let identity_globals = identity
            .getattr("content_key")?
            .getattr("__globals__")?
            .cast_into::<PyDict>()?;
        let media = required(&identity_globals, "media")?;
        media.setattr("_load_lottie", required(&lottie, "_load_lottie")?)?;
        let vector_bindings = PyDict::new(py);
        for name in ["media", "hashlib", "json"] {
            vector_bindings.set_item(name, required(&identity_globals, name)?)?;
        }
        let vector = crate::source_oracle_runtime::namespace(
            py,
            sources[5],
            &["_animated_content_digest"],
            &["OSError", "ValueError"],
            &vector_bindings,
        )?;
        identity_globals.set_item(
            "_animated_content_digest",
            required(&vector, "_animated_content_digest")?,
        )?;
        let brand_bindings = tests.copy()?;
        brand_bindings.set_item(
            "make_png",
            required(
                &required(ports, "fixture_bindings")?.cast_into::<PyDict>()?,
                "_make_png",
            )?,
        )?;
        let brand = crate::source_oracle_runtime::namespace(
            py,
            sources[34],
            &[
                "_make_png",
                "FakeTelegram",
                "AnUnusableLogoStopsThePublish",
                "PublishFormatLogoFirst",
            ],
            &["__build_class__", "str", "len", "range", "list"],
            &brand_bindings,
        )?;
        let contracts = crate::source_oracle_runtime::namespace(
            py,
            sources[35],
            &[
                "EveryPublisherSharesOneAnnouncer",
                "AFilledPackAnnouncesAgain",
                "AnnouncementRoutesThroughTheWorker",
                "MixedPublishesOneFamily",
                "ThePublishFollowsThePanelOrderNow",
            ],
            &[
                "__build_class__",
                "range",
                "str",
                "len",
                "all",
                "list",
                "enumerate",
                "SystemExit",
                "RuntimeError",
            ],
            &tests,
        )?;
        let loader = py.import("unittest")?.getattr("TestLoader")?.call0()?;
        let result = py.import("unittest")?.getattr("TestResult")?.call0()?;
        let mut cases = Vec::new();
        for (dictionary, name, total) in [
            (&cli, "CliContract", 10),
            (&cli, "PublishThroughMain", 28),
            (&dedup, "PublishDedupTest", 4),
            (&invariants, "AClosedSetIsNeverAppendedTo", 3),
            (&invariants, "MixedFamiliesStillCheckEachItemsOwnFormat", 3),
            (&brand, "AnUnusableLogoStopsThePublish", 1),
            (&brand, "PublishFormatLogoFirst", 3),
            (&contracts, "EveryPublisherSharesOneAnnouncer", 5),
            (&contracts, "AFilledPackAnnouncesAgain", 5),
            (&contracts, "AnnouncementRoutesThroughTheWorker", 4),
            (&contracts, "MixedPublishesOneFamily", 5),
            (&contracts, "ThePublishFollowsThePanelOrderNow", 2),
        ] {
            let class = required(dictionary, name)?;
            let methods: Vec<String> = loader
                .call_method1("getTestCaseNames", (&class,))?
                .extract()?;
            if methods.len() != total {
                return Err(PyValueError::new_err(
                    "original publication collection drift",
                ));
            }
            for method in methods {
                if matches!(
                    method.as_str(),
                    "test_the_single_pack_build_and_the_coin_rebuild_both_import_it"
                ) {
                    continue;
                }
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
                            "original publication {name}.{method} reported {field}; retained"
                        )));
                    }
                }
                cases.push(format!("{name}.{method}"));
            }
        }
        cases.extend(verified_retry::run(
            py,
            sources[28],
            &modules.telegram,
            ports,
            temporary,
            &result,
        )?);
        cases.extend(blank::run(
            py,
            sources[4],
            sources[23],
            sources[35],
            &modules.build,
            &required(&modules.state, "_static_is_blank")?,
            &result,
        )?);
        if cases.len() != 84
            || result.getattr("testsRun")?.extract::<usize>()? != 84
            || !result.call_method0("wasSuccessful")?.extract::<bool>()?
            || modules.environment.len() != 1
            || required(&modules.environment, "BRAND_LOGO_BOTS")?.extract::<String>()? != ""
            || modules
                .build
                .getattr("__file__")?
                .extract::<std::path::PathBuf>()?
                != source_path
        {
            return Err(PyValueError::new_err(
                "original publication execution/private environment restoration incomplete",
            ));
        }
        Ok(cases)
    })();
    let restored = stack.call_method0("close");
    let closed = handler.call_method0("close");
    restored?;
    closed?;
    let registry_restored = match (previous, registry.get_item("build_collection")?) {
        (Some(before), Some(after)) => before.is(&after),
        (None, None) => true,
        _ => false,
    };
    if !registry_restored
        || !modules.build.getattr("log")?.is(&previous_log)
        || !py.import("sys")?.getattr("stdout")?.is(&stdout)
        || log.getattr("handlers")?.len()? != 1
        || !log.getattr("handlers")?.get_item(0)?.is(&handler)
        || log.getattr("propagate")?.extract::<bool>()?
        || log.getattr("level")?.extract::<u32>()? != 20
    {
        return Err(PyValueError::new_err(
            "original publication diagnostic restoration incomplete; retained",
        ));
    }
    outcome
}
