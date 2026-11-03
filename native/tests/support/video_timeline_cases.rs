//! Original timeline consequences reuse pinned real clips, the public decoder and actual catalog resolver.
use _native::{codec_process, codec_video::VideoDecoder};
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "bounded_media.rs"]
mod bounded_media;
#[path = "read_only_api.rs"]
mod read_only_api;
#[path = "video_collision_cases.rs"]
mod video_collision_cases;

struct RestoreMethod {
    class: Py<PyAny>,
    method: &'static str,
    original: Py<PyAny>,
}
impl Drop for RestoreMethod {
    fn drop(&mut self) {
        Python::attach(|py| {
            self.class
                .bind(py)
                .setattr(self.method, self.original.bind(py))
                .expect("restore external video comparison method");
        });
    }
}

async fn cached_comparison_faults(decoder: &VideoDecoder, source: &Path) {
    // The existing clip is already decoded: faults exercise the public comparison, not another encode.
    let first = decoder.timeline_typed(source).await.unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &first.rgba,
        &decoder.timeline_typed(source).await.unwrap().rgba
    ));
    for kind in ["OSError", "TypeError", "KeyboardInterrupt", "SystemExit"] {
        let (restore, hits) = Python::attach(|py| {
            let class = py.import("PIL.Image").unwrap().getattr("Image").unwrap();
            let original = class.getattr("split").unwrap().unbind();
            let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let counter = hits.clone();
            let replacement = pyo3::types::PyCFunction::new_closure(
                py,
                None,
                None,
                move |_, _| -> PyResult<()> {
                    counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Err(match kind {
                        "OSError" => {
                            pyo3::exceptions::PyOSError::new_err("controlled video comparison")
                        }
                        "TypeError" => {
                            pyo3::exceptions::PyTypeError::new_err("controlled video comparison")
                        }
                        "KeyboardInterrupt" => pyo3::exceptions::PyKeyboardInterrupt::new_err(
                            "controlled video comparison",
                        ),
                        _ => pyo3::exceptions::PySystemExit::new_err("controlled video comparison"),
                    })
                },
            )
            .unwrap();
            class.setattr("split", replacement).unwrap();
            (
                RestoreMethod {
                    class: class.unbind(),
                    method: "split",
                    original,
                },
                hits,
            )
        });
        let result = decoder.same_image_typed(source, source).await;
        if matches!(kind, "OSError" | "TypeError") {
            assert_eq!(result.unwrap(), None);
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.kind, kind);
            assert_eq!(error.message, "controlled video comparison");
            assert!(!error.handled);
        }
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
        drop(restore);
        assert_eq!(
            decoder.same_image_typed(source, source).await.unwrap(),
            Some(true)
        );
    }
    let (restore, hits) = Python::attach(|py| {
        let class = py.import("PIL.Image").unwrap().getattr("Image").unwrap();
        let original = class.getattr("tobytes").unwrap().unbind();
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = hits.clone();
        let replacement = pyo3::types::PyCFunction::new_closure(
            py,
            None,
            None,
            move |args, _| -> PyResult<Py<pyo3::types::PyBytes>> {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(pyo3::types::PyBytes::new(args.py(), &[]).unbind())
            },
        )
        .unwrap();
        class.setattr("tobytes", replacement).unwrap();
        (
            RestoreMethod {
                class: class.unbind(),
                method: "tobytes",
                original,
            },
            hits,
        )
    });
    let error = decoder.same_image_typed(source, source).await.unwrap_err();
    assert_eq!(error.kind, "UnverifiedCodecError");
    assert_eq!(error.message, "native comparison raster is incomplete");
    assert!(!error.handled);
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 2);
    drop(restore);
    assert_eq!(
        decoder.same_image_typed(source, source).await.unwrap(),
        Some(true)
    );
    let missing = source.with_file_name("missing-comparison-input.webm");
    let error = decoder
        .same_image_typed(&missing, source)
        .await
        .unwrap_err();
    assert_eq!(error.kind, "UnverifiedCodecError");
    assert!(!error.handled);
}

