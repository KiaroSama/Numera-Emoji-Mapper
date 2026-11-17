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
mod logo_cache {
    use super::*;
    fn fetch(root: &Root, market: Value, download: Vec<u8>) -> (u8, Vec<String>) {
        let mut asked = vec![];
        let result = rt::coin_gecko_fetch::fetch_with(
            &root.0,
            1,
            Duration::ZERO,
            |url, binary, _| {
                asked.push(url.into());
                Ok(if binary {
                    rt::coin_gecko_fetch::Payload::Bytes(download.clone())
                } else {
                    rt::coin_gecko_fetch::Payload::Json(market.clone())
                })
            },
            |_| Ok(()),
        )
        .unwrap();
        (result, asked)
    }
    // tests.test_coin_logo_cache.FetchLogosCacheValidation.test_valid_image_rejects_error_bodies_and_truncation
    #[test]
    fn test_valid_image_rejects_error_bodies_and_truncation() {
        let t = Root::new();
        let good = solid(32, [20, 120, 200, 255]);
        for (name, bytes, want) in [
            ("good.png", good.clone(), true),
            ("html.png", HTML.into(), false),
            ("cut.png", good[..60].to_vec(), false),
            ("blank.png", solid(32, [0, 0, 0, 0]), false),
        ] {
            let p = t.0.join(name);
            std::fs::write(&p, bytes).unwrap();
            assert_eq!(rt::codec_image::valid_logo(&p), want, "{name}");
        }
    }
    // tests.test_coin_logo_cache.FetchLogosCacheValidation.test_bad_download_never_reaches_the_destination
    #[test]
    fn test_bad_download_never_reaches_the_destination() {
        let t = Root::new();
        let (rc, _) = fetch(
            &t,
            json!([{"symbol":"ABC","name":"","image":"http://x/abc.png"}]),
            HTML.into(),
        );
        assert_eq!(rc, 0);
        assert!(!t.0.join("logos/png/abc.png").exists());
        assert_eq!(std::fs::read_dir(t.0.join("logos/png")).unwrap().count(), 0);
    }
    // tests.test_coin_logo_cache.FetchLogosCacheValidation.test_corrupt_cached_png_is_redownloaded
    #[test]
    fn test_corrupt_cached_png_is_redownloaded() {
        let t = Root::new();
        std::fs::create_dir_all(t.0.join("logos/png")).unwrap();
        std::fs::write(t.0.join("logos/png/btc.png"), HTML).unwrap();
        let (rc, asked) = fetch(
            &t,
            json!([{"symbol":"BTC","name":"Bitcoin","image":"http://img.test/btc.png"}]),
            solid(32, [20, 120, 200, 255]),
        );
        assert_eq!(rc, 0);
        assert!(asked.contains(&"http://img.test/btc.png".into()));
        assert!(rt::codec_image::valid_logo(&t.0.join("logos/png/btc.png")));
    }
    // tests.test_coin_logo_cache.FetchLogosCacheValidation.test_good_cached_png_is_not_redownloaded
    #[test]
    fn test_good_cached_png_is_not_redownloaded() {
        let t = Root::new();
        std::fs::create_dir_all(t.0.join("logos/png")).unwrap();
        let bytes = solid(32, [20, 120, 200, 255]);
        std::fs::write(t.0.join("logos/png/btc.png"), &bytes).unwrap();
        let (_, asked) = fetch(
            &t,
            json!([{"symbol":"BTC","name":"Bitcoin","image":"http://img.test/btc.png"}]),
            bytes,
        );
        assert!(!asked.contains(&"http://img.test/btc.png".into()));
    }
    // tests.test_coin_logo_cache.FetchLogosCacheValidation.test_corrupt_leftover_png_is_not_advertised
    #[test]
    fn test_corrupt_leftover_png_is_not_advertised() {
        let t = Root::new();
        let dir = t.0.join("logos/png");
        std::fs::create_dir_all(&dir).unwrap();
        let good = solid(32, [20, 120, 200, 255]);
        std::fs::write(dir.join("junk.png"), HTML).unwrap();
        std::fs::write(dir.join("cut.png"), &good[..60]).unwrap();
        std::fs::write(dir.join("ok.png"), good).unwrap();
        assert_eq!(fetch(&t, json!([]), vec![]).0, 0);
        let rows = csv_rows(&t.0.join("keywords.csv"));
        assert!(rows.contains_key("ok"));
        assert!(!rows.contains_key("junk"));
        assert!(!rows.contains_key("cut"));
    }
}
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
mod canonical_map {
    use super::*;
    // tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_neither_writers_ids_are_lost
    #[test]
    fn test_neither_writers_ids_are_lost() {
        let f = Provider::new(2);
        let stale = doc(&f.files.mapping);
        write(
            &f.files.mapping,
            &json!({"btc":"c-btc","zzz":"written-by-the-other-tool"}),
        );
        let mut stage = f.stage(&["aaa"], &f.ours);
        let mut live = f.before.clone();
        live.push(member(3));
        let mut script = vec![
            me(),
            listing(&f.before),
            listing(&f.before),
            f.mutation(false, 2, Body::Json(ok(json!(true)))),
            listing(&live),
        ];
        download(&mut script, "f3", &f.ours);
        let server = Server::new(script);
        let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
        assert_eq!((result.0, result.1), (1, 0));
        let saved = doc(&f.files.mapping);
        assert_eq!(saved["btc"], "c-btc");
        assert_eq!(saved["zzz"], "written-by-the-other-tool");
        assert_eq!(saved["aaa"], "c3");
        assert_eq!(
            result.2["zzz"], "written-by-the-other-tool",
            "owned return map refreshes caller's canonical view"
        );
        assert!(stale.get("zzz").is_none());
        server.finish();
        f.release();
    }
    // tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_a_ticker_mapped_while_we_waited_is_not_uploaded_again
    #[test]
    fn test_a_ticker_mapped_while_we_waited_is_not_uploaded_again() {
        let f = Provider::new(2);
        let stale = doc(&f.files.mapping);
        write(
            &f.files.mapping,
            &json!({"btc":"c-btc","aaa":"c-from-the-other-provider"}),
        );
        let mut stage = f.stage(&["aaa"], &f.ours);
        let server = Server::new(vec![me()]);
        let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
        assert_eq!((result.0, result.1), (0, 0));
        assert_eq!(
            server.finish().len(),
            1,
            "already live ticker must have no mutation"
        );
        assert_eq!(f.before.len(), 2);
        assert_eq!(doc(&f.files.mapping)["aaa"], "c-from-the-other-provider");
        assert_eq!(result.2["aaa"], "c-from-the-other-provider");
        assert!(stale.get("aaa").is_none());
        f.release();
    }
    // tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_the_rest_of_the_batch_is_still_published
    #[test]
    fn test_the_rest_of_the_batch_is_still_published() {
        let f = Provider::new(2);
        write(
            &f.files.mapping,
            &json!({"btc":"c-btc","aaa":"c-from-the-other-provider"}),
        );
        let mut stage = f.stage(&["aaa", "bbb"], &f.ours);
        let mut live = f.before.clone();
        live.push(member(3));
        let mut script = vec![
            me(),
            listing(&f.before),
            listing(&f.before),
            f.mutation(false, 2, Body::Json(ok(json!(true)))),
            listing(&live),
        ];
        download(&mut script, "f3", &f.ours);
        let server = Server::new(script);
        let result = f.run(&server, &mut stage, &["aaa", "bbb"]).unwrap();
        assert_eq!((result.0, result.1), (1, 0));
        let requests = server.finish();
        let adds = requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .collect::<Vec<_>>();
        assert_eq!(
            adds.iter()
                .map(
                    |r| r.observation.as_ref().unwrap()["provider_in_flight"]["key"]
                        .as_str()
                        .unwrap()
                )
                .collect::<Vec<_>>(),
            vec!["bbb"]
        );
        assert_eq!(result.2["bbb"], "c3");
        f.release();
    }
    // tests.test_coin_providers.TheCanonicalMapIsRereadUnderTheLock.test_a_map_writer_blocks_the_publisher
    #[test]
    fn test_a_map_writer_blocks_the_publisher() {
        let f = Provider::new(2);
        let mut stage = f.stage(&["aaa"], &f.ours);
        let server = Server::new(vec![]);
        let hold = rt::locks::Lease::acquire(
            &f.root.0.join(".locks/canonical_map.lock"),
            "real map writer",
        )
        .unwrap();
        assert!(f.run(&server, &mut stage, &["aaa"]).is_err());
        assert!(server.finish().is_empty());
        drop(hold);
        drop(
            rt::locks::Lease::acquire(
                &rt::locks::pack_family_path(&f.root.0, "cryptoemoji"),
                "family released after map refusal",
            )
            .unwrap(),
        );
        f.release();
    }
}

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
mod remap_cache {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    // tests.test_remap_ids.DownloadLiveTest.test_failed_sticker_leaves_set_incomplete_and_is_retried
    #[test]
    fn test_failed_sticker_leaves_set_incomplete_and_is_retried() {
        let mut io = CacheFixture::new(&["a", "b"]);
        io.fail.insert("b".into());
        let mut cache = rt::coin_remap_cache::load(&json!({})).unwrap();
        assert_eq!(io.run(&mut cache), ids(&["s1"]));
        assert!(cache["sets"].get("s1").is_none());
        assert_eq!(
            cache["sigs"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            ids(&["a"])
        );
        assert!(cache["errors"].get("b").is_some());
        assert!(!cache.to_string().contains("TOKEN123"));
        io.fail.clear();
        io.fetched.clear();
        let mut cache = rt::coin_remap_cache::load(&doc(&io.path())).unwrap();
        assert!(io.run(&mut cache).is_empty());
        assert_eq!(io.fetched, ids(&["b"]));
        let after = rt::coin_remap_cache::load(&doc(&io.path())).unwrap();
        assert!(after["sets"].get("s1").is_some());
        assert_eq!(after["errors"], json!({}));
    }
    // tests.test_remap_ids.DownloadLiveTest.test_changed_manifest_invalidates_cache_and_prunes_stale_ids
    #[test]
    fn test_changed_manifest_invalidates_cache_and_prunes_stale_ids() {
        let mut io = CacheFixture::new(&["a", "b"]);
        let mut cache = rt::coin_remap_cache::load(&json!({})).unwrap();
        assert!(io.run(&mut cache).is_empty());
        let before = cache["sets"]["s1"].clone();
        io.members = vec![sticker("a"), sticker("c")];
        io.fetched.clear();
        assert!(io.run(&mut cache).is_empty());
        assert_eq!(io.fetched, ids(&["c"]));
        assert_ne!(cache["sets"]["s1"], before);
        let mut keys = cache["sigs"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(keys, ids(&["a", "c"]));
    }
    // tests.test_remap_ids.DownloadLiveTest.test_appended_sticker_is_picked_up
    #[test]
    fn test_appended_sticker_is_picked_up() {
        let mut io = CacheFixture::new(&["a"]);
        let mut cache = rt::coin_remap_cache::load(&json!({})).unwrap();
        io.run(&mut cache);
        io.members.push(sticker("b"));
        io.fetched.clear();
        io.run(&mut cache);
        assert_eq!(io.fetched, ids(&["b"]));
        let mut keys = cache["sigs"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(keys, ids(&["a", "b"]));
    }
    // tests.test_remap_ids.DownloadLiveTest.test_legacy_cache_keeps_signatures_but_re_verifies_sets
    #[test]
    fn test_legacy_cache_keeps_signatures_but_re_verifies_sets() {
        let t = Root::new();
        let path = t.0.join("cache.json");
        let sig = STANDARD.encode(vec![0; 1024]);
        write(&path, &json!({"done_sets":[1],"sigs":{"a":sig}}));
        let cache = rt::coin_remap_cache::load(&doc(&path)).unwrap();
        assert_eq!(cache["sets"], json!({}));
        assert_eq!(
            cache["sigs"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["a"]
        );
    }
    // tests.test_remap_ids.ACacheFromAnOlderSignatureIsDiscarded.test_signatures_of_the_wrong_length_are_dropped
    #[test]
    fn test_signatures_of_the_wrong_length_are_dropped() {
        let t = Root::new();
        let path = t.0.join("cache.json");
        write(
            &path,
            &json!({"sets":{"s1":"digest"},"sigs":{"old":STANDARD.encode(vec![0;512]),"new":STANDARD.encode(vec![0;1024])},"errors":{}}),
        );
        let cache = rt::coin_remap_cache::load(&doc(&path)).unwrap();
        assert!(
            cache["sigs"].get("old").is_none(),
            "stale-format signature survived"
        );
        assert!(cache["sigs"].get("new").is_some());
        assert_eq!(
            cache["sets"],
            json!({}),
            "completeness marks must be cleared"
        );
    }
}
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
mod remap_apply {
    use super::*;
    // tests.test_remap_ids.MainApplyTest.test_apply_refuses_ambiguous_matches
    #[test]
    fn test_apply_refuses_ambiguous_matches() {
        let f = Remap::new();
        f.local("btc", &block([200, 20, 20, 255]));
        let blobs = BTreeMap::from([
            ("a".into(), block([200, 20, 20, 255])),
            ("b".into(), block([201, 20, 20, 255])),
        ]);
        let server = Server::new(remap_script(&["a", "b"], &blobs, &[]));
        assert_eq!(f.run(&server, Some(100.0), true).unwrap(), 3);
        assert!(!f.files.output.exists());
        assert_eq!(doc(&f.files.candidates), json!({}));
        server.finish();
    }
    // tests.test_remap_ids.MainApplyTest.test_apply_writes_map_when_complete_and_unambiguous
    #[test]
    fn test_apply_writes_map_when_complete_and_unambiguous() {
        let f = Remap::new();
        write(&f.files.output, &json!({"btc":"STALE"}));
        f.local("btc", &block([200, 20, 20, 255]));
        f.local("eth", &block([20, 200, 20, 255]));
        let blobs = BTreeMap::from([
            ("a".into(), block([200, 20, 20, 255])),
            ("b".into(), block([20, 200, 20, 255])),
        ]);
        let server = Server::new(remap_script(&["a", "b"], &blobs, &[]));
        assert_eq!(f.run(&server, Some(100.0), true).unwrap(), 0);
        assert_eq!(doc(&f.files.output), json!({"btc":"a","eth":"b"}));
        assert_eq!(
            doc(&f.files.output.with_extension("prebroken.json")),
            json!({"btc":"STALE"})
        );
        server.finish();
    }
    // tests.test_remap_ids.MainApplyTest.test_far_match_is_omitted_not_mapped
    #[test]
    fn test_far_match_is_omitted_not_mapped() {
        let f = Remap::new();
        f.local("btc", &block([200, 20, 20, 255]));
        f.local("ghost", &block([20, 20, 200, 255]));
        let server = Server::new(remap_script(
            &["a"],
            &BTreeMap::from([("a".into(), block([200, 20, 20, 255]))]),
            &[],
        ));
        assert_eq!(f.run(&server, Some(50.0), true).unwrap(), 0);
        assert_eq!(doc(&f.files.output), json!({"btc":"a"}));
        server.finish();
    }
    // tests.test_remap_ids.MainApplyTest.test_apply_without_max_distance_is_a_usage_error
    #[test]
    fn test_apply_without_max_distance_is_a_usage_error() {
        let f = Remap::new();
        f.local("btc", &block([200, 20, 20, 255]));
        let server = Server::new(vec![]);
        let result = command(
            &f.root,
            "http://127.0.0.1:9",
            "http://127.0.0.1:9",
            &ids(&[
                "coin-remap-ids",
                "--emoji-dir",
                f.files.images.to_str().unwrap(),
                "--apply",
            ]),
            &[],
        );
        assert_eq!(result.code, 2);
        assert!(result.stderr.contains("--max-distance"));
        assert!(!f.files.output.exists());
        assert!(server.finish().is_empty());
    }
    // tests.test_remap_ids.MainApplyTest.test_no_local_logos_fails_instead_of_crashing
    #[test]
    fn test_no_local_logos_fails_instead_of_crashing() {
        let f = Remap::new();
        let server = Server::new(remap_script(
            &["a"],
            &BTreeMap::from([("a".into(), block([200, 20, 20, 255]))]),
            &[],
        ));
        assert_eq!(f.run(&server, None, false).unwrap(), 4);
        server.finish();
    }
}
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

mod recovery {
    use super::*;
    fn fixture() -> Provider {
        let f = Provider::new(2);
        write(&f.files.mapping, &json!({}));
        f
    }
    fn live(f: &Provider) -> Vec<Value> {
        let mut v = f.before.clone();
        v.push(member(3));
        v
    }
    fn settled(f: &Provider, result: &(usize, usize, Map<String, Value>)) {
        assert_eq!((result.0, result.1), (1, 0));
        assert_eq!(result.2["aaa"], "c3");
        assert_eq!(doc(&f.files.mapping)["aaa"], "c3");
        assert!(doc(&f.files.state)["provider_in_flight"].is_null());
    }
    // tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_the_unverified_upload_is_recorded_and_not_repeated
    #[test]
    fn test_the_unverified_upload_is_recorded_and_not_repeated() {
        let f = fixture();
        let (intent, first) = begin_unresolved(&f, &["aaa"]);
        let mut next = f.stage(&["aaa"], &f.ours);
        let (result, last) = resume(&f, &mut next, &live(&f), &f.ours);
        settled(&f, &result);
        assert_eq!(
            first
                .iter()
                .chain(&last)
                .filter(|r| r.method == "addStickerToSet")
                .count(),
            1
        );
        assert_eq!(intent["key"], "aaa");
        assert_eq!(intent["operation"], "add");
        assert_eq!(intent["set_name"], SET);
        assert_eq!(intent["expected_before"], 2);
    }
    // tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_a_re_downloaded_source_does_not_unmake_the_recovery
    #[test]
    fn test_a_re_downloaded_source_does_not_unmake_the_recovery() {
        let f = fixture();
        let (_, first) = begin_unresolved(&f, &["aaa"]);
        std::fs::write(f.files.published.join("aaa.png"), &f.foreign).unwrap();
        let mut next = f.stage(&["aaa"], &f.foreign);
        let (result, last) = resume(&f, &mut next, &live(&f), &f.ours);
        settled(&f, &result);
        assert_eq!(
            first
                .iter()
                .chain(&last)
                .filter(|r| r.method == "addStickerToSet")
                .count(),
            1
        );
    }
    // tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_a_skipped_ticker_does_not_replace_the_published_logo
    #[test]
    fn test_a_skipped_ticker_does_not_replace_the_published_logo() {
        let f = fixture();
        let published = std::fs::read(f.files.published.join("aaa.png")).unwrap();
        let mut stage = f.stage(&["aaa"], &f.foreign);
        let source = stage.source("aaa").unwrap();
        write(&f.files.mapping, &json!({"aaa":"c-the-other-tools"}));
        let server = Server::new(vec![me()]);
        let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
        server.finish();
        assert_eq!((result.0, result.1), (0, 0));
        assert_eq!(
            std::fs::read(f.files.published.join("aaa.png")).unwrap(),
            published
        );
        assert!(!source.exists());
        write(&f.files.mapping, &json!({}));
        let mut stage = f.stage(&["aaa"], &f.foreign);
        let source = stage.source("aaa").unwrap();
        let mut script = vec![
            me(),
            listing(&f.before),
            listing(&f.before),
            f.mutation(false, 2, Body::Json(ok(json!(true)))),
            listing(&live(&f)),
        ];
        download(&mut script, "f3", &f.foreign);
        let server = Server::new(script);
        let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
        settled(&f, &result);
        server.finish();
        assert_ne!(
            std::fs::read(f.files.published.join("aaa.png")).unwrap(),
            published
        );
        assert!(!source.exists());
        f.release();
    }
    // tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_another_process_cannot_swap_the_image_mid_upload
    #[test]
    fn test_another_process_cannot_swap_the_image_mid_upload() {
        let f = fixture();
        let mut stage = f.stage(&["aaa"], &f.ours);
        let source = stage.source("aaa").unwrap();
        let mut mutation = f.mutation(false, 2, Body::Json(ok(json!(true))));
        let published = f.files.published.join("aaa.png");
        let incoming = f
            .files
            .published
            .parent()
            .unwrap()
            .join(".incoming/aaa.png");
        let foreign = f.foreign.clone();
        let ours = f.ours.clone();
        mutation.action = Some(Box::new(move |r| {
            assert_eq!(r.files["file0"], ours);
            std::fs::write(&incoming, &foreign).unwrap();
            std::fs::write(&published, &foreign).unwrap();
        }));
        let mut script = vec![
            me(),
            listing(&f.before),
            listing(&f.before),
            mutation,
            listing(&live(&f)),
        ];
        download(&mut script, "f3", &f.ours);
        let server = Server::new(script);
        let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
        settled(&f, &result);
        let requests = server.finish();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "addStickerToSet")
                .count(),
            1
        );
        assert_eq!(
            rt::codec_image::source_dhash(&f.files.published.join("aaa.png")).unwrap(),
            rt::codec_image::provider_dhash(&f.ours).unwrap()
        );
        assert!(!source.exists());
        f.release();
    }
    // tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_an_unresolved_upload_keeps_its_source_for_the_next_run
    #[test]
    fn test_an_unresolved_upload_keeps_its_source_for_the_next_run() {
        let f = fixture();
        let before = std::fs::read(f.files.published.join("aaa.png")).unwrap();
        let (intent, first) = begin_unresolved(&f, &["aaa"]);
        let retained = std::path::PathBuf::from(intent["source_path"].as_str().unwrap());
        assert!(retained.is_file());
        assert_eq!(
            std::fs::read(f.files.published.join("aaa.png")).unwrap(),
            before
        );
        let mut next = f.stage(&["aaa"], &f.foreign);
        assert_ne!(next.source("aaa").unwrap(), retained);
        let (result, last) = resume(&f, &mut next, &live(&f), &f.ours);
        settled(&f, &result);
        assert_eq!(
            first
                .iter()
                .chain(&last)
                .filter(|r| r.method == "addStickerToSet")
                .count(),
            1
        );
        assert_eq!(
            rt::codec_image::source_dhash(&f.files.published.join("aaa.png")).unwrap(),
            rt::codec_image::provider_dhash(&f.ours).unwrap()
        );
    }
    // tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_recovery_never_promotes_an_image_it_cannot_prove
    #[test]
    fn test_recovery_never_promotes_an_image_it_cannot_prove() {
        let f = fixture();
        let before = std::fs::read(f.files.published.join("aaa.png")).unwrap();
        let (intent, _) = begin_unresolved(&f, &["aaa"]);
        std::fs::remove_file(intent["source_path"].as_str().unwrap()).unwrap();
        let mut next = f.stage(&["aaa"], &f.foreign);
        let log = f.root.log();
        let mut script = vec![me(), listing(&live(&f))];
        download(&mut script, "f3", &f.ours);
        let server = Server::new(script);
        let result = rt::coin_provider_effects::run(
            &f.files,
            &server.telegram().with_log(&log),
            &mut next,
            &ids(&["aaa"]),
            "T",
            1,
            &Map::new(),
        )
        .unwrap();
        server.finish();
        settled(&f, &result);
        assert_eq!(
            std::fs::read(f.files.published.join("aaa.png")).unwrap(),
            before
        );
        assert!(
            std::fs::read_to_string(log.path.unwrap())
                .unwrap()
                .contains("was NOT updated")
        );
        f.release();
    }
    // tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_an_upload_that_never_landed_is_retried_once
    #[test]
    fn test_an_upload_that_never_landed_is_retried_once() {
        let f = fixture();
        let (_, first) = begin_unresolved(&f, &["aaa"]);
        let mut stage = f.stage(&["aaa"], &f.ours);
        let mut script = vec![
            me(),
            listing(&f.before),
            listing(&f.before),
            listing(&f.before),
            f.mutation(false, 2, Body::Json(ok(json!(true)))),
            listing(&live(&f)),
        ];
        download(&mut script, "f3", &f.ours);
        let server = Server::new(script);
        let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
        let last = server.finish();
        settled(&f, &result);
        assert_eq!(
            first
                .iter()
                .chain(&last)
                .filter(|r| r.method == "addStickerToSet")
                .count(),
            2
        );
        f.release();
    }
    // tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_nothing_else_is_mutated_while_an_outcome_is_unresolved
    #[test]
    fn test_nothing_else_is_mutated_while_an_outcome_is_unresolved() {
        let f = fixture();
        let mut stage = f.stage(&["aaa", "bbb", "ccc"], &f.ours);
        let mut live = live(&f);
        live.push(member(4));
        let mut script = vec![
            me(),
            listing(&f.before),
            listing(&f.before),
            f.mutation(false, 2, Body::Json(ok(json!(true)))),
            listing(&live),
        ];
        download(&mut script, "f3", &f.ours);
        download(&mut script, "f4", &f.ours);
        let server = Server::new(script);
        let result = f.run(&server, &mut stage, &["aaa", "bbb", "ccc"]).unwrap();
        assert_eq!((result.0, result.1), (0, 3));
        let requests = server.finish();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "addStickerToSet")
                .count(),
            1
        );
        assert_eq!(doc(&f.files.state)["provider_in_flight"]["key"], "aaa");
        f.release();
    }
    // tests.test_coin_recovery.UnverifiedUploadIsRecovered.test_a_definitive_rejection_still_clears_the_intent
    #[test]
    fn test_a_definitive_rejection_still_clears_the_intent() {
        let f = fixture();
        let mut stage = f.stage(&["aaa"], &f.ours);
        let server = Server::new(vec![
            me(),
            listing(&f.before),
            listing(&f.before),
            f.mutation(
                false,
                2,
                Body::Json(reject("addStickerToSet failed: STICKER_PNG_NOPNG")),
            ),
        ]);
        let result = f.run(&server, &mut stage, &["aaa"]).unwrap();
        let requests = server.finish();
        assert_eq!((result.0, result.1), (0, 1));
        assert!(doc(&f.files.state)["provider_in_flight"].is_null());
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "addStickerToSet")
                .count(),
            1
        );
    }
}

