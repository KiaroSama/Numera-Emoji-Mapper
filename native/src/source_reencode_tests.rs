//! Original reencoding/comparison assertions with real codecs or explicitly private duration ports.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::{directories, required};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

#[path = "source_media_tests.rs"]
mod media_tests;
#[path = "source_preview_tests.rs"]
mod preview_tests;
#[path = "source_repaint_tests.rs"]
mod repaint_tests;
#[path = "source_reencode_video.rs"]
mod video;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 15] = [
    (
        "emojikit/media.py",
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
    ),
    (
        "emojikit/identity.py",
        "8cbefd550e306e671df1e52af2dd9fe6b4e3647c3aa1b13dc98bf35080d36e89",
    ),
    (
        "emojikit/media_lottie.py",
        "5112b55f21d4d241ffe9660d5294de20487d4ea1e9772188fb6bd125f7fef63b",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
    (
        "tests/_media_fixtures.py",
        "1c45ebbb66c1d06bab2fb897e1c24166646974cf02e6fd0d8b54c93c1df8f5e4",
    ),
    (
        "tests/test_media_reencode.py",
        "fa25c5717d7048d1eb2863f26d58c504314f0e6b6d6f73a8fb00376831c1fb5a",
    ),
    (
        "emojikit/video_decode.py",
        "bab9579240a23bb8ee59c1a66e496586ca881398bb4d82e956e2fb6c2b3ff13a",
    ),
    (
        "tests/test_media.py",
        "5d08353f1037fd29f09ecd8b886127c20106db92a32528a96d8a60b07c29fd6c",
    ),
    (
        "tests/test_lottie_repaint.py",
        "d45573f9181e21aa07c18cd1836460e00d83114ffb33ece783f8ab3820d8c52c",
    ),
    (
        "emojikit/repaint.py",
        "845d1443956499a93413316342f06d819e9ef89a9a8bb789dcaa53638cdbfc59",
    ),
    (
        "tests/test_repaintable.py",
        "a685ee987b52be24215d8d440b195389477c693fdaabe44f475da9b1588dde3e",
    ),
    (
        "tests/reference/repaint_gate.py",
        "7df425ea375f98e25bcf736b24ae471076317791b330ded03be6dc83967c9c9f",
    ),
    (
        "tests/test_native_preview_codec.py",
        "8f8d5d1cdf52be2f2e65359eb11bc1f8e4878ccebfff4fa64090e24050420ead",
    ),
    (
        "tests/reference/panel_preview.py",
        "832548b52fdc4862ca02d8e7d4af9eae84751ef590f333c67f1dd483e9d26024",
    ),
    (
        "tests/_video_fixtures.py",
        "3b5175f1206e0fe685c53e42ff00bbcc7cbd8a0d36ec74196a8cb342bcc70caf",
    ),
];
const REENCODE: [&str; 8] = [
    "test_an_unknown_format_is_left_alone",
    "test_an_unreadable_file_is_reported_not_raised",
    "test_animated_tgs_regzips_to_the_same_animation",
    "test_growing_past_a_format_cap_keeps_the_original",
    "test_regzip_is_deterministic",
    "test_static_bytes_change_and_pixels_do_not",
    "test_static_opaque_bytes_change_and_pixels_do_not",
    "test_the_content_key_survives_so_dedup_still_works",
];
const SINGLE_FRAME: [&str; 4] = [
    "test_fingerprint_and_content_key_agree",
    "test_the_key_is_not_merely_a_hash_of_the_file",
    "test_the_key_is_stable_across_runs",
    "test_the_key_survives_a_reencode",
];
const DURATION: [&str; 2] = [
    "test_a_remux_inside_the_cap_still_rewrites",
    "test_a_remux_over_the_duration_cap_keeps_the_original",
];
const COMPARE: [&str; 7] = [
    "test_a_different_picture_is_still_rejected",
    "test_a_re_encode_of_the_same_picture_is_the_same_picture",
    "test_an_identical_file_takes_the_exact_path",
    "test_an_unreadable_file_is_undecidable_not_negative",
    "test_animated_cannot_be_decided_and_says_so",
    "test_the_exact_key_really_does_move",
    "test_the_same_shape_in_another_colour_is_not_our_upload",
];

pub fn source_paths() -> [&'static str; 15] {
    IDENTITIES.map(|(path, _)| path)
}

pub fn source_identity(sources: &[&Source]) -> Result<(), String> {
    if sources.len() != IDENTITIES.len() {
        return Err("reencoding assertions require fifteen pinned sources".into());
    }
    for (source, (path, hash)) in sources.iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("reencoding assertion source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub async fn observe_owned(
    root: &Path,
    executable: &Path,
    workspace: &Path,
    sources: &[&Source],
) -> Result<Value, String> {
    source_identity(sources)?;
    crate::source_oracle_process::check(
        root,
        executable,
        "reencode-tests",
        sources[0],
        &sources[1..],
        &json!({"workspace":workspace}),
    )
    .await
}

fn execute(py: Python<'_>, root: &Path, workspace: &Path, sources: &[&Source]) -> PyResult<Value> {
    let original = crate::source_image_identity::build(py, sources[0], sources[1])?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let lottie = crate::source_lottie_oracle::namespace(
        py,
        sources[2],
        sources[3],
        &[
            "_load_lottie",
            "_all_layers",
            "validate_tgs",
            "to_animated_tgs",
            "lottie_still_webp",
            "lottie_preview_webp",
        ],
    )?;
    let loader = required(&lottie, "_load_lottie")?;
    original.media.setattr("_load_lottie", &loader)?;
    let animated = crate::source_oracle_runtime::namespace(
        py,
        sources[1],
        &["_animated_content_digest"],
        &["OSError", "ValueError"],
        &original.globals,
    )?;
    original.globals.set_item(
        "_animated_content_digest",
        required(&animated, "_animated_content_digest")?,
    )?;
    original.identity.setattr(
        "UPLOAD_PHASH_TOLERANCE",
        required(&original.globals, "UPLOAD_PHASH_TOLERANCE")?,
    )?;
    let ledger = Py::new(
        py,
        crate::source_temporary::Directories {
            root: workspace.into(),
            paths: Mutex::new(Vec::new()),
        },
    )?;
    let temporary = ns.call0()?;
    temporary.setattr("TemporaryDirectory", ledger.bind(py).getattr("temporary")?)?;
    temporary.setattr("mkdtemp", ledger.bind(py).getattr("mkdtemp")?)?;
    let shutil = ns.call0()?;
    shutil.setattr("rmtree", ledger.bind(py).getattr("rmtree")?)?;
    let bindings = PyDict::new(py);
    for name in ["gzip", "json", "io"] {
        bindings.set_item(name, py.import(name)?)?;
    }
    bindings.set_item("Image", py.import("PIL.Image")?)?;
    bindings.set_item("log", &original.log)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("tempfile", &temporary)?;
    bindings.set_item("shutil", &shutil)?;
    bindings.set_item("_load_lottie", &loader)?;
    bindings.set_item("TGS_MAX_BYTES", required(&lottie, "TGS_MAX_BYTES")?)?;
    let constants =
        crate::source_oracle_runtime::namespace(py, sources[0], &[], &[], &PyDict::new(py))?;
    for name in [
        "WEBM_MAX_BYTES",
        "WEBM_MAX_SECONDS",
        "SIZE",
        "WEBM_FPS",
        "_VF",
    ] {
        crate::source_oracle_runtime::assignment(py, sources[0], name, &constants)?;
        bindings.set_item(name, required(&constants, name)?)?;
    }
    bindings.set_item("dataclass", py.import("dataclasses")?.getattr("dataclass")?)?;
    let mut registration = crate::source_oracle_runtime::ModuleRegistration::new(py, &bindings)?;
    let info = crate::source_oracle_runtime::namespace(
        py,
        sources[0],
        &["VideoInfo"],
        &["__build_class__"],
        &bindings,
    );
    registration.restore()?;
    bindings.set_item("VideoInfo", required(&info?, "VideoInfo")?)?;
    let media = crate::source_oracle_runtime::module(
        py,
        sources[0],
        &["reencode_in_place", "to_video_webm", "validate_video"],
        &["len", "str", "Exception"],
        &bindings,
        "_original_reencode_media",
    )?;
    media.setattr("_GZIP_MAGIC", required(&lottie, "_GZIP_MAGIC")?)?;
    let (run, video_calls) = video::build(
        py,
        workspace,
        [sources[0], sources[1], sources[6], sources[3]],
        &media,
        &original.globals,
    )?;
    let original_run = media.getattr("_run")?;
    let original_probe = media.getattr("probe_video")?;
    let original_ffmpeg = media.getattr("ffmpeg_path")?;
    let cap = media.getattr("TGS_MAX_BYTES")?;
    let read_bytes = py
        .import("pathlib")?
        .getattr("Path")?
        .getattr("read_bytes")?;
    let png =
        crate::source_oracle_runtime::namespace(py, sources[4], &["_make_png"], &[], &bindings)?;
    bindings.set_item("_make_png", required(&png, "_make_png")?)?;
    bindings.set_item("ImageDraw", py.import("PIL.ImageDraw")?)?;
    bindings.set_item("unittest", py.import("unittest")?)?;
    bindings.set_item("mock", py.import("unittest.mock")?)?;
    bindings.set_item("media", &media)?;
    bindings.set_item("identity", &original.identity)?;
    bindings.set_item(
        "subprocess",
        py.import("types")?.getattr("SimpleNamespace")?.call0()?,
    )?;
    required(&bindings, "subprocess")?.setattr("run", run)?;
    bindings.set_item(
        "FFMPEG_TIMEOUT",
        crate::source_logging_oracle::literal(py, sources[4], "FFMPEG_TIMEOUT", false)?,
    )?;
    let tests = crate::source_oracle_runtime::namespace(
        py,
        sources[5],
        &[
            "TestReencodeGivesUsOurOwnBytes",
            "SameImageSurvivesAReEncode",
            "AnUnderDeclaredVideoKeepsItsOriginalBytes",
            "TestSingleFrameVideoKeepsAContentKey",
        ],
        &[
            "__build_class__",
            "staticmethod",
            "range",
            "iter",
            "next",
            "str",
        ],
        &bindings,
    )?;
    let unittest = py.import("unittest")?;
    let loader = unittest.getattr("TestLoader")?.call0()?;
    let result = unittest.getattr("TestResult")?.call0()?;
    let mut cases = Vec::new();
    for (class_name, methods) in [
        ("TestReencodeGivesUsOurOwnBytes", REENCODE.as_slice()),
        ("SameImageSurvivesAReEncode", COMPARE.as_slice()),
        (
            "AnUnderDeclaredVideoKeepsItsOriginalBytes",
            DURATION.as_slice(),
        ),
        (
            "TestSingleFrameVideoKeepsAContentKey",
            SINGLE_FRAME.as_slice(),
        ),
    ] {
        let class = required(&tests, class_name)?;
        if loader
            .call_method1("getTestCaseNames", (&class,))?
            .extract::<Vec<String>>()?
            != methods
        {
            return Err(PyValueError::new_err(
                "original reencoding collection drift",
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
                        "original {class_name}.{method} reported {field}; retained"
                    )));
                }
            }
            if !media.getattr("TGS_MAX_BYTES")?.is(&cap)
                || !media.getattr("_run")?.is(&original_run)
                || !media.getattr("probe_video")?.is(&original_probe)
                || !media.getattr("ffmpeg_path")?.is(&original_ffmpeg)
                || !py
                    .import("pathlib")?
                    .getattr("Path")?
                    .getattr("read_bytes")?
                    .is(&read_bytes)
            {
                return Err(PyValueError::new_err(
                    "original reencoding cap patch was not restored",
                ));
            }
            cases.push(format!("{class_name}.{method}"));
        }
    }
    if result.getattr("testsRun")?.extract::<usize>()? != 21
        || !result.call_method0("wasSuccessful")?.extract::<bool>()?
    {
        return Err(PyValueError::new_err(
            "original reencoding execution incomplete",
        ));
    }
    let media_cases = media_tests::run(
        py,
        root,
        [sources[7], sources[4]],
        &original,
        &media,
        &lottie,
        &bindings,
    )?;
    let repaint_cases = repaint_tests::run(py, [sources[8], sources[9]], &bindings)?;
    let gate_cases = repaint_tests::gate(py, [sources[10], sources[11], sources[0]], &bindings)?;
    let preview_cases = preview_tests::run(
        py,
        root,
        workspace,
        [sources[12], sources[13], sources[14]],
        media.as_any(),
        &lottie,
        ledger.bind(py).as_any(),
    )?;
    let processes = video_calls.observed()?;
    let process_root = workspace.join("process");
    directories(workspace, &process_root, &mut 8, 0, &mut Vec::new())?;
    for (directory, expected) in [(&process_root, "logs"), (&process_root.join("logs"), "tmp")] {
        let members = std::fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        if members != BTreeSet::from([std::ffi::OsString::from(expected)]) {
            return Err(PyValueError::new_err(
                "unexpected original process scratch; retained",
            ));
        }
    }
    if std::fs::read_dir(process_root.join("logs/tmp"))?
        .next()
        .is_some()
        || crate::process_owner::cleanup_pending(&process_root)
    {
        return Err(PyValueError::new_err(
            "original video process custody unresolved; retained",
        ));
    }
    let ledger = ledger.borrow(py);
    let paths = ledger
        .paths
        .lock()
        .map_err(|_| PyRuntimeError::new_err("reencoding directory accounting poisoned"))?;
    if paths.len() != 26 + media_tests::FILES.len() + repaint_tests::FILES.len()
        || paths
            .iter()
            .any(|(_, cleaned)| !cleaned.load(Ordering::SeqCst))
    {
        return Err(PyValueError::new_err(
            "original reencoding teardown incomplete; retained",
        ));
    }
    let actual = std::fs::read_dir(workspace)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    if actual
        != paths
            .iter()
            .map(|(path, _)| path.clone())
            .chain([process_root.clone()])
            .collect::<BTreeSet<_>>()
        || crate::process_owner::cleanup_pending(workspace)
    {
        return Err(PyValueError::new_err(
            "unexpected original reencoding scratch; retained",
        ));
    }
    let allowed: [&[&str]; 25] = [
        &["f.bin"],
        &["e.webp"],
        &["c.tgs"],
        &["big.tgs"],
        &["d1.tgs", "d2.tgs"],
        &["t.webp"],
        &["a.webp"],
        &["b.webp"],
        &["a.png", "a.webp", "b.png"],
        &["a.png", "a.webp"],
        &["a.png"],
        &["a.png", "torn.png"],
        &["a.png", "b.png"],
        &["a.png", "a.webp"],
        &["ours.png", "theirs.png"],
        &["v.webm"],
        &[],
        &["v.webm"],
        &[],
        &["agree.webm"],
        &["a.webm", "b.webm"],
        &["stable.webm"],
        &["remux.webm"],
        &["one.webm"],
        &["remux.webm"],
    ];
    for ((path, _), expected) in paths.iter().zip(
        allowed
            .into_iter()
            .chain(media_tests::FILES.iter().copied())
            .chain(repaint_tests::FILES.iter().copied())
            .chain([preview_tests::FILES]),
    ) {
        directories(workspace, path, &mut 16, 0, &mut Vec::new())?;
        let members = std::fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        if members
            != expected
                .iter()
                .map(|name| std::ffi::OsString::from(*name))
                .collect()
        {
            return Err(PyValueError::new_err(
                "original reencoding file inventory drift; retained",
            ));
        }
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if !entry.metadata()?.is_file() || entry.metadata()?.len() > 2 * 1024 * 1024 {
                return Err(PyValueError::new_err(
                    "unexpected original reencoding output; retained",
                ));
            }
        }
    }
    for (path, _) in paths.iter() {
        std::fs::remove_dir_all(path)?;
    }
    std::fs::remove_dir_all(process_root)?;
    Ok(
        json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9","Pillow":"12.3.0",
        "source_hashes":IDENTITIES.into_iter().collect::<std::collections::BTreeMap<_,_>>(),
        "tests_run":21,"media_tests_run":42,"repaint_tests_run":12,"gate_tests_run":11,"preview_tests_run":1,"preview_requests":12,"preview_cases":preview_cases,"temporary_calls":68,"processes":processes,"cases":cases,"media_cases":media_cases,"repaint_cases":repaint_cases,"gate_cases":gate_cases,"errors":0,"failures":0,"skipped":0,
        "expected_failures":0,"unexpected_successes":0,
        "excluded_classes":[],"excluded_media_cases":[],
        "excluded_repaint_cases":["TheRepaintApiLivesInRepaint.test_the_public_names_live_in_repaint_only"],
        "ports":"all21 unchanged original reencoding, all42 media,12 Lottie repaint,11 flag/consent and1 complete native-preview test with12 exact original/native byte comparisons; same actual native codec in-process request port, original root/logs temporary redirected to shared ledger; original preview functions/video generator/source outputs remain independent; one repaint module-architecture assertion explicitly excluded; original static/vector/video identity, conversion, validation, decoder/cache/sample/probe/PNG/Lottie and GIF/alpha/count helpers; real Pillow/gzip/json/FFmpeg; closed original subprocess argv through existing5wall3idle capture inside15wall8idle source session instead of original120s timeout; alpha-frame helper originally has no timeout; shared delayed directories; original duration and decoder/count tests privately patch their original globals and restore; native unsupported-format refusal and actual-cap fixture remain explicit boundary adaptations"}),
    )
}

pub fn observe(root: &Path, workspace: &Path, sources: &[&Source]) -> Result<Value, String> {
    source_identity(sources)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("reencoding assertion workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect reencoding assertion workspace")?
        .next()
        .is_some()
    {
        return Err("reencoding assertion workspace must be empty".into());
    }
    crate::source_roster_media::fixture(&root)?;
    preview_tests::admit(&root)?;
    Python::initialize();
    Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        if !py
            .import("sys")
            .and_then(|sys| sys.getattr("dont_write_bytecode"))
            .and_then(|value| value.extract::<bool>())
            .map_err(|_| "cannot inspect reencoding bytecode policy")?
        {
            return Err("reencoding replay requires disabled import bytecode writes".into());
        }
        execute(py, &root, &workspace, sources).map_err(|error| {
            format!(
                "original reencoding replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })
}
