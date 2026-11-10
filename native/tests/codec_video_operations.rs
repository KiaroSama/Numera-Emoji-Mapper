//! Real video conversion/preview/remux observations retain independent source provenance.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn video_outputs_match_source_pixels_and_preserve_existing_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/video-operations.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["emojikit/media.py"],
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53"
    );
    let validation: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/video-validation.json"
    ))
    .unwrap();
    assert_eq!(validation["source_revision"], fixture["source_revision"]);
    assert_eq!(
        validation["source_sha256"],
        fixture["source_sha256"]["emojikit/media.py"]
    );
    let cases = validation["cases"].as_array().unwrap();
    for case in [&cases[0], &cases[8]] {
        let result = _native::codec_video_validate::validate_info_typed(
            &case["info"],
            case["bytes"].as_u64().unwrap(),
            case["filename"].as_str().unwrap(),
        );
        if let Some(expected) = case["error"].as_str() {
            let error = result.unwrap_err();
            assert_eq!(error.kind, "MediaError");
            assert_eq!(error.message, expected);
            assert!(error.handled);
            assert_eq!(
                _native::codec_video_validate::validate_info(
                    &case["info"],
                    case["bytes"].as_u64().unwrap(),
                    case["filename"].as_str().unwrap()
                )
                .unwrap_err(),
                expected
            );
        } else {
            result.unwrap();
        }
    }
    for (field, value) in [
        ("width", Value::Null),
        ("height", json!(false)),
        ("duration", json!("unknown")),
        ("duration", serde_json::from_str("1e1000").unwrap()),
        ("codec", Value::Null),
        ("fps", json!([])),
        ("has_audio", json!("false")),
        ("container", json!(12)),
    ] {
        let mut info = cases[0]["info"].clone();
        info[field] = value;
        let error = _native::codec_video_validate::validate_info_typed(&info, 1, "unverified.webm")
            .unwrap_err();
        assert_eq!(error.kind, "UnverifiedCodecError", "{field}");
        assert!(!error.handled, "{field}");
        assert!(_native::codec_error::CodecError::worker_reply(Err(error)).is_err());
    }
    let scratch = tooling_fixture::Root::new("video operations");
    let missing = scratch.0.join("missing.webm");
    let error = _native::codec_video_validate::usable_at(None, &missing).unwrap_err();
    assert_eq!(error.kind, "ValueError");
    assert_eq!(
        error.message,
        "video worker requires an explicit project root"
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let decoder = _native::codec_video::VideoDecoder::new(&root).unwrap();
        for (index, case) in fixture["conversions"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            let source = root.join(case["input"].as_str().unwrap());
            assert_eq!(hash(&std::fs::read(&source).unwrap()), case["input_sha256"]);
            let output = scratch.0.join(format!("convert-{index}.webm"));
            let request = json!({"operation":"convert","source":source,"format":"video",
                "target_format":"video","output":output});
            let worker_root = root.clone();
            let fingerprint = tokio::task::spawn_blocking(move || {
                _native::codec_worker::execute_typed_at(Some(&worker_root), &request)
            })
            .await
            .unwrap()
            .unwrap();
            assert_eq!(
                fingerprint["content_key"],
                format!("v:{}", &case["expected"]["rgba_sha256"].as_str().unwrap()[..32])
            );
            assert_eq!(fingerprint["fmt"], "video");
            assert_eq!(fingerprint["extension"], ".webm");
            let info = decoder.probe_typed(&output).await.unwrap();
            for field in [
                "width",
                "height",
                "duration",
                "fps",
                "codec",
                "has_audio",
                "container",
            ] {
                assert_eq!(info[field], case["expected"][field], "{index} {field}");
            }
            let raw = decoder.frames_rgba_typed(&output, 10).await.unwrap();
            assert_eq!(hash(&raw), case["expected"]["rgba_sha256"], "{index}");
            assert_eq!(json!(raw.len()), case["expected"]["frame_bytes"]);
            _native::codec_video_validate::validate(&decoder, &output)
                .await
                .unwrap();
            let before = std::fs::read(&output).unwrap();
            assert!(
                _native::codec_video_encode::convert(&decoder, &source, &output, 262144)
                    .await
                    .is_err()
            );
            assert_eq!(std::fs::read(&output).unwrap(), before);
        }
        for (index, case) in fixture["previews"].as_array().unwrap().iter().enumerate() {
            let source = root.join(case["input"].as_str().unwrap());
            let output = scratch.0.join(format!("preview-{index}.webp"));
            _native::codec_video_preview::preview_typed(
                &decoder,
                &source,
                &output,
                case["size"].as_u64().unwrap() as u32,
                case["fps"].as_u64().unwrap() as u32,
                case["still"].as_bool().unwrap(),
            )
            .await
            .unwrap();
            let bytes = std::fs::read(&output).unwrap();
            assert_eq!(hash(&bytes), case["webp_sha256"], "{index}");
            assert_eq!(json!(bytes.len()), case["bytes"]);
            assert!(
                _native::codec_video_preview::preview(&decoder, &source, &output, 104, 15, false)
                    .await
                    .is_err()
            );
            assert_eq!(std::fs::read(&output).unwrap(), bytes);
        }
        for (index, case) in fixture["remuxes"].as_array().unwrap().iter().enumerate() {
            let output = scratch.0.join(format!("remux-{index}.webm"));
            std::fs::copy(root.join(case["input"].as_str().unwrap()), &output).unwrap();
            let before = std::fs::read(&output).unwrap();
            let changed = if index == 1 {
                let owner = root.clone();
                let prepared = output.clone();
                let (result, warnings) = tokio::task::spawn_blocking(move || {
                    let mut warnings = Vec::new();
                    let result = _native::codec_video_encode::prepare_at(
                        Some(&owner), &prepared,
                        &json!({"reencode":true,"tint":"bad"}), &mut warnings,
                    );
                    (result, warnings)
                })
                .await
                .unwrap();
                let error = result.unwrap_err();
                assert_eq!(error.kind, "MediaError");
                assert_eq!(error.message, "tint must be #RRGGBB, got 'bad'");
                assert!(error.handled);
                assert!(warnings.is_empty());
                std::fs::read(&output).unwrap() != before
            } else if index == 0 {
                let outcome = _native::codec_video_encode::reencode_typed(&decoder, &output)
                    .await
                    .unwrap();
                assert!(outcome.warning.is_none());
                outcome.rewritten
            } else {
                _native::codec_video_encode::reencode(&decoder, &output)
                    .await
                    .unwrap()
            };
            assert_eq!(json!(changed), case["changed"]);
            assert_eq!(
                hash(&decoder.frames_rgba(&output, 10).await.unwrap()),
                case["rgba_sha256"]
            );
        }
        let refused = scratch.0.join("refused.webm");
        let source = root.join("tests/fixtures/native/video-codec/alpha.webm");
        let error = _native::codec_video_encode::convert_typed(&decoder, &source, &refused, 1)
            .await
            .unwrap_err();
        assert_eq!(error.kind, "MediaError");
        assert!(error.handled);
        assert!(error.message.starts_with("could not fit alpha.webm under 1 bytes (got "));
        assert!(error.message.ends_with(")."));
        assert_eq!(
            _native::codec_error::CodecError::worker_reply(Err(error))
                .unwrap()
                .exit,
            1
        );
        assert!(!refused.exists());
        let corrupt = scratch.0.join("corrupt.webm");
        std::fs::write(&corrupt, b"not a video").unwrap();
        assert!(
            !_native::codec_video_encode::reencode(&decoder, &corrupt)
                .await
                .unwrap()
        );
        assert_eq!(std::fs::read(&corrupt).unwrap(), b"not a video");
        let preview = scratch.0.join("refused-preview.webp");
        let error = _native::codec_video_preview::preview_typed(
            &decoder, &corrupt, &preview, 52, 15, true,
        )
        .await
        .unwrap_err();
        assert_eq!(error.kind, "UndecodableVideo");
        assert!(error.handled);
        assert!(error.message.starts_with(
            "corrupt.webm: the codec could not be read, so the decoder that preserves its alpha cannot be chosen: Command '"
        ));
        assert!(error.message.contains("returned non-zero exit status"));
        assert_eq!(
            _native::codec_error::CodecError::worker_reply(Err(error))
                .unwrap()
                .exit,
            1
        );
        assert!(!preview.exists());
        let legacy = _native::codec_video_preview::preview(
            &decoder, &corrupt, &preview, 52, 15, true,
        )
        .await
        .unwrap_err();
        assert!(legacy.starts_with("external video codec failed with exit "));
        assert!(!preview.exists());
        assert_eq!(std::fs::read(&corrupt).unwrap(), b"not a video");
        assert!(!std::fs::read_dir(&scratch.0).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".video-output")
        }));
    });
    assert!(!_native::process_owner::cleanup_pending(&root));
    scratch.clean().unwrap();
    assert!(!scratch.0.exists());
}

