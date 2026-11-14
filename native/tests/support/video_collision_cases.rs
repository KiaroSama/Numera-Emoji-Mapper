//! Original lossless video recipes retain terminal frames, presentation times and cache identities.
use _native::{codec_process, codec_video::VideoDecoder, codec_video_timeline};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

pub async fn check(root: &Path, decoder: &VideoDecoder) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let red = [220_u8, 20, 20, 255].repeat(100 * 100);
    let blue = [20_u8, 20, 220, 255].repeat(100 * 100);
    let ffmpeg = codec_process::executable("ffmpeg").unwrap();
    let environment = std::env::vars_os().collect();
    let mut inputs = BTreeMap::<PathBuf, Vec<u8>>::new();
    let mut clips = BTreeMap::new();
    let pattern = (0..100)
        .flat_map(|y| {
            (0..100).flat_map(move |x| [2 * x as u8, 2 * y as u8, ((x + y) % 256) as u8, 255])
        })
        .collect::<Vec<_>>();
    for name in ["red", "short", "first", "middle", "last", "vfr", "pattern"] {
        let count = match name {
            "short" => 29,
            "vfr" => 4,
            "pattern" => 6,
            _ => 30,
        };
        let raw = (0..count)
            .flat_map(|index| {
                let different = match name {
                    "first" => index == 0,
                    "middle" => index == 15,
                    "last" => index == 29,
                    "vfr" => index == 1,
                    _ => false,
                };
                (if name == "pattern" {
                    &pattern
                } else if different {
                    &blue
                } else {
                    &red
                })
                .iter()
                .copied()
            })
            .collect::<Vec<_>>();
        let source = root.join(format!("boundary-{name}.rgba"));
        std::fs::write(&source, &raw).unwrap();
        inputs.insert(source.clone(), raw);
        let output = root.join(format!("boundary-{name}.webm"));
        let mut arguments = vec![
            "-y".into(),
            "-v".into(),
            "error".into(),
            "-f".into(),
            "rawvideo".into(),
            "-pixel_format".into(),
            "rgba".into(),
            "-video_size".into(),
            "100x100".into(),
            "-framerate".into(),
            "30".into(),
            "-i".into(),
            source.to_str().unwrap().into(),
        ];
        if name == "vfr" {
            arguments.extend([
                "-vf".into(),
                "settb=1/1000,setpts=if(eq(N\\,0)\\,0\\,if(eq(N\\,1)\\,490\\,if(eq(N\\,2)\\,500\\,967)))".into(),
                "-fps_mode".into(), "passthrough".into(), "-enc_time_base".into(), "1/1000".into(),
                "-bsf:v".into(), "setts=duration=33".into(),
            ]);
        }
        // Lossless libvpx and alpha match the original oracle; no GPU encoder is equivalent.
        arguments.extend([
            "-c:v".into(),
            "libvpx-vp9".into(),
            "-threads".into(),
            "1".into(),
            "-cpu-used".into(),
            "8".into(),
            "-lossless".into(),
            "1".into(),
            "-pix_fmt".into(),
            "yuva420p".into(),
            "-auto-alt-ref".into(),
            "0".into(),
            output.to_str().unwrap().into(),
        ]);
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "video boundary fixture deadline exceeded"
        );
        codec_process::capture_with_budget(
            root,
            &ffmpeg,
            &arguments,
            1024 * 1024,
            None,
            &environment,
            codec_process::Budget {
                wall: remaining.min(Duration::from_secs(5)),
                idle: remaining.min(Duration::from_secs(3)),
                warning: None,
            },
        )
        .await
        .unwrap();
        let bytes = std::fs::read(&output).unwrap();
        assert!(!bytes.is_empty());
        inputs.insert(output.clone(), bytes);
        clips.insert(name, output);
    }
    let encoded = root.join("boundary-encoded.webm");
    let arguments = vec![
        "-y".into(),
        "-v".into(),
        "error".into(),
        "-c:v".into(),
        "libvpx-vp9".into(),
        "-i".into(),
        clips["pattern"].to_str().unwrap().into(),
        "-c:v".into(),
        "libvpx-vp9".into(),
        "-threads".into(),
        "1".into(),
        "-crf".into(),
        "35".into(),
        "-b:v".into(),
        "0".into(),
        "-pix_fmt".into(),
        "yuva420p".into(),
        "-auto-alt-ref".into(),
        "0".into(),
        encoded.to_str().unwrap().into(),
    ];
    let remaining = deadline.saturating_duration_since(Instant::now());
    assert!(!remaining.is_zero());
    codec_process::capture_with_budget(
        root,
        &ffmpeg,
        &arguments,
        1024 * 1024,
        None,
        &environment,
        codec_process::Budget {
            wall: remaining.min(Duration::from_secs(5)),
            idle: remaining.min(Duration::from_secs(3)),
            warning: None,
        },
    )
    .await
    .unwrap();
    inputs.insert(encoded.clone(), std::fs::read(&encoded).unwrap());
    clips.insert("encoded", encoded);
    for (left, right, expected) in [
        ("short", "last", false),
        ("short", "red", true),
        ("red", "first", false),
        ("red", "middle", false),
        ("red", "last", false),
        ("red", "vfr", false),
        ("pattern", "encoded", true),
    ] {
        for (a, b) in [(left, right), (right, left)] {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero());
            assert_eq!(
                tokio::time::timeout(remaining, decoder.same_image_typed(&clips[a], &clips[b]))
                    .await
                    .unwrap()
                    .unwrap(),
                Some(expected),
                "{a}/{b}",
            );
        }
    }
    let terminal = tokio::time::timeout(
        deadline.saturating_duration_since(Instant::now()),
        decoder.same_image_typed(&clips["short"], &clips["last"]),
    )
    .await
    .unwrap()
    .unwrap();
    let sticker = serde_json::json!({"file_unique_id":"fuid-live","custom_emoji_id":"live"});
    assert!(
        _native::reconcile::confirm(&serde_json::json!({
            "before":[],"after":[sticker],"content_key":"v:short","same_image":terminal,
        }))
        .unwrap_err()
        .contains("content cannot be proven ours")
    );
    let timing = tokio::time::timeout(
        deadline.saturating_duration_since(Instant::now()),
        decoder.timeline(&clips["vfr"]),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(timing.times.len(), 4);
    assert!((timing.times[2] - timing.times[1] - 0.010).abs() < 0.0005);
    assert!(timing.duration > timing.times[3]);
    assert!((timing.duration - 1.0).abs() < 0.0005);

    let size = inputs[&clips["red"]]
        .len()
        .max(inputs[&clips["first"]].len());
    let timestamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let mut aliases = Vec::new();
    for (directory, name) in [("one", "red"), ("two", "first")] {
        let folder = root.join(directory);
        std::fs::create_dir(&folder).unwrap();
        let path = folder.join("clip.webm");
        let mut bytes = inputs[&clips[name]].clone();
        bytes.resize(size, 0);
        std::fs::write(&path, &bytes).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(timestamp)
            .unwrap();
        assert_eq!(path.metadata().unwrap().modified().unwrap(), timestamp);
        inputs.insert(path.clone(), bytes);
        aliases.push(path);
    }
    assert_eq!(
        aliases[0].metadata().unwrap().len(),
        aliases[1].metadata().unwrap().len()
    );
    // Native APIs require absolute paths: equal basename/size/mtime still cannot share a cache key.
    let mut samples = Vec::new();
    for path in [&aliases[0], &aliases[1], &aliases[1]] {
        samples.push(
            tokio::time::timeout(
                deadline.saturating_duration_since(Instant::now()),
                decoder.frames_rgba(path, 30),
            )
            .await
            .unwrap()
            .unwrap(),
        );
    }
    assert_ne!(samples[0], samples[1]);
    assert_eq!(samples[1], samples[2]);
    let directory = root.join("near-catalog");
    std::fs::create_dir(&directory).unwrap();
    let mut catalog =
        _native::catalog::Catalog::open(&directory.join("catalog.db"), "fixture", root).unwrap();
    let mut admitted = BTreeMap::new();
    for name in ["red", "last", "pattern", "encoded"] {
        let source = directory.join(format!("{name}.webm"));
        std::fs::write(&source, &inputs[&clips[name]]).unwrap();
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero());
        let fingerprint = tokio::time::timeout(remaining, decoder.fingerprint(&source))
            .await
            .unwrap()
            .unwrap();
        if name == "encoded" {
            assert_ne!(
                fingerprint["content_key"], admitted["pattern"],
                "original lossy fixture must require near matching"
            );
        }
        let result = tokio::time::timeout(
            deadline.saturating_duration_since(Instant::now()),
            _native::ingest::add_verified(
                &mut catalog,
                serde_json::json!({"content_key":fingerprint["content_key"],"fmt":"video","file_path":source,"phash":fingerprint["phash"],"file_unique_id":name}),
                "fixture", 4,
                async |a: &Path, b: &Path, _: &str| decoder.same_image_typed(a, b).await.map_err(|error| error.message),
                async |path: &Path, key: &str| _native::codec_format::collision_key(path, key),
            ),
        ).await.unwrap().unwrap();
        assert_eq!(result.1, name != "encoded", "{name}");
        if name == "encoded" {
            assert_eq!(result.0, admitted["pattern"]);
            assert!(!source.exists());
        } else {
            assert_eq!(std::fs::read(&source).unwrap(), inputs[&clips[name]]);
        }
        admitted.insert(name, result.0);
    }
    assert_ne!(admitted["red"], admitted["last"]);
    let rows = catalog.all(Some("video")).unwrap();
    assert_eq!(rows.len(), 3);
    drop(catalog);
    let mut reopened =
        _native::catalog::Catalog::open(&directory.join("catalog.db"), "reopen", root).unwrap();
    assert_eq!(reopened.all(Some("video")).unwrap(), rows);
    assert_eq!(
        reopened
            .dispatch(serde_json::json!({"operation":"seen","file_unique_id":"encoded"}))
            .unwrap(),
        admitted["pattern"]
    );
    assert_eq!(
        std::fs::read(directory.join("pattern.webm")).unwrap(),
        inputs[&clips["pattern"]]
    );
    drop(reopened);
    assert!(Instant::now() < deadline);
    for (path, bytes) in inputs {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert!(!_native::process_owner::cleanup_pending(root));
}

pub fn alignment() {
    let rgba = [
        [220_u8, 20, 20, 255].repeat(64 * 64),
        [20_u8, 20, 220, 255].repeat(64 * 64),
    ]
    .concat();
    let left = codec_video_timeline::Timeline {
        rgba: std::sync::Arc::new(rgba),
        times: vec![0.0, 0.3],
        duration: 1.0,
    };
    let right = codec_video_timeline::Timeline {
        rgba: left.rgba.clone(),
        times: vec![0.0, 0.4],
        duration: 1.0,
    };
    assert_eq!(codec_video_timeline::same_image(&left, &right), Some(false));
    assert_eq!(codec_video_timeline::same_image(&right, &left), Some(false));
}
