use super::*;

mod test_media {
    use super::*;
    mod test_detection {
        use super::*;
        // tests.test_media.TestDetection.test_png_is_static
        #[test]
        fn test_png_is_static() {
            let t = Scratch::new();
            let p = png(&t.join("a.png"), [0, 128, 255, 255], (80, 60));
            assert_eq!(rt::codec_format::detect(&p).unwrap(), "static");
        }
        // tests.test_media.TestDetection.test_webp_is_static
        #[test]
        fn test_webp_is_static() {
            let t = Scratch::new();
            let p = webp(
                &t.join("a.webp"),
                &RgbaImage::from_pixel(80, 60, Rgba([0, 128, 255, 255])),
                false,
                80.0,
            );
            assert_eq!(rt::codec_format::detect(&p).unwrap(), "static");
        }
        // tests.test_media.TestDetection.test_tgs_is_animated
        #[test]
        fn test_tgs_is_animated() {
            let t = Scratch::new();
            let out = t.join("a.tgs");
            rt::codec_lottie_package::package(&fixture("lottie/red_circle_512.json"), &out)
                .unwrap();
            assert_eq!(rt::codec_format::detect(&out).unwrap(), "animated");
        }
        // tests.test_media.TestDetection.test_sticker_format_mapping
        #[test]
        fn test_sticker_format_mapping() {
            let t = Scratch::new();
            let server = TelegramFixture::new(1, &[]);
            let tg = rt::telegram::Telegram::new("synthetic".into(), server.base.clone()).unwrap();
            let mut catalog = rt::catalog::Catalog::open(&t.join("catalog.db"), UTC, &t.0).unwrap();
            let witness = FormatWitness(Default::default());
            let mut collector = rt::collector::Collector {
                cat: &mut catalog,
                media: &witness,
                data: t.0.clone(),
                threshold: -1,
            };
            let runtime = runtime();
            for flags in [
                json!({"is_video":true}),
                json!({"is_animated":true}),
                json!({}),
            ] {
                let mut sticker = flags;
                sticker["file_id"] = json!("0");
                let error = collector
                    .download_ingest(&tg, &runtime, &sticker, json!({}))
                    .unwrap_err();
                assert_eq!(error, "controlled classification witness refusal");
            }
            assert_eq!(*witness.0.lock().unwrap(), vec!["webm", "tgs", "dl"]);
            assert!(std::fs::read_dir(t.join("tmp")).unwrap().next().is_none());
        }
    }
    mod test_static_hashing {
        use super::*;
        // tests.test_media.TestStaticHashing.test_rgb_under_transparent_pixels_cannot_move_the_hash
        #[test]
        fn test_rgb_under_transparent_pixels_cannot_move_the_hash() {
            let mut shape = RgbaImage::new(64, 64);
            for x in 10..40 {
                for y in 8..52 {
                    shape[(x, y)] = Rgba([30, 160, 220, 255]);
                }
            }
            let mut other = shape.clone();
            for x in 0..64 {
                for y in 0..64 {
                    if other[(x, y)][3] == 0 {
                        other[(x, y)] = Rgba([(x * 7 % 256) as u8, (y * 13 % 256) as u8, 90, 0]);
                    }
                }
            }
            assert_eq!(
                raster::dhash(&shape).unwrap(),
                raster::dhash(&other).unwrap()
            );
        }
        // tests.test_media.TestStaticHashing.test_the_hash_still_separates_genuinely_different_art
        #[test]
        fn test_the_hash_still_separates_genuinely_different_art() {
            let mut a = RgbaImage::new(64, 64);
            let mut b = a.clone();
            for x in 4..30 {
                for y in 4..60 {
                    a[(x, y)] = Rgba([200, 40, 40, 255]);
                }
            }
            for x in 34..60 {
                for y in 4..60 {
                    b[(x, y)] = Rgba([200, 40, 40, 255]);
                }
            }
            assert!((raster::dhash(&a).unwrap() ^ raster::dhash(&b).unwrap()).count_ones() > 6);
        }
        // tests.test_media.TestStaticHashing.test_an_opaque_image_hashes_exactly_as_before
        #[test]
        fn test_an_opaque_image_hashes_exactly_as_before() {
            let t = Scratch::new();
            let mut rgba = RgbaImage::from_pixel(40, 40, Rgba([10, 90, 200, 255]));
            for x in (0..40).step_by(3) {
                for y in 0..40 {
                    rgba[(x, y)] = Rgba([240, 240, 10, 255]);
                }
            }
            let rgb = image::DynamicImage::ImageRgba8(rgba.clone()).into_rgb8();
            let path = t.join("rgb.png");
            rgb.save(&path).unwrap();
            let decoded = rt::codec_image::decoded(&path).unwrap();
            assert_eq!(decoded.mode, "RGB");
            assert_eq!(
                raster::source_dhash(&decoded).unwrap(),
                raster::dhash(&rgba).unwrap()
            );
        }
        // tests.test_media.TestStaticHashing.test_identical_content_same_key
        #[test]
        fn test_identical_content_same_key() {
            let t = Scratch::new();
            let a = png(&t.join("a.png"), [200, 30, 30, 255], (80, 60));
            let b = png(&t.join("b.png"), [200, 30, 30, 255], (80, 60));
            assert_eq!(key(&a), key(&b));
        }
        // tests.test_media.TestStaticHashing.test_different_content_different_key
        #[test]
        fn test_different_content_different_key() {
            let t = Scratch::new();
            let a = png(&t.join("a.png"), [200, 30, 30, 255], (80, 60));
            let b = png(&t.join("c.png"), [30, 200, 30, 255], (80, 60));
            assert_ne!(key(&a), key(&b));
        }
        // tests.test_media.TestStaticHashing.test_to_static_png_is_100
        #[test]
        fn test_to_static_png_is_100() {
            let t = Scratch::new();
            let source = png(&t.join("s.png"), [10, 20, 30, 255], (40, 90));
            let output = t.join("out.png");
            rt::codec_image::to_static_png(&source, &output).unwrap();
            assert_eq!(raster::open(&output).unwrap().rgba.dimensions(), (100, 100));
        }
        // tests.test_media.TestStaticHashing.test_phash_near_duplicate
        #[test]
        fn test_phash_near_duplicate() {
            let t = Scratch::new();
            let a = png(&t.join("a.png"), [120, 120, 120, 255], (64, 64));
            let b = webp(
                &t.join("b.webp"),
                &raster::open(&a).unwrap().rgba,
                true,
                80.0,
            );
            assert!((phash(&a) ^ phash(&b)).count_ones() <= 5);
        }
    }
    mod test_animated {
        use super::*;
        // tests.test_media.TestAnimated.test_tgs_roundtrip_valid
        #[test]
        fn test_tgs_roundtrip_valid() {
            let t = Scratch::new();
            let out = t.join("c.tgs");
            rt::codec_lottie_package::package(&fixture("lottie/red_circle_512.json"), &out)
                .unwrap();
            rt::codec_lottie_validate::validate_file(&out).unwrap();
            assert!(std::fs::metadata(out).unwrap().len() <= 65536);
        }
        // tests.test_media.TestAnimated.test_tgs_content_key_stable
        #[test]
        fn test_tgs_content_key_stable() {
            let t = Scratch::new();
            let a = t.join("1.tgs");
            let b = t.join("2.tgs");
            for out in [&a, &b] {
                rt::codec_lottie_package::package(&fixture("lottie/red_circle_512.json"), out)
                    .unwrap();
            }
            assert_eq!(
                rt::codec_lottie_identity::fingerprint(&a).unwrap()["content_key"],
                rt::codec_lottie_identity::fingerprint(&b).unwrap()["content_key"]
            );
        }
        // tests.test_media.TestAnimated.test_valid_512_canvas_is_preserved
        #[test]
        fn test_valid_512_canvas_is_preserved() {
            let t = Scratch::new();
            let out = t.join("c.tgs");
            rt::codec_lottie_package::package(&fixture("lottie/red_circle_512.json"), &out)
                .unwrap();
            let doc = rt::native_lottie::load(&out).unwrap().to_value().unwrap();
            assert_eq!(doc["w"], 512);
            assert_eq!(doc["h"], 512);
        }
        // tests.test_media.TestAnimated.test_wrong_canvas_is_rejected
        #[test]
        fn test_wrong_canvas_is_rejected() {
            let t = Scratch::new();
            let mut doc = lottie();
            doc["w"] = json!(100);
            doc["h"] = json!(100);
            let out = tgs(&t.join("x.tgs"), &doc);
            assert_eq!(
                rt::codec_lottie_validate::validate_file_typed(&out)
                    .unwrap_err()
                    .kind,
                "MediaError"
            );
        }
        // tests.test_media.TestAnimated.test_overlong_timeline_is_rejected
        #[test]
        fn test_overlong_timeline_is_rejected() {
            let t = Scratch::new();
            let mut doc = lottie();
            doc["op"] = json!(59940);
            let out = tgs(&t.join("x.tgs"), &doc);
            assert_eq!(
                rt::codec_lottie_validate::validate_file_typed(&out)
                    .unwrap_err()
                    .kind,
                "MediaError"
            );
        }
        // tests.test_media.TestAnimated.test_empty_timeline_is_rejected
        #[test]
        fn test_empty_timeline_is_rejected() {
            let t = Scratch::new();
            let mut doc = lottie();
            doc["ip"] = json!(0);
            doc["op"] = json!(0);
            let out = tgs(&t.join("x.tgs"), &doc);
            assert_eq!(
                rt::codec_lottie_validate::validate_file_typed(&out)
                    .unwrap_err()
                    .kind,
                "MediaError"
            );
        }
        // tests.test_media.TestAnimated.test_uncompressed_json_is_not_a_tgs
        #[test]
        fn test_uncompressed_json_is_not_a_tgs() {
            let t = Scratch::new();
            let path = file(
                &t.join("plain.tgs"),
                &serde_json::to_vec(&lottie()).unwrap(),
            );
            assert_eq!(
                rt::codec_lottie_validate::validate_file_typed(&path)
                    .unwrap_err()
                    .kind,
                "MediaError"
            );
        }
        // tests.test_media.TestAnimated.test_list_root_raises_media_error_not_attribute_error
        #[test]
        fn test_list_root_raises_media_error_not_attribute_error() {
            let t = Scratch::new();
            let path = tgs(&t.join("list.tgs"), &json!([]));
            assert_eq!(
                rt::codec_lottie_validate::validate_file_typed(&path)
                    .unwrap_err()
                    .kind,
                "MediaError"
            );
        }
        // tests.test_media.TestAnimated.test_decompression_is_bounded
        #[test]
        fn test_decompression_is_bounded() {
            let t = Scratch::new();
            let bytes = gzip(&vec![b'0'; rt::native_lottie::MAX_UNPACKED + 1024], 6, 0);
            let path = file(&t.join("bomb.tgs"), &bytes);
            assert_eq!(
                rt::codec_lottie_validate::validate_file_typed(&path)
                    .unwrap_err()
                    .kind,
                "MediaError"
            );
        }
    }
    #[path = "subtract_masks.rs"]
    mod subtract_masks_are_refused_before_telegram_refuses_them;
    mod test_fingerprint_matches_the_separate_calls {
        use super::*;
        // tests.test_media.TestFingerprintMatchesTheSeparateCalls.test_static_agrees_with_content_key_and_perceptual_hash
        #[test]
        fn test_static_agrees_with_content_key_and_perceptual_hash() {
            let t = Scratch::new();
            let path = png(&t.join("a.png"), [200, 40, 60, 255], (80, 60));
            let fingerprint = fingerprint(&path);
            assert_eq!(fingerprint["content_key"], key(&path));
            assert_eq!(fingerprint["phash"], json!(phash(&path)));
            assert!(key(&path).starts_with("s:"));
            assert!(!fingerprint["phash"].is_null());
        }
        // tests.test_media.TestFingerprintMatchesTheSeparateCalls.test_video_agrees_with_content_key_and_perceptual_hash
        #[test]
        fn test_video_agrees_with_content_key_and_perceptual_hash() {
            let t = Scratch::new();
            let source = gif(&t.join("anim.gif"));
            let output = t.join("anim.webm");
            video(&t.0, &source, &output);
            let d = decoder(&t.0);
            let runtime = runtime();
            let f = runtime.block_on(d.fingerprint(&output)).unwrap();
            assert_eq!(
                f["content_key"],
                runtime.block_on(d.content_key(&output)).unwrap()
            );
            assert_eq!(
                f["phash"],
                json!(runtime.block_on(d.perceptual_hash(&output)).unwrap())
            );
            assert!(f["content_key"].as_str().unwrap().starts_with("v:"));
        }
        // tests.test_media.TestFingerprintMatchesTheSeparateCalls.test_a_transparent_webm_survives_a_re_encode
        #[test]
        fn test_a_transparent_webm_survives_a_re_encode() {
            let t = Scratch::new();
            let source = png(&t.join("dot.png"), [10, 200, 90, 255], (80, 40));
            let first = t.join("src.webm");
            let second = t.join("again.webm");
            video(&t.0, &source, &first);
            video(&t.0, &first, &second);
            assert!(clear_pixels(&t.0, &second) > 0);
        }
        // tests.test_media.TestFingerprintMatchesTheSeparateCalls.test_only_a_real_video_input_names_the_alpha_decoder
        #[test]
        fn test_only_a_real_video_input_names_the_alpha_decoder() {
            if !observe(
                "test_media::test_fingerprint_matches_the_separate_calls::test_only_a_real_video_input_names_the_alpha_decoder",
            ) {
                return;
            }
            let t = Scratch::new();
            let gif = gif(&t.join("a.gif"));
            let webm = t.join("src.webm");
            video(&t.0, &gif, &webm);
            let d = decoder(&t.0);
            let runtime = runtime();
            for (index, source, is_video) in [(0, &webm, true), (1, &gif, false)] {
                assert_eq!(
                    runtime.block_on(d.decoder_args(source)).unwrap(),
                    if is_video {
                        vec!["-c:v", "libvpx-vp9"]
                    } else {
                        vec![]
                    }
                );
                clear_calls();
                runtime
                    .block_on(rt::codec_video_encode::convert_typed(
                        &d,
                        source,
                        &t.join(&format!("o{index}.webm")),
                        262144,
                    ))
                    .unwrap();
                let commands = calls();
                let command = commands.last().unwrap();
                let input = command.iter().position(|v| v == "-i").unwrap();
                assert_eq!(command[..input].iter().any(|v| v == "libvpx-vp9"), is_video);
            }
        }
        // tests.test_media.TestFingerprintMatchesTheSeparateCalls.test_video_decodes_once_not_twice
        #[test]
        fn test_video_decodes_once_not_twice() {
            if !observe(
                "test_media::test_fingerprint_matches_the_separate_calls::test_video_decodes_once_not_twice",
            ) {
                return;
            }
            let t = Scratch::new();
            let source = gif(&t.join("anim.gif"));
            let out = t.join("anim.webm");
            video(&t.0, &source, &out);
            let d = decoder(&t.0);
            clear_calls();
            runtime().block_on(d.fingerprint(&out)).unwrap();
            let commands = calls();
            let decodes = commands
                .iter()
                .filter(|command| command.iter().any(|arg| arg == "-i"))
                .count();
            assert_eq!(
                decodes, 1,
                "fingerprint must perform exactly one actual frame decode"
            );
            // The process-level observer also sees the independent decoder
            // capability query, which the original media._run spy did not see.
            let capability = commands
                .iter()
                .filter(|command| !command.iter().any(|arg| arg == "-i"))
                .collect::<Vec<_>>();
            assert!(
                capability.len() <= 1,
                "capability discovery must not repeat"
            );
            assert!(capability.into_iter().all(|command| {
                command == &vec!["-v".to_owned(), "error".to_owned(), "-decoders".to_owned()]
            }));
        }
        // tests.test_media.TestFingerprintMatchesTheSeparateCalls.test_animated_has_no_raster_hash_and_still_keys
        #[test]
        fn test_animated_has_no_raster_hash_and_still_keys() {
            let t = Scratch::new();
            let path = tgs(&t.join("x.tgs"), &empty_lottie());
            let f = rt::codec_lottie_identity::fingerprint(&path).unwrap();
            assert_eq!(
                f["content_key"],
                rt::native_lottie::fingerprint(&path).unwrap()["content_key"]
            );
            assert!(f["phash"].is_null());
        }
        // tests.test_media.TestFingerprintMatchesTheSeparateCalls.test_an_unreadable_source_fails_the_same_way_it_always_did
        #[test]
        fn test_an_unreadable_source_fails_the_same_way_it_always_did() {
            let t = Scratch::new();
            let path = file(&t.join("bad.png"), b"not an image at all");
            let separate = rt::codec_image::decoded(&path).unwrap_err();
            let merged = rt::codec_image::fingerprint_typed(&path).unwrap_err();
            assert_eq!(merged.kind, separate.kind);
            assert_eq!(merged.message, separate.message);
        }
    }
    #[path = "blank_detection.rs"]
    mod test_blank_detection;
    mod test_video {
        use super::*;
        // tests.test_media.TestVideo.test_gif_to_webm_valid
        #[test]
        fn test_gif_to_webm_valid() {
            let t = Scratch::new();
            let source = gif(&t.join("anim.gif"));
            let out = t.join("anim.webm");
            video(&t.0, &source, &out);
            runtime()
                .block_on(rt::codec_video_validate::validate_typed(
                    &decoder(&t.0),
                    &out,
                ))
                .unwrap();
            assert_eq!(rt::codec_format::detect(&out).unwrap(), "video");
        }
        // tests.test_media.TestVideo.test_video_content_key
        #[test]
        fn test_video_content_key() {
            let t = Scratch::new();
            let source = gif(&t.join("anim.gif"));
            let out = t.join("anim.webm");
            video(&t.0, &source, &out);
            assert!(
                runtime()
                    .block_on(decoder(&t.0).content_key(&out))
                    .unwrap()
                    .starts_with("v:")
            );
        }
        // tests.test_media.TestVideo.test_compliant_webm_is_accepted
        #[test]
        fn test_compliant_webm_is_accepted() {
            let t = Scratch::new();
            let out = lavfi(&t.0, "ok.webm", "100x100", 30, 2.0, VP9);
            runtime()
                .block_on(rt::codec_video_validate::validate_typed(
                    &decoder(&t.0),
                    &out,
                ))
                .unwrap();
        }
        // tests.test_media.TestVideo.test_audio_stream_is_rejected
        #[test]
        fn test_audio_stream_is_rejected() {
            let t = Scratch::new();
            let out = t.join("audio.webm");
            let mut a = args(&[
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=100x100:rate=30:duration=2",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=2",
                "-c:v",
                "libvpx-vp9",
                "-pix_fmt",
                "yuva420p",
                "-crf",
                "50",
                "-b:v",
                "0",
                "-c:a",
                "libopus",
                "-shortest",
            ]);
            a.push(out.to_string_lossy().into());
            ffmpeg(&t.0, &a);
            let error = runtime()
                .block_on(rt::codec_video_validate::validate_typed(
                    &decoder(&t.0),
                    &out,
                ))
                .unwrap_err();
            assert_eq!(error.kind, "MediaError");
            assert!(error.message.contains("audio"));
        }
        // tests.test_media.TestVideo.test_excessive_frame_rate_is_rejected
        #[test]
        fn test_excessive_frame_rate_is_rejected() {
            let t = Scratch::new();
            let out = lavfi(
                &t.0,
                "test_excessive_frame_rate_is_rejected.webm",
                "100x100",
                60,
                2.0,
                VP9,
            );
            let error = runtime()
                .block_on(rt::codec_video_validate::validate_typed(
                    &decoder(&t.0),
                    &out,
                ))
                .unwrap_err();
            assert_eq!(error.kind, "MediaError");
            assert!(error.message.contains("fps"));
        }
        // tests.test_media.TestVideo.test_wrong_codec_is_rejected
        #[test]
        fn test_wrong_codec_is_rejected() {
            let t = Scratch::new();
            let out = lavfi(
                &t.0,
                "test_wrong_codec_is_rejected.webm",
                "100x100",
                30,
                2.0,
                &["-c:v", "libvpx", "-crf", "50", "-b:v", "0", "-an"],
            );
            let error = runtime()
                .block_on(rt::codec_video_validate::validate_typed(
                    &decoder(&t.0),
                    &out,
                ))
                .unwrap_err();
            assert_eq!(error.kind, "MediaError");
            assert!(error.message.contains("codec"));
        }
        // tests.test_media.TestVideo.test_wrong_container_is_rejected
        #[test]
        fn test_wrong_container_is_rejected() {
            let t = Scratch::new();
            let out = lavfi(
                &t.0,
                "test_wrong_container_is_rejected.webm",
                "100x100",
                30,
                2.0,
                &[
                    "-c:v",
                    "libvpx-vp9",
                    "-pix_fmt",
                    "yuva420p",
                    "-crf",
                    "50",
                    "-b:v",
                    "0",
                    "-an",
                    "-f",
                    "mp4",
                ],
            );
            let error = runtime()
                .block_on(rt::codec_video_validate::validate_typed(
                    &decoder(&t.0),
                    &out,
                ))
                .unwrap_err();
            assert_eq!(error.kind, "MediaError");
            assert!(error.message.contains("container"));
        }
        // tests.test_media.TestVideo.test_wrong_dimensions_are_rejected
        #[test]
        fn test_wrong_dimensions_are_rejected() {
            let t = Scratch::new();
            let out = lavfi(
                &t.0,
                "test_wrong_dimensions_are_rejected.webm",
                "512x512",
                30,
                2.0,
                VP9,
            );
            let error = runtime()
                .block_on(rt::codec_video_validate::validate_typed(
                    &decoder(&t.0),
                    &out,
                ))
                .unwrap_err();
            assert_eq!(error.kind, "MediaError");
            assert!(error.message.contains("dimensions"));
        }
    }
}
