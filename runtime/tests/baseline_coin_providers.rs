//! 117 authoritative original coin/provider IDs, including8 inherited executions.
#[path = "baseline_coin_providers_support/mod.rs"]
mod support;
use numera_runtime as rt;
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, time::Duration};
use support::*;

mod blank_provider_logo {
    use super::*;
    // tests.test_coin_providers.BlankProviderLogo.test_fully_transparent_image_is_rejected
    #[test]
    fn test_fully_transparent_image_is_rejected() {
        let t = Root::new();
        let dest = t.0.join("out.png");
        assert!(!rt::codec_image::provider_png(&solid(64, [0, 0, 0, 0]), &dest).unwrap());
        assert!(!dest.exists(), "blank emoji must not be written");
    }
    // tests.test_coin_providers.BlankProviderLogo.test_almost_empty_image_is_rejected
    #[test]
    fn test_almost_empty_image_is_rejected() {
        let t = Root::new();
        let dest = t.0.join("out.png");
        let mut img = image::RgbaImage::from_pixel(64, 64, image::Rgba([0, 0, 0, 0]));
        for x in 0..4 {
            img.put_pixel(x, 0, image::Rgba([255, 0, 0, 255]));
        }
        assert!(!rt::codec_image::provider_png(&png(img), &dest).unwrap());
        assert!(!dest.exists());
    }
    // tests.test_coin_providers.BlankProviderLogo.test_real_logo_is_still_accepted
    #[test]
    fn test_real_logo_is_still_accepted() {
        let t = Root::new();
        let dest = t.0.join("out.png");
        assert!(rt::codec_image::provider_png(&ramp(false), &dest).unwrap());
        let img = image::open(dest).unwrap();
        assert_eq!((img.width(), img.height()), (100, 100));
        assert_eq!(img.color(), image::ColorType::Rgba8);
    }
}
#[path = "baseline_coin_providers_support/logo_cache.rs"]
mod logo_cache;
mod verified_publish {
    use super::*;
    // tests.test_coin_providers.VerifiedPublish.test_the_live_count_is_sent_as_expected_before
    #[test]
    pub(super) fn test_the_live_count_is_sent_as_expected_before() {
        assert_verified_normal("normal");
    }
    // tests.test_coin_providers.VerifiedPublish.test_the_id_is_ours_even_when_another_sticker_appears
    #[test]
    pub(super) fn test_the_id_is_ours_even_when_another_sticker_appears() {
        let (f, result, _) = publish_case("foreign-tail");
        assert_eq!((result.0, result.1), (1, 0));
        assert_eq!(result.2["aaa"], "c3");
        assert_ne!(result.2["aaa"], "c4", "tail attribution is the bug");
        assert_eq!(doc(&f.files.mapping)["aaa"], "c3");
    }
    // tests.test_coin_providers.VerifiedPublish.test_an_unidentifiable_sticker_is_not_mapped
    #[test]
    pub(super) fn test_an_unidentifiable_sticker_is_not_mapped() {
        let (_, result, _) = publish_case("duplicate");
        assert_eq!((result.0, result.1), (0, 1));
        assert!(!result.2.contains_key("aaa"));
    }
    // tests.test_coin_providers.VerifiedPublish.test_a_failed_add_is_neither_mapped_nor_counted_as_added
    #[test]
    pub(super) fn test_a_failed_add_is_neither_mapped_nor_counted_as_added() {
        let (f, result, _) = publish_case("rejected");
        assert_eq!((result.0, result.1), (0, 1));
        assert!(!result.2.contains_key("aaa"));
        assert!(doc(&f.files.state)["provider_in_flight"].is_null());
    }
    // tests.test_coin_providers.VerifiedPublish.test_an_ambiguous_add_is_resolved_from_live_state_not_re_sent
    #[test]
    pub(super) fn test_an_ambiguous_add_is_resolved_from_live_state_not_re_sent() {
        assert_verified_normal("ambiguous");
    }
    // tests.test_coin_providers.VerifiedPublish.test_a_full_set_rolls_over_and_the_new_set_is_recorded_after_it_exists
    #[test]
    pub(super) fn test_a_full_set_rolls_over_and_the_new_set_is_recorded_after_it_exists() {
        let (f, result, r) = publish_case("rollover");
        assert_eq!((result.0, result.1), (1, 0));
        let creates = r
            .iter()
            .filter(|r| r.method == "createNewStickerSet")
            .collect::<Vec<_>>();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].form["name"], NEXT_SET);
        assert_eq!(
            doc(&f.files.state)["sets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![SET, NEXT_SET]
        );
        assert_eq!(result.2["aaa"], "c201");
    }
    // tests.test_coin_providers.VerifiedPublish.test_a_second_publisher_is_refused_instead_of_appending_too
    #[test]
    pub(super) fn test_a_second_publisher_is_refused_instead_of_appending_too() {
        let f = Provider::new(2);
        let mut stage = f.stage(&["aaa"], &f.ours);
        let server = Server::new(vec![]);
        let hold = rt::locks::Lease::acquire(
            &rt::locks::pack_family_path(&f.root.0, "cryptoemoji"),
            "real provider family hold",
        )
        .unwrap();
        assert!(f.run(&server, &mut stage, &["aaa"]).is_err());
        assert!(server.finish().is_empty());
        drop(hold);
        f.release();
    }
    // tests.test_coin_providers.VerifiedPublish.test_a_rebuild_holding_the_family_lock_blocks_a_top_up
    #[test]
    pub(super) fn test_a_rebuild_holding_the_family_lock_blocks_a_top_up() {
        let f = Provider::new(2);
        let mut stage = f.stage(&["aaa"], &f.ours);
        let server = Server::new(vec![]);
        let rebuild_family = rt::locks::pack_family_path(&f.root.0, "cryptoemoji");
        let hold = rt::locks::Lease::acquire(&rebuild_family, "real rebuild family hold").unwrap();
        assert!(f.run(&server, &mut stage, &["aaa"]).is_err());
        assert!(server.finish().is_empty());
        drop(hold);
        f.release();
    }
}
#[path = "baseline_coin_providers_support/canonical_map.rs"]
mod canonical_map;

