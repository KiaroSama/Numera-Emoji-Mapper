//! Real VP8/VP9 fixtures exercise alpha-preserving decode and canonical sampling.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
#[path = "support/video_timeline_cases.rs"]
mod video_timeline_cases;
static PIL_METHODS: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn original_alpha_codec_and_sampling_contracts_use_real_shared_clips() {
    let _guard = PIL_METHODS.lock().unwrap();
    use _native::codec_process::{Budget, capture_with_budget, executable};
    use pyo3::prelude::*;
    use std::collections::BTreeMap;
    use std::time::Duration;

    let fixture = tooling_fixture::Root::new("original video identity");
    let recipes = [
        ("opaque", 6, 30, "libvpx-vp9", Some(255_u8)),
        ("faint", 6, 30, "libvpx-vp9", Some(129)),
        ("clear", 6, 30, "libvpx-vp9", Some(0)),
        ("vp8_opaque", 6, 30, "libvpx", Some(255)),
        ("vp8_faint", 6, 30, "libvpx", Some(120)),
        ("fast30", 12, 30, "libvpx-vp9", None),
        ("fast15", 8, 15, "libvpx-vp9", None),
        ("single", 1, 30, "libvpx-vp9", None),
    ];
    let mut frames = BTreeMap::new();
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let image = py.import("PIL.Image")?;
        let draw = py.import("PIL.ImageDraw")?;
        for (name, count, _, _, alpha) in recipes {
            let directory = fixture.0.join(format!("{name}_frames"));
            std::fs::create_dir(&directory)?;
            for index in 0..count {
                let rgba = alpha.map_or((0, 0, 0, 255), |a| (255, 0, 0, a));
                let frame = image.call_method1("new", ("RGBA", (64, 64), rgba))?;
                if alpha.is_none() {
                    let painter = draw.call_method1("Draw", (&frame,))?;
                    let options = pyo3::types::PyDict::new(py);
                    options.set_item("fill", (255, 255, 255, 255))?;
                    if index == 0 {
                        painter.call_method("rectangle", ([0, 0, 31, 63],), Some(&options))?;
                    } else {
                        painter.call_method("ellipse", ([8, 8, 55, 55],), Some(&options))?;
                    }
                }
                let path = directory.join(format!("{index:03}.png"));
                let saved = frame.call_method1("save", (&path,));
                let closed = frame.call_method0("close");
                saved?;
                closed?;
                frames.insert(path.clone(), std::fs::read(path)?);
            }
        }
        Ok(())
    })
    .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ffmpeg = executable("ffmpeg").unwrap();
    let environment = std::env::vars_os().collect();
    let mut clips = BTreeMap::new();
    runtime.block_on(async {
        for (name, _, fps, codec, _) in recipes {
            let output = fixture.0.join(format!("{name}.webm"));
            let args = vec![
                "-y".into(),
                "-v".into(),
                "error".into(),
                "-framerate".into(),
                fps.to_string(),
                "-i".into(),
                fixture
                    .0
                    .join(format!("{name}_frames/%03d.png"))
                    .to_string_lossy()
                    .into_owned(),
                "-c:v".into(),
                codec.into(),
                "-pix_fmt".into(),
                "yuva420p".into(),
                "-auto-alt-ref".into(),
                "0".into(),
                output.to_string_lossy().into_owned(),
            ];
            capture_with_budget(
                &fixture.0,
                &ffmpeg,
                &args,
                1024 * 1024,
                None,
                &environment,
                Budget {
                    wall: Duration::from_secs(5),
                    idle: Duration::from_secs(3),
                    warning: None,
                },
            )
            .await
            .unwrap();
            assert!(!std::fs::read(&output).unwrap().is_empty());
            clips.insert(name, output);
        }
        let remux = fixture.0.join("remux.webm");
        let args = vec![
            "-y".into(),
            "-v".into(),
            "error".into(),
            "-i".into(),
            clips["opaque"].to_string_lossy().into_owned(),
            "-c".into(),
            "copy".into(),
            remux.to_string_lossy().into_owned(),
        ];
        capture_with_budget(
            &fixture.0,
            &ffmpeg,
            &args,
            1024 * 1024,
            None,
            &environment,
            Budget {
                wall: Duration::from_secs(5),
                idle: Duration::from_secs(3),
                warning: None,
            },
        )
        .await
        .unwrap();
        clips.insert("remux", remux);
        let noext = fixture.0.join("downloaded_blob");
        std::fs::write(&noext, std::fs::read(&clips["faint"]).unwrap()).unwrap();
        clips.insert("noext", noext);
        let original = clips
            .iter()
            .map(|(name, path)| (*name, std::fs::read(path).unwrap()))
            .collect::<BTreeMap<_, _>>();
        let decoder = _native::codec_video::VideoDecoder::new(&fixture.0).unwrap();
        let mut keys = BTreeMap::new();
        for name in [
            "opaque",
            "faint",
            "clear",
            "vp8_opaque",
            "vp8_faint",
            "noext",
            "remux",
        ] {
            keys.insert(name, decoder.content_key(&clips[name]).await.unwrap());
        }
        for (left, right) in [
            ("opaque", "faint"),
            ("clear", "opaque"),
            ("vp8_opaque", "vp8_faint"),
        ] {
            assert_ne!(keys[left], keys[right], "{left}/{right}: opacity collapsed");
            assert_eq!(
                decoder
                    .same_image_typed(&clips[left], &clips[right])
                    .await
                    .unwrap(),
                Some(false)
            );
        }
        assert_ne!(
            original["remux"], original["opaque"],
            "remux did not rewrite container bytes"
        );
        assert_eq!(keys["remux"], keys["opaque"]);
        assert_eq!(
            decoder.same_image(&clips["remux"], &clips["opaque"]).await,
            Some(true)
        );
        for (name, codec, expected) in [
            ("opaque", "vp9", "libvpx-vp9"),
            ("vp8_opaque", "vp8", "libvpx"),
        ] {
            assert_eq!(decoder.probe(&clips[name]).await.unwrap()["codec"], codec);
            assert_eq!(
                decoder.decoder_args(&clips[name]).await.unwrap(),
                ["-c:v", expected]
            );
        }
        assert_eq!(
            decoder.decoder_args(&clips["noext"]).await.unwrap(),
            ["-c:v", "libvpx-vp9"]
        );
        assert_eq!(keys["noext"], keys["faint"]);
        let swapped = fixture.0.join("swapped.webm");
        for (name, codec) in [("vp8_opaque", "libvpx"), ("opaque", "libvpx-vp9")] {
            std::fs::write(&swapped, &original[name]).unwrap();
            assert_eq!(
                decoder.decoder_args(&swapped).await.unwrap(),
                ["-c:v", codec]
            );
        }
        for name in ["fast30", "fast15", "opaque", "single"] {
            let merged = decoder.fingerprint(&clips[name]).await.unwrap();
            assert_eq!(
                merged["phash"],
                json!(decoder.perceptual_hash(&clips[name]).await.unwrap()),
                "{name}"
            );
            assert_eq!(
                merged["content_key"],
                decoder.content_key(&clips[name]).await.unwrap(),
                "{name}"
            );
            assert!(!merged["phash"].is_null());
            if name == "single" {
                let digest = Sha256::digest(&original[name])
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                let byte_key = format!("v:{}", &digest[..32]);
                assert!(merged["content_key"].as_str().unwrap().starts_with("v:"));
                assert_ne!(
                    merged["content_key"], byte_key,
                    "single frame became a container hash"
                );
            }
        }
        let broken = fixture.0.join("broken.webm");
        let invalid = [
            b"\x1a\x45\xdf\xa3".as_slice(),
            b"garbage".repeat(64).as_slice(),
        ]
        .concat();
        std::fs::write(&broken, &invalid).unwrap();
        assert_eq!(decoder.same_image(&broken, &clips["opaque"]).await, None);
        assert_eq!(std::fs::read(&broken).unwrap(), invalid);
        // The original private _premultiplied call is observed through a real frame and public comparison.
        assert!(
            decoder
                .frames_rgba(&clips["opaque"], 10)
                .await
                .unwrap()
                .len()
                >= 64 * 64 * 4
        );
        for (name, path) in &clips {
            assert_eq!(std::fs::read(path).unwrap(), original[name]);
        }
        for (path, bytes) in &frames {
            assert_eq!(std::fs::read(path).unwrap(), *bytes);
        }
    });
    runtime.shutdown_timeout(Duration::from_secs(1));
    assert!(!_native::process_owner::cleanup_pending(&fixture.0));
    assert_eq!(
        std::fs::read_dir(fixture.0.join("logs/tmp"))
            .unwrap()
            .count(),
        0
    );
    assert!(!std::fs::read_dir(&fixture.0).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".video-output")
    }));
    fixture.clean().unwrap();
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn original_publication_blank_rule_checks_every_complete_frame_without_validating_tgs() {
    let _guard = PIL_METHODS.lock().unwrap();
    let frame = |count: usize, alpha: u8| {
        let mut bytes = vec![0; 64 * 64 * 4];
        for pixel in bytes.as_chunks_mut::<4>().0.iter_mut().take(count) {
            pixel[3] = alpha;
        }
        bytes
    };
    for (raw, expected) in [
        (Vec::new(), true),
        (vec![0; 64 * 64 * 4 - 1], true),
        (frame(0, 255).repeat(4), false),
        (frame(3, 255).repeat(3), false),
        (frame(8, 255), false),
        (frame(9, 10), false),
        (frame(9, 11), true),
        (
            [
                frame(0, 255),
                frame(0, 255),
                frame(500, 255),
                frame(900, 255),
            ]
            .concat(),
            true,
        ),
    ] {
        assert_eq!(
            _native::publication_media::video_sample_usable(&raw),
            expected
        );
    }
    let fixture = tooling_fixture::Root::new("publication animated admission");
    let source = fixture.0.join("invalid.tgs");
    let bytes = b"not valid Lottie; upload admission does not validate it";
    std::fs::write(&source, bytes).unwrap();
    assert_eq!(
        _native::codec_worker::execute_typed_at(
            Some(&fixture.0),
            &json!({
                "operation":"publication_usable","source":source,"format":"animated"
            })
        )
        .unwrap(),
        true
    );
    assert!(
        _native::codec_worker::execute_typed_at(
            Some(&fixture.0),
            &json!({
                "operation":"usable","source":source,"format":"animated"
            })
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    let blank = fixture.0.join("blank.png");
    pyo3::Python::initialize();
    pyo3::Python::attach(|py| -> pyo3::PyResult<()> {
        use pyo3::prelude::*;
        let image = py
            .import("PIL.Image")?
            .call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))?;
        let result = (|| {
            image.call_method1("putpixel", ((5, 5), (255, 255, 255, 255)))?;
            image.call_method1("save", (&blank, "PNG"))?;
            Ok(())
        })();
        image.call_method0("close")?;
        result
    })
    .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    pyo3::Python::attach(|py| {
        use pyo3::prelude::*;
        let function =
            pyo3::wrap_pyfunction!(_native::publication_media::publication_static_usable, py)
                .unwrap();
        assert!(
            !function
                .call1((&blank,))
                .unwrap()
                .extract::<bool>()
                .unwrap()
        );
        assert!(
            function
                .call1((&source,))
                .unwrap()
                .extract::<bool>()
                .unwrap()
        );
        assert!(function.call1((fixture.0.join("missing.png"),)).is_err());
    });
    runtime.block_on(async {
        assert!(
            !_native::publication_media::usable(&fixture.0, &blank, "static")
                .await
                .unwrap()
        );
        assert!(
            _native::publication_media::usable(&fixture.0, &blank, "mixed")
                .await
                .unwrap()
        );
        assert!(
            _native::publication_media::usable(&fixture.0, &source, "static")
                .await
                .unwrap()
        );
        assert!(
            _native::publication_media::usable(
                &fixture.0,
                &fixture.0.join("missing.webm"),
                "video"
            )
            .await
            .is_err()
        );
    });
    fixture.clean().unwrap();
}