#[test]
fn original_video_conversion_alpha_and_real_container_refusals_are_preserved() {
    use _native::{codec_process, codec_video::VideoDecoder};
    use pyo3::prelude::*;
    use pyo3::types::{PyDict, PyList};
    use std::time::Duration;

    let scratch = tooling_fixture::Root::new("original video contracts café");
    let gif = scratch.0.join("anim.gif");
    let png = scratch.0.join("dot.png");
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = py.import("PIL.Image")?;
        let frames = PyList::empty(py);
        let saved = (|| -> PyResult<()> {
            for i in 0..6 {
                frames.append(
                    module.call_method1("new", ("RGB", (64, 64), (10 * i % 255, 50, 200)))?,
                )?;
            }
            let options = PyDict::new(py);
            options.set_item("format", "GIF")?;
            options.set_item("save_all", true)?;
            options.set_item("append_images", frames.get_slice(1, frames.len()))?;
            options.set_item("duration", 80)?;
            options.set_item("loop", 0)?;
            frames
                .get_item(0)?
                .call_method("save", (&gif,), Some(&options))?;
            Ok(())
        })();
        let mut closed = Ok(());
        for frame in frames.iter() {
            if let Err(error) = frame.call_method0("close")
                && closed.is_ok()
            {
                closed = Err(error);
            }
        }
        saved?;
        closed?;
        let image = module.call_method1("new", ("RGBA", (80, 40), (10, 200, 90, 255)))?;
        let saved = image.call_method1("save", (&png, "PNG"));
        let closed = image.call_method0("close");
        saved?;
        closed?;
        Ok(())
    })
    .unwrap();
    let gif_before = std::fs::read(&gif).unwrap();
    let png_before = std::fs::read(&png).unwrap();
    let ffmpeg = codec_process::executable("ffmpeg").unwrap();
    let environment = std::env::vars_os().collect();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let decoder = VideoDecoder::new(&scratch.0).unwrap();
        let webm = scratch.0.join("anim.webm");
        _native::codec_video_encode::convert(&decoder, &gif, &webm, 262144)
            .await
            .unwrap();
        _native::codec_video_validate::validate(&decoder, &webm)
            .await
            .unwrap();
        assert_eq!(_native::codec_format::detect(&webm).unwrap(), "video");
        let fingerprint = decoder.fingerprint(&webm).await.unwrap();
        assert!(
            fingerprint["content_key"]
                .as_str()
                .unwrap()
                .starts_with("v:")
        );
        assert_eq!(
            decoder.content_key(&webm).await.unwrap(),
            fingerprint["content_key"]
        );
        assert_eq!(
            json!(decoder.perceptual_hash(&webm).await.unwrap()),
            fingerprint["phash"]
        );
        assert!(fingerprint["phash"].is_u64());
        assert_eq!(
            decoder.decoder_args(&webm).await.unwrap(),
            ["-c:v", "libvpx-vp9"]
        );
        assert!(decoder.decoder_args(&gif).await.unwrap().is_empty());
        assert!(decoder.decoder_args(&png).await.unwrap().is_empty());
        let padded = scratch.0.join("padded.webm");
        let again = scratch.0.join("again.webm");
        _native::codec_video_encode::convert(&decoder, &png, &padded, 262144)
            .await
            .unwrap();
        _native::codec_video_encode::convert(&decoder, &padded, &again, 262144)
            .await
            .unwrap();
        let args = [
            "-v",
            "error",
            "-c:v",
            "libvpx-vp9",
            "-i",
            again.to_str().unwrap(),
            "-frames:v",
            "1",
            "-vf",
            "format=rgba",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-",
        ]
        .map(String::from);
        let frame = codec_process::capture_with_budget(
            &scratch.0,
            &ffmpeg,
            &args,
            100 * 100 * 4,
            None,
            &environment,
            codec_process::Budget {
                wall: Duration::from_secs(5),
                idle: Duration::from_secs(3),
                warning: None,
            },
        )
        .await
        .unwrap()
        .stdout;
        assert_eq!(frame.len(), 100 * 100 * 4);
        assert!(
            frame.iter().skip(3).step_by(4).any(|alpha| *alpha == 0),
            "non-square transparent padding was flattened"
        );

        for (name, size, rate, codec, audio, container, expected) in [
            ("ok.webm", "100x100", "30", "libvpx-vp9", false, None, None),
            (
                "audio.webm",
                "100x100",
                "30",
                "libvpx-vp9",
                true,
                None,
                Some("audio"),
            ),
            (
                "fast.webm",
                "100x100",
                "60",
                "libvpx-vp9",
                false,
                None,
                Some("fps"),
            ),
            (
                "vp8.webm",
                "100x100",
                "30",
                "libvpx",
                false,
                None,
                Some("codec"),
            ),
            (
                "x.mp4",
                "100x100",
                "30",
                "libvpx-vp9",
                false,
                Some("mp4"),
                Some("container"),
            ),
            (
                "big.webm",
                "512x512",
                "30",
                "libvpx-vp9",
                false,
                None,
                Some("dimensions"),
            ),
        ] {
            println!("original video validation {name}");
            let path = scratch.0.join(name);
            let mut args = vec![
                "-y".into(),
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                format!("testsrc2=size={size}:rate={rate}:duration=2"),
            ];
            if audio {
                args.extend(
                    ["-f", "lavfi", "-i", "sine=frequency=440:duration=2"].map(String::from),
                );
            }
            args.extend(["-c:v", codec].map(String::from));
            if codec == "libvpx-vp9" {
                args.extend(["-pix_fmt", "yuva420p"].map(String::from));
            }
            args.extend(["-crf", "50", "-b:v", "0"].map(String::from));
            if audio {
                args.extend(["-c:a", "libopus", "-shortest"].map(String::from));
            } else {
                args.push("-an".into());
            }
            if let Some(container) = container {
                args.extend(["-f", container].map(String::from));
            }
            args.push(path.to_str().unwrap().into());
            codec_process::capture_with_budget(
                &scratch.0,
                &ffmpeg,
                &args,
                1024 * 1024,
                None,
                &environment,
                codec_process::Budget {
                    wall: Duration::from_secs(5),
                    idle: Duration::from_secs(3),
                    warning: None,
                },
            )
            .await
            .unwrap();
            let before = std::fs::read(&path).unwrap();
            assert!(!before.is_empty());
            let result = _native::codec_video_validate::validate_typed(&decoder, &path).await;
            if let Some(expected) = expected {
                let error = result.unwrap_err();
                assert_eq!(error.kind, "MediaError", "{name}");
                assert!(error.handled, "{name}");
                assert!(error.message.contains(expected), "{name}: {error}");
                let reply = _native::codec_error::CodecError::worker_reply(Err(error)).unwrap();
                assert_eq!(reply.exit, 1);
                assert_eq!(reply.envelope["ok"], false);
            } else {
                result.unwrap();
            }
            assert_eq!(std::fs::read(path).unwrap(), before);
        }
        assert!(
            std::fs::read_dir(scratch.0.join("logs/tmp"))
                .unwrap()
                .next()
                .is_none()
        );
    });
    runtime.shutdown_timeout(Duration::from_secs(1));
    assert_eq!(std::fs::read(gif).unwrap(), gif_before);
    assert_eq!(std::fs::read(png).unwrap(), png_before);
    assert!(!_native::process_owner::cleanup_pending(&scratch.0));
    assert!(!std::fs::read_dir(&scratch.0).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".video-output")
    }));
    scratch.clean().unwrap();
}

