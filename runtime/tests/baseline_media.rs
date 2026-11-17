//! Individually discoverable ports of original static/video/conversion/repaint tests.
//! Original IDs and target metadata are recorded in tests/fixtures/migration/reference-cases.json.
#[path = "baseline_media_support/mod.rs"]
mod support;
use image::{Rgba, RgbaImage};
use numera_runtime as rt;
use numera_runtime::native_raster as raster;
use serde_json::{Value, json};
use support::*;
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
    mod subtract_masks_are_refused_before_telegram_refuses_them {
        use super::*;
        // tests.test_media.SubtractMasksAreRefusedBeforeTelegramRefusesThem.test_a_subtract_mask_is_rejected
        #[test]
        fn test_a_subtract_mask_is_rejected() {
            let t = Scratch::new();
            let path = tgs(&t.join("x.tgs"), &masked("s", false));
            let error = rt::codec_lottie_validate::validate_file_typed(&path).unwrap_err();
            assert_eq!(error.kind, "MediaError");
            assert!(error.message.contains("SUBTRACT"));
        }
        // tests.test_media.SubtractMasksAreRefusedBeforeTelegramRefusesThem.test_a_subtract_mask_INSIDE_A_PRECOMP_is_rejected
        #[test]
        fn test_a_subtract_mask_inside_a_precomp_is_rejected() {
            let t = Scratch::new();
            let path = tgs(&t.join("x.tgs"), &masked("s", true));
            let error = rt::codec_lottie_validate::validate_file_typed(&path).unwrap_err();
            assert_eq!(error.kind, "MediaError");
            assert!(error.message.contains("SUBTRACT"));
        }
        // tests.test_media.SubtractMasksAreRefusedBeforeTelegramRefusesThem.test_an_ADD_mask_is_still_allowed
        #[test]
        fn test_an_add_mask_is_still_allowed() {
            let t = Scratch::new();
            for precomp in [true, false] {
                let path = tgs(&t.join("x.tgs"), &masked("a", precomp));
                rt::codec_lottie_validate::validate_file_typed(&path).unwrap();
            }
        }
    }
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
    mod test_blank_detection {
        use super::*;
        // tests.test_media.TestBlankDetection.test_fully_transparent_is_blank
        #[test]
        fn test_fully_transparent_is_blank() {
            assert!(raster::is_blank(&RgbaImage::new(100, 100)));
        }
        // tests.test_media.TestBlankDetection.test_a_visible_image_is_not_blank
        #[test]
        fn test_a_visible_image_is_not_blank() {
            assert!(!raster::is_blank(&RgbaImage::from_pixel(
                100,
                100,
                Rgba([10, 20, 30, 255])
            )));
        }
        // tests.test_media.TestBlankDetection.test_the_visibility_floor_is_exact
        #[test]
        fn test_the_visibility_floor_is_exact() {
            assert!(raster::is_blank(&RgbaImage::from_pixel(
                100,
                100,
                Rgba([5, 5, 5, 10])
            )));
            assert!(!raster::is_blank(&RgbaImage::from_pixel(
                100,
                100,
                Rgba([5, 5, 5, 11])
            )));
        }
        // tests.test_media.TestBlankDetection.test_a_few_visible_pixels_still_count_as_blank
        #[test]
        fn test_a_few_visible_pixels_still_count_as_blank() {
            let mut image = RgbaImage::new(100, 100);
            for x in 0..8 {
                image[(x, 0)] = Rgba([255, 255, 255, 255]);
            }
            assert!(raster::is_blank(&image));
            image[(8, 0)] = Rgba([255, 255, 255, 255]);
            assert!(!raster::is_blank(&image));
        }
    }
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
mod test_make_emoji_pngs {
    use super::*;
    mod test_blank_guards {
        use super::*;
        // tests.test_make_emoji_pngs.TestBlankGuards.test_is_blank_transparent
        #[test]
        fn test_is_blank_transparent() {
            assert!(raster::is_blank(&RgbaImage::new(100, 100)));
        }
        // tests.test_make_emoji_pngs.TestBlankGuards.test_is_blank_opaque
        #[test]
        fn test_is_blank_opaque() {
            assert!(!raster::is_blank(&RgbaImage::from_pixel(
                100,
                100,
                Rgba([10, 120, 200, 255])
            )));
        }
        // tests.test_make_emoji_pngs.TestBlankGuards.test_convert_raster_blank_returns_false
        #[test]
        fn test_convert_raster_blank_returns_false() {
            let t = Scratch::new();
            let source = png(&t.join("blank.png"), [0, 0, 0, 0], (50, 50));
            let out = t.join("out.png");
            assert!(rt::codec_image::to_static_png(&source, &out).is_err());
            assert!(!out.exists());
        }
        // tests.test_make_emoji_pngs.TestBlankGuards.test_convert_raster_solid_returns_true_and_100
        #[test]
        fn test_convert_raster_solid_returns_true_and_100() {
            let t = Scratch::new();
            let source = png(&t.join("solid.png"), [200, 30, 30, 255], (40, 70));
            let out = t.join("out.png");
            rt::codec_image::to_static_png(&source, &out).unwrap();
            assert_eq!(raster::open(&out).unwrap().rgba.dimensions(), (100, 100));
        }
    }
    mod test_general_run {
        use super::*;
        // tests.test_make_emoji_pngs.TestGeneralRun.test_valid_output_is_reused
        #[test]
        fn test_valid_output_is_reused() {
            let c = Convert::new();
            let source = c.raster("a.png", RED, (40, 70));
            assert_eq!(c.run(), 0);
            let out = c.output.join("a.png");
            age(&out, 1700000000);
            age(&source, 1699999940);
            assert_eq!(c.run(), 0);
            assert_eq!(
                std::fs::metadata(out)
                    .unwrap()
                    .modified()
                    .unwrap()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                1700000000
            );
        }
        // tests.test_make_emoji_pngs.TestGeneralRun.test_newer_source_is_reconverted
        #[test]
        fn test_newer_source_is_reconverted() {
            let c = Convert::new();
            let source = c.raster("a.png", RED, (40, 70));
            assert_eq!(c.run(), 0);
            png(&source, BLUE, (40, 70));
            age(&c.output.join("a.png"), 1700000000);
            age(&source, 1700000060);
            assert_eq!(c.run(), 0);
            let center = c.center("a.png");
            assert!(center[2] > center[0]);
        }
        // tests.test_make_emoji_pngs.TestGeneralRun.test_corrupt_output_is_rebuilt
        #[test]
        fn test_corrupt_output_is_rebuilt() {
            let c = Convert::new();
            c.raster("a.png", RED, (40, 70));
            file(&c.output.join("a.png"), b"not a png at all");
            assert_eq!(c.run(), 0);
            let image = raster::open(&c.output.join("a.png")).unwrap().rgba;
            assert_eq!(image.dimensions(), (100, 100));
            assert!(!raster::is_blank(&image));
        }
        // tests.test_make_emoji_pngs.TestGeneralRun.test_wrong_size_output_is_rebuilt
        #[test]
        fn test_wrong_size_output_is_rebuilt() {
            let c = Convert::new();
            c.raster("a.png", RED, (40, 70));
            png(&c.output.join("a.png"), BLUE, (64, 64));
            assert_eq!(c.run(), 0);
            let image = raster::open(&c.output.join("a.png")).unwrap().rgba;
            assert_eq!(image.dimensions(), (100, 100));
            assert!(!raster::is_blank(&image));
        }
        // tests.test_make_emoji_pngs.TestGeneralRun.test_blank_output_is_rebuilt
        #[test]
        fn test_blank_output_is_rebuilt() {
            let c = Convert::new();
            c.raster("a.png", RED, (40, 70));
            png(&c.output.join("a.png"), [0, 0, 0, 0], (100, 100));
            assert_eq!(c.run(), 0);
            let image = raster::open(&c.output.join("a.png")).unwrap().rgba;
            assert_eq!(image.dimensions(), (100, 100));
            assert!(!raster::is_blank(&image));
        }
        // tests.test_make_emoji_pngs.TestGeneralRun.test_pick_sources_prefers_svg_over_rasters
        #[test]
        fn test_pick_sources_prefers_svg_over_rasters() {
            let c = Convert::new();
            c.raster("foo.png", RED, (40, 70));
            image::RgbImage::from_pixel(40, 70, image::Rgb([240, 20, 20]))
                .save(c.source.join("foo.jpg"))
                .unwrap();
            file(&c.source.join("foo.svg"), BLUE_SVG.as_bytes());
            assert_eq!(c.run(), 0);
            let calls = c.codec.calls.lock().unwrap();
            let sources = calls
                .iter()
                .filter(|v| v["operation"] == "convert")
                .map(|v| {
                    std::path::Path::new(v["source"].as_str().unwrap())
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                })
                .collect::<Vec<_>>();
            assert_eq!(sources, vec!["foo.svg"]);
        }
        // tests.test_make_emoji_pngs.TestGeneralRun.test_svg_wins_over_same_stem_png
        #[test]
        fn test_svg_wins_over_same_stem_png() {
            let c = Convert::new();
            c.raster("foo.png", RED, (40, 70));
            file(&c.source.join("foo.svg"), BLUE_SVG.as_bytes());
            assert_eq!(c.run(), 0);
            let count = std::fs::read_dir(&c.output)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|v| v.path().extension().is_some_and(|s| s == "png"))
                .count();
            assert_eq!(count, 1);
            let center = c.center("foo.png");
            assert!(center[2] > center[0]);
        }
        // tests.test_make_emoji_pngs.TestGeneralRun.test_all_failed_exits_nonzero
        #[test]
        fn test_all_failed_exits_nonzero() {
            let c = Convert::new();
            for name in ["a.png", "b.png"] {
                c.raster(name, [0, 0, 0, 0], (40, 70));
            }
            assert_eq!(c.run(), 4);
            assert!(
                std::fs::read_dir(&c.output)
                    .unwrap()
                    .filter_map(Result::ok)
                    .all(|v| v.path().extension().is_none_or(|s| s != "png"))
            );
        }
        // tests.test_make_emoji_pngs.TestGeneralRun.test_some_failed_exits_partial
        #[test]
        fn test_some_failed_exits_partial() {
            let c = Convert::new();
            c.raster("good.png", RED, (40, 70));
            c.raster("bad.png", [0, 0, 0, 0], (40, 70));
            assert_eq!(c.run(), 3);
        }
        // tests.test_make_emoji_pngs.TestGeneralRun.test_marker_names_the_file_being_converted
        #[test]
        fn test_marker_names_the_file_being_converted() {
            let c = Convert::new();
            c.raster("a.png", RED, (40, 70));
            assert_eq!(c.run(), 0);
            assert_eq!(*c.codec.marks.lock().unwrap(), vec!["a"]);
            assert!(!c.output.join(".svg_cur").exists());
        }
        // tests.test_make_emoji_pngs.TestGeneralRun.test_interrupted_file_is_quarantined_and_reported
        #[test]
        fn test_interrupted_file_is_quarantined_and_reported() {
            if !is_child() {
                let t = Scratch::new();
                let output = child(
                    "test_make_emoji_pngs::test_general_run::test_interrupted_file_is_quarantined_and_reported",
                    &t.0,
                    &[],
                );
                assert!(output.contains("REVIEW"));
                return;
            }
            let c = Convert::new();
            c.raster("a.png", RED, (40, 70));
            file(&c.output.join(".svg_cur"), b"a");
            c.run();
            assert!(!c.output.join("a.png").exists());
            assert!(
                std::fs::read_to_string(c.output.join(".svg_skip.txt"))
                    .unwrap()
                    .contains("a")
            );
        }
    }
    mod coin_defaults_point_at_the_coins_folder {
        use super::*;
        // tests.test_make_emoji_pngs.CoinDefaultsPointAtTheCoinsFolder.test_the_legacy_conversion_reads_and_writes_under_coins
        #[test]
        fn test_the_legacy_conversion_reads_and_writes_under_coins() {
            let t = Scratch::new();
            file(&t.join("coins/logos/svg/vector.svg"), BLUE_SVG.as_bytes());
            png(&t.join("coins/logos/png/raster.png"), RED, (40, 70));
            let config = config(&t.0, &[]);
            let codec = Codec::new(&t.0);
            let log = rt::logging::RunLog::open(&t.0, "legacy-defaults", vec![]);
            assert_eq!(
                rt::convert_cli::run_with_codec(&config, &[], &log, |_| codec.clone()).unwrap(),
                0
            );
            for name in ["vector", "raster"] {
                assert_eq!(
                    raster::open(&t.join(&format!("coins/logos/emoji/{name}.png")))
                        .unwrap()
                        .rgba
                        .dimensions(),
                    (100, 100)
                );
            }
            assert!(!t.join("logos").exists());
        }
        // tests.test_make_emoji_pngs.CoinDefaultsPointAtTheCoinsFolder.test_auto_keywords_load_for_the_default_coin_source
        #[test]
        fn test_auto_keywords_load_for_the_default_coin_source() {
            if !is_child() {
                let t = Scratch::new();
                let output = child(
                    "test_make_emoji_pngs::coin_defaults_point_at_the_coins_folder::test_auto_keywords_load_for_the_default_coin_source",
                    &t.0,
                    &[],
                );
                assert!(output.contains("keywords=1"));
                return;
            }
            let t = Scratch::new();
            file(
                &t.join("coins/keywords.csv"),
                b"ticker,name,format,file,keywords\na,Alpha,png,logos/png/a.png,\"a, Alpha\"\n",
            );
            let default = t.join("coins/logos/emoji");
            let corpus = t.join("corpus");
            let other = t.join("other");
            for source in [&default, &corpus, &other] {
                png(&source.join("a.png"), RED, (100, 100));
            }
            let source = match "default" {
                "default" => &default,
                "configured" => &corpus,
                _ => &other,
            };
            let c = config(
                &t.0,
                &[
                    ("FAKE_TOKEN", "x".into()),
                    (
                        "COIN_EMOJI_DIR",
                        if "default" == "configured" {
                            corpus.to_string_lossy().into()
                        } else {
                            String::new()
                        },
                    ),
                ],
            );
            let log = rt::logging::RunLog::open(&t.0, "keyword-defaults", vec!["x".into()]);
            assert_eq!(
                rt::ticker_build::run(
                    &c,
                    &[
                        "--base".into(),
                        "t".into(),
                        "--title".into(),
                        "T".into(),
                        "--user-id".into(),
                        "1".into(),
                        "--source-dir".into(),
                        source.to_string_lossy().into(),
                        "--token-env".into(),
                        "FAKE_TOKEN".into(),
                        "--dry-run".into()
                    ],
                    &log
                )
                .unwrap(),
                0
            );
        }
        // tests.test_make_emoji_pngs.CoinDefaultsPointAtTheCoinsFolder.test_auto_keywords_load_for_the_operators_coin_corpus
        #[test]
        fn test_auto_keywords_load_for_the_operators_coin_corpus() {
            if !is_child() {
                let t = Scratch::new();
                let output = child(
                    "test_make_emoji_pngs::coin_defaults_point_at_the_coins_folder::test_auto_keywords_load_for_the_operators_coin_corpus",
                    &t.0,
                    &[],
                );
                assert!(output.contains("keywords=1"));
                return;
            }
            let t = Scratch::new();
            file(
                &t.join("coins/keywords.csv"),
                b"ticker,name,format,file,keywords\na,Alpha,png,logos/png/a.png,\"a, Alpha\"\n",
            );
            let default = t.join("coins/logos/emoji");
            let corpus = t.join("corpus");
            let other = t.join("other");
            for source in [&default, &corpus, &other] {
                png(&source.join("a.png"), RED, (100, 100));
            }
            let source = match "configured" {
                "default" => &default,
                "configured" => &corpus,
                _ => &other,
            };
            let c = config(
                &t.0,
                &[
                    ("FAKE_TOKEN", "x".into()),
                    (
                        "COIN_EMOJI_DIR",
                        if "configured" == "configured" {
                            corpus.to_string_lossy().into()
                        } else {
                            String::new()
                        },
                    ),
                ],
            );
            let log = rt::logging::RunLog::open(&t.0, "keyword-defaults", vec!["x".into()]);
            assert_eq!(
                rt::ticker_build::run(
                    &c,
                    &[
                        "--base".into(),
                        "t".into(),
                        "--title".into(),
                        "T".into(),
                        "--user-id".into(),
                        "1".into(),
                        "--source-dir".into(),
                        source.to_string_lossy().into(),
                        "--token-env".into(),
                        "FAKE_TOKEN".into(),
                        "--dry-run".into()
                    ],
                    &log
                )
                .unwrap(),
                0
            );
        }
        // tests.test_make_emoji_pngs.CoinDefaultsPointAtTheCoinsFolder.test_a_general_source_gets_no_coin_keywords
        #[test]
        fn test_a_general_source_gets_no_coin_keywords() {
            if !is_child() {
                let t = Scratch::new();
                let output = child(
                    "test_make_emoji_pngs::coin_defaults_point_at_the_coins_folder::test_a_general_source_gets_no_coin_keywords",
                    &t.0,
                    &[],
                );
                assert!(output.contains("keywords=0"));
                return;
            }
            let t = Scratch::new();
            file(
                &t.join("coins/keywords.csv"),
                b"ticker,name,format,file,keywords\na,Alpha,png,logos/png/a.png,\"a, Alpha\"\n",
            );
            let default = t.join("coins/logos/emoji");
            let corpus = t.join("corpus");
            let other = t.join("other");
            for source in [&default, &corpus, &other] {
                png(&source.join("a.png"), RED, (100, 100));
            }
            let source = match "other" {
                "default" => &default,
                "configured" => &corpus,
                _ => &other,
            };
            let c = config(
                &t.0,
                &[
                    ("FAKE_TOKEN", "x".into()),
                    (
                        "COIN_EMOJI_DIR",
                        if "other" == "configured" {
                            corpus.to_string_lossy().into()
                        } else {
                            String::new()
                        },
                    ),
                ],
            );
            let log = rt::logging::RunLog::open(&t.0, "keyword-defaults", vec!["x".into()]);
            assert_eq!(
                rt::ticker_build::run(
                    &c,
                    &[
                        "--base".into(),
                        "t".into(),
                        "--title".into(),
                        "T".into(),
                        "--user-id".into(),
                        "1".into(),
                        "--source-dir".into(),
                        source.to_string_lossy().into(),
                        "--token-env".into(),
                        "FAKE_TOKEN".into(),
                        "--dry-run".into()
                    ],
                    &log
                )
                .unwrap(),
                0
            );
        }
    }
}
mod test_media_reencode {
    use super::*;
    mod test_reencode_gives_us_our_own_bytes {
        use super::*;
        // tests.test_media_reencode.TestReencodeGivesUsOurOwnBytes.test_static_bytes_change_and_pixels_do_not
        #[test]
        fn test_static_bytes_change_and_pixels_do_not() {
            let t = Scratch::new();
            let mut pixels = RgbaImage::new(80, 60);
            for x in 20..60 {
                for y in 15..45 {
                    pixels[(x, y)] = Rgba([200, 40, 90, 255]);
                }
            }
            pixels[(2, 2)] = Rgba([7, 9, 11, 0]);
            let source = webp(&t.join("t.webp"), &pixels, true, 80.0);
            let bytes = std::fs::read(&source).unwrap();
            let before = raster::open(&source).unwrap().rgba;
            assert!(rt::codec_image::reencode_static(&source).unwrap());
            assert_ne!(std::fs::read(&source).unwrap(), bytes);
            assert_eq!(raster::open(&source).unwrap().rgba, before);
            assert_eq!(before[(2, 2)].0, [7, 9, 11, 0]);
        }
        // tests.test_media_reencode.TestReencodeGivesUsOurOwnBytes.test_static_opaque_bytes_change_and_pixels_do_not
        #[test]
        fn test_static_opaque_bytes_change_and_pixels_do_not() {
            let t = Scratch::new();
            let source = webp(
                &t.join("a.webp"),
                &RgbaImage::from_pixel(80, 60, Rgba([200, 40, 90, 255])),
                false,
                80.0,
            );
            let bytes = std::fs::read(&source).unwrap();
            let before = raster::open(&source).unwrap().rgba;
            assert!(rt::codec_image::reencode_static(&source).unwrap());
            assert_ne!(std::fs::read(&source).unwrap(), bytes);
            assert_eq!(raster::open(&source).unwrap().rgba, before);
        }
        // tests.test_media_reencode.TestReencodeGivesUsOurOwnBytes.test_the_content_key_survives_so_dedup_still_works
        #[test]
        fn test_the_content_key_survives_so_dedup_still_works() {
            let t = Scratch::new();
            let source = webp(
                &t.join("b.webp"),
                &RgbaImage::from_pixel(80, 60, Rgba([10, 180, 60, 255])),
                false,
                80.0,
            );
            let before = key(&source);
            rt::codec_image::reencode_static(&source).unwrap();
            assert_eq!(key(&source), before);
        }
        // tests.test_media_reencode.TestReencodeGivesUsOurOwnBytes.test_animated_tgs_regzips_to_the_same_animation
        #[test]
        fn test_animated_tgs_regzips_to_the_same_animation() {
            let t = Scratch::new();
            let doc = empty_lottie();
            let source = tgs(&t.join("c.tgs"), &doc);
            let before = std::fs::read(&source).unwrap();
            rt::codec_lottie_prepare::rewrite(&source, None).unwrap();
            assert_eq!(
                rt::native_lottie::load(&source)
                    .unwrap()
                    .to_value()
                    .unwrap(),
                doc
            );
            assert_ne!(std::fs::read(&source).unwrap(), before);
        }
        // tests.test_media_reencode.TestReencodeGivesUsOurOwnBytes.test_regzip_is_deterministic
        #[test]
        fn test_regzip_is_deterministic() {
            let t = Scratch::new();
            let doc = empty_lottie();
            let mut outputs = Vec::new();
            for name in ["d1.tgs", "d2.tgs"] {
                let source = file(
                    &t.join(name),
                    &gzip(&serde_json::to_vec(&doc).unwrap(), 1, 123),
                );
                rt::codec_lottie_prepare::rewrite(&source, None).unwrap();
                outputs.push(std::fs::read(source).unwrap());
            }
            assert_eq!(outputs[0], outputs[1]);
        }
        // tests.test_media_reencode.TestReencodeGivesUsOurOwnBytes.test_an_unreadable_file_is_reported_not_raised
        #[test]
        fn test_an_unreadable_file_is_reported_not_raised() {
            let t = Scratch::new();
            let source = file(&t.join("e.webp"), b"not an image at all");
            let outcome = rt::codec_image::reencode_static_typed(&source).unwrap();
            assert!(!outcome.rewritten);
            assert!(outcome.warning.is_some());
            assert_eq!(std::fs::read(source).unwrap(), b"not an image at all");
        }
        // tests.test_media_reencode.TestReencodeGivesUsOurOwnBytes.test_growing_past_a_format_cap_keeps_the_original
        #[test]
        fn test_growing_past_a_format_cap_keeps_the_original() {
            let t = Scratch::new();
            let doc: Value = serde_json::from_slice(
                &std::fs::read(fixture("native/tgs-prepare/over-cap.json")).unwrap(),
            )
            .unwrap();
            let source = tgs(&t.join("big.tgs"), &doc);
            let before = std::fs::read(&source).unwrap();
            let encoded = rt::native_lottie::Document::from_value(&doc)
                .unwrap()
                .encode_gzip(true)
                .unwrap();
            assert!(
                encoded.len() > 65536,
                "fixture must cross actual production cap"
            );
            let result = rt::codec_lottie_prepare::rewrite_typed(&source, None).unwrap();
            assert!(!result.rewritten);
            assert!(result.warning.unwrap().contains("keeping the original"));
            assert_eq!(std::fs::read(source).unwrap(), before);
        }
        // tests.test_media_reencode.TestReencodeGivesUsOurOwnBytes.test_an_unknown_format_is_left_alone
        #[test]
        fn test_an_unknown_format_is_left_alone() {
            let t = Scratch::new();
            let source = file(&t.join("f.bin"), b"\x00\x01\x02");
            let before = std::fs::metadata(&source).unwrap().modified().unwrap();
            let uninitialized_root = t.join("must-not-exist");
            let result = rt::codec_worker::execute_typed_at(
                Some(&uninitialized_root),
                &json!({"operation":"prepare","source":source,"format":"sticker-shaped-thing","reencode":true}),
            );
            let error = result.as_ref().unwrap_err();
            assert_eq!(error.kind, "ValueError");
            assert_eq!(error.message, "unsupported media format");
            assert!(error.handled);
            let reply = rt::codec_error::CodecError::worker_reply(result).unwrap();
            assert_eq!(reply.exit, 1);
            assert_eq!(
                reply.envelope,
                json!({"ok":false,"error":"unsupported media format"})
            );
            assert_eq!(std::fs::read(&source).unwrap(), b"\x00\x01\x02");
            assert_eq!(
                std::fs::metadata(&source).unwrap().modified().unwrap(),
                before
            );
            assert!(!uninitialized_root.exists());
            let entries = std::fs::read_dir(&t.0)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>();
            assert_eq!(entries, vec![std::ffi::OsString::from("f.bin")]);
        }
    }
    mod an_under_declared_video_keeps_its_original_bytes {
        use super::*;
        // tests.test_media_reencode.AnUnderDeclaredVideoKeepsItsOriginalBytes.test_a_remux_over_the_duration_cap_keeps_the_original
        #[test]
        fn test_a_remux_over_the_duration_cap_keeps_the_original() {
            let t = Scratch::new();
            let source = lavfi(&t.0, "v.webm", "100x100", 30, 3.9, VP9);
            let before = std::fs::read(&source).unwrap();
            let result = runtime()
                .block_on(rt::codec_video_remux::reencode_typed(
                    &decoder(&t.0),
                    &source,
                ))
                .unwrap();
            assert!(!result.rewritten);
            assert!(result.warning.unwrap().contains("over the 3.0s cap"));
            assert_eq!(std::fs::read(source).unwrap(), before);
        }
        // tests.test_media_reencode.AnUnderDeclaredVideoKeepsItsOriginalBytes.test_a_remux_inside_the_cap_still_rewrites
        #[test]
        fn test_a_remux_inside_the_cap_still_rewrites() {
            let t = Scratch::new();
            let source = lavfi(&t.0, "v.webm", "100x100", 30, 2.5, VP9);
            let before = std::fs::read(&source).unwrap();
            assert!(
                runtime()
                    .block_on(rt::codec_video_remux::reencode_typed(
                        &decoder(&t.0),
                        &source
                    ))
                    .unwrap()
                    .rewritten
            );
            assert_ne!(std::fs::read(source).unwrap(), before);
        }
    }
    mod test_single_frame_video_keeps_a_content_key {
        use super::*;
        // tests.test_media_reencode.TestSingleFrameVideoKeepsAContentKey.test_the_key_survives_a_reencode
        #[test]
        fn test_the_key_survives_a_reencode() {
            let t = Scratch::new();
            let source = one_frame(&t.0, "one.webm");
            let bytes = std::fs::read(&source).unwrap();
            let before = runtime()
                .block_on(decoder(&t.0).content_key(&source))
                .unwrap();
            assert!(
                runtime()
                    .block_on(rt::codec_video_remux::reencode_typed(
                        &decoder(&t.0),
                        &source
                    ))
                    .unwrap()
                    .rewritten
            );
            assert_ne!(std::fs::read(&source).unwrap(), bytes);
            assert_eq!(
                runtime()
                    .block_on(decoder(&t.0).content_key(&source))
                    .unwrap(),
                before
            );
        }
        // tests.test_media_reencode.TestSingleFrameVideoKeepsAContentKey.test_fingerprint_and_content_key_agree
        #[test]
        fn test_fingerprint_and_content_key_agree() {
            let t = Scratch::new();
            let source = one_frame(&t.0, "agree.webm");
            let d = decoder(&t.0);
            let runtime = runtime();
            assert_eq!(
                runtime.block_on(d.fingerprint(&source)).unwrap()["content_key"],
                runtime.block_on(d.content_key(&source)).unwrap()
            );
        }
        // tests.test_media_reencode.TestSingleFrameVideoKeepsAContentKey.test_the_key_is_stable_across_runs
        #[test]
        fn test_the_key_is_stable_across_runs() {
            let t = Scratch::new();
            let source = one_frame(&t.0, "stable.webm");
            let d = decoder(&t.0);
            let runtime = runtime();
            let first = runtime.block_on(d.content_key(&source)).unwrap();
            assert_eq!(runtime.block_on(d.content_key(&source)).unwrap(), first);
            runtime
                .block_on(rt::codec_video_remux::reencode_typed(&d, &source))
                .unwrap();
            assert_eq!(
                runtime
                    .block_on(decoder(&t.0).content_key(&source))
                    .unwrap(),
                first
            );
        }
        // tests.test_media_reencode.TestSingleFrameVideoKeepsAContentKey.test_the_key_is_not_merely_a_hash_of_the_file
        #[test]
        fn test_the_key_is_not_merely_a_hash_of_the_file() {
            let t = Scratch::new();
            let source = one_frame(&t.0, "a.webm");
            let clone = t.join("b.webm");
            let mut a = args(&["-v", "error", "-y", "-i"]);
            a.push(source.to_string_lossy().into());
            a.extend(args(&["-c", "copy"]));
            a.push(clone.to_string_lossy().into());
            ffmpeg(&t.0, &a);
            assert_ne!(
                std::fs::read(&source).unwrap(),
                std::fs::read(&clone).unwrap()
            );
            let runtime = runtime();
            let d = decoder(&t.0);
            assert_eq!(
                runtime.block_on(d.content_key(&source)).unwrap(),
                runtime.block_on(d.content_key(&clone)).unwrap()
            );
        }
    }
    mod same_image_survives_a_re_encode {
        use super::*;
        // tests.test_media_reencode.SameImageSurvivesAReEncode.test_the_exact_key_really_does_move
        #[test]
        fn test_the_exact_key_really_does_move() {
            let t = Scratch::new();
            let source = art(0);
            let encoded = lossy(&t.0, &source, "a.webp");
            assert_ne!(key(&source), key(&encoded));
        }
        // tests.test_media_reencode.SameImageSurvivesAReEncode.test_a_re_encode_of_the_same_picture_is_the_same_picture
        #[test]
        fn test_a_re_encode_of_the_same_picture_is_the_same_picture() {
            let t = Scratch::new();
            let source = art(0);
            let encoded = lossy(&t.0, &source, "a.webp");
            assert_eq!(rt::codec_image::same_image(&encoded, &source), Some(true));
        }
        // tests.test_media_reencode.SameImageSurvivesAReEncode.test_an_identical_file_takes_the_exact_path
        #[test]
        fn test_an_identical_file_takes_the_exact_path() {
            let source = art(0);
            assert_eq!(rt::codec_image::same_image(&source, &source), Some(true));
        }
        // tests.test_media_reencode.SameImageSurvivesAReEncode.test_a_different_picture_is_still_rejected
        #[test]
        fn test_a_different_picture_is_still_rejected() {
            let t = Scratch::new();
            let source = art(0);
            let different = art(9);
            let encoded = lossy(&t.0, &source, "a.webp");
            assert_eq!(
                rt::codec_image::same_image(&encoded, &different),
                Some(false)
            );
        }
        // tests.test_media_reencode.SameImageSurvivesAReEncode.test_animated_cannot_be_decided_and_says_so
        #[test]
        fn test_animated_cannot_be_decided_and_says_so() {
            assert_eq!(
                rt::codec_lottie_identity::same_image(&art(0), &art(9)),
                None
            );
        }
        // tests.test_media_reencode.SameImageSurvivesAReEncode.test_the_same_shape_in_another_colour_is_not_our_upload
        #[test]
        fn test_the_same_shape_in_another_colour_is_not_our_upload() {
            let t = Scratch::new();
            let square = |name: &str, color| {
                let mut image = RgbaImage::new(100, 100);
                for x in 20..=79 {
                    for y in 20..=79 {
                        image[(x, y)] = Rgba(color);
                    }
                }
                save(&t.join(name), &image)
            };
            let ours = square("ours.png", [200, 30, 30, 255]);
            let theirs = square("theirs.png", [10, 200, 40, 255]);
            assert!((phash(&ours) ^ phash(&theirs)).count_ones() <= 6);
            assert_eq!(rt::codec_image::same_image(&ours, &theirs), Some(false));
        }
        // tests.test_media_reencode.SameImageSurvivesAReEncode.test_an_unreadable_file_is_undecidable_not_negative
        #[test]
        fn test_an_unreadable_file_is_undecidable_not_negative() {
            let t = Scratch::new();
            let bad = file(&t.join("torn.png"), b"not an image");
            assert_eq!(rt::codec_image::same_image(&bad, &art(0)), None);
        }
    }
}
mod test_repaintable {
    use super::*;
    mod the_flag_is_on_the_sticker_not_the_set {
        use super::*;
        // tests.test_repaintable.TheFlagIsOnTheStickerNotTheSet.test_a_repaintable_sticker_is_recognised
        #[test]
        fn test_a_repaintable_sticker_is_recognised() {
            assert!(rt::repaint_gate::is_repaintable(
                &json!({"needs_repainting":true})
            ));
        }
        // tests.test_repaintable.TheFlagIsOnTheStickerNotTheSet.test_an_ordinary_sticker_is_not
        #[test]
        fn test_an_ordinary_sticker_is_not() {
            assert!(!rt::repaint_gate::is_repaintable(&json!({"emoji":"😀"})));
            assert!(!rt::repaint_gate::is_repaintable(
                &json!({"needs_repainting":false})
            ));
        }
        // tests.test_repaintable.TheFlagIsOnTheStickerNotTheSet.test_absence_is_not_repaintable
        #[test]
        fn test_absence_is_not_repaintable() {
            assert!(!rt::repaint_gate::is_repaintable(&json!({})));
        }
        // tests.test_repaintable.TheFlagIsOnTheStickerNotTheSet.test_the_set_level_answer_is_not_the_stickers_answer
        #[test]
        fn test_the_set_level_answer_is_not_the_stickers_answer() {
            let set = json!({"needs_repainting":null,"stickers":vec![json!({"needs_repainting":true});3]});
            assert!(!rt::repaint_gate::is_repaintable(&set));
            assert!(
                set["stickers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(rt::repaint_gate::is_repaintable)
            );
        }
    }
    mod the_gate_asks_before_ingesting {
        use super::*;
        // tests.test_repaintable.TheGateAsksBeforeIngesting.test_nothing_repaintable_asks_nothing
        #[test]
        fn test_nothing_repaintable_asks_nothing() {
            let mut out = Vec::new();
            let mut asked = Vec::new();
            assert!(
                rt::repaint_gate::decide(
                    &[],
                    "ask",
                    true,
                    |p| {
                        asked.push(p.to_owned());
                        Ok(Some("n".into()))
                    },
                    &mut out
                )
                .unwrap()
            );
            assert!(asked.is_empty());
            assert!(out.is_empty());
        }
        // tests.test_repaintable.TheGateAsksBeforeIngesting.test_yes_keeps_them
        #[test]
        fn test_yes_keeps_them() {
            let mut out = Vec::new();
            assert!(
                rt::repaint_gate::decide(
                    &["a".into(), "b".into()],
                    "ask",
                    true,
                    |_| Ok(Some("y".into())),
                    &mut out
                )
                .unwrap()
            );
            assert!(String::from_utf8(out).unwrap().contains("REPAINTABLE"));
        }
        // tests.test_repaintable.TheGateAsksBeforeIngesting.test_anything_but_yes_skips
        #[test]
        fn test_anything_but_yes_skips() {
            for answer in ["n", "", "no", "maybe", "  N  "] {
                assert!(
                    !rt::repaint_gate::decide(
                        &["a".into()],
                        "ask",
                        true,
                        |_| Ok(Some(answer.into())),
                        &mut Vec::new()
                    )
                    .unwrap()
                );
            }
        }
        // tests.test_repaintable.TheGateAsksBeforeIngesting.test_skip_and_keep_never_prompt
        #[test]
        fn test_skip_and_keep_never_prompt() {
            for (mode, expected) in [("skip", false), ("keep", true)] {
                assert_eq!(
                    rt::repaint_gate::decide(
                        &["a".into()],
                        mode,
                        true,
                        |_| panic!("explicit mode must not prompt"),
                        &mut Vec::new()
                    )
                    .unwrap(),
                    expected
                );
            }
        }
        // tests.test_repaintable.TheGateAsksBeforeIngesting.test_with_no_terminal_the_answer_is_skip
        #[test]
        fn test_with_no_terminal_the_answer_is_skip() {
            let mut out = Vec::new();
            assert!(
                !rt::repaint_gate::decide(
                    &["a".into()],
                    "ask",
                    false,
                    |_| panic!("no terminal must never prompt"),
                    &mut out
                )
                .unwrap()
            );
            let out = String::from_utf8(out).unwrap();
            assert!(out.contains("nobody answered"));
            assert!(out.contains("--repaintable keep"));
        }
        // tests.test_repaintable.TheGateAsksBeforeIngesting.test_an_unanswerable_prompt_is_a_no_not_a_crash
        #[test]
        fn test_an_unanswerable_prompt_is_a_no_not_a_crash() {
            for kind in [
                std::io::ErrorKind::UnexpectedEof,
                std::io::ErrorKind::Interrupted,
            ] {
                let mut out = Vec::new();
                assert!(
                    !rt::repaint_gate::decide(
                        &["a".into()],
                        "ask",
                        true,
                        |_| Err(kind.into()),
                        &mut out
                    )
                    .unwrap()
                );
                assert!(String::from_utf8(out).unwrap().contains("nobody answered"));
            }
        }
        // tests.test_repaintable.TheGateAsksBeforeIngesting.test_the_warning_names_them_and_bounds_the_list
        #[test]
        fn test_the_warning_names_them_and_bounds_the_list() {
            let labels = (0..30).map(|i| format!("id{i}")).collect::<Vec<_>>();
            let mut out = Vec::new();
            rt::repaint_gate::decide(&labels, "skip", true, |_| panic!("skip"), &mut out).unwrap();
            let out = String::from_utf8(out).unwrap();
            for text in ["30 of these emoji are REPAINTABLE", "id0", "+22 more"] {
                assert!(out.contains(text));
            }
            assert!(!out.contains("id8"));
        }
    }
    mod fetch_pack_honours_the_gate {
        use super::*;
        // tests.test_repaintable.FetchPackHonoursTheGate.test_skip_leaves_them_out_of_the_catalog
        #[test]
        fn test_skip_leaves_them_out_of_the_catalog() {
            let t = Scratch::new();
            let (counts, rows, mut downloaded) = fetch(&t.0, 4, &[1, 3], "skip", None, false);
            assert_eq!(counts["new"], 2);
            assert_eq!(counts["repaintable"], 2);
            assert_eq!(rows.len(), 2);
            downloaded.sort();
            assert_eq!(downloaded, vec!["0", "2"]);
        }
        // tests.test_repaintable.FetchPackHonoursTheGate.test_keep_ingests_them
        #[test]
        fn test_keep_ingests_them() {
            let t = Scratch::new();
            let (counts, rows, _) = fetch(&t.0, 4, &[1, 3], "keep", None, false);
            assert_eq!(counts["new"], 4);
            assert_eq!(counts["repaintable"], 0);
            assert_eq!(rows.len(), 4);
        }
        // tests.test_repaintable.FetchPackHonoursTheGate.test_an_ordinary_pack_is_untouched
        #[test]
        fn test_an_ordinary_pack_is_untouched() {
            let t = Scratch::new();
            let (counts, rows, _) = fetch(&t.0, 3, &[], "skip", None, false);
            assert_eq!(counts["new"], 3);
            assert_eq!(counts["repaintable"], 0);
            assert_eq!(rows.len(), 3);
        }
    }
    mod fetch_emoji_ids_honours_the_gate {
        use super::*;
        // tests.test_repaintable.FetchEmojiIdsHonoursTheGate.test_skip_leaves_them_out_of_the_catalog
        #[test]
        fn test_skip_leaves_them_out_of_the_catalog() {
            let t = Scratch::new();
            let (counts, rows, downloaded) = fetch(&t.0, 3, &[1], "skip", None, true);
            assert_eq!(counts["new"], 2);
            assert_eq!(counts["repaintable"], 1);
            assert_eq!(rows.len(), 2);
            assert!(!downloaded.iter().any(|id| id == "1"));
        }
        // tests.test_repaintable.FetchEmojiIdsHonoursTheGate.test_keep_ingests_them
        #[test]
        fn test_keep_ingests_them() {
            let t = Scratch::new();
            let (counts, rows, _) = fetch(&t.0, 3, &[1], "keep", None, true);
            assert_eq!(counts["new"], 3);
            assert_eq!(counts["repaintable"], 0);
            assert_eq!(rows.len(), 3);
        }
    }
    mod both_entry_points_offer_the_same_choice {
        use super::*;
        // tests.test_repaintable.BothEntryPointsOfferTheSameChoice.test_the_modes_are_shared
        #[test]
        fn test_the_modes_are_shared() {
            assert_eq!(rt::repaint_gate::MODES, ["ask", "skip", "keep"]);
        }
        // tests.test_repaintable.BothEntryPointsOfferTheSameChoice.test_ask_is_the_default_in_both
        #[test]
        fn test_ask_is_the_default_in_both() {
            for source in [
                include_str!("../src/fetch_cli.rs"),
                include_str!("../src/fetch_ids_cli.rs"),
            ] {
                assert!(source.contains(r#""ask".to_owned()"#));
                assert!(
                    source.contains("crate::repaint_gate::MODES")
                        || source.contains("validate_repaint")
                );
            }
        }
    }
    mod tint_parsing {
        use super::*;
        // tests.test_repaintable.TintParsing.test_it_accepts_both_spellings
        #[test]
        fn test_it_accepts_both_spellings() {
            for text in ["#FF8800", "ff8800"] {
                assert_eq!(rt::codec_tint::parse_tint(text).unwrap(), [255, 136, 0]);
            }
        }
        // tests.test_repaintable.TintParsing.test_it_refuses_anything_else
        #[test]
        fn test_it_refuses_anything_else() {
            for text in ["white", "#FFF", "#GGGGGG", "#FF88000", ""] {
                assert!(rt::codec_tint::parse_tint(text).is_err());
            }
        }
    }
    mod baking_the_repaint_ourselves {
        use super::*;
        // tests.test_repaintable.BakingTheRepaintOurselves.test_a_static_emoji_keeps_its_shape_and_loses_its_hue
        #[test]
        fn test_a_static_emoji_keeps_its_shape_and_loses_its_hue() {
            let t = Scratch::new();
            let source = save(&t.join("s.png"), &shape(3));
            let before = raster::open(&source).unwrap().rgba;
            assert!(rt::codec_image::repaint_static(&source, [255, 255, 255]).unwrap());
            let after = raster::open(&source).unwrap().rgba;
            assert_eq!(
                before.pixels().map(|p| p[3]).collect::<Vec<_>>(),
                after.pixels().map(|p| p[3]).collect::<Vec<_>>()
            );
            let visible = after.pixels().filter(|p| p[3] > 0).collect::<Vec<_>>();
            assert!(!visible.is_empty());
            assert!(visible.into_iter().all(|p| p.0[..3] == [255, 255, 255]));
        }
        // tests.test_repaintable.BakingTheRepaintOurselves.test_an_animated_emoji_is_recoloured_through_its_lottie
        #[test]
        fn test_an_animated_emoji_is_recoloured_through_its_lottie() {
            let t = Scratch::new();
            let source = tgs(&t.join("a.tgs"), &tinted_lottie());
            assert!(rt::codec_lottie_prepare::rewrite(&source, Some([255, 255, 255])).unwrap());
            let doc = rt::native_lottie::load(&source)
                .unwrap()
                .to_value()
                .unwrap();
            let items = &doc["layers"][0]["shapes"][0]["it"];
            assert_eq!(items[0]["c"]["k"], json!([1.0, 1.0, 1.0, 1.0]));
            assert_eq!(items[1]["c"]["k"], json!([1.0, 1.0, 1.0, 1.0]));
            assert_eq!(
                items[2]["g"]["k"]["k"],
                json!([0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0])
            );
            assert_eq!(doc["op"], 30);
        }
        // tests.test_repaintable.BakingTheRepaintOurselves.test_video_is_refused_rather_than_silently_skipped
        #[test]
        fn test_video_is_refused_rather_than_silently_skipped() {
            let t = Scratch::new();
            let source = file(&t.join("v.webm"), b"not really a webm");
            let before = std::fs::read(&source).unwrap();
            let mut warnings = Vec::new();
            assert!(
                rt::codec_video_encode::prepare_at(
                    None,
                    &source,
                    &json!({"tint":"#FFFFFF"}),
                    &mut warnings
                )
                .is_err()
            );
            assert!(warnings.iter().any(|v| v.contains("repaint not supported")));
            assert_eq!(std::fs::read(source).unwrap(), before);
        }
    }
    mod tint_answers_the_gate {
        use super::*;
        // tests.test_repaintable.TintAnswersTheGate.test_a_tinted_run_ingests_what_skip_would_have_dropped
        #[test]
        fn test_a_tinted_run_ingests_what_skip_would_have_dropped() {
            let t = Scratch::new();
            let (counts, rows, _) = fetch(&t.0, 3, &[1], "skip", Some("#FFFFFF"), true);
            assert_eq!(counts["new"], 3);
            assert_eq!(counts["repaintable"], 0);
            assert_eq!(counts["repainted"], 1);
            assert_eq!(rows.len(), 3);
        }
        // tests.test_repaintable.TintAnswersTheGate.test_without_a_tint_skip_still_drops_it
        #[test]
        fn test_without_a_tint_skip_still_drops_it() {
            let t = Scratch::new();
            let (counts, _, _) = fetch(&t.0, 3, &[1], "skip", None, true);
            assert_eq!(counts["new"], 2);
            assert_eq!(counts["repaintable"], 1);
            assert_eq!(counts["repainted"], 0);
        }
        // tests.test_repaintable.TintAnswersTheGate.test_the_tint_is_recorded_on_the_item
        #[test]
        fn test_the_tint_is_recorded_on_the_item() {
            let t = Scratch::new();
            let (_, rows, _) = fetch(&t.0, 3, &[1], "keep", Some("#FF0000"), true);
            let marks = rows
                .iter()
                .flat_map(|row| row["keywords"].as_array().unwrap())
                .filter_map(Value::as_str)
                .filter(|kw| kw.starts_with("tint:"))
                .collect::<Vec<_>>();
            assert_eq!(marks, vec!["tint:#FF0000"]);
        }
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
