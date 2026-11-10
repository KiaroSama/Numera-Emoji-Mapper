//! A fixed original transport subset shares the publisher's private client and temporary ledger.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::atomic::{AtomicUsize, Ordering};

#[pyclass]
struct Patch {
    builtins: Py<PyDict>,
    calls: AtomicUsize,
    stdout_calls: AtomicUsize,
}
#[pymethods]
impl Patch {
    #[getter]
    fn object<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        crate::source_oracle_runtime::host_import(py, "unittest.mock")?
            .getattr("patch")?
            .getattr("object")
    }
    fn __call__<'py>(&self, py: Python<'py>, target: &str) -> PyResult<Bound<'py, PyAny>> {
        if target == "sys.stdout" {
            self.stdout_calls.fetch_add(1, Ordering::Relaxed);
            return crate::source_oracle_runtime::host_import(py, "unittest.mock")?
                .getattr("patch")?
                .call1((target,));
        }
        if target != "builtins.print" {
            return Err(PyValueError::new_err(
                "unexpected original transport patch target",
            ));
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        let values = PyDict::new(py);
        values.set_item(
            "print",
            crate::source_oracle_runtime::host_import(py, "unittest.mock")?
                .getattr("Mock")?
                .call0()?,
        )?;
        crate::source_oracle_runtime::host_import(py, "unittest.mock")?
            .getattr("patch")?
            .call_method1("dict", (self.builtins.bind(py), values))
    }
}

const CLIENT: [(&str, &str); 31] = [
    (
        "TokenRedaction",
        "test_exception_text_never_carries_the_token",
    ),
    ("TokenRedaction", "test_network_retry_output_is_redacted"),
    (
        "WaitsReachTheLogFile",
        "test_a_flood_wait_is_logged_not_only_printed",
    ),
    (
        "WaitsReachTheLogFile",
        "test_a_network_retry_is_logged_with_the_token_redacted",
    ),
    (
        "StickerSetInvalidScope",
        "test_lookup_fails_fast_without_sleeping",
    ),
    (
        "StickerSetInvalidScope",
        "test_create_still_waits_for_the_name_lock",
    ),
    (
        "AddedCheckUsesIdentity",
        "test_usable_fuids_accepts_a_well_formed_set",
    ),
    (
        "AddedCheckUsesIdentity",
        "test_usable_fuids_rejects_a_missing_id",
    ),
    (
        "AddedCheckUsesIdentity",
        "test_our_own_new_sticker_is_recognised",
    ),
    (
        "AddedCheckUsesIdentity",
        "test_nothing_new_means_not_applied",
    ),
    (
        "AddedCheckUsesIdentity",
        "test_one_new_but_FOREIGN_sticker_is_not_our_upload",
    ),
    (
        "AddedCheckUsesIdentity",
        "test_one_new_sticker_we_cannot_verify_is_unknown",
    ),
    (
        "AddedCheckUsesIdentity",
        "test_two_new_identities_are_unattributable",
    ),
    (
        "AddedCheckUsesIdentity",
        "test_a_replacement_is_not_read_as_our_add",
    ),
    (
        "AddedCheckUsesIdentity",
        "test_without_a_snapshot_nothing_is_claimed",
    ),
    (
        "AddedCheckUsesIdentity",
        "test_stickers_without_identity_are_unknown_not_applied",
    ),
    (
        "AddedCheckUsesIdentity",
        "test_duplicate_identities_are_not_trusted",
    ),
    (
        "CreateAdoptionVerifiesContent",
        "test_existence_alone_is_not_adopted",
    ),
    (
        "CreateAdoptionVerifiesContent",
        "test_our_own_image_is_adopted",
    ),
    (
        "CreateAdoptionVerifiesContent",
        "test_a_populated_set_is_not_the_shape_we_would_have_left",
    ),
    (
        "CreateAdoptionVerifiesContent",
        "test_unverifiable_content_stays_unknown",
    ),
    (
        "CreateAdoptionVerifiesContent",
        "test_missing_set_is_a_definite_no",
    ),
    (
        "PostAddSizeIsMeasured",
        "test_a_foreign_arrival_alone_does_not_book_a_size",
    ),
    (
        "PostAddSizeIsMeasured",
        "test_an_unverifiable_arrival_does_not_book_a_size",
    ),
    (
        "PostAddSizeIsMeasured",
        "test_our_sticker_present_books_the_true_size",
    ),
    (
        "PostAddSizeIsMeasured",
        "test_a_missing_set_is_never_booked_as_zero",
    ),
    ("TriStateLiveReads", "test_exists"),
    ("TriStateLiveReads", "test_missing_is_a_real_zero"),
    (
        "RepaintingIsCreationOnly",
        "test_the_flag_is_sent_when_asked_for",
    ),
    (
        "RepaintingIsCreationOnly",
        "test_it_is_ABSENT_by_default_not_false",
    ),
    (
        "TriStateLiveReads",
        "test_unknown_raises_instead_of_returning_zero",
    ),
];

