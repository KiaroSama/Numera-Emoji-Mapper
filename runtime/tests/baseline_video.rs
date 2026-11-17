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
mod test_video_identity_fidelity {
    use super::*;
    mod alpha_fidelity_is_established_or_refused {
        use super::*;
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_a_healthy_decoder_still_separates_them
        #[test]
        fn test_a_healthy_decoder_still_separates_them() {
            let t = Scratch::new();
            let a = alpha(&t.0, "opaque", 255, "libvpx-vp9");
            let b = alpha(&t.0, "translucent", 128, "libvpx-vp9");
            assert_ne!(key(&t.0, &a), key(&t.0, &b));
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_a_missing_decoder_refuses_instead_of_dropping_alpha
        #[test]
        fn test_a_missing_decoder_refuses_instead_of_dropping_alpha() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_a_missing_decoder_refuses_instead_of_dropping_alpha",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("missing-decoder");
            assert_eq!(
                runtime()
                    .block_on(d.fingerprint_typed(&clip))
                    .unwrap_err()
                    .kind,
                "UndecodableVideo"
            );
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_a_missing_decoder_cannot_collapse_two_clips_into_one_key
        #[test]
        fn test_a_missing_decoder_cannot_collapse_two_clips_into_one_key() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_a_missing_decoder_cannot_collapse_two_clips_into_one_key",
            ) else {
                return;
            };
            let a = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let b = alpha(&fault.root, "translucent", 128, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("missing-decoder");
            let runtime = runtime();
            let keys = [a, b]
                .iter()
                .map(|p| match runtime.block_on(d.fingerprint_typed(p)) {
                    Ok(v) => Some(v["content_key"].clone()),
                    Err(e) => {
                        assert_eq!(e.kind, "UndecodableVideo");
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(keys, vec![None, None]);
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_a_probe_failure_is_not_cached_as_a_successful_fallback
        #[test]
        fn test_a_probe_failure_is_not_cached_as_a_successful_fallback() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_a_probe_failure_is_not_cached_as_a_successful_fallback",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("flaky-probe");
            let runtime = runtime();
            assert_eq!(
                runtime
                    .block_on(d.decoder_args_typed(&clip))
                    .unwrap_err()
                    .kind,
                "UndecodableVideo"
            );
            assert_eq!(
                runtime.block_on(d.decoder_args_typed(&clip)).unwrap(),
                vec!["-c:v", "libvpx-vp9"]
            );
            assert_eq!(
                fault.count(),
                3,
                "two real probes plus one decoder-capability query must run"
            );
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_a_container_naming_no_codec_is_not_read_as_needing_nothing
        #[test]
        fn test_a_container_naming_no_codec_is_not_read_as_needing_nothing() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_a_container_naming_no_codec_is_not_read_as_needing_nothing",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("no-codec");
            let error = runtime().block_on(d.decoder_args_typed(&clip)).unwrap_err();
            assert_eq!(error.kind, "UndecodableVideo");
            assert!(error.message.contains("no video codec"));
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_an_unreadable_capability_check_is_not_an_answer_of_no
        #[test]
        fn test_an_unreadable_capability_check_is_not_an_answer_of_no() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_an_unreadable_capability_check_is_not_an_answer_of_no",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("capability-error");
            let runtime = runtime();
            let error = runtime.block_on(d.decoder_args_typed(&clip)).unwrap_err();
            assert_eq!(error.kind, "UndecodableVideo");
            assert!(error.message.contains("could not ask ffmpeg"));
            fault.set("healthy");
            assert_eq!(
                runtime.block_on(d.decoder_args_typed(&clip)).unwrap(),
                vec!["-c:v", "libvpx-vp9"]
            );
            assert_eq!(
                fault.count(),
                2,
                "a failed capability query must not cache a no answer"
            );
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_an_undecodable_video_never_reaches_the_catalog
        #[test]
        fn test_an_undecodable_video_never_reaches_the_catalog() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_an_undecodable_video_never_reaches_the_catalog",
            ) else {
                return;
            };
            let a = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let b = alpha(&fault.root, "translucent", 128, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("missing-decoder");
            let db = fault.root.join("must-not-open.db");
            let outcome = runtime().block_on(d.fingerprint_typed(&a));
            assert_eq!(outcome.unwrap_err().kind, "UndecodableVideo");
            assert!(!db.exists());
            assert!(a.is_file() && b.is_file());
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_the_three_public_apis_agree_after_a_recovery
        #[test]
        fn test_the_three_public_apis_agree_after_a_recovery() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_the_three_public_apis_agree_after_a_recovery",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("missing-decoder");
            let runtime = runtime();
            assert_eq!(
                runtime
                    .block_on(d.fingerprint_typed(&clip))
                    .unwrap_err()
                    .kind,
                "UndecodableVideo"
            );
            fault.reset_tool();
            let info = runtime.block_on(d.fingerprint_typed(&clip)).unwrap();
            assert_eq!(
                info["content_key"],
                runtime.block_on(d.content_key(&clip)).unwrap()
            );
            assert_eq!(
                info["phash"],
                json!(runtime.block_on(d.perceptual_hash(&clip)).unwrap())
            );
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_concurrent_callers_get_one_consistent_answer
        #[test]
        fn test_concurrent_callers_get_one_consistent_answer() {
            let t = Scratch::new();
            let clip = alpha(&t.0, "opaque", 255, "libvpx-vp9");
            let d = std::sync::Arc::new(decoder(&t.0));
            let (send, receive) = std::sync::mpsc::channel();
            let mut threads = Vec::new();
            for _ in 0..8 {
                let d = d.clone();
                let clip = clip.clone();
                let send = send.clone();
                threads.push(std::thread::spawn(move || {
                    send.send(runtime().block_on(d.decoder_args_typed(&clip)).unwrap())
                        .unwrap();
                }));
            }
            drop(send);
            for _ in 0..8 {
                assert_eq!(
                    receive
                        .recv_timeout(std::time::Duration::from_secs(60))
                        .unwrap(),
                    vec!["-c:v", "libvpx-vp9"]
                );
            }
            for thread in threads {
                thread.join().unwrap();
            }
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_a_still_image_is_not_subjected_to_the_video_rule
        #[test]
        fn test_a_still_image_is_not_subjected_to_the_video_rule() {
            let t = Scratch::new();
            let still = png(&t.join("still.png"), [10, 200, 90, 255], (80, 60));
            assert!(
                runtime()
                    .block_on(decoder(&t.0).decoder_args_typed(&still))
                    .unwrap()
                    .is_empty()
            );
        }
    }
    mod vp8_is_held_to_the_same_rule {
        use super::*;
        // tests.test_video_identity_fidelity.Vp8IsHeldToTheSameRule.test_vp8_alpha_separates_two_clips
        #[test]
        fn test_vp8_alpha_separates_two_clips() {
            let t = Scratch::new();
            let a = alpha(&t.0, "vp8-opaque", 255, "libvpx");
            let b = alpha(&t.0, "vp8-translucent", 120, "libvpx");
            assert_ne!(key(&t.0, &a), key(&t.0, &b));
        }
        // tests.test_video_identity_fidelity.Vp8IsHeldToTheSameRule.test_vp8_without_its_decoder_refuses
        #[test]
        fn test_vp8_without_its_decoder_refuses() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::vp8_is_held_to_the_same_rule::test_vp8_without_its_decoder_refuses",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx");
            let d = decoder(&fault.root);
            fault.set("missing-decoder");
            assert_eq!(
                runtime()
                    .block_on(d.fingerprint_typed(&clip))
                    .unwrap_err()
                    .kind,
                "UndecodableVideo"
            );
        }
    }
}
mod test_video_timeline_comparison {
    use super::*;
    mod the_whole_timeline_is_compared {
        use super::*;
        // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_a_re_encode_of_the_same_clip_is_still_the_same_clip
        #[test]
        fn test_a_re_encode_of_the_same_clip_is_still_the_same_clip() {
            let f = Timelines::new(&["blue", "blue_again"]);
            assert_eq!(
                same(&f.scratch.0, f.get("blue"), f.get("blue_again")),
                Some(true)
            );
        }
        // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_the_same_opening_with_a_different_tail_is_not
        #[test]
        fn test_the_same_opening_with_a_different_tail_is_not() {
            let f = Timelines::new(&["blue", "green"]);
            assert_eq!(
                same(&f.scratch.0, f.get("blue"), f.get("green")),
                Some(false)
            );
        }
        // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_a_shorter_clip_is_not_the_same_clip
        #[test]
        fn test_a_shorter_clip_is_not_the_same_clip() {
            let f = Timelines::new(&["blue", "short"]);
            assert_eq!(
                same(&f.scratch.0, f.get("blue"), f.get("short")),
                Some(false)
            );
        }
        // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_a_fade_in_is_not_the_solid_version_of_itself
        #[test]
        fn test_a_fade_in_is_not_the_solid_version_of_itself() {
            let f = Timelines::new(&["fade", "solid"]);
            assert_eq!(
                same(&f.scratch.0, f.get("fade"), f.get("solid")),
                Some(false)
            );
        }
        // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_one_differing_frame_in_the_middle_is_not
        #[test]
        fn test_one_differing_frame_in_the_middle_is_not() {
            let f = Timelines::new(&["blue", "spiked"]);
            assert_eq!(
                key(&f.scratch.0, f.get("blue")),
                key(&f.scratch.0, f.get("spiked"))
            );
            assert_eq!(
                same(&f.scratch.0, f.get("blue"), f.get("spiked")),
                Some(false)
            );
        }
        // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_an_unreadable_clip_is_undecidable_not_negative
        #[test]
        fn test_an_unreadable_clip_is_undecidable_not_negative() {
            let f = Timelines::new(&["blue"]);
            let bad = file(
                &f.scratch.join("broken.webm"),
                b"\x1a\x45\xdf\xa3not a real stream",
            );
            assert_eq!(same(&f.scratch.0, f.get("blue"), &bad), None);
        }
        // tests.test_video_timeline_comparison.TheWholeTimelineIsCompared.test_the_comparison_is_bounded
        #[test]
        fn test_the_comparison_is_bounded() {
            let f = Timelines::new(&["blue"]);
            let timeline = runtime()
                .block_on(decoder(&f.scratch.0).timeline_typed(f.get("blue")))
                .unwrap();
            assert!(timeline.times.len() <= 128);
            assert_eq!(
                timeline.rgba.len(),
                timeline.times.len() * rt::codec_video_timeline::FRAME_BYTES
            );
        }
    }
    mod reconciliation_writes_nothing_for_a_foreign_clip {
        use super::*;
        // tests.test_video_timeline_comparison.ReconciliationWritesNothingForAForeignClip.test_a_foreign_clip_sharing_our_opening_resolves_to_nothing
        #[test]
        fn test_a_foreign_clip_sharing_our_opening_resolves_to_nothing() {
            let f = Timelines::new(&["blue", "green"]);
            let t = Scratch::new();
            let source = t.join("ours.webm");
            std::fs::copy(f.get("blue"), &source).unwrap();
            let mut cat = catalog(&t.0);
            let (_ours, _) = add(&mut cat, &t.0, &source, "ours", -1).unwrap();
            let server = Transport::new(std::collections::BTreeMap::from([(
                "served".into(),
                f.get("green").into(),
            )]));
            let result = resolve(&mut cat, &t.0, &server, "served").unwrap();
            assert_eq!(result, None);
        }
        // tests.test_video_timeline_comparison.ReconciliationWritesNothingForAForeignClip.test_our_own_clip_still_resolves_to_our_key
        #[test]
        fn test_our_own_clip_still_resolves_to_our_key() {
            let f = Timelines::new(&["blue", "blue"]);
            let t = Scratch::new();
            let source = t.join("ours.webm");
            std::fs::copy(f.get("blue"), &source).unwrap();
            let mut cat = catalog(&t.0);
            let (ours, _) = add(&mut cat, &t.0, &source, "ours", -1).unwrap();
            let server = Transport::new(std::collections::BTreeMap::from([(
                "served".into(),
                f.get("blue").into(),
            )]));
            let result = resolve(&mut cat, &t.0, &server, "served").unwrap();
            assert_eq!(result, Some(ours));
        }
        // tests.test_video_timeline_comparison.ReconciliationWritesNothingForAForeignClip.test_no_foreign_fuid_is_bound_and_it_survives_a_reopen
        #[test]
        fn test_no_foreign_fuid_is_bound_and_it_survives_a_reopen() {
            let f = Timelines::new(&["blue", "green"]);
            let t = Scratch::new();
            let source = t.join("ours.webm");
            std::fs::copy(f.get("blue"), &source).unwrap();
            let mut cat = catalog(&t.0);
            let (_ours, _) = add(&mut cat, &t.0, &source, "ours", -1).unwrap();
            let server = Transport::new(std::collections::BTreeMap::from([(
                "served".into(),
                f.get("green").into(),
            )]));
            let result = resolve(&mut cat, &t.0, &server, "served").unwrap();
            assert_eq!(result, None);
            drop(cat);
            let mut reopened = catalog(&t.0);
            assert_eq!(seen(&mut reopened, "fuid-served"), Value::Null);
            assert_eq!(reopened.all(None).unwrap().len(), 1);
        }
    }
}
mod test_video_collision_ingest {
    use super::*;
    mod native_video_boundaries {
        use super::*;
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_collision_preserves_both_files_and_distinct_identifiers_after_reopen
        #[test]
        fn test_collision_preserves_both_files_and_distinct_identifiers_after_reopen() {
            let f = Collision::new();
            let t = Scratch::new();
            assert_eq!(key(&t.0, f.get("red")), key(&t.0, f.get("middle")));
            let mut cat = catalog(&t.0);
            let (red, _) = add(&mut cat, &t.0, &f.copy(&t.0, "red"), "fuid-red", -1).unwrap();
            let (middle, new) =
                add(&mut cat, &t.0, &f.copy(&t.0, "middle"), "fuid-middle", -1).unwrap();
            assert!(new);
            assert_ne!(red, middle);
            assert!(t.join("middle.webm").is_file());
            let (alias, new) =
                add(&mut cat, &t.0, &f.copy(&t.0, "remux"), "fuid-remux", -1).unwrap();
            assert!(!new);
            assert_eq!(alias, middle);
            drop(cat);
            let mut cat = catalog(&t.0);
            assert_eq!(cat.all(None).unwrap().len(), 2);
            for (id, key) in [
                ("fuid-red", &red),
                ("fuid-middle", &middle),
                ("fuid-remux", &middle),
            ] {
                assert_eq!(seen(&mut cat, id), json!(key));
            }
            let a = cat.get(&red).unwrap().unwrap();
            let b = cat.get(&middle).unwrap().unwrap();
            assert_eq!(
                same(
                    &t.0,
                    std::path::Path::new(a["file_path"].as_str().unwrap()),
                    std::path::Path::new(b["file_path"].as_str().unwrap())
                ),
                Some(false)
            );
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_different_unmatched_tail_is_checked_in_both_directions
        #[test]
        fn test_different_unmatched_tail_is_checked_in_both_directions() {
            let f = Collision::new();
            for (a, b) in [("short", "last"), ("last", "short")] {
                assert_eq!(same(&f.scratch.0, f.get(a), f.get(b)), Some(false));
            }
            for (a, b) in [("short", "red"), ("red", "short")] {
                assert_eq!(same(&f.scratch.0, f.get(a), f.get(b)), Some(true));
            }
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_sample_cache_does_not_alias_relative_paths_in_different_directories
        #[test]
        fn test_sample_cache_does_not_alias_relative_paths_in_different_directories() {
            if !is_child() {
                let t = Scratch::new();
                child(
                    "test_video_collision_ingest::native_video_boundaries::test_sample_cache_does_not_alias_relative_paths_in_different_directories",
                    &t.0,
                    &[],
                );
                return;
            }
            let f = Collision::new();
            let t = Scratch::new();
            let size = std::fs::metadata(f.get("red"))
                .unwrap()
                .len()
                .max(std::fs::metadata(f.get("first")).unwrap().len())
                as usize;
            for (folder, name) in [("one", "red"), ("two", "first")] {
                let mut bytes = std::fs::read(f.get(name)).unwrap();
                bytes.resize(size, 0);
                let path = file(&t.join(&format!("{folder}/clip.webm")), &bytes);
                age(&path, 1700000000);
            }
            let d = decoder(&t.0);
            let runtime = runtime();
            std::env::set_current_dir(t.join("one")).unwrap();
            assert!(
                runtime
                    .block_on(d.frames_rgba_typed(std::path::Path::new("clip.webm"), 30))
                    .is_err()
            );
            let first = runtime
                .block_on(d.frames_rgba_typed(&std::fs::canonicalize("clip.webm").unwrap(), 30))
                .unwrap();
            std::env::set_current_dir(t.join("two")).unwrap();
            assert!(
                runtime
                    .block_on(d.frames_rgba_typed(std::path::Path::new("clip.webm"), 30))
                    .is_err()
            );
            let second = runtime
                .block_on(d.frames_rgba_typed(&std::fs::canonicalize("clip.webm").unwrap(), 30))
                .unwrap();
            let fresh = runtime
                .block_on(
                    decoder(&t.0)
                        .frames_rgba_typed(&std::fs::canonicalize("clip.webm").unwrap(), 30),
                )
                .unwrap();
            assert_eq!(second, fresh);
            assert_ne!(first, second);
            std::env::set_current_dir(project()).unwrap();
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_sample_cache_revalidates_after_toolchain_change
        #[test]
        fn test_sample_cache_revalidates_after_toolchain_change() {
            let Some(fault) = fault_case(
                "test_video_collision_ingest::native_video_boundaries::test_sample_cache_revalidates_after_toolchain_change",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let d = decoder(&fault.root);
            let runtime = runtime();
            runtime.block_on(d.frames_rgba_typed(&clip, 10)).unwrap();
            fault.set("missing-decoder");
            age(&fault.ffmpeg, 1700000300);
            let error = runtime
                .block_on(d.frames_rgba_typed(&clip, 10))
                .unwrap_err();
            assert_eq!(error.kind, "UndecodableVideo");
            assert!(error.message.contains("no libvpx-vp9 decoder"));
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_vfr_fixture_has_complete_native_timing_evidence
        #[test]
        fn test_vfr_fixture_has_complete_native_timing_evidence() {
            let f = Collision::new();
            let timeline = runtime()
                .block_on(decoder(&f.scratch.0).timeline_typed(f.get("vfr")))
                .unwrap();
            assert_eq!(timeline.times.len(), 4);
            assert!(((timeline.times[2] - timeline.times[1]) - 0.010).abs() < 0.0005);
            assert!(timeline.duration > *timeline.times.last().unwrap());
            assert!((timeline.duration - 1.0).abs() < 0.0005);
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_native_first_last_and_variable_rate_frames_cannot_disappear
        #[test]
        fn test_native_first_last_and_variable_rate_frames_cannot_disappear() {
            let f = Collision::new();
            for name in ["first", "middle", "last", "vfr"] {
                assert_eq!(same(&f.scratch.0, f.get("red"), f.get(name)), Some(false));
            }
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_exact_lookup_recovery_never_binds_the_foreign_clip
        #[test]
        fn test_exact_lookup_recovery_never_binds_the_foreign_clip() {
            let f = Collision::new();
            let t = Scratch::new();
            let mut cat = catalog(&t.0);
            add(&mut cat, &t.0, &f.copy(&t.0, "red"), "fuid-red", -1).unwrap();
            let server = Transport::new(std::collections::BTreeMap::from([(
                "foreign".into(),
                f.get("middle").into(),
            )]));
            assert_eq!(resolve(&mut cat, &t.0, &server, "foreign").unwrap(), None);
            assert_eq!(seen(&mut cat, "fuid-foreign"), Value::Null);
            assert_eq!(bases(&mut cat), json!([]));
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_recovery_and_resume_bind_the_colliding_item_only
        #[test]
        fn test_recovery_and_resume_bind_the_colliding_item_only() {
            let f = Collision::new();
            let t = Scratch::new();
            let mut cat = catalog(&t.0);
            let (red, _) = add(&mut cat, &t.0, &f.copy(&t.0, "red"), "fuid-red", -1).unwrap();
            let (middle, _) =
                add(&mut cat, &t.0, &f.copy(&t.0, "middle"), "fuid-middle", -1).unwrap();
            let db = rusqlite::Connection::open(t.join("catalog.db")).unwrap();
            db.execute("UPDATE items SET phash=-1 WHERE content_key=?", [&middle])
                .unwrap();
            drop(db);
            let server = Transport::new(std::collections::BTreeMap::from([(
                "live".into(),
                f.get("remux").into(),
            )]));
            let mut state = json!({"name":"fixture","fmt":"video","keys":[]});
            reconcile(&mut cat, &t.0, &server, &mut state).unwrap();
            assert_eq!(state["keys"], json!([middle]));
            drop(cat);
            let mut cat = catalog(&t.0);
            assert_eq!(seen(&mut cat, "fuid-live"), middle);
            assert_eq!(
                cat.dispatch(json!({"operation":"custom_id","base":"audit","content_key":middle}))
                    .unwrap(),
                "live"
            );
            assert_eq!(
                cat.dispatch(json!({"operation":"published","base":"audit","content_key":red}))
                    .unwrap(),
                false
            );
            reconcile(&mut cat, &t.0, &server, &mut state).unwrap();
            assert_eq!(state["keys"], json!([middle]));
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_upload_confirmation_checks_the_unmatched_terminal_frame
        #[test]
        fn test_upload_confirmation_checks_the_unmatched_terminal_frame() {
            let f = Collision::new();
            let t = Scratch::new();
            let mut cat = catalog(&t.0);
            let (short, _) = add(&mut cat, &t.0, &f.copy(&t.0, "short"), "fuid-short", -1).unwrap();
            let mut item = cat.get(&short).unwrap().unwrap();
            item["emojis"] = json!(["x"]);
            item["keywords"] = json!([]);
            let server = Transport::new(std::collections::BTreeMap::from([(
                "live".into(),
                f.get("last").into(),
            )]));
            let tg = server.telegram();
            let runtime = runtime();
            let codec = Codec::new(&t.0);
            let p = rt::publication::Publisher {
                tg: &tg,
                cat: &mut cat,
                media: &codec,
                data: t.0.clone(),
                runtime: &runtime,
                config: None,
            };
            let error = p
                .mutate(&item, "fixture", Some("Fixture"), 1, false)
                .unwrap_err();
            assert!(matches!(error, rt::telegram::ApiError::Unproven(_)));
            assert!(error.to_string().contains("not proven ours"));
            assert_eq!(seen(&mut cat, "fuid-live"), Value::Null);
            assert_eq!(bases(&mut cat), json!([]));
            assert!(
                server
                    .calls
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|path| path.ends_with("/createNewStickerSet"))
            );
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_near_merge_verifies_content_and_accepts_a_real_reencode
        #[test]
        fn test_near_merge_verifies_content_and_accepts_a_real_reencode() {
            let f = Collision::new();
            let t = Scratch::new();
            let mut cat = catalog(&t.0);
            let (red, _) = add(&mut cat, &t.0, &f.copy(&t.0, "red"), "fuid-red", 4).unwrap();
            let (last, new) = add(&mut cat, &t.0, &f.copy(&t.0, "last"), "fuid-last", 4).unwrap();
            assert!(new);
            assert_ne!(red, last);
            let (pattern, _) =
                add(&mut cat, &t.0, &f.copy(&t.0, "pattern"), "fuid-pattern", 4).unwrap();
            assert_ne!(key(&t.0, f.get("pattern")), key(&t.0, f.get("encoded")));
            let (encoded, new) =
                add(&mut cat, &t.0, &f.copy(&t.0, "encoded"), "fuid-encoded", 4).unwrap();
            assert!(!new);
            assert_eq!(encoded, pattern);
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_unreadable_exact_candidate_preserves_input_and_binds_nothing
        #[test]
        fn test_unreadable_exact_candidate_preserves_input_and_binds_nothing() {
            let f = Collision::new();
            let t = Scratch::new();
            let mut cat = catalog(&t.0);
            let (red, _) = add(&mut cat, &t.0, &f.copy(&t.0, "red"), "fuid-red", -1).unwrap();
            let row = cat.get(&red).unwrap().unwrap();
            std::fs::remove_file(row["file_path"].as_str().unwrap()).unwrap();
            let path = f.copy(&t.0, "middle");
            assert!(add(&mut cat, &t.0, &path, "foreign", -1).is_err());
            assert!(path.is_file());
            assert_eq!(seen(&mut cat, "foreign"), Value::Null);
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_untracked_storage_collision_never_overwrites_the_existing_bytes
        #[test]
        fn test_untracked_storage_collision_never_overwrites_the_existing_bytes() {
            let f = Collision::new();
            let t = Scratch::new();
            let source = f.copy(&t.0, "middle");
            let destination = file(&t.join("occupied.webm"), b"unrelated sentinel");
            let codec = Codec::new(&t.0);
            let stored = runtime()
                .block_on(rt::media_store::store(
                    &codec,
                    &source,
                    &destination,
                    "video",
                    &key(&t.0, &source),
                    &json!({}),
                ))
                .unwrap();
            assert_eq!(std::fs::read(&destination).unwrap(), b"unrelated sentinel");
            assert_ne!(stored, destination);
            assert_eq!(
                std::fs::read(stored).unwrap(),
                std::fs::read(f.get("middle")).unwrap()
            );
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_all_ingest_paths_preserve_existing_destination_and_incoming_difference
        #[test]
        fn test_all_ingest_paths_preserve_existing_destination_and_incoming_difference() {
            let f = Collision::new();
            let t = Scratch::new();
            for route in ["pack", "ids", "local"] {
                let data = t.join(route);
                std::fs::create_dir(&data).unwrap();
                let sentinel = file(
                    &data.join("tmp/another-writer.download"),
                    b"not this run's scratch file",
                );
                let clips = std::collections::BTreeMap::from([
                    ("red".into(), f.get("red").into()),
                    ("middle".into(), f.get("middle").into()),
                ]);
                let codec = Codec::new(&data);
                if route == "local" {
                    let cfg = config(&data, &[]);
                    let log = rt::logging::RunLog::open(&data, "local-collision", vec![]);
                    let args = vec![
                        f.get("red").to_string_lossy().into_owned(),
                        f.get("middle").to_string_lossy().into_owned(),
                        "--data-dir".into(),
                        data.to_string_lossy().into(),
                    ];
                    assert_eq!(
                        rt::local_cli::run_with_codec(&cfg, &args, &log, |_| Ok(codec.clone()))
                            .unwrap(),
                        0
                    );
                } else {
                    let server = Transport::new(clips);
                    let tg = server.telegram();
                    let mut cat = catalog(&data);
                    let mut collector = rt::collector::Collector {
                        cat: &mut cat,
                        media: &codec,
                        data: data.clone(),
                        threshold: -1,
                    };
                    let runtime = runtime();
                    let log = rt::logging::RunLog::open(&data, "remote-collision", vec![]);
                    let counts = if route == "pack" {
                        collector
                            .fetch_pack(&tg, &runtime, "fixture", 0, "keep")
                            .unwrap()
                    } else {
                        rt::fetch_ids_cli::fetch_ids(
                            &mut collector,
                            &tg,
                            &runtime,
                            &["red".into(), "middle".into()],
                            "keep",
                            None,
                            &log,
                        )
                        .unwrap()
                    };
                    assert_eq!(counts["failed"], 0);
                }
                assert_eq!(
                    std::fs::read(sentinel).unwrap(),
                    b"not this run's scratch file"
                );
                let cat = catalog(&data);
                let rows = cat.all(None).unwrap();
                assert_eq!(rows.len(), 2);
                assert_ne!(rows[0]["file_path"], rows[1]["file_path"]);
                for row in &rows {
                    assert!(std::path::Path::new(row["file_path"].as_str().unwrap()).is_file());
                }
                assert_eq!(
                    same(
                        &data,
                        std::path::Path::new(rows[0]["file_path"].as_str().unwrap()),
                        std::path::Path::new(rows[1]["file_path"].as_str().unwrap())
                    ),
                    Some(false)
                );
            }
        }
    }
    mod timeline_alignment {
        use super::*;
        // tests.test_video_collision_ingest.TimelineAlignment.test_equal_frames_at_different_times_do_not_prove_equivalent_timelines
        #[test]
        fn test_equal_frames_at_different_times_do_not_prove_equivalent_timelines() {
            let raw = [RED.repeat(64 * 64), BLUE.repeat(64 * 64)].concat();
            let a = rt::codec_video_timeline::Timeline {
                rgba: std::sync::Arc::new(raw.clone()),
                times: vec![0.0, 0.3],
                duration: 1.0,
            };
            let b = rt::codec_video_timeline::Timeline {
                rgba: std::sync::Arc::new(raw),
                times: vec![0.0, 0.4],
                duration: 1.0,
            };
            assert_eq!(rt::codec_video_timeline::same_image(&a, &b), Some(false));
        }
    }
}
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
                "a32dbab6e3e694f1fac9656997fa2edb6e3b2ede"
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
                assert_eq!(result, json!({"bytes":expected.len()}));
                assert_eq!(
                    std::fs::read(&output).unwrap(),
                    expected,
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