#[test]
fn video_probe_and_frozen_stream_match_pinned_source() {
    let _guard = PIL_METHODS.lock().unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/video-codec.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/video_decode.py"],
        "bab9579240a23bb8ee59c1a66e496586ca881398bb4d82e956e2fb6c2b3ff13a"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let decoder = _native::codec_video::VideoDecoder::new(&root).unwrap();
        let mut keys = Vec::new();
        for case in cases {
            let path = root.join(case["input"].as_str().unwrap());
            let hash = |bytes: &[u8]| {
                Sha256::digest(bytes)
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<String>()
            };
            assert_eq!(hash(&std::fs::read(&path).unwrap()), case["input_sha256"]);
            let info = decoder.probe(&path).await.unwrap();
            for field in [
                "width",
                "height",
                "duration",
                "codec",
                "fps",
                "has_audio",
                "container",
            ] {
                assert_eq!(
                    info[field], case["expected"][field],
                    "{} {field}",
                    case["input"]
                );
            }
            assert_eq!(
                json!(decoder.decoder_args(&path).await.unwrap()),
                case["expected"]["decoder_args"]
            );
            let raw = decoder.frames_rgba(&path, 10).await.unwrap();
            assert_eq!(
                raw.len() as u64,
                case["expected"]["frame_bytes"].as_u64().unwrap()
            );
            assert_eq!(
                hash(&raw),
                case["expected"]["rgba_sha256"],
                "{}",
                case["input"]
            );
            let fingerprint = decoder.fingerprint(&path).await.unwrap();
            assert_eq!(fingerprint["content_key"], case["expected"]["content_key"]);
            assert_eq!(fingerprint["phash"], case["expected"]["phash"]);
            keys.push(fingerprint["content_key"].as_str().unwrap().to_owned());
        }
        assert_ne!(keys[0], keys[1], "opacity is part of video identity");
        assert_eq!(keys[1], keys[2], "extensionless VP9 must keep alpha");
        let structured: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/video-fingerprint.json"
        ))
        .unwrap();
        assert_eq!(structured["source_sha256"], fixture["source_sha256"]);
        let variants = structured["cases"].as_array().unwrap();
        assert_eq!(variants.len(), 3);
        assert_ne!(
            variants[0]["expected"]["phash"],
            variants[0]["expected"]["native_first_phash"]
        );
        assert_eq!(variants[2]["expected"]["empty_sample_retried"], true);
        for case in variants {
            let path = root.join(case["input"].as_str().unwrap());
            let hash = |bytes: &[u8]| {
                Sha256::digest(bytes)
                    .iter()
                    .map(|v| format!("{v:02x}"))
                    .collect::<String>()
            };
            assert_eq!(hash(&std::fs::read(&path).unwrap()), case["input_sha256"]);
            let raw = decoder.frames_rgba(&path, 10).await.unwrap();
            assert_eq!(hash(&raw), case["expected"]["rgba_sha256"]);
            assert_eq!(raw.len() as u64, case["expected"]["frame_bytes"]);
            let info = decoder.fingerprint(&path).await.unwrap();
            assert_eq!(info["content_key"], case["expected"]["content_key"]);
            assert_eq!(info["phash"], case["expected"]["phash"]);
            assert_eq!(
                decoder.content_key(&path).await.unwrap(),
                info["content_key"]
            );
            assert_eq!(
                json!(decoder.perceptual_hash(&path).await.unwrap()),
                info["phash"]
            );
        }
        let refusal: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/video-probe-error.json"
        ))
        .unwrap();
        assert_eq!(refusal["source_revision"], fixture["source_revision"]);
        assert_eq!(
            refusal["source_sha256"],
            fixture["source_sha256"]["emojikit/media.py"]
        );
        assert_eq!(refusal["expected_failure"], true);
        let corrupt = root.join(refusal["input"].as_str().unwrap());
        let digest = Sha256::digest(std::fs::read(&corrupt).unwrap())
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>();
        assert_eq!(digest, refusal["input_sha256"]);
        assert!(decoder.probe(&corrupt).await.is_err());
        assert!(decoder.decoder_args(&corrupt).await.is_err());
        assert!(decoder.fingerprint(&corrupt).await.is_err());
        assert_eq!(
            decoder
                .same_image_typed(&corrupt, &root.join(cases[0]["input"].as_str().unwrap()))
                .await
                .unwrap(),
            None
        );
        assert_eq!(_native::codec_image::frame_phash(&[0; 16]), None);
        let timeline_fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/video-timeline.json"
        ))
        .unwrap();
        assert_eq!(timeline_fixture["source_sha256"], fixture["source_sha256"]);
        let mut clips = std::collections::BTreeMap::new();
        for case in timeline_fixture["cases"].as_array().unwrap() {
            let path = root.join(case["input"].as_str().unwrap());
            let hash = |bytes: &[u8]| {
                Sha256::digest(bytes)
                    .iter()
                    .map(|value| format!("{value:02x}"))
                    .collect::<String>()
            };
            assert_eq!(hash(&std::fs::read(&path).unwrap()), case["input_sha256"]);
            let timeline = decoder.timeline_typed(&path).await.unwrap();
            assert_eq!(hash(&timeline.rgba), case["expected"]["rgba_sha256"]);
            assert_eq!(json!(timeline.times), case["expected"]["times"]);
            assert_eq!(json!(timeline.duration), case["expected"]["duration"]);
            assert_eq!(timeline.rgba.len() as u64, case["expected"]["frame_bytes"]);
            let cached = decoder.timeline(&path).await.unwrap();
            assert!(std::sync::Arc::ptr_eq(&timeline.rgba, &cached.rgba));
            assert!(
                _native::codec_video_timeline::parse(
                    (*timeline.rgba).clone(),
                    b"",
                    timeline.duration
                )
                .is_err()
            );
            clips.insert(case["name"].as_str().unwrap(), path);
        }
        for pair in timeline_fixture["pairs"].as_array().unwrap() {
            let left = &clips[pair["left"].as_str().unwrap()];
            let right = &clips[pair["right"].as_str().unwrap()];
            assert_eq!(
                json!(decoder.same_image_typed(left, right).await.unwrap()),
                pair["expected"]
            );
            assert_eq!(
                json!(decoder.same_image_typed(right, left).await.unwrap()),
                pair["expected"]
            );
        }
        video_timeline_cases::compare(&decoder, &clips).await;
        let timing: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/video-timing.json"
        ))
        .unwrap();
        assert_eq!(
            timing["source_sha256"],
            fixture["source_sha256"]["emojikit/video_decode.py"]
        );
        for case in timing["cases"].as_array().unwrap() {
            use base64::Engine;
            let stderr = base64::engine::general_purpose::STANDARD
                .decode(case["stderr_base64"].as_str().unwrap())
                .unwrap();
            let result = _native::codec_video_timeline::parse_typed(
                std::path::Path::new("sample.webm"),
                vec![0; 64 * 64 * 4],
                &stderr,
                timing["duration"].as_f64().unwrap(),
            );
            assert_eq!(json!(result.is_ok()), case["expected"]["ok"]);
            if let Ok(value) = result {
                assert_eq!(json!(value.times), case["expected"]["times"]);
                assert_eq!(json!(value.duration), case["expected"]["duration"]);
            }
        }
        let validation: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/video-validation.json"
        ))
        .unwrap();
        assert_eq!(
            validation["source_sha256"],
            fixture["source_sha256"]["emojikit/media.py"]
        );
        for case in validation["cases"].as_array().unwrap() {
            let result = _native::codec_video_validate::validate_info(
                &case["info"],
                case["bytes"].as_u64().unwrap(),
                case["filename"].as_str().unwrap(),
            );
            assert_eq!(json!(result.err()), case["error"]);
        }
        for case in validation["blank"].as_array().unwrap() {
            let path = root.join(case["input"].as_str().unwrap());
            assert_eq!(
                json!(
                    _native::codec_video_validate::is_blank(&decoder, &path)
                        .await
                        .unwrap()
                ),
                case["blank"]
            );
        }
        let scratch = Scratch(
            root.join("logs/tmp")
                .join(format!("video swap {}", std::process::id())),
        );
        std::fs::create_dir(&scratch.0).unwrap();
        let swapped = scratch.0.join("swapped.webm");
        std::fs::write(&swapped, std::fs::read(&corrupt).unwrap()).unwrap();
        assert!(decoder.decoder_args(&swapped).await.is_err());
        for case in [&cases[3], &cases[0]] {
            std::fs::write(
                &swapped,
                std::fs::read(root.join(case["input"].as_str().unwrap())).unwrap(),
            )
            .unwrap();
            assert_eq!(
                json!(decoder.decoder_args(&swapped).await.unwrap()),
                case["expected"]["decoder_args"]
            );
            assert_eq!(
                decoder.fingerprint(&swapped).await.unwrap()["content_key"],
                case["expected"]["content_key"]
            );
        }
        std::fs::remove_dir_all(&scratch.0).unwrap();
        assert!(!scratch.0.exists());
        assert!(decoder.frames_rgba(&corrupt, 0).await.is_err());
        assert!(
            decoder
                .decoder_args(&root.join("tests/fixtures/native/static-codec/opaque.png"))
                .await
                .unwrap()
                .is_empty()
        );
    });
    video_timeline_cases::resolve(&root, &runtime);
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
}
