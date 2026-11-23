//! Individually discoverable ports of 86 collected original video/media-boundary cases.
//! Original IDs and target metadata are recorded in tests/fixtures/migration/reference-cases.json.
#[path = "baseline_video_support/mod.rs"]
mod support;
use image::{Rgba, RgbaImage};
use numera_runtime as rt;
use numera_runtime::native_raster as raster;
use serde_json::{Value, json};
use support::*;
mod test_identity_video {
    use super::*;
    mod alpha_is_part_of_the_identity {
        use super::*;
        // tests.test_identity_video.AlphaIsPartOfTheIdentity.test_identical_rgb_with_different_alpha_are_different_items
        #[test]
        fn test_identical_rgb_with_different_alpha_are_different_items() {
            let f = Identity::new(&["opaque", "faint"]);
            assert_ne!(
                key(&f.scratch.0, f.get("opaque")),
                key(&f.scratch.0, f.get("faint"))
            );
        }
        // tests.test_identity_video.AlphaIsPartOfTheIdentity.test_they_are_not_the_same_image
        #[test]
        fn test_they_are_not_the_same_image() {
            let f = Identity::new(&["opaque", "faint"]);
            assert_eq!(
                same(&f.scratch.0, f.get("opaque"), f.get("faint")),
                Some(false)
            );
        }
        // tests.test_identity_video.AlphaIsPartOfTheIdentity.test_fully_transparent_is_not_fully_opaque
        #[test]
        fn test_fully_transparent_is_not_fully_opaque() {
            let f = Identity::new(&["clear", "opaque"]);
            assert_ne!(
                key(&f.scratch.0, f.get("clear")),
                key(&f.scratch.0, f.get("opaque"))
            );
            assert_eq!(
                same(&f.scratch.0, f.get("clear"), f.get("opaque")),
                Some(false)
            );
        }
        // tests.test_identity_video.AlphaIsPartOfTheIdentity.test_a_genuine_remux_is_still_the_same_item
        #[test]
        fn test_a_genuine_remux_is_still_the_same_item() {
            let f = Identity::new(&["opaque"]);
            let rewritten = remux(&f.scratch.0, f.get("opaque"), "remux.webm");
            assert_eq!(
                key(&f.scratch.0, &rewritten),
                key(&f.scratch.0, f.get("opaque"))
            );
            assert_eq!(same(&f.scratch.0, &rewritten, f.get("opaque")), Some(true));
        }
    }
    mod the_decoder_comes_from_the_codec {
        use super::*;
        // tests.test_identity_video.TheDecoderComesFromTheCodec.test_a_vp9_clip_gets_the_alpha_capable_vp9_decoder
        #[test]
        fn test_a_vp9_clip_gets_the_alpha_capable_vp9_decoder() {
            let f = Identity::new(&["opaque"]);
            let d = decoder(&f.scratch.0);
            let runtime = runtime();
            assert_eq!(
                runtime.block_on(d.probe_typed(f.get("opaque"))).unwrap()["codec"],
                "vp9"
            );
            assert_eq!(
                runtime
                    .block_on(d.decoder_args_typed(f.get("opaque")))
                    .unwrap(),
                vec!["-c:v", "libvpx-vp9"]
            );
        }
        // tests.test_identity_video.TheDecoderComesFromTheCodec.test_a_vp8_clip_is_not_forced_through_the_vp9_decoder
        #[test]
        fn test_a_vp8_clip_is_not_forced_through_the_vp9_decoder() {
            let f = Identity::new(&["vp8_opaque"]);
            let d = decoder(&f.scratch.0);
            let runtime = runtime();
            assert_eq!(
                runtime
                    .block_on(d.probe_typed(f.get("vp8_opaque")))
                    .unwrap()["codec"],
                "vp8"
            );
            assert_eq!(
                runtime
                    .block_on(d.decoder_args_typed(f.get("vp8_opaque")))
                    .unwrap(),
                vec!["-c:v", "libvpx"]
            );
        }
        // tests.test_identity_video.TheDecoderComesFromTheCodec.test_vp8_alpha_is_read_too
        #[test]
        fn test_vp8_alpha_is_read_too() {
            let f = Identity::new(&["vp8_opaque", "vp8_faint"]);
            assert_ne!(
                key(&f.scratch.0, f.get("vp8_opaque")),
                key(&f.scratch.0, f.get("vp8_faint"))
            );
            assert_eq!(
                same(&f.scratch.0, f.get("vp8_opaque"), f.get("vp8_faint")),
                Some(false)
            );
        }
        // tests.test_identity_video.TheDecoderComesFromTheCodec.test_an_extensionless_download_decodes_the_same_way
        #[test]
        fn test_an_extensionless_download_decodes_the_same_way() {
            let f = Identity::new(&["faint"]);
            let p = f.scratch.join("downloaded_blob");
            std::fs::copy(f.get("faint"), &p).unwrap();
            let d = decoder(&f.scratch.0);
            assert_eq!(
                runtime().block_on(d.decoder_args_typed(&p)).unwrap(),
                vec!["-c:v", "libvpx-vp9"]
            );
            assert_eq!(key(&f.scratch.0, &p), key(&f.scratch.0, f.get("faint")));
        }
        // tests.test_identity_video.TheDecoderComesFromTheCodec.test_the_decoder_choice_follows_the_bytes_not_the_name
        #[test]
        fn test_the_decoder_choice_follows_the_bytes_not_the_name() {
            let f = Identity::new(&["vp8_opaque", "opaque"]);
            let p = f.scratch.join("swapped.webm");
            let d = decoder(&f.scratch.0);
            let runtime = runtime();
            std::fs::copy(f.get("vp8_opaque"), &p).unwrap();
            assert_eq!(
                runtime.block_on(d.decoder_args_typed(&p)).unwrap(),
                vec!["-c:v", "libvpx"]
            );
            std::fs::copy(f.get("opaque"), &p).unwrap();
            assert_eq!(
                runtime.block_on(d.decoder_args_typed(&p)).unwrap(),
                vec!["-c:v", "libvpx-vp9"]
            );
        }
    }
    mod one_sampling_rule_for_every_answer {
        use super::*;
        // tests.test_identity_video.OneSamplingRuleForEveryAnswer.test_fingerprint_and_perceptual_hash_agree
        #[test]
        fn test_fingerprint_and_perceptual_hash_agree() {
            let f = Identity::new(&["fast30", "fast15", "opaque", "single"]);
            let d = decoder(&f.scratch.0);
            let runtime = runtime();
            for name in ["fast30", "fast15", "opaque", "single"] {
                assert_eq!(
                    runtime.block_on(d.fingerprint_typed(f.get(name))).unwrap()["phash"],
                    json!(runtime.block_on(d.perceptual_hash(f.get(name))).unwrap())
                );
            }
        }
        // tests.test_identity_video.OneSamplingRuleForEveryAnswer.test_fingerprint_and_content_key_agree
        #[test]
        fn test_fingerprint_and_content_key_agree() {
            let f = Identity::new(&["fast30", "opaque", "single"]);
            let d = decoder(&f.scratch.0);
            let runtime = runtime();
            for name in ["fast30", "opaque", "single"] {
                assert_eq!(
                    runtime.block_on(d.fingerprint_typed(f.get(name))).unwrap()["content_key"],
                    runtime.block_on(d.content_key(f.get(name))).unwrap()
                );
            }
        }
        // tests.test_identity_video.OneSamplingRuleForEveryAnswer.test_a_single_frame_clip_still_gets_a_real_content_key
        #[test]
        fn test_a_single_frame_clip_still_gets_a_real_content_key() {
            let f = Identity::new(&["single"]);
            let key = key(&f.scratch.0, f.get("single"));
            let byte_hash = format!(
                "v:{}",
                &raster::digest_hex(&std::fs::read(f.get("single")).unwrap())[..32]
            );
            assert!(key.starts_with("v:"));
            assert_ne!(key, byte_hash);
        }
    }
    mod a_failed_look_is_not_an_answer {
        use super::*;
        // tests.test_identity_video.AFailedLookIsNotAnAnswer.test_an_undecodable_file_is_undecidable_not_equal
        #[test]
        fn test_an_undecodable_file_is_undecidable_not_equal() {
            let f = Identity::new(&["opaque"]);
            let mut bytes = b"\x1a\x45\xdf\xa3".to_vec();
            bytes.extend(b"garbage".repeat(64));
            let broken = file(&f.scratch.join("broken.webm"), &bytes);
            assert_eq!(same(&f.scratch.0, &broken, f.get("opaque")), None);
        }
        // tests.test_identity_video.AFailedLookIsNotAnAnswer.test_video_colour_is_actually_compared
        #[test]
        fn test_video_colour_is_actually_compared() {
            let f = Identity::new(&["opaque"]);
            let rewritten = remux(&f.scratch.0, f.get("opaque"), "remux.webm");
            let bytes = runtime()
                .block_on(decoder(&f.scratch.0).frames_rgba_typed(f.get("opaque"), 10))
                .unwrap();
            assert!(bytes.len() >= 64 * 64 * 4);
            assert!(
                raster::premultiplied_pixels(
                    &rt::codec_image::rgba_from_frame(&bytes[..64 * 64 * 4], 64).unwrap()
                )
                .len()
                    == 64 * 64 * 4
            );
            assert_eq!(same(&f.scratch.0, &rewritten, f.get("opaque")), Some(true));
        }
    }
}
#[path = "baseline_video_support/decoded_identity.rs"]
mod decoded_identity;
#[path = "baseline_video_support/timeline_comparison.rs"]
mod test_video_timeline_comparison;
mod test_media_boundary {
    use super::*;
    mod media_boundary {
        use super::*;
        // tests.test_media_boundary.MediaBoundary.test_codec_timeout_and_svg_do_not_import_application_cli
        #[test]
        fn test_codec_timeout_and_svg_do_not_import_application_cli() {
            let t = Scratch::new();
            let path=file(&t.join("art.svg"),br##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="#ff0000"/></svg>"##);
            assert!(
                !rt::codec_process::codec_budget(None)
                    .unwrap()
                    .wall
                    .is_zero()
            );
            let image = rt::native_svg::load_rgba(&path).unwrap();
            assert_eq!(image[(32, 32)].0, [255, 0, 0, 255]);
            assert_eq!(
                raster::fit_canvas(&image, 100).unwrap().dimensions(),
                (100, 100)
            );
            let keywords = file(
                &t.join("keywords.csv"),
                b"ticker,keywords\nABC,\"abc, Alpha\"\n",
            );
            assert_eq!(
                rt::ticker_keywords::load(&keywords).unwrap(),
                std::collections::BTreeMap::from([("abc".into(), "abc, Alpha".into())])
            );
        }
        // tests.test_media_boundary.MediaBoundary.test_gif_animation_metadata_is_codec_only
        #[test]
        fn test_gif_animation_metadata_is_codec_only() {
            let t = Scratch::new();
            use image::codecs::gif::GifEncoder;
            let red = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]));
            let blue = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 255, 255]));
            for (name, moving) in [("still.gif", false), ("moving.gif", true)] {
                let mut encoder = GifEncoder::new(std::fs::File::create(t.join(name)).unwrap());
                encoder
                    .encode_frame(image::Frame::from_parts(
                        red.clone(),
                        0,
                        0,
                        image::Delay::from_numer_denom_ms(100, 1),
                    ))
                    .unwrap();
                if moving {
                    encoder
                        .encode_frame(image::Frame::from_parts(
                            blue.clone(),
                            0,
                            0,
                            image::Delay::from_numer_denom_ms(100, 1),
                        ))
                        .unwrap();
                }
            }
            file(&t.join("bad.gif"), b"not a GIF");
            for (name, expected) in [
                ("still.gif", false),
                ("moving.gif", true),
                ("bad.gif", false),
            ] {
                assert_eq!(rt::codec_worker::execute_typed_at(None,&json!({"operation":"animation_info","source":t.join(name),"format":"static"})).unwrap(),expected);
            }
        }
    }
}
#[path = "baseline_video_support/media_bounds.rs"]
mod media_bounds;
mod test_media_fitting {
    use super::*;
    mod fitting_preserves_opacity {
        use super::*;
        // tests.test_media_fitting.FittingPreservesOpacity.test_a_partly_transparent_pixel_keeps_its_colour_and_its_alpha
        #[test]
        fn test_a_partly_transparent_pixel_keeps_its_colour_and_its_alpha() {
            for alpha in [1, 64, 128, 192, 254] {
                let source = RgbaImage::from_pixel(100, 100, Rgba([255, 0, 0, alpha]));
                assert_pixel(
                    raster::fit_canvas(&source, 100).unwrap()[(50, 50)].0,
                    [255, 0, 0, alpha],
                    false,
                );
            }
        }
        // tests.test_media_fitting.FittingPreservesOpacity.test_opaque_art_is_the_control_that_always_passed
        #[test]
        fn test_opaque_art_is_the_control_that_always_passed() {
            let source = RgbaImage::from_pixel(100, 100, Rgba([12, 200, 90, 255]));
            assert_pixel(
                raster::fit_canvas(&source, 100).unwrap()[(50, 50)].0,
                [12, 200, 90, 255],
                false,
            );
        }
        // tests.test_media_fitting.FittingPreservesOpacity.test_every_fitting_entry_point_shares_one_implementation
        #[test]
        fn test_every_fitting_entry_point_shares_one_implementation() {
            let t = Scratch::new();
            let source = png(&t.join("in.png"), [255, 0, 0, 128], (100, 100));
            let raw = raster::open(&source).unwrap().rgba;
            let direct = raster::fit_canvas(&raw, 100).unwrap();
            let converted = t.join("converted.png");
            rt::codec_image::to_static_png_typed(&source, &converted).unwrap();
            let provider = t.join("provider.png");
            assert!(
                rt::codec_image::provider_png(&std::fs::read(&source).unwrap(), &provider).unwrap()
            );
            for actual in [
                raster::open(&converted).unwrap().rgba,
                raster::open(&provider).unwrap().rgba,
            ] {
                assert_eq!(actual, direct);
            }
            let source_code = include_str!("../src/codec_image.rs");
            assert!(source_code.matches("raster::fit_canvas(").count() >= 2);
        }
        // tests.test_media_fitting.FittingPreservesOpacity.test_a_resized_image_keeps_its_opacity
        #[test]
        fn test_a_resized_image_keeps_its_opacity() {
            let source = RgbaImage::from_pixel(40, 40, Rgba([0, 100, 255, 96]));
            let output = raster::fit_canvas(&source, 100).unwrap();
            assert_eq!(output.dimensions(), (100, 100));
            assert_pixel(output[(50, 50)].0, [0, 100, 255, 96], true);
        }
        // tests.test_media_fitting.FittingPreservesOpacity.test_a_cutout_stays_a_cutout
        #[test]
        fn test_a_cutout_stays_a_cutout() {
            let mut source = RgbaImage::from_pixel(60, 60, Rgba([255, 128, 0, 200]));
            for x in 20..40 {
                for y in 20..40 {
                    source[(x, y)] = Rgba([0, 0, 0, 0]);
                }
            }
            let output = raster::fit_canvas(&source, 100).unwrap();
            assert_eq!(output[(50, 50)][3], 0);
            assert_pixel(output[(5, 5)].0, [255, 128, 0, 200], true);
        }
        // tests.test_media_fitting.FittingPreservesOpacity.test_a_transparent_border_is_trimmed_not_composited
        #[test]
        fn test_a_transparent_border_is_trimmed_not_composited() {
            let mut source = RgbaImage::new(80, 80);
            for x in 30..50 {
                for y in 30..50 {
                    source[(x, y)] = Rgba([10, 220, 30, 150]);
                }
            }
            assert_pixel(
                raster::fit_canvas(&source, 100).unwrap()[(50, 50)].0,
                [10, 220, 30, 150],
                true,
            );
        }
        // tests.test_media_fitting.FittingPreservesOpacity.test_a_partial_alpha_gradient_survives_end_to_end
        #[test]
        fn test_a_partial_alpha_gradient_survives_end_to_end() {
            let t = Scratch::new();
            let source = RgbaImage::from_fn(100, 100, |x, _| {
                Rgba([0, 0, 255, (x * 255 / 99).max(1) as u8])
            });
            let input = save(&t.join("in.png"), &source);
            let out = t.join("out.png");
            rt::codec_image::to_static_png_typed(&input, &out).unwrap();
            let decoded = raster::open(&out).unwrap().rgba;
            for x in [25, 50, 75] {
                assert_pixel(decoded[(x, 50)].0, [0, 0, 255, (x * 255 / 99) as u8], false);
            }
        }
    }
    mod compositing_onto_an_opaque_backdrop_is_still_correct {
        use super::*;
        // tests.test_media_fitting.CompositingOntoAnOpaqueBackdropIsStillCorrect.test_flattening_onto_white_still_blends
        #[test]
        fn test_flattening_onto_white_still_blends() {
            let source = RgbaImage::from_pixel(10, 10, Rgba([255, 0, 0, 128]));
            let flat = raster::on_white(&source);
            let pixel = flat[(5, 5)];
            assert_eq!(pixel[0], 255);
            assert!(pixel[1].abs_diff(127) <= 2);
            assert!(pixel[2].abs_diff(127) <= 2);
        }
    }
}
mod test_lottie_repaint {
    use super::*;
    mod a_timeline_must_be_finite_numbers {
        use super::*;
        // tests.test_lottie_repaint.ATimelineMustBeFiniteNumbers.test_a_valid_timeline_still_passes
        #[test]
        fn test_a_valid_timeline_still_passes() {
            let t = Scratch::new();
            let path = file(
                &t.join("x.tgs"),
                &gzip(&serde_json::to_vec(&valid()).unwrap(), 6, 0),
            );
            rt::codec_lottie_validate::validate_file_typed(&path).unwrap();
        }
        // tests.test_lottie_repaint.ATimelineMustBeFiniteNumbers.test_nan_in_any_timeline_field_is_refused
        #[test]
        fn test_nan_in_any_timeline_field_is_refused() {
            let t = Scratch::new();
            for field in ["fr", "ip", "op"] {
                let mut doc = valid();
                doc[field] = json!("__nonfinite__");
                let raw = serde_json::to_string(&doc)
                    .unwrap()
                    .replace("\"__nonfinite__\"", "NaN");
                assert!(!raw.contains("__nonfinite__"));
                assert!(raw.contains("NaN"));
                let path = file(&t.join("x.tgs"), &gzip(raw.as_bytes(), 6, 0));
                let error = rt::codec_lottie_validate::validate_file_typed(&path).unwrap_err();
                assert_eq!(error.kind, "MediaError");
                assert!(error.message.contains(field));
            }
        }
        // tests.test_lottie_repaint.ATimelineMustBeFiniteNumbers.test_infinity_is_refused_by_name_not_by_luck
        #[test]
        fn test_infinity_is_refused_by_name_not_by_luck() {
            let t = Scratch::new();
            for (field, value) in [("fr", "Infinity"), ("ip", "-Infinity"), ("op", "Infinity")] {
                let mut doc = valid();
                doc[field] = json!("__nonfinite__");
                let raw = serde_json::to_string(&doc)
                    .unwrap()
                    .replace("\"__nonfinite__\"", value);
                assert!(!raw.contains("__nonfinite__"));
                assert!(raw.contains(value));
                let path = file(&t.join("x.tgs"), &gzip(raw.as_bytes(), 6, 0));
                assert_eq!(
                    rt::codec_lottie_validate::validate_file_typed(&path)
                        .unwrap_err()
                        .kind,
                    "MediaError"
                );
            }
        }
        // tests.test_lottie_repaint.ATimelineMustBeFiniteNumbers.test_a_non_numeric_field_is_still_refused
        #[test]
        fn test_a_non_numeric_field_is_still_refused() {
            let t = Scratch::new();
            let mut doc = valid();
            doc["fr"] = json!("thirty");
            let path = file(
                &t.join("x.tgs"),
                &gzip(&serde_json::to_vec(&doc).unwrap(), 6, 0),
            );
            assert_eq!(
                rt::codec_lottie_validate::validate_file_typed(&path)
                    .unwrap_err()
                    .kind,
                "MediaError"
            );
        }
    }
    mod repainting_reaches_every_colour {
        use super::*;
        // tests.test_lottie_repaint.RepaintingReachesEveryColour.test_a_flat_colour_is_recoloured
        #[test]
        fn test_a_flat_colour_is_recoloured() {
            let node = tint(&json!({"c":{"a":0,"k":[1.0,0.0,0.0,1.0]}}));
            assert_eq!(
                &node["c"]["k"].as_array().unwrap()[..3],
                &[json!(0.0), json!(0.0), json!(1.0)]
            );
        }
        // tests.test_lottie_repaint.RepaintingReachesEveryColour.test_a_flat_colour_keeps_its_own_alpha
        #[test]
        fn test_a_flat_colour_keeps_its_own_alpha() {
            let node = tint(&json!({"c":{"a":0,"k":[1.0,0.0,0.0,0.25]}}));
            assert_eq!(node["c"]["k"][3], 0.25);
        }
        // tests.test_lottie_repaint.RepaintingReachesEveryColour.test_an_animated_colour_is_recoloured_too
        #[test]
        fn test_an_animated_colour_is_recoloured_too() {
            let node = tint(
                &json!({"c":{"a":1,"k":[{"t":0,"s":[1.0,0.0,0.0,1.0]},{"t":30,"s":[0.0,1.0,0.0,0.5]}]}}),
            );
            for index in [0, 1] {
                assert_eq!(
                    &node["c"]["k"][index]["s"].as_array().unwrap()[..3],
                    &[json!(0.0), json!(0.0), json!(1.0)]
                );
            }
            assert_eq!(node["c"]["k"][1]["s"][3], 0.5);
        }
        // tests.test_lottie_repaint.RepaintingReachesEveryColour.test_legacy_keyframe_end_values_are_recoloured
        #[test]
        fn test_legacy_keyframe_end_values_are_recoloured() {
            let node =
                tint(&json!({"c":{"a":1,"k":[{"t":0,"s":[1.0,0.0,0.0],"e":[0.0,1.0,0.0]}]}}));
            assert_eq!(
                &node["c"]["k"][0]["e"].as_array().unwrap()[..3],
                &[json!(0.0), json!(0.0), json!(1.0)]
            );
        }
    }
    mod gradient_opacity_survives_a_repaint {
        use super::*;
        // tests.test_lottie_repaint.GradientOpacitySurvivesARepaint.test_the_colour_stops_are_recoloured
        #[test]
        fn test_the_colour_stops_are_recoloured() {
            let node = tint(&gradient());
            let stops = node["g"]["k"]["k"].as_array().unwrap();
            for start in [1, 5] {
                assert_eq!(
                    &stops[start..start + 3],
                    &[json!(0.0), json!(0.0), json!(1.0)]
                );
            }
        }
        // tests.test_lottie_repaint.GradientOpacitySurvivesARepaint.test_the_opacity_ramp_is_left_alone
        #[test]
        fn test_the_opacity_ramp_is_left_alone() {
            let before = gradient();
            let node = tint(&before);
            assert_eq!(
                &node["g"]["k"]["k"].as_array().unwrap()[8..],
                &before["g"]["k"]["k"].as_array().unwrap()[8..]
            );
        }
        // tests.test_lottie_repaint.GradientOpacitySurvivesARepaint.test_the_offsets_are_left_alone
        #[test]
        fn test_the_offsets_are_left_alone() {
            let before = gradient();
            let node = tint(&before);
            for offset in [0, 4] {
                assert_eq!(node["g"]["k"]["k"][offset], before["g"]["k"]["k"][offset]);
            }
        }
        // tests.test_lottie_repaint.GradientOpacitySurvivesARepaint.test_a_gradient_with_no_declared_stop_count_is_handled_safely
        #[test]
        fn test_a_gradient_with_no_declared_stop_count_is_handled_safely() {
            let clean = tint(&json!({"g":{"k":{"a":0,"k":[0.0,1.0,0.0,0.0,1.0,0.0,1.0,0.0]}}}));
            assert_eq!(
                &clean["g"]["k"]["k"].as_array().unwrap()[1..4],
                &[json!(0.0), json!(0.0), json!(1.0)]
            );
            let ragged = json!({"g":{"k":{"a":0,"k":[0.0,1.0,0.0,0.0,1.0,0.5]}}});
            assert_eq!(tint(&ragged)["g"]["k"]["k"], ragged["g"]["k"]["k"]);
        }
    }
    mod the_repaint_api_lives_in_repaint {
        use super::*;
        // tests.test_lottie_repaint.TheRepaintApiLivesInRepaint.test_the_public_names_live_in_repaint_only
        #[test]
        fn test_the_public_names_live_in_repaint_only() {
            let t = Scratch::new();
            let link = production_link::ProductionLink::new(&t.0);
            link.admitted_and_executed(&t.0,"admitted-repaint",r##"fn main(){assert_eq!(numera_runtime::codec_tint::parse_tint("#0000FF").unwrap(),[0,0,255]);let _:fn(&std::path::Path,Option<[u8;3]>)->Result<bool,String>=numera_runtime::codec_lottie_prepare::rewrite;let _:fn(&std::path::Path,[u8;3])->Result<bool,String>=numera_runtime::codec_image::repaint_static;}"##);
            // Check each forbidden name independently: one missing symbol must
            // not conceal a re-export of the other under a different type.
            for (name, source) in [
                (
                    "parse_tint",
                    r##"fn main(){let _=numera_runtime::codec_image::parse_tint("#0000FF");}"##,
                ),
                (
                    "repaint_in_place",
                    r##"fn main(){let _=numera_runtime::codec_image::repaint_in_place;}"##,
                ),
            ] {
                let error = link.refused(&t.0, &format!("forbidden-{name}"), source, true);
                assert!(
                    error.contains("E0425"),
                    "the actual production symbol must be absent: {error}"
                );
                assert!(error.contains(name));
            }
        }
    }
}
mod test_native_preview_codec {
    use super::*;
    mod native_preview_codec {
        use super::*;
        // tests.test_native_preview_codec.NativePreviewCodec.test_static_and_lottie_preview_bytes_match_source
        #[test]
        fn test_static_and_lottie_preview_bytes_match_source() {
            let t = Scratch::new();
            let folder = fixture("native/baseline-video");
            let manifest: Value =
                serde_json::from_slice(&std::fs::read(folder.join("manifest.json")).unwrap())
                    .unwrap();
            assert_eq!(
                manifest["source_revision"],
                "a39a049c02f8dcc90d18fe8771869f82dd655221"
            );
            assert_eq!(manifest["dependencies"]["Pillow"], "12.3.0");
            assert_eq!(manifest["dependencies"]["rlottie-python"], "1.3.8");
            assert_eq!(manifest["dependencies"]["resvg-py"], "0.5.0");
            let rows = manifest["cases"].as_array().unwrap();
            assert_eq!(rows.len(), 12);
            let observed = rows
                .iter()
                .map(|row| {
                    (
                        row["format"].as_str().unwrap(),
                        row["size"].as_u64().unwrap(),
                        row["fps"].as_u64().unwrap(),
                        row["still"].as_bool().unwrap(),
                    )
                })
                .collect::<std::collections::BTreeSet<_>>();
            let expected = ["static", "animated", "video"]
                .into_iter()
                .flat_map(|format| {
                    [(104, 15), (72, 10)]
                        .into_iter()
                        .flat_map(move |(size, fps)| {
                            [true, false]
                                .into_iter()
                                .map(move |still| (format, size, fps, still))
                        })
                })
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                observed, expected,
                "all twelve original preview subcases must run"
            );
            for (index, row) in rows.iter().enumerate() {
                let source = project().join(row["input"].as_str().unwrap());
                let bytes = std::fs::read(&source).unwrap();
                assert_eq!(raster::digest_hex(&bytes), row["input_sha256"]);
                let expected =
                    std::fs::read(folder.join(row["expected"].as_str().unwrap())).unwrap();
                assert_eq!(raster::digest_hex(&expected), row["expected_sha256"]);
                let output = t.join(&format!("preview-{index}.webp"));
                let result=rt::codec_worker::execute_typed_at(Some(&t.0),&json!({"operation":"preview","source":source,"format":row["format"],"key":format!("fixture:{}",row["format"].as_str().unwrap()),"fps":row["fps"],"size":row["size"],"still":row["still"],"output":output})).unwrap();
                let actual = std::fs::read(&output).unwrap();
                #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
                if row["format"] == "video" {
                    assert_eq!(result, json!({"bytes": actual.len()}));
                    let got = raster::decode_animation(&actual, Default::default()).unwrap();
                    let want = raster::decode_animation(&expected, Default::default()).unwrap();
                    assert_eq!(got.loop_count, want.loop_count);
                    assert_eq!(got.frames.len(), want.frames.len());
                    for (g, w) in got.frames.iter().zip(want.frames.iter()) {
                        assert_eq!(g.rgba.dimensions(), w.rgba.dimensions());
                        assert_eq!(g.duration_ms, w.duration_ms);
                        let max_diff = g
                            .rgba
                            .as_raw()
                            .iter()
                            .zip(w.rgba.as_raw().iter())
                            .map(|(a, b)| a.abs_diff(*b))
                            .max()
                            .unwrap_or(0);
                        assert!(
                            max_diff <= 8,
                            "aarch64 video preview pixel drift {max_diff} > 8 for {row}"
                        );
                    }
                    continue;
                }
                assert_eq!(result, json!({"bytes":expected.len()}));
                assert_eq!(
                    actual, expected,
                    "original exact preview bytes differ for {row}"
                );
            }
            assert!(!t.join("catalog.db").exists());
        }
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