#[test]
fn original_single_frame_key_survives_stream_copy_and_agrees_with_fingerprint() {
    use _native::{codec_process, codec_video::VideoDecoder};
    use std::time::Duration;
    let scratch = tooling_fixture::Root::new("original one-frame video café");
    let ffmpeg = codec_process::executable("ffmpeg").unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let environment = std::env::vars_os().collect();
    let source = scratch.0.join("one.webm");
    let clone = scratch.0.join("clone.webm");
    // One original30fps/one-frame encode serves all four assertions; no pixel fixture substitution.
    let encode = [
        "-y",
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=100x100:rate=30:duration=1",
        "-frames:v",
        "1",
        "-c:v",
        "libvpx-vp9",
        "-pix_fmt",
        "yuva420p",
        "-crf",
        "50",
        "-b:v",
        "0",
        "-an",
        source.to_str().unwrap(),
    ]
    .map(String::from);
    runtime.block_on(async {
        println!("original single-frame encode");
        codec_process::capture_with_budget(
            &scratch.0,
            &ffmpeg,
            &encode,
            1024 * 1024,
            None,
            &environment,
            codec_process::Budget {
                wall: Duration::from_secs(5),
                idle: Duration::from_secs(3),
                warning: None,
            },
        )
        .await
        .unwrap();
        let original = std::fs::read(&source).unwrap();
        assert!(!original.is_empty());
        let decoder = VideoDecoder::new(&scratch.0).unwrap();
        let key = decoder.content_key(&source).await.unwrap();
        let fingerprint = decoder.fingerprint(&source).await.unwrap();
        assert_eq!(fingerprint["content_key"], key);
        assert!(
            fingerprint["phash"].as_u64().is_some(),
            "a decoded frame must have a phash"
        );
        assert_eq!(decoder.content_key(&source).await.unwrap(), key);
        let copy = [
            "-y",
            "-i",
            source.to_str().unwrap(),
            "-c",
            "copy",
            clone.to_str().unwrap(),
        ]
        .map(String::from);
        println!("original single-frame independent stream copy");
        codec_process::capture_with_budget(
            &scratch.0,
            &ffmpeg,
            &copy,
            1024 * 1024,
            None,
            &environment,
            codec_process::Budget {
                wall: Duration::from_secs(5),
                idle: Duration::from_secs(3),
                warning: None,
            },
        )
        .await
        .unwrap();
        assert_ne!(
            std::fs::read(&clone).unwrap(),
            original,
            "copy fixture must change container bytes"
        );
        assert_eq!(decoder.content_key(&clone).await.unwrap(), key);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        println!("original single-frame native reencode");
        assert!(
            _native::codec_video_encode::reencode(&decoder, &source)
                .await
                .unwrap()
        );
        let remuxed = std::fs::read(&source).unwrap();
        assert_ne!(remuxed, original);
        assert_eq!(decoder.content_key(&source).await.unwrap(), key);
        assert_eq!(decoder.fingerprint(&source).await.unwrap(), fingerprint);
        assert!(
            std::fs::read_dir(scratch.0.join("logs/tmp"))
                .unwrap()
                .next()
                .is_none()
        );
        assert!(!std::fs::read_dir(&scratch.0).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".video-output")
        }));
    });
    runtime.shutdown_timeout(Duration::from_secs(1));
    assert!(!_native::process_owner::cleanup_pending(&scratch.0));
    scratch.clean().unwrap();
}
