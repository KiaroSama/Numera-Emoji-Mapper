use super::*;

mod test_media_bounds {
    use super::*;
    mod test_child_process_timeout {
        use super::*;
        // tests.test_media_bounds.TestChildProcessTimeout.test_every_ffmpeg_call_site_is_bounded
        #[test]
        fn test_every_ffmpeg_call_site_is_bounded() {
            let Some(fault) = fault_case(
                "test_media_bounds::test_child_process_timeout::test_every_ffmpeg_call_site_is_bounded",
            ) else {
                return;
            };
            let clip = file(
                &fault.root.join("clip.webm"),
                b"\x1a\x45\xdf\xa3 not a real video",
            );
            fault.set("hang");
            let runtime = runtime();
            for operation in [
                "probe_video",
                "to_video_webm",
                "_video_content_digest",
                "fingerprint",
                "_video_frames_rgba",
                "_first_video_frame",
            ] {
                let d = decoder(&fault.root);
                let start = std::time::Instant::now();
                let result: Result<(), String> = match operation {
                    "probe_video" => runtime
                        .block_on(d.probe_typed(&clip))
                        .map(|_| ())
                        .map_err(|e| e.message),
                    "to_video_webm" => runtime
                        .block_on(rt::codec_video_encode::convert_typed(
                            &d,
                            &clip,
                            &fault.root.join("out.webm"),
                            262144,
                        ))
                        .map(|_| ())
                        .map_err(|e| e.message),
                    "_video_content_digest" => runtime.block_on(d.content_key(&clip)).map(|_| ()),
                    "fingerprint" => runtime
                        .block_on(d.fingerprint_typed(&clip))
                        .map(|_| ())
                        .map_err(|e| e.message),
                    "_video_frames_rgba" => runtime
                        .block_on(d.frames_rgba_typed(&clip, 10))
                        .map(|_| ())
                        .map_err(|e| e.message),
                    _ => runtime.block_on(d.perceptual_hash(&clip)).map(|_| ()),
                };
                assert!(result.is_err(), "hung {operation} returned an identity");
                assert!(start.elapsed() < std::time::Duration::from_secs(60));
                assert!(!fault.root.join("out.webm").exists());
            }
            let pids = std::fs::read_to_string(&fault.pids).unwrap();
            assert_eq!(pids.lines().count(), 6);
        }
        // tests.test_media_bounds.TestChildProcessTimeout.test_timeout_is_configurable
        #[test]
        fn test_timeout_is_configurable() {
            let configured = rt::codec_process::codec_budget(Some("1")).unwrap();
            assert_eq!(configured.wall, std::time::Duration::from_secs(1));
            let fallback = rt::codec_process::codec_budget(Some("nonsense")).unwrap();
            assert_eq!(fallback.wall, std::time::Duration::from_secs(300));
            assert!(fallback.warning.is_some());
        }
        // tests.test_media_bounds.TestChildProcessTimeout.test_explicit_timeout_kills_the_child
        #[test]
        fn test_explicit_timeout_kills_the_child() {
            let Some(fault) = fault_case(
                "test_media_bounds::test_child_process_timeout::test_explicit_timeout_kills_the_child",
            ) else {
                return;
            };
            fault.set("hang");
            let start = std::time::Instant::now();
            let result = runtime().block_on(rt::codec_process::capture_with_budget(
                &fault.root,
                &fault.ffmpeg,
                &["-version".into()],
                1024 * 1024,
                None,
                &std::env::vars_os().collect(),
                rt::codec_process::Budget {
                    wall: std::time::Duration::from_millis(500),
                    idle: std::time::Duration::from_millis(500),
                    warning: None,
                },
            ));
            assert!(result.is_err());
            assert!(start.elapsed() < std::time::Duration::from_secs(60));
            assert_eq!(
                std::fs::read_to_string(&fault.pids)
                    .unwrap()
                    .lines()
                    .count(),
                1
            );
        }
    }
    mod test_phash_threshold_range {
        use super::*;
        // tests.test_media_bounds.TestPhashThresholdRange.test_usable_thresholds_are_accepted
        #[test]
        fn test_usable_thresholds_are_accepted() {
            let t = Scratch::new();
            let cat = catalog(&t.0);
            let path = png(&t.join("a.png"), [1, 2, 3, 255], (8, 8));
            for threshold in [-1, 0, 8, 16] {
                let result = runtime()
                    .block_on(rt::ingest::identify(
                        &cat,
                        &json!({"content_key":"s:a","fmt":"static","file_path":path,"phash":0}),
                        threshold,
                        async |_: &std::path::Path, _: &std::path::Path, _: &str| {
                            panic!("empty catalog")
                        },
                        async |_: &std::path::Path, _: &str| panic!("no collision"),
                    ))
                    .unwrap();
                assert_eq!(result, ("s:a".into(), true));
            }
        }
        // tests.test_media_bounds.TestPhashThresholdRange.test_out_of_range_thresholds_are_refused
        #[test]
        fn test_out_of_range_thresholds_are_refused() {
            let t = Scratch::new();
            let cat = catalog(&t.0);
            let path = png(&t.join("candidate.png"), [1, 2, 3, 255], (8, 8));
            let before = std::fs::read(&path).unwrap();
            let fingerprint = rt::codec_image::fingerprint_typed(&path).unwrap();
            let incoming = json!({"content_key":fingerprint["content_key"],"fmt":"static","file_path":path,"phash":fingerprint["phash"]});
            // Admit the very same real candidate at the actual maximum, so an
            // unrelated malformed-candidate error cannot satisfy a refusal.
            let accepted = runtime()
                .block_on(rt::ingest::identify(
                    &cat,
                    &incoming,
                    16,
                    async |_: &std::path::Path, _: &std::path::Path, _: &str| {
                        panic!("empty catalog")
                    },
                    async |_: &std::path::Path, _: &str| panic!("no collision"),
                ))
                .unwrap();
            assert_eq!(
                accepted,
                (
                    fingerprint["content_key"].as_str().unwrap().to_owned(),
                    true
                )
            );
            for threshold in [-2, 17, 64, 999] {
                let error = runtime()
                    .block_on(rt::ingest::identify(
                        &cat,
                        &incoming,
                        threshold,
                        async |_: &std::path::Path, _: &std::path::Path, _: &str| {
                            panic!("invalid threshold")
                        },
                        async |_: &std::path::Path, _: &str| panic!("invalid threshold"),
                    ))
                    .unwrap_err();
                assert_eq!(
                    error, "phash threshold must be -1 or0..16",
                    "threshold={threshold}"
                );
            }
            assert!(cat.all(None).unwrap().is_empty());
            assert_eq!(std::fs::read(path).unwrap(), before);
        }
        // tests.test_media_bounds.TestPhashThresholdRange.test_unrelated_images_survive_the_widest_allowed_threshold
        #[test]
        fn test_unrelated_images_survive_the_widest_allowed_threshold() {
            let t = Scratch::new();
            let noise = |seed: u64| {
                RgbaImage::from_fn(64, 64, |x, y| {
                    let i = u64::from(y * 64 + x);
                    let c = (((i * 2654435761 + seed * 40503) >> 7) & 255) as u8;
                    Rgba([c, c, c, 255])
                })
            };
            let a = raster::dhash(&noise(1)).unwrap();
            let b = raster::dhash(&noise(2)).unwrap();
            let distance = (a ^ b).count_ones();
            assert!(distance > 16 && distance <= 64);
            let mut cat = catalog(&t.0);
            for (name, hash) in [("a", a), ("b", b)] {
                let path = png(&t.join(&format!("{name}.png")), [1, 2, 3, 255], (8, 8));
                let (_,new)=runtime().block_on(rt::ingest::add_verified(&mut cat,json!({"content_key":format!("s:k-{name}"),"fmt":"static","file_path":path,"phash":hash}),UTC,16,async|_:&std::path::Path,_:&std::path::Path,_:&str|panic!("unrelated hashes must not nominate each other"),async|_:&std::path::Path,_:&str|panic!("distinct keys"))).unwrap();
                assert!(new);
            }
            assert_eq!(cat.all(None).unwrap().len(), 2);
        }
        // tests.test_media_bounds.TestPhashThresholdRange.test_cli_rejects_an_out_of_range_threshold
        #[test]
        fn test_cli_rejects_an_out_of_range_threshold() {
            let t = Scratch::new();
            let config = config(&t.0, &[]);
            let log = rt::logging::RunLog::open(&t.0, "threshold", vec![]);
            let result = rt::local_cli::run_with_codec::<Codec>(
                &config,
                &[
                    "--phash-threshold".into(),
                    "999".into(),
                    "--data-dir".into(),
                    t.0.to_string_lossy().into(),
                ],
                &log,
                |_| panic!("invalid threshold must refuse before codec creation"),
            );
            assert!(result.unwrap_err().contains("out of range"));
        }
        // tests.test_media_bounds.TestPhashThresholdRange.test_check_phash_threshold_refuses_the_full_hash_width
        #[test]
        fn test_check_phash_threshold_refuses_the_full_hash_width() {
            let t = Scratch::new();
            let cat = catalog(&t.0);
            let result = runtime().block_on(rt::ingest::identify(
                &cat,
                &json!({}),
                64,
                async |_: &std::path::Path, _: &std::path::Path, _: &str| {
                    panic!("full width must refuse")
                },
                async |_: &std::path::Path, _: &str| panic!("full width must refuse"),
            ));
            assert!(result.unwrap_err().contains("threshold"));
        }
    }
    mod test_blank_media_refused {
        use super::*;
        // tests.test_media_bounds.TestBlankMediaRefused.test_fully_transparent_source_is_refused
        #[test]
        fn test_fully_transparent_source_is_refused() {
            let t = Scratch::new();
            let source = png(&t.join("blank.png"), [0, 0, 0, 0], (120, 90));
            let out = t.join("out.png");
            assert_eq!(
                rt::codec_image::to_static_png_typed(&source, &out)
                    .unwrap_err()
                    .kind,
                "MediaError"
            );
            assert!(!out.exists());
        }
        // tests.test_media_bounds.TestBlankMediaRefused.test_barely_visible_source_is_refused
        #[test]
        fn test_barely_visible_source_is_refused() {
            let t = Scratch::new();
            let source = png(&t.join("blank.png"), [255, 0, 0, 10], (120, 90));
            let out = t.join("out.png");
            assert_eq!(
                rt::codec_image::to_static_png_typed(&source, &out)
                    .unwrap_err()
                    .kind,
                "MediaError"
            );
            assert!(!out.exists());
        }
        // tests.test_media_bounds.TestBlankMediaRefused.test_visible_source_still_converts
        #[test]
        fn test_visible_source_still_converts() {
            let t = Scratch::new();
            let source = png(&t.join("solid.png"), [255, 0, 0, 255], (120, 90));
            let out = t.join("out.png");
            rt::codec_image::to_static_png_typed(&source, &out).unwrap();
            assert_eq!(raster::open(&out).unwrap().rgba.dimensions(), (100, 100));
        }
        // tests.test_media_bounds.TestBlankMediaRefused.test_add_media_does_not_ingest_a_blank_image
        #[test]
        fn test_add_media_does_not_ingest_a_blank_image() {
            let t = Scratch::new();
            let source = png(&t.join("blank.png"), [0, 0, 0, 0], (120, 90));
            let data = t.join("collection");
            let c = config(&t.0, &[]);
            let log = rt::logging::RunLog::open(&t.0, "blank-ingest", vec![]);
            let result = rt::local_cli::run_with_codec(
                &c,
                &[
                    source.to_string_lossy().into(),
                    "--data-dir".into(),
                    data.to_string_lossy().into(),
                ],
                &log,
                |_| Ok(Codec::new(&t.0)),
            )
            .unwrap();
            assert_eq!(result, 4);
            assert!(
                rt::catalog::Catalog::open(&data.join("catalog.db"), UTC, &t.0)
                    .unwrap()
                    .all(None)
                    .unwrap()
                    .is_empty()
            );
        }
    }
}