mod logo_review_distance {
    use super::*;
    // tests.test_verify_logos.LogoReviewDistance.test_the_same_mark_inverted_is_not_reported_as_different
    #[test]
    fn test_the_same_mark_inverted_is_not_reported_as_different() {
        let theirs = noise("some-coin");
        let ours = noise("some-coin-inverted");
        let plain = (rt::codec_image::provider_dhash(&ours).unwrap()
            ^ rt::codec_image::provider_dhash(&theirs).unwrap())
        .count_ones();
        assert!(plain > 20, "fixture must be inversion-sensitive");
        assert!(rt::codec_image::logo_distance(&ours, &theirs).unwrap() <= 8);
    }
    // tests.test_verify_logos.LogoReviewDistance.test_a_genuinely_different_image_is_still_far
    #[test]
    fn test_a_genuinely_different_image_is_still_far() {
        assert!(
            rt::codec_image::logo_distance(&noise("solana"), &noise("solama-the-llama")).unwrap()
                > 20
        );
    }
}
mod verify_fix {
    use super::*;
    fn perform(f: &Verify, kind: &str) -> (bool, Vec<Request>) {
        let mut script = vec![
            listing(&f.before()),
            f.mutation(if kind == "lost-reply" {
                Body::Disconnect
            } else if kind == "rejected" {
                Body::Json(reject("replaceStickerInSet failed: STICKER_INVALID"))
            } else {
                Body::Json(ok(json!(true)))
            }),
        ];
        if kind == "lost-reply" {
            script.push(listing(&f.after()));
        }
        if kind != "rejected" {
            script.push(if kind == "unchanged" {
                listing(&f.before())
            } else if kind == "shifted" {
                listing(&[sticker(NEW), sticker("c")])
            } else {
                listing(&f.after())
            });
            if !matches!(kind, "unchanged" | "shifted") {
                if kind == "foreign" {
                    download(&mut script, &format!("fid-{NEW}"), &noise("stranger"));
                } else {
                    uploaded_download(&mut script, &format!("fid-{NEW}"));
                }
            }
        }
        let server = Server::new(script);
        let result = f.run(&server).unwrap();
        (result, server.finish())
    }
    // tests.test_verify_logos.VerifyLogosFix.test_timeout_after_apply_is_verified_not_resent
    #[test]
    fn test_timeout_after_apply_is_verified_not_resent() {
        let f = Verify::new();
        let (ok, r) = perform(&f, "lost-reply");
        assert!(ok);
        assert_eq!(
            r.iter()
                .filter(|r| r.method == "replaceStickerInSet")
                .count(),
            1
        );
        assert_eq!(doc(&f.files.map)["btc"], NEW);
    }
    // tests.test_verify_logos.VerifyLogosFix.test_the_uploaded_body_is_bytes_a_retry_can_resend
    #[test]
    fn test_the_uploaded_body_is_bytes_a_retry_can_resend() {
        let f = Verify::new();
        let (_, r) = perform(&f, "lost-reply");
        let body = &r
            .iter()
            .find(|r| r.method == "replaceStickerInSet")
            .unwrap()
            .files["file0"];
        assert!(body.starts_with(b"\x89PNG"));
        assert!(image::load_from_memory(body).is_ok());
    }
    // tests.test_verify_logos.VerifyLogosFix.test_every_entry_sharing_the_old_cid_is_repointed
    #[test]
    fn test_every_entry_sharing_the_old_cid_is_repointed() {
        let f = Verify::new();
        let (ok, r) = perform(&f, "normal");
        assert!(ok);
        assert_eq!(
            doc(&f.files.map),
            json!({"btc":NEW,"wbtc":NEW,"eth":"cid-eth"})
        );
        assert!(!f.files.intent.exists());
        let body = &r
            .iter()
            .find(|r| r.method == "replaceStickerInSet")
            .unwrap()
            .files["file0"];
        assert_eq!(
            &std::fs::read(f.files.images.join("btc.png")).unwrap(),
            body
        );
        assert_eq!(
            std::fs::read(f.root.0.join("png/btc.png")).unwrap(),
            noise("official-btc")
        );
        assert_eq!(
            std::fs::read_dir(f.root.0.join(".incoming"))
                .unwrap()
                .count(),
            0
        );
    }
    // tests.test_verify_logos.VerifyLogosFix.test_a_definite_failure_leaves_the_oracle_untouched
    #[test]
    fn test_a_definite_failure_leaves_the_oracle_untouched() {
        let f = Verify::new();
        assert!(!perform(&f, "rejected").0);
        f.unchanged();
        assert!(!f.files.intent.exists());
    }
    // tests.test_verify_logos.VerifyLogosFix.test_a_shifted_set_is_not_trusted_as_the_replacement
    #[test]
    fn test_a_shifted_set_is_not_trusted_as_the_replacement() {
        let f = Verify::new();
        let before = doc(&f.files.map);
        assert!(!perform(&f, "shifted").0);
        assert_eq!(doc(&f.files.map), before);
        f.unchanged();
    }
    // tests.test_verify_logos.VerifyLogosFix.test_an_unchanged_set_is_not_trusted_either
    #[test]
    fn test_an_unchanged_set_is_not_trusted_either() {
        let f = Verify::new();
        let before = doc(&f.files.map);
        assert!(!perform(&f, "unchanged").0);
        assert_eq!(doc(&f.files.map), before);
        f.unchanged();
        assert!(!f.files.intent.exists());
    }
    // tests.test_verify_logos.VerifyLogosFix.test_a_replacement_whose_image_is_not_ours_is_refused
    #[test]
    fn test_a_replacement_whose_image_is_not_ours_is_refused() {
        let f = Verify::new();
        let before = doc(&f.files.map);
        assert!(!perform(&f, "foreign").0);
        assert_eq!(doc(&f.files.map), before);
        f.unchanged();
    }
    // tests.test_verify_logos.VerifyLogosFix.test_a_lost_confirmation_read_leaves_a_recoverable_intent
    #[test]
    fn test_a_lost_confirmation_read_leaves_a_recoverable_intent() {
        let f = Verify::new();
        let before = doc(&f.files.map);
        f.pending();
        assert_eq!(doc(&f.files.map), before);
        let intent = doc(&f.files.intent);
        assert_eq!(intent["operation"], "replace");
        assert_eq!(intent["key"], "btc");
        assert_eq!(intent["old_cid"], OLD);
        assert_eq!(intent["set_name"], SET);
        assert_eq!(intent["set_index"], 1);
        assert_eq!(intent["before"], json!(["a", OLD, "c"]));
    }
    // tests.test_verify_logos.VerifyLogosFix.test_the_next_run_recovers_the_replacement_from_the_intent
    #[test]
    fn test_the_next_run_recovers_the_replacement_from_the_intent() {
        let f = Verify::new();
        let uploaded = f.pending();
        f.unchanged();
        let stage = std::path::PathBuf::from(doc(&f.files.intent)["stage"].as_str().unwrap());
        assert!(stage.is_dir());
        let r = f.recovered(&uploaded);
        assert!(r.iter().all(|r| r.method != "replaceStickerInSet"));
        assert!(!stage.exists());
    }
    // tests.test_verify_logos.VerifyLogosFix.test_recovery_refuses_when_the_live_image_is_not_ours
    #[test]
    fn test_recovery_refuses_when_the_live_image_is_not_ours() {
        let f = Verify::new();
        f.pending();
        let before = doc(&f.files.map);
        let mut script = vec![listing(&f.after())];
        download(&mut script, &format!("fid-{NEW}"), &noise("stranger"));
        let server = Server::new(script);
        assert!(!rt::coin_replace_disk::run(&f.files, &server.telegram()).unwrap());
        server.finish();
        assert_eq!(doc(&f.files.map), before);
        assert!(f.files.intent.exists());
        f.unchanged();
    }
    // tests.test_verify_logos.VerifyLogosFix.test_recovery_is_idempotent_after_the_map_was_already_repointed
    #[test]
    fn test_recovery_is_idempotent_after_the_map_was_already_repointed() {
        let f = Verify::new();
        let bytes = f.pending();
        write(&f.files.map, &json!({"btc":NEW,"wbtc":NEW,"eth":"cid-eth"}));
        f.recovered(&bytes);
    }
    // tests.test_verify_logos.VerifyLogosFix.test_an_unreadable_set_leaves_the_intent_for_the_run_after
    #[test]
    fn test_an_unreadable_set_leaves_the_intent_for_the_run_after() {
        let f = Verify::new();
        f.pending();
        let server = Server::new(vec![unavailable()]);
        assert!(!rt::coin_replace_disk::run(&f.files, &server.telegram()).unwrap());
        server.finish();
        assert!(f.files.intent.exists());
        f.unchanged();
    }
    // tests.test_verify_logos.VerifyLogosFix.test_an_intent_cannot_be_reconciled_into_a_different_map
    #[test]
    fn test_an_intent_cannot_be_reconciled_into_a_different_map() {
        let mut f = Verify::new();
        let bytes = f.pending();
        let original = f.files.map.clone();
        let other = f.root.0.join("other_map.json");
        write(&other, &json!({"btc":OLD}));
        f.files.map = other.clone();
        let server = Server::new(vec![]);
        assert!(!rt::coin_replace_disk::run(&f.files, &server.telegram()).unwrap());
        server.finish();
        assert_eq!(doc(&other), json!({"btc":OLD}));
        assert!(f.files.intent.exists());
        f.files.map = original;
        f.recovered(&bytes);
    }
    // tests.test_verify_logos.VerifyLogosFix.test_an_intent_is_bound_to_its_state_file_too
    #[test]
    fn test_an_intent_is_bound_to_its_state_file_too() {
        let mut f = Verify::new();
        let state_a = f.root.0.join("state_a.json");
        let state_b = f.root.0.join("state_b.json");
        write(&state_a, &doc(&f.files.state));
        write(&state_b, &doc(&f.files.state));
        f.files.state = state_a;
        let bytes = f.pending();
        let original = f.files.state.clone();
        let before = doc(&f.files.map);
        f.files.state = state_b;
        let server = Server::new(vec![]);
        assert!(!rt::coin_replace_disk::run(&f.files, &server.telegram()).unwrap());
        server.finish();
        assert_eq!(doc(&f.files.map), before);
        f.files.state = original;
        f.recovered(&bytes);
    }
    // tests.test_verify_logos.VerifyLogosFix.test_an_intent_with_no_recorded_target_is_refused
    #[test]
    fn test_an_intent_with_no_recorded_target_is_refused() {
        let f = Verify::new();
        f.pending();
        let mut legacy = doc(&f.files.intent);
        legacy.as_object_mut().unwrap().remove("map_target");
        legacy.as_object_mut().unwrap().remove("state_target");
        write(&f.files.intent, &legacy);
        let before = doc(&f.files.map);
        let server = Server::new(vec![]);
        assert!(!rt::coin_replace_disk::run(&f.files, &server.telegram()).unwrap());
        server.finish();
        assert_eq!(doc(&f.files.map), before);
        assert!(f.files.intent.exists());
    }
    // tests.test_verify_logos.VerifyLogosFix.test_a_busy_map_lock_stops_the_fix_before_it_touches_the_pack
    #[test]
    fn test_a_busy_map_lock_stops_the_fix_before_it_touches_the_pack() {
        let f = Verify::new();
        let before = doc(&f.files.map);
        let _map = rt::locks::Lease::acquire(
            &f.root.0.join(".locks/canonical_map.lock"),
            "actual foreign map writer",
        )
        .unwrap();
        let server = Server::new(vec![]);
        assert!(f.run(&server).is_err());
        assert!(server.finish().is_empty());
        assert_eq!(doc(&f.files.map), before);
        assert!(!f.files.intent.exists());
        f.unchanged();
    }
    // tests.test_verify_logos.VerifyLogosFix.test_fix_locks_on_the_pack_family_not_on_this_script
    #[test]
    fn test_fix_locks_on_the_pack_family_not_on_this_script() {
        let f = Verify::new();
        assert_eq!(f.files.base, "cryptoemoji");
        let family = rt::locks::pack_family_path(&f.root.0, "cryptoemoji");
        assert_eq!(
            family,
            rt::locks::pack_family_path(&f.root.0, &f.files.base)
        );
        let _hold = rt::locks::Lease::acquire(&family, "actual provider family holder").unwrap();
        let server = Server::new(vec![]);
        assert!(f.run(&server).is_err());
        assert!(server.finish().is_empty());
        assert!(!f.files.intent.exists());
    }
    // tests.test_verify_logos.VerifyLogosFix.test_verified_new_cid_rules
    #[test]
    fn test_verified_new_cid_rules() {
        let before = ids(&["a", OLD]);
        assert_eq!(
            rt::coin_replace_proof::new_id(&before, &ids(&["a", NEW]), 1),
            Some(NEW.into())
        );
        for after in [vec![NEW], vec!["z", NEW], vec!["a", "a"], vec!["a", OLD]] {
            assert_eq!(
                rt::coin_replace_proof::new_id(&before, &ids(&after), 1),
                None,
                "{after:?}"
            );
        }
    }
}

