//! Original finite-timeline and tint assertions inside the existing media source worker.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

const TIMELINE: &[&str] = &[
    "test_a_non_numeric_field_is_still_refused",
    "test_a_valid_timeline_still_passes",
    "test_infinity_is_refused_by_name_not_by_luck",
    "test_nan_in_any_timeline_field_is_refused",
];
const COLOUR: &[&str] = &[
    "test_a_flat_colour_is_recoloured",
    "test_a_flat_colour_keeps_its_own_alpha",
    "test_an_animated_colour_is_recoloured_too",
    "test_legacy_keyframe_end_values_are_recoloured",
];
const GRADIENT: &[&str] = &[
    "test_a_gradient_with_no_declared_stop_count_is_handled_safely",
    "test_the_colour_stops_are_recoloured",
    "test_the_offsets_are_left_alone",
    "test_the_opacity_ramp_is_left_alone",
];

pub(super) fn run<'py>(
    py: Python<'py>,
    sources: [&Source; 2],
    bindings: &Bound<'py, PyDict>,
) -> PyResult<Vec<String>> {
    let [test, repaint] = sources;
    let functions = crate::source_oracle_runtime::namespace(
        py,
        repaint,
        &["_numeric", "_recolour", "_tint_gradient", "_tint_lottie"],
        &[
            "all",
            "isinstance",
            "list",
            "int",
            "float",
            "range",
            "min",
            "len",
            "dict",
        ],
        &PyDict::new(py),
    )?;
    let api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    api.setattr("_tint_lottie", required(&functions, "_tint_lottie")?)?;
    let bindings = bindings.copy()?;
    bindings.set_item("repaint", api)?;
    bindings.set_item("copy", py.import("copy")?)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &[
            "_tgs",
            "ATimelineMustBeFiniteNumbers",
            "RepaintingReachesEveryColour",
            "GradientOpacitySurvivesARepaint",
        ],
        &["__build_class__", "dict", "float", "str"],
        &bindings,
    )?;
    crate::source_oracle_runtime::assignment(py, test, "VALID", &tests)?;
    let unittest = py.import("unittest")?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    for (class_name, methods) in [
        ("ATimelineMustBeFiniteNumbers", TIMELINE),
        ("RepaintingReachesEveryColour", COLOUR),
        ("GradientOpacitySurvivesARepaint", GRADIENT),
    ] {
        let class = required(&tests, class_name)?;
        if loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract::<Vec<String>>()?
            != methods
        {
            return Err(PyValueError::new_err("original repaint collection drift"));
        }
        for method in methods {
            class.call1((*method,))?.call_method1("run", (&result,))?;
            for field in [
                "errors",
                "failures",
                "skipped",
                "expectedFailures",
                "unexpectedSuccesses",
            ] {
                if result.getattr(field)?.len()? != 0 {
                    return Err(PyValueError::new_err(format!(
                        "original {class_name}.{method} reported {field}: {}; retained",
                        crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                    )));
                }
            }
            cases.push(format!("{class_name}.{method}"));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 12
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original repaint execution incomplete",
        ));
    }
    Ok(cases)
}

pub(super) fn gate<'py>(
    py: Python<'py>,
    sources: [&Source; 3],
    bindings: &Bound<'py, PyDict>,
) -> PyResult<Vec<String>> {
    let [test, gate, media] = sources;
    let bindings = bindings.copy()?;
    let sys = py.import("sys")?;
    let stdin = sys.getattr("stdin")?;
    let stderr = sys.getattr("stderr")?;
    bindings.set_item("sys", &sys)?;
    bindings.set_item(
        "redirect_stderr",
        py.import("contextlib")?.getattr("redirect_stderr")?,
    )?;
    let predicate = crate::source_oracle_runtime::namespace(
        py,
        media,
        &["is_repaintable"],
        &["bool"],
        &PyDict::new(py),
    )?;
    let api = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    api.setattr("is_repaintable", required(&predicate, "is_repaintable")?)?;
    bindings.set_item("media", api)?;
    let original = crate::source_oracle_runtime::namespace(
        py,
        gate,
        &["repaintable_gate"],
        &["len", "print", "input", "EOFError", "KeyboardInterrupt"],
        &bindings,
    )?;
    for name in ["REPAINT_MODES", "_REPAINT_SAMPLE", "_NO_ANSWER"] {
        crate::source_oracle_runtime::assignment(py, gate, name, &original)?;
    }
    let bp = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
    bp.setattr("repaintable_gate", required(&original, "repaintable_gate")?)?;
    bindings.set_item("bp", bp)?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &[
            "TheFlagIsOnTheStickerNotTheSet",
            "TheGateAsksBeforeIngesting",
        ],
        &[
            "__build_class__",
            "list",
            "range",
            "all",
            "str",
            "EOFError",
            "KeyboardInterrupt",
        ],
        &bindings,
    )?;
    let unittest = py.import("unittest")?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let observed = (|| -> PyResult<Vec<String>> {
        let mut cases = Vec::new();
        for (class_name, methods) in [
            (
                "TheFlagIsOnTheStickerNotTheSet",
                &[
                    "test_a_repaintable_sticker_is_recognised",
                    "test_absence_is_not_repaintable",
                    "test_an_ordinary_sticker_is_not",
                    "test_the_set_level_answer_is_not_the_stickers_answer",
                ][..],
            ),
            (
                "TheGateAsksBeforeIngesting",
                &[
                    "test_an_unanswerable_prompt_is_a_no_not_a_crash",
                    "test_anything_but_yes_skips",
                    "test_nothing_repaintable_asks_nothing",
                    "test_skip_and_keep_never_prompt",
                    "test_the_warning_names_them_and_bounds_the_list",
                    "test_with_no_terminal_the_answer_is_skip",
                    "test_yes_keeps_them",
                ][..],
            ),
        ] {
            let class = required(&tests, class_name)?;
            if loader
                .call_method1("getTestCaseNames", (&class,))?
                .extract::<Vec<String>>()?
                != methods
            {
                return Err(PyValueError::new_err(
                    "original repaint gate collection drift",
                ));
            }
            for method in methods {
                class.call1((*method,))?.call_method1("run", (&result,))?;
                for field in [
                    "errors",
                    "failures",
                    "skipped",
                    "expectedFailures",
                    "unexpectedSuccesses",
                ] {
                    if result.getattr(field)?.len()? != 0 {
                        return Err(PyValueError::new_err(format!(
                            "original {class_name}.{method} reported {field}: {}; retained",
                            crate::source_tooling_oracle::assertion_detail(result.as_any(), field)?
                        )));
                    }
                }
                if !sys.getattr("stdin")?.is(&stdin) || !sys.getattr("stderr")?.is(&stderr) {
                    return Err(PyValueError::new_err(
                        "original repaint gate did not restore its streams",
                    ));
                }
                cases.push(format!("{class_name}.{method}"));
            }
        }
        if result.getattr("testsRun")?.extract::<usize>()? != 11
            || !result.call_method0("wasSuccessful")?.extract::<bool>()?
        {
            return Err(PyValueError::new_err(
                "original repaint gate execution incomplete",
            ));
        }
        Ok(cases)
    })();
    let restore_stdin = sys.setattr("stdin", stdin);
    let restore_stderr = sys.setattr("stderr", stderr);
    restore_stdin?;
    restore_stderr?;
    observed
}

pub(super) const FILES: &[&[&str]] = &[&["x.tgs"], &["x.tgs"], &["x.tgs"], &["x.tgs"]];
