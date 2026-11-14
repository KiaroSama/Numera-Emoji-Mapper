//! Original frozen clips verify interpreter-free video sampling, alpha, timing and preparation.
use numera_runtime::{codec_video::VideoDecoder, codec_video_probe::VideoInfo};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
fn fixture(name: &str) -> Value {
    serde_json::from_slice(
        &std::fs::read(project().join(format!("tests/fixtures/native/{name}.json"))).unwrap(),
    )
    .unwrap()
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = project().join(format!(
            "logs/tmp/native-video-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(!numera_runtime::process_owner::cleanup_pending(&self.0));
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}

#[test]
fn source_metadata_retains_wide_integers_nonfinite_values_and_surrogates() {
    let info=VideoInfo::from_stdout(br#"{"streams":[{"codec_type":"video","width":"-123456789012345678901234567890","height":true,"codec_name":null,"avg_frame_rate":" 3_0 / 2 "},{"codec_type":"audio"}],"format":{"duration":2,"format_name":["webm"]}}"#).unwrap();
    let value = info.to_value().unwrap();
    assert_eq!(
        value["width"].to_string(),
        "-123456789012345678901234567890"
    );
    assert_eq!(value["height"], 1);
    assert_eq!(value["codec"], "None");
    assert_eq!(value["container"], "['webm']");
    assert_eq!(value["fps"], 15.0);
    assert!(info.has_audio);
    for duration in ["NaN", "Infinity", "-Infinity", "1e1000"] {
        let info =
            VideoInfo::from_stdout(format!(r#"{{"format":{{"duration":{duration}}}}}"#).as_bytes())
                .unwrap();
        assert!(!info.duration.is_finite());
        assert_eq!(info.to_value().unwrap_err().kind, "UnverifiedCodecError");
    }
    let info=VideoInfo::from_stdout(br#"{"streams":[{"codec_type":"video","width":"-123456789012345678901234567890","height":true,"codec_name":"\ud800","avg_frame_rate":"31"}],"format":{"duration":4,"format_name":"\udfff"}}"#).unwrap();
    assert_eq!(info.codec.repr(), "'\\ud800'");
    assert_eq!(info.to_value().unwrap_err().kind, "UnverifiedCodecError");
    assert_eq!(
        numera_runtime::codec_video_validate::validate_source_info(&info, 262145, "sample.webm")
            .unwrap_err()
            .message,
        "sample.webm: dimensions -123456789012345678901234567890x1 != 100x100; duration 4.00s > 3.0s; codec '\\ud800' != 'vp9'; 31.00 fps > 30 fps; container '\\udfff' is not webm; size 262145 > 262144 bytes"
    );
    for (rate, expected) in [
        ("30", 30.),
        ("30/", 30.),
        (" 3_0 / ٢ ", 15.),
        ("30/0", 0.),
        ("30/-0", 0.),
        ("1/2/3", 0.),
        ("", 0.),
        ("bad", 0.),
        ("1/Infinity", 0.),
        ("-0/1", -0.),
        ("1e1000/1", f64::INFINITY),
    ] {
        let raw = json!({"streams":[{"codec_type":"video","avg_frame_rate":rate}]}).to_string();
        assert_eq!(
            VideoInfo::from_stdout(raw.as_bytes())
                .unwrap()
                .fps
                .to_bits(),
            expected.to_bits(),
            "{rate}"
        );
    }
}

#[test]
fn malformed_metadata_and_validation_keep_source_refusal_order() {
    for (raw, kind, message) in [
        (
            "null",
            "AttributeError",
            "'NoneType' object has no attribute 'get'",
        ),
        (
            "[]",
            "AttributeError",
            "'list' object has no attribute 'get'",
        ),
        (
            r#"{"streams":1}"#,
            "TypeError",
            "'int' object is not iterable",
        ),
        (
            r#"{"streams":[null]}"#,
            "AttributeError",
            "'NoneType' object has no attribute 'get'",
        ),
        (
            r#"{"format":null}"#,
            "AttributeError",
            "'NoneType' object has no attribute 'get'",
        ),
        (
            r#"{"streams":[{"codec_type":"video","width":"bad","height":[]}],"format":null}"#,
            "ValueError",
            "invalid literal for int() with base 10: 'bad'",
        ),
        (
            r#"{"streams":[{"codec_type":"video","width":[1]}]}"#,
            "TypeError",
            "int() argument must be a string, a bytes-like object or a real number, not 'list'",
        ),
        (
            r#"{"streams":[{"codec_type":"video","width":"later"},null]}"#,
            "AttributeError",
            "'NoneType' object has no attribute 'get'",
        ),
        (
            r#"{"streams":[{"codec_type":"video","width":Infinity}]}"#,
            "OverflowError",
            "cannot convert float infinity to integer",
        ),
        (
            r#"{"streams":[{"codec_type":"video","width":NaN}]}"#,
            "ValueError",
            "cannot convert float NaN to integer",
        ),
        (
            r#"{"format":{"duration":"bad"}}"#,
            "ValueError",
            "could not convert string to float: 'bad'",
        ),
        (
            r#"{"format":{"duration":[1]}}"#,
            "TypeError",
            "float() argument must be a string or a real number, not 'list'",
        ),
    ] {
        let error = VideoInfo::from_stdout(raw.as_bytes()).unwrap_err();
        assert_eq!(error.kind, kind, "{raw}");
        assert_eq!(error.message, message, "{raw}");
        assert_eq!(error.handled, kind == "ValueError");
    }
    for raw in [b"{".as_slice(), b"\xef\xbb\xbf{}".as_slice()] {
        assert_eq!(
            VideoInfo::from_stdout(raw).unwrap_err().kind,
            "JSONDecodeError"
        );
    }
    let cases = fixture("video-validation");
    for case in cases["cases"].as_array().unwrap() {
        let result = numera_runtime::codec_video_validate::validate_info_typed(
            &case["info"],
            case["bytes"].as_u64().unwrap(),
            case["filename"].as_str().unwrap(),
        );
        assert_eq!(
            result
                .err()
                .map(|error| json!(error.message))
                .unwrap_or(Value::Null),
            case["error"]
        );
    }
    let raw=br#"{"streams":[{"codec_type":"video","width":100,"height":100,"codec_name":"vp9","avg_frame_rate":"NaN"}],"format":{"duration":NaN,"format_name":"webm"}}"#;
    numera_runtime::codec_video_validate::validate_source_info(
        &VideoInfo::from_stdout(raw).unwrap(),
        0,
        "sample.webm",
    )
    .unwrap();
}

#[test]
fn original_clips_preserve_alpha_sampling_fingerprints_and_timeline_comparison() {
    let scratch = Scratch::new();
    let root = project();
    let decoder = VideoDecoder::new(&scratch.0).unwrap();
    runtime().block_on(async {
        for name in ["video-codec", "video-fingerprint"] {
            let manifest = fixture(name);
            for case in manifest["cases"].as_array().unwrap() {
                let path = root.join(case["input"].as_str().unwrap());
                let expected = &case["expected"];
                assert_eq!(hash(&std::fs::read(&path).unwrap()), case["input_sha256"]);
                let info = decoder.probe_typed(&path).await.unwrap();
                for key in [
                    "width",
                    "height",
                    "duration",
                    "codec",
                    "fps",
                    "has_audio",
                    "container",
                ] {
                    if let Some(value) = expected.get(key) {
                        assert_eq!(&info[key], value, "{} {key}", path.display());
                    }
                }
                let raw = decoder.frames_rgba_typed(&path, 10).await.unwrap();
                assert_eq!(raw.len() as u64, expected["frame_bytes"].as_u64().unwrap());
                assert_eq!(hash(&raw), expected["rgba_sha256"]);
                let identity = decoder.fingerprint_typed(&path).await.unwrap();
                assert_eq!(identity["content_key"], expected["content_key"]);
                assert_eq!(identity["phash"], expected["phash"]);
                if let Some(args) = expected.get("decoder_args") {
                    assert_eq!(
                        json!(decoder.decoder_args_typed(&path).await.unwrap()),
                        *args
                    );
                }
            }
        }
        let manifest = fixture("video-timeline");
        let mut timelines = BTreeMap::new();
        for case in manifest["cases"].as_array().unwrap() {
            let path = root.join(case["input"].as_str().unwrap());
            let timeline = decoder.timeline_typed(&path).await.unwrap();
            assert_eq!(hash(&timeline.rgba), case["expected"]["rgba_sha256"]);
            assert_eq!(json!(timeline.times), case["expected"]["times"]);
            assert_eq!(json!(timeline.duration), case["expected"]["duration"]);
            timelines.insert(case["name"].as_str().unwrap().to_owned(), timeline);
        }
        for pair in manifest["pairs"].as_array().unwrap() {
            assert_eq!(
                json!(numera_runtime::codec_video_timeline::same_image(
                    &timelines[pair["left"].as_str().unwrap()],
                    &timelines[pair["right"].as_str().unwrap()]
                )),
                pair["expected"]
            );
        }
        let error = decoder
            .probe_typed(&root.join("tests/fixtures/native/video-codec/truncated.webm"))
            .await
            .unwrap_err();
        assert_eq!(error.kind, "CalledProcessError");
        assert!(!error.handled);
    });
}

#[test]
fn native_timestamps_follow_original_byte_grammar_and_complete_frame_bounds() {
    use base64::Engine;
    let manifest = fixture("video-timing");
    for case in manifest["cases"].as_array().unwrap() {
        let stderr = base64::engine::general_purpose::STANDARD
            .decode(case["stderr_base64"].as_str().unwrap())
            .unwrap();
        let result = numera_runtime::codec_video_timeline::parse_typed(
            Path::new("sample.webm"),
            vec![0; 64 * 64 * 4],
            &stderr,
            manifest["duration"].as_f64().unwrap(),
        );
        assert_eq!(
            result.is_ok(),
            case["expected"]["ok"].as_bool().unwrap(),
            "{case}"
        );
        if let Ok(timeline) = result {
            assert_eq!(json!(timeline.times), case["expected"]["times"]);
            assert_eq!(json!(timeline.duration), case["expected"]["duration"]);
        }
    }
    let error = numera_runtime::codec_video_timeline::parse_typed(
        Path::new("sample.webm"),
        vec![0; 64 * 64 * 4],
        b"n:0 pts:0 pts_time:\xff",
        1.,
    )
    .err()
    .unwrap();
    assert_eq!(error.kind, "ValueError");
    assert_eq!(error.message, "could not convert string to float: b'\\xff'");
    for count in [0, 129] {
        let error = numera_runtime::codec_video_timeline::parse_typed(
            Path::new("sample.webm"),
            vec![0; count * 64 * 64 * 4],
            b"n:0 pts:0 pts_time:0",
            1.,
        )
        .err()
        .unwrap();
        assert_eq!(error.kind, "UndecodableVideo");
    }
}

#[test]
fn native_previews_vp9_conversion_and_remux_publish_complete_outputs() {
    let scratch = Scratch::new();
    let root = project();
    let decoder = VideoDecoder::new(&scratch.0).unwrap();
    let manifest = fixture("video-operations");
    let preview_frames = fixture("video-preview-frames");
    assert_eq!(
        preview_frames["source_revision"],
        manifest["source_revision"]
    );
    assert_eq!(preview_frames["source_sha256"], manifest["source_sha256"]);
    assert_eq!(preview_frames["Pillow"], "12.3.0");
    assert_eq!(
        preview_frames["cases"].as_array().unwrap().len(),
        manifest["previews"].as_array().unwrap().len()
    );
    runtime().block_on(async {
        for (index, case) in manifest["conversions"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let output = scratch.0.join(format!("converted-{index}.webm"));
            numera_runtime::codec_video_encode::convert_typed(
                &decoder,
                &root.join(case["input"].as_str().unwrap()),
                &output,
                262144,
            )
            .await
            .unwrap();
            numera_runtime::codec_video_validate::validate_typed(&decoder, &output)
                .await
                .unwrap();
            let raw = decoder.frames_rgba_typed(&output, 10).await.unwrap();
            assert_eq!(hash(&raw), case["expected"]["rgba_sha256"]);
        }
        for (index, case) in manifest["previews"].as_array().unwrap().iter().enumerate() {
            let output = scratch.0.join(format!("preview-{index}.webp"));
            let size = case["size"].as_u64().unwrap() as u32;
            let result = numera_runtime::codec_video_preview::preview_typed(
                &decoder,
                &root.join(case["input"].as_str().unwrap()),
                &output,
                size,
                case["fps"].as_u64().unwrap() as u32,
                case["still"].as_bool().unwrap(),
            )
            .await
            .unwrap();
            assert_eq!(result["bytes"], std::fs::metadata(&output).unwrap().len());
            let golden = &preview_frames["cases"][index];
            for field in ["input", "size", "fps", "still", "webp_sha256"] {
                assert_eq!(golden[field], case[field]);
            }
            assert_eq!(
                hash(&std::fs::read(root.join(case["input"].as_str().unwrap())).unwrap()),
                golden["input_sha256"]
            );
            let bytes = std::fs::read(&output).unwrap();
            let decoded = numera_runtime::native_raster::open(&output).unwrap();
            assert_eq!(decoded.rgba.dimensions(), (size, size));
            assert!(decoded.rgba.pixels().any(|p| p[3] > 0));
            assert_eq!(json!(decoded.mode), golden["expected"]["mode"]);
            let animation =
                numera_runtime::native_raster::decode_animation(&bytes, Default::default())
                    .unwrap();
            let expected_frames = golden["expected"]["frames"].as_array().unwrap();
            assert_eq!(
                animation.frames.len(),
                expected_frames.len(),
                "preview {index}: optimized frame count"
            );
            assert_eq!(json!(animation.loop_count), golden["expected"]["loop"]);
            for (frame, expected) in animation.frames.iter().zip(expected_frames) {
                assert_eq!(
                    json!([frame.rgba.width(), frame.rgba.height()]),
                    golden["expected"]["size"]
                );
                assert_eq!(json!(frame.duration_ms), expected["duration_ms"]);
                assert_eq!(hash(frame.rgba.as_raw()), expected["rgba_sha256"]);
            }
        }
        for (index, case) in manifest["remuxes"].as_array().unwrap().iter().enumerate() {
            let output = scratch.0.join(format!("remux-{index}.webm"));
            std::fs::copy(root.join(case["input"].as_str().unwrap()), &output).unwrap();
            let before = std::fs::read(&output).unwrap();
            let outcome = numera_runtime::codec_video_remux::reencode_typed(&decoder, &output)
                .await
                .unwrap();
            assert_eq!(json!(outcome.rewritten), case["changed"]);
            assert!(outcome.warning.is_none());
            assert_ne!(std::fs::read(&output).unwrap(), before);
            assert_eq!(
                hash(&decoder.frames_rgba_typed(&output, 10).await.unwrap()),
                case["rgba_sha256"]
            );
        }
        let source = root.join("tests/fixtures/native/video-codec/alpha.webm");
        let output = scratch.0.join("refused.webm");
        let error =
            numera_runtime::codec_video_encode::convert_typed(&decoder, &source, &output, 1)
                .await
                .unwrap_err();
        assert_eq!(error.kind, "MediaError");
        assert!(!output.exists());
        let gallery = scratch.0.join("gallery.webm");
        numera_runtime::codec_video_preview::gallery_typed(&source, &gallery).unwrap();
        assert_eq!(
            std::fs::read(&gallery).unwrap(),
            std::fs::read(&source).unwrap()
        );
    });
    assert!(!std::fs::read_dir(&scratch.0).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".video-output")
    }));
}
