//! Individually discoverable ports of original static/video/conversion/repaint tests.
//! Original IDs and target metadata are recorded in tests/fixtures/migration/reference-cases.json.
#[path = "baseline_media_support/mod.rs"]
mod support;
use image::{Rgba, RgbaImage};
use numera_runtime as rt;
use numera_runtime::native_raster as raster;
use serde_json::{Value, json};
use support::*;
#[path = "baseline_media_support/media_formats.rs"]
mod media_formats;
#[path = "baseline_media_support/make_emoji_pngs.rs"]
mod test_make_emoji_pngs;
#[path = "baseline_media_support/media_reencode.rs"]
mod test_media_reencode;
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