pub(super) fn run(
    py: Python<'_>,
    sources: &[&Source],
    telegram: &Bound<'_, PyDict>,
    temporary: &Bound<'_, PyAny>,
    result: &Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let builtin = required(telegram, "__builtins__")?.cast_into::<PyDict>()?;
    let print = required(&builtin, "print")?;
    let sys = py.import("sys")?;
    let stdout = sys.getattr("stdout")?;
    let logging = py.import("logging")?;
    let registry = logging
        .getattr("Logger")?
        .getattr("manager")?
        .getattr("loggerDict")?
        .cast_into::<PyDict>()?;
    let previous = registry.get_item("build_pack")?;
    let previous_log = required(telegram, "log")?;
    let log = logging.getattr("Logger")?.call1(("build_pack",))?;
    log.setattr("propagate", false)?;
    log.call_method1("setLevel", (logging.getattr("WARNING")?,))?;
    let handler = logging.getattr("NullHandler")?.call0()?;
    log.call_method1("addHandler", (&handler,))?;
    let stack = py.import("contextlib")?.getattr("ExitStack")?.call0()?;
    let outcome = (|| -> PyResult<Vec<String>> {
        // assertLogs resolves its original name; register only this private real logger.
        for (dictionary, key, value) in [(&registry, "build_pack", &log), (telegram, "log", &log)] {
            let values = PyDict::new(py);
            values.set_item(key, value)?;
            let guard = py
                .import("unittest.mock")?
                .getattr("patch")?
                .call_method1("dict", (dictionary, values))?;
            stack.call_method1("enter_context", (guard,))?;
        }
        let patch = Py::new(
            py,
            Patch {
                builtins: builtin.clone().unbind(),
                calls: AtomicUsize::new(0),
                stdout_calls: AtomicUsize::new(0),
            },
        )?;
        let mock = ns.call0()?;
        mock.setattr("Mock", py.import("unittest.mock")?.getattr("Mock")?)?;
        mock.setattr("patch", &patch)?;
        let api = ns.call0()?;
        for name in [
            "Telegram",
            "_usable_fuids",
            "NAME_LOCK_TIMEOUT",
            "time",
            "AmbiguousUploadError",
            "LiveStateUnknown",
            "SetState",
        ] {
            api.setattr(name, required(telegram, name)?)?;
        }
        let engine = ns.call0()?;
        engine.setattr("time", required(telegram, "time")?)?;
        let png_bindings = PyDict::new(py);
        png_bindings.set_item("Image", py.import("PIL.Image")?)?;
        let png = crate::source_oracle_runtime::namespace(
            py,
            sources[29],
            &["_png"],
            &[],
            &png_bindings,
        )?;
        let bindings = PyDict::new(py);
        for (name, value) in [
            ("unittest", py.import("unittest")?.into_any()),
            ("requests", py.import("requests")?.into_any()),
            ("Path", py.import("pathlib")?.getattr("Path")?),
            ("tempfile", temporary.clone()),
            ("_png", required(&png, "_png")?),
            ("tg_api", api),
            ("bp", engine),
            ("mock", mock),
        ] {
            bindings.set_item(name, value)?;
        }
        let client = crate::source_oracle_runtime::namespace(
            py,
            sources[27],
            &[
                "TokenRedaction",
                "WaitsReachTheLogFile",
                "StickerSetInvalidScope",
                "AddedCheckUsesIdentity",
                "CreateAdoptionVerifiesContent",
                "PostAddSizeIsMeasured",
                "TriStateLiveReads",
                "RepaintingIsCreationOnly",
            ],
            &["__build_class__", "len", "sum", "RuntimeError", "str"],
            &bindings,
        )?;
        let bodies = crate::source_oracle_runtime::namespace(
            py,
            sources[28],
            &["_Reply", "ANonObjectBodyGoesThroughTheRetryPath"],
            &["__build_class__", "RuntimeError", "AttributeError"],
            &bindings,
        )?;
        let before: usize = result.getattr("testsRun")?.extract()?;
        let mut cases = Vec::new();
        for (namespace, class, method) in CLIENT
            .into_iter()
            .map(|(class, method)| (&client, class, method))
            .chain([
                (
                    &bodies,
                    "ANonObjectBodyGoesThroughTheRetryPath",
                    "test_a_list_body_is_retried_then_reported_not_an_attribute_error",
                ),
                (
                    &bodies,
                    "ANonObjectBodyGoesThroughTheRetryPath",
                    "test_an_add_with_a_body_that_is_not_an_object_asks_the_applied_check",
                ),
            ])
        {
            required(namespace, class)?
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
                    return Err(PyValueError::new_err(format!(
                        "original transport {class}.{method} reported {field}: {}; retained",
                        crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                    )));
                }
            }
            cases.push(format!("{class}.{method}"));
        }
        if result.getattr("testsRun")?.extract::<usize>()? != before + 33
            || cases.len() != 33
            || patch.borrow(py).calls.load(Ordering::Relaxed) != 4
            || patch.borrow(py).stdout_calls.load(Ordering::Relaxed) != 1
            || !required(&builtin, "print")?.is(&print)
        {
            return Err(PyValueError::new_err(
                "original transport execution or print restoration incomplete",
            ));
        }
        Ok(cases)
    })();
    let restored = stack.call_method0("close");
    let closed = handler.call_method0("close");
    restored?;
    closed?;
    let registry_restored = match (previous, registry.get_item("build_pack")?) {
        (Some(before), Some(after)) => before.is(&after),
        (None, None) => true,
        _ => false,
    };
    if !registry_restored
        || !required(telegram, "log")?.is(&previous_log)
        || !sys.getattr("stdout")?.is(&stdout)
        || !required(&builtin, "print")?.is(&print)
        || log.getattr("handlers")?.len()? != 1
        || !log.getattr("handlers")?.get_item(0)?.is(&handler)
        || log.getattr("level")?.extract::<u32>()? != 30
        || log.getattr("propagate")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original transport diagnostic state restoration incomplete; retained",
        ));
    }
    outcome
}
