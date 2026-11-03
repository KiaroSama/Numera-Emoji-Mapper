//! Original R01 positive controls share four real encodes; refusals use codec_process_policy.
use _native::codec_process::{Budget, capture_with_budget, executable};
use _native::codec_video::VideoDecoder;
use pyo3::prelude::*;
use std::collections::BTreeMap;
use std::time::Duration;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[test]
fn original_fidelity_real_alpha_concurrent_callers_and_still_exemption() {
    let fixture = tooling_fixture::Root::new("original video fidelity");
    // Pinned1a36 tests/test_video_identity_fidelity.py::_encode, not the64px identity recipe.
    let recipes = [
        ("opaque", 255_u8, "libvpx-vp9"),
        ("translucent", 128, "libvpx-vp9"),
        ("vp8-opaque", 255, "libvpx"),
        ("vp8-translucent", 120, "libvpx"),
    ];
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let image = py.import("PIL.Image")?;
        for (name, alpha, _) in recipes {
            let frame = image.call_method1("new", ("RGBA", (100, 100), (200, 30, 30, alpha)))?;
            let saved = frame.call_method1("save", (fixture.0.join(format!("{name}.png")),));
            let closed = frame.call_method0("close");
            saved?;
            closed?;
        }
        let still = image.call_method1("new", ("RGBA", (80, 60), (10, 200, 90, 255)))?;
        let saved = still.call_method1("save", (fixture.0.join("still.png"),));
        let closed = still.call_method0("close");
        saved?;
        closed?;
        Ok(())
    })
    .unwrap();
    let images = recipes
        .iter()
        .map(|(name, _, _)| fixture.0.join(format!("{name}.png")))
        .chain([fixture.0.join("still.png")])
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect::<BTreeMap<_, _>>();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let ffmpeg = executable("ffmpeg").unwrap();
    let environment = std::env::vars_os().collect();
    let mut clips = BTreeMap::new();
    runtime.block_on(async {
        for (name, _, codec) in recipes {
            let input = fixture.0.join(format!("{name}.png"));
            let output = fixture.0.join(format!("{name}.webm"));
            // CPU libvpx is required by the exact original alpha-bearing recipe; no GPU equivalent.
            let args = vec![
                "-y".into(),
                "-loop".into(),
                "1".into(),
                "-t".into(),
                "1".into(),
                "-i".into(),
                input.to_string_lossy().into_owned(),
                "-c:v".into(),
                codec.into(),
                "-pix_fmt".into(),
                "yuva420p".into(),
                "-auto-alt-ref".into(),
                "0".into(),
                "-r".into(),
                "10".into(),
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
        let original_clips = clips
            .iter()
            .map(|(name, path)| (*name, std::fs::read(path).unwrap()))
            .collect::<BTreeMap<_, _>>();
        for (opaque, translucent, codec, expected) in [
            ("opaque", "translucent", "vp9", "libvpx-vp9"),
            ("vp8-opaque", "vp8-translucent", "vp8", "libvpx"),
        ] {
            let decoder = VideoDecoder::new(&fixture.0).unwrap();
            assert_eq!(decoder.probe(&clips[opaque]).await.unwrap()["codec"], codec);
            assert_eq!(
                decoder.decoder_args(&clips[opaque]).await.unwrap(),
                ["-c:v", expected]
            );
            let left = decoder.content_key(&clips[opaque]).await.unwrap();
            let right = decoder.content_key(&clips[translucent]).await.unwrap();
            assert_ne!(
                left, right,
                "original R01 alpha-only {codec} siblings collapsed"
            );
        }
        let decoder = std::sync::Arc::new(VideoDecoder::new(&fixture.0).unwrap());
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(8));
        let permits = std::sync::Arc::new(tokio::sync::Semaphore::new(2));
        let mut tasks = tokio::task::JoinSet::new();
        for _ in 0..8 {
            let decoder = decoder.clone();
            let barrier = barrier.clone();
            let permits = permits.clone();
            let clip = clips["opaque"].clone();
            tasks.spawn(async move {
                tokio::time::timeout(Duration::from_secs(10), async {
                    barrier.wait().await;
                    let _permit = permits.acquire_owned().await.unwrap();
                    decoder.decoder_args(&clip).await
                })
                .await
                .expect("concurrent decoder caller hung")
            });
        }
        let mut answers = Vec::new();
        while let Some(answer) = tasks.join_next().await {
            answers.push(answer);
        }
        assert!(tasks.is_empty());
        assert_eq!(answers.len(), 8);
        for answer in answers {
            assert_eq!(answer.unwrap().unwrap(), ["-c:v", "libvpx-vp9"]);
        }
        // A real still image must bypass probing entirely, even when capture custody would refuse.
        let still_root = fixture.0.join("still custody");
        std::fs::create_dir(&still_root).unwrap();
        std::fs::create_dir(still_root.join("logs")).unwrap();
        std::fs::write(still_root.join("logs/tmp"), b"must not capture a still").unwrap();
        let still = fixture.0.join("still.png");
        assert!(
            VideoDecoder::new(&still_root)
                .unwrap()
                .decoder_args(&still)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            std::fs::read(still_root.join("logs/tmp")).unwrap(),
            b"must not capture a still"
        );
        assert_eq!(
            std::fs::read_dir(still_root.join("logs")).unwrap().count(),
            1
        );
        assert_eq!(std::fs::read_dir(&still_root).unwrap().count(), 1);
        for (name, path) in &clips {
            assert_eq!(std::fs::read(path).unwrap(), original_clips[name]);
        }
    });
    runtime.shutdown_timeout(Duration::from_secs(1));
    for (path, bytes) in images {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    // VideoDecoder must not rewrite, remove or add a sibling to any original clip.
    assert_eq!(clips.len(), 4);
    for (name, _, _) in recipes {
        let path = &clips[name];
        let metadata = std::fs::symlink_metadata(path).unwrap();
        assert!(metadata.is_file() && !metadata.file_type().is_symlink());
        assert!(metadata.len() > 0);
    }
    assert_eq!(std::fs::read_dir(&fixture.0).unwrap().count(), 11);
    assert_eq!(
        std::fs::read_dir(fixture.0.join("logs/tmp"))
            .unwrap()
            .count(),
        0
    );
    assert!(!_native::process_owner::cleanup_pending(&fixture.0));
    fixture.clean().unwrap();
}
