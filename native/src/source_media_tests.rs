//! Unchanged media assertions in the existing reencoding worker and private namespaces.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::path::Path;

const DETECTION: &[&str] = &[
    "test_png_is_static",
    "test_sticker_format_mapping",
    "test_tgs_is_animated",
    "test_webp_is_static",
];
const STATIC: &[&str] = &[
    "test_an_opaque_image_hashes_exactly_as_before",
    "test_different_content_different_key",
    "test_identical_content_same_key",
    "test_phash_near_duplicate",
    "test_rgb_under_transparent_pixels_cannot_move_the_hash",
    "test_the_hash_still_separates_genuinely_different_art",
    "test_to_static_png_is_100",
];
const ANIMATED: &[&str] = &[
    "test_decompression_is_bounded",
    "test_empty_timeline_is_rejected",
    "test_list_root_raises_media_error_not_attribute_error",
    "test_overlong_timeline_is_rejected",
    "test_tgs_content_key_stable",
    "test_tgs_roundtrip_valid",
    "test_uncompressed_json_is_not_a_tgs",
    "test_valid_512_canvas_is_preserved",
    "test_wrong_canvas_is_rejected",
];
const MASKS: &[&str] = &[
    "test_a_subtract_mask_INSIDE_A_PRECOMP_is_rejected",
    "test_a_subtract_mask_is_rejected",
    "test_an_ADD_mask_is_still_allowed",
];
const FINGERPRINT: &[&str] = &[
    "test_a_transparent_webm_survives_a_re_encode",
    "test_an_unreadable_source_fails_the_same_way_it_always_did",
    "test_animated_has_no_raster_hash_and_still_keys",
    "test_only_a_real_video_input_names_the_alpha_decoder",
    "test_static_agrees_with_content_key_and_perceptual_hash",
    "test_video_agrees_with_content_key_and_perceptual_hash",
    "test_video_decodes_once_not_twice",
];
const BLANK: &[&str] = &[
    "test_a_few_visible_pixels_still_count_as_blank",
    "test_a_visible_image_is_not_blank",
    "test_fully_transparent_is_blank",
    "test_the_visibility_floor_is_exact",
];
const VIDEO: &[&str] = &[
    "test_audio_stream_is_rejected",
    "test_compliant_webm_is_accepted",
    "test_excessive_frame_rate_is_rejected",
    "test_gif_to_webm_valid",
    "test_video_content_key",
    "test_wrong_codec_is_rejected",
    "test_wrong_container_is_rejected",
    "test_wrong_dimensions_are_rejected",
];
pub(super) fn run<'py>(
    py: Python<'py>,
    root: &Path,
    sources: [&Source; 2],
    original: &crate::source_image_identity::Modules<'py>,
    media: &Bound<'py, PyModule>,
    lottie: &Bound<'py, PyDict>,
    bindings: &Bound<'py, PyDict>,
) -> PyResult<Vec<String>> {
    let [test, fixture] = sources;
    let builtins = required(lottie, "__builtins__")?;
    for name in ["int", "open"] {
        builtins.set_item(name, py.import("builtins")?.getattr(name)?)?;
    }
    let error = required(lottie, "MediaError")?;
    // The static recipe's old RuntimeError port is replaced only in this private namespace.
    original
        .media
        .getattr("to_static_png")?
        .getattr("__globals__")?
        .set_item("MediaError", &error)?;
    for name in ["to_static_png", "is_blank_image", "telegram_sticker_format"] {
        media.setattr(name, original.media.getattr(name)?)?;
    }
    for name in [
        "to_animated_tgs",
        "validate_tgs",
        "TGS_SIZE",
        "TGS_MAX_UNPACKED",
        "TGS_MAX_BYTES",
    ] {
        media.setattr(name, required(lottie, name)?)?;
    }
    media.setattr("MediaError", error)?;
    media.setattr("VISIBLE_ALPHA", 10)?;
    media.setattr("BLANK_MAX_VISIBLE", 8)?;
    original
        .identity
        .setattr("_dhash", required(&original.globals, "_dhash")?)?;
    let bindings = bindings.copy()?;
    bindings.set_item("identity", &original.identity)?;
    bindings.set_item("media", media)?;
    bindings.set_item("copy", py.import("copy")?)?;
    bindings.set_item(
        "FIXTURES",
        py.import("pathlib")?
            .getattr("Path")?
            .call1((root.join("tests/fixtures"),))?,
    )?;
    // Both actual tools were admitted by the shared video port before class decorators run.
    bindings.set_item("HAS_FFMPEG", true)?;
    bindings.set_item("video_decode", media.getattr("video_decode")?)?;
    let helpers = crate::source_oracle_runtime::namespace(
        py,
        fixture,
        &["_make_anim_gif", "_clear_pixels", "_ffmpeg_calls"],
        &["range", "sum", "len", "str"],
        &bindings,
    )?;
    for name in ["_make_anim_gif", "_clear_pixels", "_ffmpeg_calls"] {
        bindings.set_item(name, required(&helpers, name)?)?;
    }
    let tests = crate::source_oracle_runtime::namespace(
        py,
        test,
        &[
            "TestDetection",
            "TestStaticHashing",
            "TestAnimated",
            "SubtractMasksAreRefusedBeforeTelegramRefusesThem",
            "TestFingerprintMatchesTheSeparateCalls",
            "TestBlankDetection",
            "TestVideo",
        ],
        &[
            "__build_class__",
            "range",
            "str",
            "dict",
            "list",
            "bool",
            "len",
            "open",
            "type",
            "Exception",
        ],
        &bindings,
    )?;
    let unittest = py.import("unittest")?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    let original_run = media.getattr("_run")?;
    for (class_name, methods) in [
        ("TestDetection", DETECTION),
        ("TestStaticHashing", STATIC),
        ("TestAnimated", ANIMATED),
        ("SubtractMasksAreRefusedBeforeTelegramRefusesThem", MASKS),
        ("TestFingerprintMatchesTheSeparateCalls", FINGERPRINT),
        ("TestBlankDetection", BLANK),
        ("TestVideo", VIDEO),
    ] {
        let class = required(&tests, class_name)?;
        let expected = methods.to_vec();
        if loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract::<Vec<String>>()?
            != expected
        {
            return Err(PyValueError::new_err("original media collection drift"));
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
            if !media.getattr("_run")?.is(&original_run) {
                return Err(PyValueError::new_err(
                    "original media process patch was not restored",
                ));
            }
            cases.push(format!("{class_name}.{method}"));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 42
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original media assertion execution incomplete",
        ));
    }
    Ok(cases)
}

pub(super) const FILES: &[&[&str]] = &[
    &["a.png"],
    &[],
    &["a.tgs"],
    &["a.webp"],
    &[],
    &["a.png", "c.png"],
    &["a.png", "b.png"],
    &["a.png", "b.webp"],
    &[],
    &[],
    &["out.png", "s.png"],
    &["bomb.tgs"],
    &["x.tgs"],
    &["list.tgs"],
    &["x.tgs"],
    &["1.tgs", "2.tgs"],
    &["c.tgs"],
    &["plain.tgs"],
    &["c.tgs"],
    &["x.tgs"],
    &["x.tgs"],
    &["x.tgs"],
    &["x.tgs"],
    &["again.webm", "dot.png", "src.webm"],
    &["bad.png"],
    &["x.tgs"],
    &["a.gif", "o.webm", "src.webm"],
    &["a.png"],
    &["anim.gif", "anim.webm"],
    &["anim.gif", "anim.webm"],
    &["audio.webm"],
    &["ok.webm"],
    &["fast.webm"],
    &["anim.gif", "anim.webm"],
    &["anim.gif", "anim.webm"],
    &["vp8.webm"],
    &["x.mp4"],
    &["big.webm"],
];