mod inherited_canonical_map {
    // tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_the_live_count_is_sent_as_expected_before
    #[test]
    fn test_the_live_count_is_sent_as_expected_before() {
        super::verified_publish::test_the_live_count_is_sent_as_expected_before();
    }
    // tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_the_id_is_ours_even_when_another_sticker_appears
    #[test]
    fn test_the_id_is_ours_even_when_another_sticker_appears() {
        super::verified_publish::test_the_id_is_ours_even_when_another_sticker_appears();
    }
    // tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_an_unidentifiable_sticker_is_not_mapped
    #[test]
    fn test_an_unidentifiable_sticker_is_not_mapped() {
        super::verified_publish::test_an_unidentifiable_sticker_is_not_mapped();
    }
    // tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_a_failed_add_is_neither_mapped_nor_counted_as_added
    #[test]
    fn test_a_failed_add_is_neither_mapped_nor_counted_as_added() {
        super::verified_publish::test_a_failed_add_is_neither_mapped_nor_counted_as_added();
    }
    // tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_an_ambiguous_add_is_resolved_from_live_state_not_re_sent
    #[test]
    fn test_an_ambiguous_add_is_resolved_from_live_state_not_re_sent() {
        super::verified_publish::test_an_ambiguous_add_is_resolved_from_live_state_not_re_sent();
    }
    // tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_a_full_set_rolls_over_and_the_new_set_is_recorded_after_it_exists
    #[test]
    fn test_a_full_set_rolls_over_and_the_new_set_is_recorded_after_it_exists() {
        super::verified_publish::test_a_full_set_rolls_over_and_the_new_set_is_recorded_after_it_exists();
    }
    // tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_a_second_publisher_is_refused_instead_of_appending_too
    #[test]
    fn test_a_second_publisher_is_refused_instead_of_appending_too() {
        super::verified_publish::test_a_second_publisher_is_refused_instead_of_appending_too();
    }
    // tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_a_rebuild_holding_the_family_lock_blocks_a_top_up
    #[test]
    fn test_a_rebuild_holding_the_family_lock_blocks_a_top_up() {
        super::verified_publish::test_a_rebuild_holding_the_family_lock_blocks_a_top_up();
    }
}
#[path = "baseline_coin_providers_support/remap_cache.rs"]
mod remap_cache;
mod nearest {
    use super::*;
    // tests.test_remap_ids.NearestTest.test_identical_rows_never_produce_negative_or_nan_distance
    #[test]
    fn test_identical_rows_never_produce_negative_or_nan_distance() {
        let row = vec![255.0; 768];
        let live = vec![row.clone(); 3];
        let result = rt::coin_remap_math::nearest(std::slice::from_ref(&row), &live).unwrap();
        assert!(result.squared.iter().all(|d| *d >= 0.0));
        assert!(result.squared.iter().all(|d| !d.sqrt().is_nan()));
        assert!(result.second.iter().all(|d| *d >= 0.0));
    }
    // tests.test_remap_ids.NearestTest.test_single_candidate_has_infinite_margin
    #[test]
    fn test_single_candidate_has_infinite_margin() {
        let result = rt::coin_remap_math::nearest(&[vec![0.0; 768]], &[vec![1.0; 768]]).unwrap();
        assert_eq!(result.indices, vec![0]);
        assert!(result.second.iter().all(|d| d.is_infinite()));
    }
}
mod informative_signatures {
    use super::*;
    fn marks() -> (Vec<u8>, Vec<u8>) {
        let left = image::RgbaImage::from_fn(100, 100, |x, y| {
            if (10..45).contains(&x) && (10..90).contains(&y) {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            }
        });
        let ring = image::RgbaImage::from_fn(100, 100, |x, y| {
            let d = ((x as f64 - 50.0).powi(2) + (y as f64 - 50.0).powi(2)).sqrt();
            if (20..80).contains(&x) && (20..80).contains(&y) && (22.0..=29.0).contains(&d) {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            }
        });
        (png(left), png(ring))
    }
    fn norm(a: &[u8], b: &[u8]) -> f64 {
        let a = rt::codec_image::remap_signature(a).unwrap();
        let b = rt::codec_image::remap_signature(b).unwrap();
        a.iter()
            .zip(b)
            .map(|(a, b)| (f64::from(*a) - f64::from(b)).powi(2))
            .sum::<f64>()
            .sqrt()
    }
    // tests.test_remap_ids.TheSignatureMustNotBeBlindToBlackOnTransparent.test_two_different_black_marks_are_not_identical
    #[test]
    fn test_two_different_black_marks_are_not_identical() {
        let (a, b) = marks();
        assert!(
            norm(&a, &b) > 0.0,
            "different black marks must not measure identical"
        );
    }
    // tests.test_remap_ids.TheSignatureMustNotBeBlindToBlackOnTransparent.test_a_black_mark_carries_information_at_all
    #[test]
    fn test_a_black_mark_carries_information_at_all() {
        let (a, _) = marks();
        let sig = rt::codec_image::remap_signature(&a).unwrap();
        assert_eq!(sig.len(), 1024);
        assert!(
            *sig.iter().max().unwrap() > 0,
            "allzero signature cannot see this image"
        );
    }
    // tests.test_remap_ids.TheSignatureMustNotBeBlindToBlackOnTransparent.test_colour_still_separates_two_marks_of_the_same_shape
    #[test]
    fn test_colour_still_separates_two_marks_of_the_same_shape() {
        assert!(
            norm(&block([220, 30, 30, 255]), &block([30, 30, 220, 255])) > 0.0,
            "colour information was lost"
        );
    }
}
#[path = "baseline_coin_providers_support/remap_apply.rs"]
mod remap_apply;
mod shared_contracts {
    use super::*;
    // tests.test_coin_providers.VerifyLogosModule.test_module_imports_cleanly
    #[test]
    fn test_module_imports_cleanly() {
        let t = Root::new();
        let run: fn(&rt::config::Config, &[String], &rt::logging::RunLog) -> Result<u8, String> =
            rt::coin_verify_cli::run;
        assert_eq!(
            run(&config(&t.0, &[]), &ids(&["--help"]), &t.log()).unwrap(),
            0
        );
    }
    // tests.test_coin_providers.VerifyLogosModule.test_it_uses_the_shared_pack_lock
    #[test]
    fn test_it_uses_the_shared_pack_lock() {
        let t = Root::new();
        let actual = rt::locks::pack_family_path(&t.0, "cryptoemoji");
        let holder = rt::locks::Lease::acquire(&actual, "shared verify/provider family").unwrap();
        let f = rt::coin_provider_recover::Files {
            root: t.0.clone(),
            state: t.0.join("state.json"),
            mapping: t.0.join("map.json"),
            published: t.0.join("emoji"),
            base: "cryptoemoji".into(),
        };
        std::fs::create_dir(&f.published).unwrap();
        write(&f.state, &json!({"sets":[]}));
        write(&f.mapping, &json!({}));
        let mut stage = rt::coin_provider_stage::Stage::new(&f.published).unwrap();
        let server = Server::new(vec![]);
        assert!(
            rt::coin_provider_effects::run(
                &f,
                &server.telegram(),
                &mut stage,
                &ids(&["aaa"]),
                "T",
                1,
                &Map::new()
            )
            .is_err()
        );
        assert!(server.finish().is_empty());
        drop(holder);
        assert!(
            include_str!("../src/coin_verify_cli.rs").contains("crate::locks::pack_family_path")
        );
    }
    // tests.test_coin_providers.CommandExitCodes.test_both_fetchers_publish_through_the_same_helper
    #[test]
    fn test_both_fetchers_publish_through_the_same_helper() {
        let source = include_str!("../src/coin_provider_cli.rs");
        assert_eq!(
            source.matches("crate::coin_provider_effects::run(").count(),
            1,
            "one shared mutation implementation must serve both provider branches"
        );
        let t = Root::new();
        for provider in ["paprika", "cmc"] {
            assert_eq!(
                rt::coin_provider_cli::run(
                    &config(&t.0, &[]),
                    provider,
                    &ids(&["--help"]),
                    &t.log()
                )
                .unwrap(),
                0
            );
        }
    }
    // tests.test_coin_providers.OnePackFamilyOneLock.test_every_coin_tool_locks_on_the_pack_base
    #[test]
    fn test_every_coin_tool_locks_on_the_pack_base() {
        let t = Root::new();
        let family = rt::locks::pack_family_path(&t.0, "cryptoemoji");
        assert_eq!(family, t.0.join(".locks/pack_cryptoemoji.lock"));
        for (name, source) in [
            ("provider", include_str!("../src/coin_provider_recover.rs")),
            ("rebuild", include_str!("../src/coin_rebuild_effects.rs")),
            ("replace", include_str!("../src/coin_replace_effects.rs")),
        ] {
            assert!(
                source.contains("pack_family_path"),
                "{name} must use shared family lock"
            );
        }
    }
    // tests.test_coin_providers.TheProvidersAndTheRebuildShareOneStateFile.test_both_tools_name_the_same_state_file
    #[test]
    fn test_both_tools_name_the_same_state_file() {
        let provider = include_str!("../src/coin_provider_cli.rs");
        let rebuild = include_str!("../src/coin_rebuild_cli.rs");
        assert!(provider.contains("state: directory.join(\"rebuild_dedup_state.json\")"));
        assert!(rebuild.contains("rebuild_dedup_state.json"));
        assert!(!provider.contains("state: directory.join(\"rebuild_state.json\")"));
        assert_eq!(
            provider
                .matches("crate::coin_provider_effects::run(")
                .count(),
            1
        );
    }
    // tests.test_coin_providers.TheProvidersAndTheRebuildShareOneStateFile.test_every_set_record_the_providers_write_carries_live
    #[test]
    fn test_every_set_record_the_providers_write_carries_live() {
        for source in [
            include_str!("../src/coin_provider_publish.rs"),
            include_str!("../src/coin_provider_proof.rs"),
        ] {
            let records = source
                .split("json!({\"index\":")
                .skip(1)
                .collect::<Vec<_>>();
            assert_eq!(
                records.len(),
                1,
                "set-record writers moved; recheck original source contract"
            );
            assert!(records[0].split('}').next().unwrap().contains("\"live\""));
        }
        assert_eq!(
            rt::coin_state::problem(
                &json!({"sets":[{"index":1,"name":"s1","title":"T","live":1}],"order":[],"cursor":0,"in_flight":null}),
                None
            ),
            ""
        );
    }
    // tests.test_coin_providers.TheProvidersAndTheRebuildShareOneStateFile.test_the_provider_intent_cannot_collide_with_the_rebuild_s
    #[test]
    fn test_the_provider_intent_cannot_collide_with_the_rebuild_s() {
        let intent = json!({"key":"aaa","operation":"add","set_name":"s1","set_index":1,"expected_before":0});
        let mut shared = json!({"sets":[{"index":1,"name":"s1","title":"T"}],"order":[],"cursor":0,"in_flight":null,"provider_in_flight":intent});
        assert_eq!(rt::coin_state::problem(&shared, None), "");
        let provider = shared
            .as_object_mut()
            .unwrap()
            .shift_remove("provider_in_flight")
            .unwrap();
        shared["in_flight"] = provider;
        assert_ne!(
            rt::coin_state::problem(&shared, Some(&[json!({"rep":"zzz"})])),
            ""
        );
    }
    // tests.test_coin_providers.TheProvidersAndTheRebuildShareOneStateFile.test_a_provider_top_up_does_not_brick_the_next_rebuild
    #[test]
    fn test_a_provider_top_up_does_not_brick_the_next_rebuild() {
        let mut state = json!({"sets":[{"index":1,"name":"s1","title":"T"}],"order":["plan-a","plan-b"],"cursor":2,"in_flight":null});
        rt::coin_provider_proof::record(&mut state, "newcoin").unwrap();
        assert_eq!(state["provider_added"], json!(["newcoin"]));
        assert_eq!(
            state["order"].as_array().unwrap().len()
                + state["provider_added"].as_array().unwrap().len(),
            3
        );
        assert_eq!(rt::coin_state::problem(&state, None), "");
    }
    // tests.test_coin_providers.TheProvidersAndTheRebuildShareOneStateFile.test_the_tally_never_counts_one_upload_twice
    #[test]
    fn test_the_tally_never_counts_one_upload_twice() {
        let mut state = json!({"provider_added":["aaa"]});
        rt::coin_provider_proof::record(&mut state, "aaa").unwrap();
        assert_eq!(state["provider_added"], json!(["aaa"]));
        assert!(
            rt::coin_state::problem(
                &json!({"sets":[],"order":[],"cursor":0,"provider_added":["aaa","aaa"]}),
                None
            )
            .contains("repeats")
        );
    }
    // tests.test_remap_ids.PackFamilyLockTest.test_it_is_the_same_lock_the_pack_mutators_take
    #[test]
    fn test_it_is_the_same_lock_the_pack_mutators_take() {
        let t = Root::new();
        let family = rt::locks::pack_family_path(&t.0, "cryptoemoji");
        assert_eq!(family, t.0.join(".locks/pack_cryptoemoji.lock"));
        for src in [
            include_str!("../src/coin_remap.rs"),
            include_str!("../src/coin_provider_recover.rs"),
            include_str!("../src/coin_rebuild_effects.rs"),
        ] {
            assert!(src.contains("pack_family_path"));
        }
    }
}
mod provider_search {
    use super::*;
    // tests.test_coin_providers.AnOutageIsNotNoMatch.test_retry_unmatched_searches_a_cached_miss_again
    #[test]
    fn test_retry_unmatched_searches_a_cached_miss_again() {
        let original = json!({"aaa":{"id":null,"conf":"no-results","name":"Alpha Coin"}});
        let mut cache = original.as_object().unwrap().clone();
        let mut searched = vec![];
        let missing = vec![("Alpha Coin".into(), "aaa".into())];
        let result = rt::coin_provider_resolve::resolve(
            &missing,
            &mut cache,
            false,
            |n, t| {
                searched.push((n.to_owned(), t.to_owned()));
                Ok(rt::coin_provider_resolve::Search::Match {
                    id: Some("alpha-coin".into()),
                    confidence: "symbol".into(),
                })
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(
            result,
            rt::coin_provider_resolve::Resolved {
                quota: false,
                unknown: 0
            }
        );
        assert!(searched.is_empty());
        let result = rt::coin_provider_resolve::resolve(
            &missing,
            &mut cache,
            true,
            |n, t| {
                searched.push((n.to_owned(), t.to_owned()));
                Ok(rt::coin_provider_resolve::Search::Match {
                    id: Some("alpha-coin".into()),
                    confidence: "symbol".into(),
                })
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(
            result,
            rt::coin_provider_resolve::Resolved {
                quota: false,
                unknown: 0
            }
        );
        assert_eq!(searched, vec![("Alpha Coin".into(), "aaa".into())]);
        assert_eq!(cache["aaa"]["id"], "alpha-coin");
    }
}
mod remap_family_serialization {
    use super::*;
    // tests.test_remap_ids.ApplyIsSerialisedAgainstThePackFamily.test_a_provider_cannot_land_a_map_entry_during_the_live_read
    #[test]
    fn test_a_provider_cannot_land_a_map_entry_during_the_live_read() {
        let f = Remap::new();
        f.local("btc", &block([200, 20, 20, 255]));
        write(&f.files.output, &json!({"btc":"STALE"}));
        let landed = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let observed = landed.clone();
        let root = f.root.0.clone();
        let output = f.files.output.clone();
        let mut script = remap_script(
            &["a"],
            &BTreeMap::from([("a".into(), block([200, 20, 20, 255]))]),
            &[],
        );
        script[0].action = Some(Box::new(move |_| {
            observed
                .lock()
                .unwrap()
                .push(provider_top_up(&root, &output, "new", "z"));
        }));
        let server = Server::new(script);
        assert_eq!(f.run(&server, Some(100.0), true).unwrap(), 0);
        assert_eq!(landed.lock().unwrap().len(), 1);
        assert!(
            landed.lock().unwrap()[0].is_err(),
            "foreign writer must be refused, not raced"
        );
        assert_eq!(doc(&f.files.output), json!({"btc":"a"}));
        server.finish();
        provider_top_up(&f.root.0, &f.files.output, "new", "z").unwrap();
        assert_eq!(doc(&f.files.output), json!({"btc":"a","new":"z"}));
    }
    // tests.test_remap_ids.ApplyIsSerialisedAgainstThePackFamily.test_a_held_pack_lock_stops_apply_before_it_reads_anything
    #[test]
    fn test_a_held_pack_lock_stops_apply_before_it_reads_anything() {
        let f = Remap::new();
        f.local("btc", &block([200, 20, 20, 255]));
        write(&f.files.output, &json!({"btc":"STALE"}));
        let hold = rt::locks::Lease::acquire(
            &rt::locks::pack_family_path(&f.root.0, "cryptoemoji"),
            "actual provider hold",
        )
        .unwrap();
        let server = Server::new(vec![]);
        assert_eq!(f.run(&server, Some(100.0), true).unwrap(), 4);
        assert!(
            server.finish().is_empty(),
            "lock must precede every live read/download"
        );
        assert_eq!(doc(&f.files.output), json!({"btc":"STALE"}));
        assert!(!f.files.output.with_extension("prebroken.json").exists());
        drop(hold);
    }
}
mod native_numerical_dependency {
    use super::*;
    // tests.test_remap_ids.NumpyIsAnExtra.test_a_missing_numpy_says_which_file_to_install
    // Migration: NumPy is removed; actual standalone remap must work with no interpreter/wheel lookup possible.
    #[test]
    fn test_a_missing_numpy_says_which_file_to_install() {
        let f = Remap::new();
        f.local("btc", &block([200, 20, 20, 255]));
        let server = Server::new(remap_script(
            &["a"],
            &BTreeMap::from([("a".into(), block([200, 20, 20, 255]))]),
            &[],
        ));
        let result = command(
            &f.root,
            &server.base,
            &server.base,
            &ids(&[
                "coin-remap-ids",
                "--emoji-dir",
                f.files.images.to_str().unwrap(),
                "--state",
                f.files.state.to_str().unwrap(),
                "--cache",
                f.files.cache.to_str().unwrap(),
                "--out",
                f.files.output.to_str().unwrap(),
                "--candidates",
                f.files.candidates.to_str().unwrap(),
            ]),
            &[
                ("PATH", ""),
                (
                    "PYTHONHOME",
                    f.root.0.join("missing-python").to_str().unwrap(),
                ),
                ("TELEGRAM_BOT_TOKEN", TOKEN),
            ],
        );
        assert_eq!(result.code, 0, "{}", result.stderr);
        assert!(result.stdout.contains("remap matched=1"));
        let cache = doc(&f.files.cache);
        assert_eq!(
            cache["sigs"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            vec!["a"]
        );
        use base64::Engine;
        let signature = base64::engine::general_purpose::STANDARD
            .decode(cache["sigs"]["a"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            signature,
            rt::codec_image::remap_signature(&block([200, 20, 20, 255])).unwrap()
        );
        assert!(!f.files.output.exists());
        assert!(!f.files.candidates.exists());
        server.finish();
    }
    // tests.test_remap_ids.NumpyIsAnExtra.test_nothing_else_first_party_imports_numpy
    // Migration: locked actual standalone dependency graph contains neither NumPy nor a Python ABI crate.
    #[test]
    fn test_nothing_else_first_party_imports_numpy() {
        let lock = include_str!("../Cargo.lock");
        assert!(!lock.contains("name = \"numpy\""));
        assert!(!lock.contains("name = \"pyo3\""));
        assert!(!include_str!("../src/coin_remap_math.rs").contains("use pyo3"));
        let result = rt::coin_remap_math::nearest(&[vec![0.0; 768]], &[vec![1.0; 768]]).unwrap();
        assert_eq!(result.indices, vec![0]);
        assert_eq!(result.squared, vec![768.0]);
    }
}

#[path = "baseline_coin_providers_support/recovery.rs"]
mod recovery;

#[path = "baseline_coin_providers_support/logo_similarity.rs"]
mod logo_similarity;
#[path = "baseline_coin_providers_support/verify_fix.rs"]
mod verify_fix;

#[path = "baseline_coin_providers_support/verify_main.rs"]
mod verify_main;

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