pub async fn compare(decoder: &VideoDecoder, clips: &BTreeMap<&str, PathBuf>) {
    let root = super::tooling_fixture::Root::new("original timeline missing recipes");
    let recipes = ["blue-again", "fade", "solid"];
    Python::initialize();
    let mut inputs = BTreeMap::new();
    Python::attach(|py| -> PyResult<()> {
        let image = py.import("PIL.Image")?;
        for name in recipes {
            let directory = root.0.join(name);
            std::fs::create_dir(&directory)?;
            for index in 0..30 {
                let color = match name {
                    "blue-again" if index < 10 => (220, 20, 20, 255),
                    "fade" => (20, 20, 220, index * 8),
                    _ => (20, 20, 220, 255),
                };
                let frame = image.call_method1("new", ("RGBA", (100, 100), color))?;
                let path = directory.join(format!("{index:04}.png"));
                let saved = frame.call_method1("save", (&path, "PNG"));
                let closed = frame.call_method0("close");
                saved?;
                closed?;
                inputs.insert(path.clone(), std::fs::read(path)?);
            }
        }
        Ok(())
    })
    .unwrap();
    let ffmpeg = codec_process::executable("ffmpeg").unwrap();
    let environment = std::env::vars_os().collect();
    let mut generated = BTreeMap::new();
    for name in recipes {
        let output = root.0.join(format!("{name}.webm"));
        // CPU libvpx is the exact original alpha-capable recipe; a GPU encode is not equivalent.
        let args = vec![
            "-y".into(),
            "-v".into(),
            "error".into(),
            "-framerate".into(),
            "30".into(),
            "-i".into(),
            root.0
                .join(format!("{name}/%04d.png"))
                .to_str()
                .unwrap()
                .into(),
            "-c:v".into(),
            "libvpx-vp9".into(),
            "-pix_fmt".into(),
            "yuva420p".into(),
            "-auto-alt-ref".into(),
            "0".into(),
            output.to_str().unwrap().into(),
        ];
        codec_process::capture_with_budget(
            &root.0,
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
        let bytes = std::fs::read(&output).unwrap();
        assert!(!bytes.is_empty());
        generated.insert(name, (output, bytes));
    }
    assert_eq!(
        decoder
            .same_image_typed(&clips["blue"], &generated["blue-again"].0)
            .await
            .unwrap(),
        Some(true)
    );
    assert_eq!(
        decoder
            .same_image_typed(&generated["fade"].0, &generated["solid"].0)
            .await
            .unwrap(),
        Some(false)
    );
    assert_eq!(
        decoder.content_key(&clips["blue"]).await.unwrap(),
        decoder.content_key(&clips["spiked"]).await.unwrap(),
        "original one-frame collision must be real"
    );
    assert_eq!(
        decoder
            .same_image_typed(&clips["blue"], &clips["spiked"])
            .await
            .unwrap(),
        Some(false)
    );
    let broken = root.0.join("broken.webm");
    let bytes = b"\x1a\x45\xdf\xa3not a real stream";
    std::fs::write(&broken, bytes).unwrap();
    assert_eq!(
        decoder
            .same_image_typed(&clips["blue"], &broken)
            .await
            .unwrap(),
        None
    );
    assert_eq!(std::fs::read(&broken).unwrap(), bytes);
    let timeline = decoder.timeline(&clips["blue"]).await.unwrap();
    assert_eq!(timeline.times.len(), 30);
    assert!(timeline.times.len() <= 128);
    assert_eq!(timeline.rgba.len(), timeline.times.len() * 64 * 64 * 4);
    cached_comparison_faults(decoder, &clips["blue"]).await;
    video_collision_cases::alignment();
    video_collision_cases::check(&root.0, decoder).await;
    for (path, bytes) in inputs {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    for (_, (path, bytes)) in generated {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert_eq!(
        std::fs::read_dir(root.0.join("logs/tmp")).unwrap().count(),
        0
    );
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    root.clean().unwrap();
}

pub fn resolve(project: &Path, runtime: &tokio::runtime::Runtime) {
    use _native::{
        catalog::Catalog, media_native_adapter::NativeMediaAdapter, publication::Publisher,
        telegram::Telegram,
    };
    use bounded_media::BoundedCodec;
    use read_only_api::{Fixture, Reply, TOKEN};
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/video-timeline.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let mut clips = BTreeMap::new();
    for name in ["blue", "green", "spiked"] {
        let case = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap();
        let path = project.join(case["input"].as_str().unwrap());
        assert_eq!(
            _native::tooling_manifest::digest(&path).unwrap(),
            case["input_sha256"]
        );
        clips.insert(
            name,
            (
                path.clone(),
                std::fs::read(path).unwrap(),
                case["expected"]["content_key"].as_str().unwrap().to_owned(),
            ),
        );
    }
    let root = super::tooling_fixture::Root::new("original video resolver consequences");
    let codec = BoundedCodec {
        native: NativeMediaAdapter {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
            root: project.into(),
            scratch: root.0.join("unused envelopes"),
        },
        deadline: Instant::now() + Duration::from_secs(60),
    };
    assert_eq!(clips["blue"].2, clips["spiked"].2);
    for (label, served, expected) in [
        ("foreign", "green", false),
        ("ours", "blue", true),
        ("sample-collision", "spiked", false),
    ] {
        let directory = root.0.join(label);
        std::fs::create_dir(&directory).unwrap();
        let database = directory.join("catalog.db");
        let mut cat = Catalog::open(&database, "original video fixture", project).unwrap();
        cat.insert(&json!({"content_key":clips["blue"].2,"fmt":"video","file_path":clips["blue"].0,"phash":Value::Null}), "fixture").unwrap();
        // Null nominates the sole real rival without inventing an independently unrecorded phash.
        let temporary = directory.join("tmp");
        std::fs::create_dir(&temporary).unwrap();
        let sentinel = temporary.join("caller-owned");
        std::fs::write(&sentinel, b"preserve caller bytes").unwrap();
        let mut server = Fixture::new(vec![
            Reply::api(
                "getFile",
                "file_id",
                "fid",
                json!({"file_path":"probe.webm"}),
            ),
            Reply::file("probe.webm", &clips[served].1),
        ]);
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let result = Publisher { tg: &tg, cat: &mut cat, media: &codec, data: directory.clone(), runtime, config: None }
            .resolve(&json!({"file_unique_id":"fuid-probe","file_id":"fid","is_video":true,"is_animated":false}));
        let requests = server.finish();
        assert_eq!(
            result.unwrap(),
            expected.then(|| clips["blue"].2.clone()),
            "{label}"
        );
        assert_eq!(requests.unwrap().len(), 2);
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"preserve caller bytes");
        assert_eq!(std::fs::read_dir(&temporary).unwrap().count(), 1);
        drop(cat);
        let mut reopened = Catalog::open(&database, "reopen", project).unwrap();
        assert_eq!(
            reopened
                .dispatch(json!({"operation":"seen","file_unique_id":"fuid-probe"}))
                .unwrap(),
            if expected {
                json!(clips["blue"].2)
            } else {
                Value::Null
            }
        );
        assert!(reopened.dispatch(json!({"operation":"custom_id","base":"cryptoemoji","content_key":clips["blue"].2})).unwrap().is_null());
        assert_eq!(reopened.all(Some("video")).unwrap().len(), 1);
    }
    let directory = root.0.join("collision-resume");
    std::fs::create_dir(&directory).unwrap();
    let database = directory.join("catalog.db");
    let mut cat = Catalog::open(&database, "fixture", project).unwrap();
    let sampled = &clips["blue"].2;
    let collision = format!(
        "v:fixture-collision:{}",
        sampled.strip_prefix("v:").unwrap()
    );
    for (key, name, phash) in [
        (sampled.as_str(), "blue", Value::Null),
        (collision.as_str(), "spiked", json!(u64::MAX)),
    ] {
        assert!(
            cat.insert(
                &json!({"content_key":key,"fmt":"video","file_path":clips[name].0,"phash":phash}),
                "fixture"
            )
            .unwrap()
        );
    }
    let temporary = directory.join("tmp");
    std::fs::create_dir(&temporary).unwrap();
    let sentinel = temporary.join("caller-owned");
    std::fs::write(&sentinel, b"preserve caller bytes").unwrap();
    let sticker = json!({"file_id":"remux-live","file_unique_id":"fuid-collision-live","custom_emoji_id":"live","is_video":true});
    let set_reply = || {
        Reply::api(
            "getStickerSet",
            "name",
            "fixture",
            json!({"name":"fixture","stickers":[sticker.clone()]}),
        )
    };
    let mut server = Fixture::new(vec![
        set_reply(),
        Reply::api(
            "getFile",
            "file_id",
            "remux-live",
            json!({"file_path":"collision.webm"}),
        ),
        Reply::file("collision.webm", &clips["spiked"].1),
        set_reply(),
    ]);
    let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
    let mut state = json!({"name":"fixture","fmt":"video","keys":[]});
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut publisher = Publisher {
            tg: &tg,
            cat: &mut cat,
            media: &codec,
            data: directory.clone(),
            runtime,
            config: None,
        };
        publisher.reconcile(&mut state, "audit").unwrap();
        assert_eq!(state["keys"], json!([collision]));
        assert_eq!(state["live"], 1);
        let previous = state.clone();
        publisher.reconcile(&mut state, "audit").unwrap();
        assert_eq!(state, previous);
    }));
    let requests = server.finish();
    result.unwrap();
    assert_eq!(requests.unwrap().len(), 4);
    drop(cat);
    let mut reopened = Catalog::open(&database, "reopen", project).unwrap();
    assert_eq!(
        reopened
            .dispatch(json!({"operation":"seen","file_unique_id":"fuid-collision-live"}))
            .unwrap(),
        collision
    );
    assert_eq!(
        reopened
            .dispatch(json!({"operation":"custom_id","base":"audit","content_key":collision}))
            .unwrap(),
        "live"
    );
    assert_eq!(
        reopened
            .dispatch(json!({"operation":"published","base":"audit","content_key":sampled}))
            .unwrap(),
        false
    );
    assert_eq!(reopened.all(Some("video")).unwrap().len(), 2);
    drop(reopened);
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"preserve caller bytes");
    assert_eq!(std::fs::read_dir(&temporary).unwrap().count(), 1);
    assert!(Instant::now() < codec.deadline);
    for (_, (path, bytes, _)) in clips {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert!(!codec.native.scratch.exists());
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    root.clean().unwrap();
}