mod verify_main {
    use super::*;
    fn fixture() -> Verify {
        let f = Verify::new();
        write(&f.files.map, &json!({"sol":OLD}));
        f
    }
    const MARKETS: &str = "/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page=1&sparkline=false";
    fn invoke(
        f: &Verify,
        server: &Server,
        values: &[(&str, &str)],
        only: &str,
    ) -> rt::tooling_process::Completed {
        command(&f.root, &server.base, &server.base, &f.args(only), values)
    }
    // tests.test_verify_logos.VerifyLogosMainContracts.test_a_requested_fix_that_failed_exits_nonzero
    #[test]
    fn test_a_requested_fix_that_failed_exits_nonzero() {
        let f = fixture();
        let server = Server::new(vec![json_reply(MARKETS, 200, json!([]))]);
        let result = invoke(
            &f,
            &server,
            &[("TELEGRAM_BOT_TOKEN", TOKEN), ("PACK_OWNER_USER_ID", "7")],
            "sol",
        );
        assert_eq!(result.code, 4, "{}{}", result.stdout, result.stderr);
        assert_eq!(server.finish().len(), 1);
        f.unchanged();
    }
    // tests.test_verify_logos.VerifyLogosMainContracts.test_missing_owner_id_is_a_usage_error_not_a_traceback
    #[test]
    fn test_missing_owner_id_is_a_usage_error_not_a_traceback() {
        let f = fixture();
        let server = Server::new(vec![]);
        let result = invoke(&f, &server, &[("TELEGRAM_BOT_TOKEN", TOKEN)], "sol");
        assert_eq!(result.code, 2);
        assert!(!result.stderr.contains("panicked"));
        assert!(server.finish().is_empty());
    }
    // tests.test_verify_logos.VerifyLogosMainContracts.test_non_numeric_owner_id_is_a_usage_error
    #[test]
    fn test_non_numeric_owner_id_is_a_usage_error() {
        let f = fixture();
        let server = Server::new(vec![]);
        let result = invoke(
            &f,
            &server,
            &[
                ("TELEGRAM_BOT_TOKEN", TOKEN),
                ("PACK_OWNER_USER_ID", "not-a-number"),
            ],
            "sol",
        );
        assert_eq!(result.code, 2);
        assert!(server.finish().is_empty());
    }
    // tests.test_verify_logos.VerifyLogosMainContracts.test_missing_token_is_a_usage_error
    #[test]
    fn test_missing_token_is_a_usage_error() {
        let f = fixture();
        let server = Server::new(vec![]);
        assert_eq!(
            invoke(&f, &server, &[("PACK_OWNER_USER_ID", "7")], "sol").code,
            2
        );
        assert!(server.finish().is_empty());
    }
    // tests.test_verify_logos.VerifyLogosMainContracts.test_unreadable_state_file_is_a_usage_error
    #[test]
    fn test_unreadable_state_file_is_a_usage_error() {
        let f = fixture();
        std::fs::write(&f.files.state, b"{not json").unwrap();
        let server = Server::new(vec![]);
        assert_eq!(
            invoke(
                &f,
                &server,
                &[("TELEGRAM_BOT_TOKEN", TOKEN), ("PACK_OWNER_USER_ID", "7")],
                "sol"
            )
            .code,
            2
        );
        assert!(server.finish().is_empty());
    }
    // tests.test_verify_logos.VerifyLogosMainContracts.test_an_unresolved_intent_blocks_every_replacement
    #[test]
    fn test_an_unresolved_intent_blocks_every_replacement() {
        let f = Verify::new();
        f.pending();
        let server = Server::new(vec![unavailable()]);
        let result = invoke(
            &f,
            &server,
            &[("TELEGRAM_BOT_TOKEN", TOKEN), ("PACK_OWNER_USER_ID", "7")],
            "sol",
        );
        assert_eq!(result.code, 4, "{}{}", result.stdout, result.stderr);
        let requests = server.finish();
        assert_eq!(requests.len(), 1);
        assert!(
            requests
                .iter()
                .all(|r| r.method != "replaceStickerInSet" && !r.method.starts_with("GET /api"))
        );
        assert!(f.files.intent.exists());
        f.unchanged();
    }
    // tests.test_verify_logos.VerifyLogosMainContracts.test_a_clear_intent_lets_the_run_proceed
    #[test]
    fn test_a_clear_intent_lets_the_run_proceed() {
        let f = fixture();
        let mut script = vec![
            json_reply(
                MARKETS,
                200,
                json!([{"symbol":"sol","name":"Solana","image":"https://fixture.invalid/btc.png"}]),
            ),
            provider_reply("/btc.png", 200, &noise("official-btc")),
            listing(&f.before()),
            reply("replaceStickerInSet", ok(json!(true))),
            listing(&f.after()),
        ];
        uploaded_download(&mut script, &format!("fid-{NEW}"));
        let server = Server::new(script);
        let result = invoke(
            &f,
            &server,
            &[("TELEGRAM_BOT_TOKEN", TOKEN), ("PACK_OWNER_USER_ID", "7")],
            "sol",
        );
        assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
        let requests = server.finish();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "replaceStickerInSet")
                .count(),
            1
        );
        assert_eq!(doc(&f.files.map)["sol"], NEW);
        assert!(!f.files.intent.exists());
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
