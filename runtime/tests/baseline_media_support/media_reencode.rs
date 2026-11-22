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
