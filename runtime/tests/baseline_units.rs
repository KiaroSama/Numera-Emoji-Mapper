//! One-to-one domain-unit ports of original Python methods. No interpreter or live APIs.
//! Original IDs and target metadata are recorded in tests/fixtures/migration/reference-cases.json.
#[path = "baseline_units_support/mod.rs"]
mod support;
use numera_runtime as rt;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use support::*;

mod operator_config {
    use super::*;
    // tests.test_operator_config.OperatorConfigTest.test_an_unset_key_is_named_in_the_stop
    #[test]
    fn test_an_unset_key_is_named_in_the_stop() {
        let t = Scratch::new();
        let c = config(&t.0, &[("COIN_PACK_TITLE", "t")]);
        let e = c.require("COIN_PACK_BASE").unwrap_err();
        assert!(e.contains("COIN_PACK_BASE"));
        assert!(!e.contains("COIN_PACK_TITLE"));
    }
    // tests.test_operator_config.OperatorConfigTest.test_unset_logo_bots_stops_but_empty_means_none
    #[test]
    fn test_unset_logo_bots_stops_but_empty_means_none() {
        let t = Scratch::new();
        let c = config(&t.0, &[]);
        assert!(
            c.brand_logo(false, None)
                .unwrap_err()
                .contains("BRAND_LOGO_BOTS")
        );
        assert!(c.logo_bots(false).unwrap().is_empty());
        assert_eq!(
            config(&t.0, &[("BRAND_LOGO_BOTS", "")])
                .brand_logo(false, None)
                .unwrap(),
            (vec![], None)
        );
    }
    // tests.test_operator_config.OperatorConfigTest.test_bot_list_ignores_case_and_at
    #[test]
    fn test_bot_list_ignores_case_and_at() {
        let t = Scratch::new();
        let c = config(&t.0, &[("BRAND_LOGO_BOTS", "@YourEmojiBot, other_bot")]);
        assert_eq!(
            c.logo_bots(true).unwrap(),
            vec!["other_bot", "youremojibot"]
        );
    }
    // tests.test_operator_config.OperatorConfigTest.test_a_listed_bot_needs_an_existing_logo
    #[test]
    fn test_a_listed_bot_needs_an_existing_logo() {
        let t = Scratch::new();
        let c = config(&t.0, &[("BRAND_LOGO_BOTS", "YourEmojiBot")]);
        assert!(
            c.brand_logo(false, None)
                .unwrap_err()
                .contains("BRAND_LOGO_PATH")
        );
        assert!(
            config(
                &t.0,
                &[
                    ("BRAND_LOGO_BOTS", "YourEmojiBot"),
                    ("BRAND_LOGO_PATH", "private/does-not-exist.png")
                ]
            )
            .brand_logo(false, None)
            .is_err()
        );
    }
    // tests.test_operator_config.OperatorConfigTest.test_a_configured_logo_is_used
    #[test]
    fn test_a_configured_logo_is_used() {
        let t = Scratch::new();
        let logo = file(&t.join("logo.png"), b"png");
        let c = config(
            &t.0,
            &[
                ("BRAND_LOGO_BOTS", "YourEmojiBot"),
                ("BRAND_LOGO_PATH", logo.to_str().unwrap()),
            ],
        );
        assert_eq!(
            c.brand_logo(false, None).unwrap(),
            (vec!["youremojibot".into()], Some(logo))
        );
    }
    // tests.test_operator_config.OperatorConfigTest.test_a_relative_logo_path_is_under_the_project_root
    #[test]
    fn test_a_relative_logo_path_is_under_the_project_root() {
        let t = Scratch::new();
        let c = config(&t.0, &[("BRAND_LOGO_PATH", "private/brand-logo.png")]);
        assert_eq!(
            c.logo_path(false).unwrap(),
            Some(t.join("private/brand-logo.png"))
        );
    }
    // tests.test_operator_config.OperatorConfigTest.test_disabled_needs_no_configuration
    #[test]
    fn test_disabled_needs_no_configuration() {
        let t = Scratch::new();
        assert_eq!(
            config(&t.0, &[]).brand_logo(true, None).unwrap(),
            (vec![], None)
        );
    }
    // tests.test_operator_config.OperatorConfigTest.test_keywords_default_to_logo
    #[test]
    fn test_keywords_default_to_logo() {
        let t = Scratch::new();
        assert_eq!(config(&t.0, &[]).logo_keywords(), vec!["logo"]);
        assert_eq!(
            config(&t.0, &[("BRAND_LOGO_KEYWORDS", "yourbrand, logo")]).logo_keywords(),
            vec!["yourbrand", "logo"]
        );
    }
}
mod media_paths {
    use super::*;
    // tests.test_media_paths.MediaPathTests.test_inside_the_data_folder_is_stored_relative
    #[test]
    fn test_inside_the_data_folder_is_stored_relative() {
        let t = Scratch::new();
        let data = t.join("collection");
        let inside = file(&data.join("media/static/a.webp"), b"x");
        {
            let mut cat = catalog(&data, &t.0);
            add(&mut cat, "s:a", &inside);
            assert_eq!(cat.get("s:a").unwrap().unwrap()["file_path"], json!(inside));
        }
        assert_eq!(raw_paths(&data)["s:a"], "./media/static/a.webp");
    }
    // tests.test_media_paths.MediaPathTests.test_outside_the_data_folder_stays_absolute
    #[test]
    fn test_outside_the_data_folder_stays_absolute() {
        let t = Scratch::new();
        let data = t.join("collection");
        let archived = file(&t.join("archive/001_static.webp"), b"x");
        {
            let mut cat = catalog(&data, &t.0);
            add(&mut cat, "s:b", &archived);
        }
        assert_eq!(std::path::Path::new(&raw_paths(&data)["s:b"]), archived);
    }
    // tests.test_media_paths.MediaPathTests.test_a_moved_data_folder_still_finds_every_file
    #[test]
    fn test_a_moved_data_folder_still_finds_every_file() {
        let t = Scratch::new();
        let data = t.join("collection");
        let inside = file(&data.join("media/static/a.webp"), b"inside");
        let archived = file(&t.join("archive/b.webp"), b"archive");
        {
            let mut cat = catalog(&data, &t.0);
            add(&mut cat, "s:a", &inside);
            add(&mut cat, "s:b", &archived);
        }
        let moved = t.join("renamed project/collection");
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        std::fs::rename(data, &moved).unwrap();
        let items = {
            let cat = catalog(&moved, &t.0);
            cat.all(None)
                .unwrap()
                .into_iter()
                .map(|i| {
                    (
                        i["content_key"].as_str().unwrap().to_owned(),
                        std::path::PathBuf::from(i["file_path"].as_str().unwrap()),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(std::fs::read(&items["s:a"]).unwrap(), b"inside");
        assert!(items["s:a"].starts_with(&moved));
        assert_eq!(items["s:b"], archived);
    }
    // tests.test_media_paths.MediaPathTests.test_a_raw_reader_resolves_like_the_catalog
    #[test]
    fn test_a_raw_reader_resolves_like_the_catalog() {
        let t = Scratch::new();
        let data = t.join("collection");
        let inside = file(&data.join("media/static/a.webp"), b"x");
        {
            let mut cat = catalog(&data, &t.0);
            add(&mut cat, "s:a", &inside);
        }
        assert_eq!(
            rt::paths::resolve(&data, &raw_paths(&data)["s:a"], &t.0),
            inside
        );
    }
    // tests.test_media_paths.MediaPathTests.test_windows_case_does_not_make_a_path_outside
    #[cfg_attr(not(windows), ignore = "case-insensitive paths are a Windows property")]
    #[test]
    fn test_windows_case_does_not_make_a_path_outside() {
        #[cfg(windows)]
        {
            let t = Scratch::new();
            let data = t.join("collection");
            let inside = data.join("media/static/a.webp");
            assert_eq!(
                rt::paths::store(
                    std::path::Path::new(&data.to_string_lossy().to_uppercase()),
                    &inside
                )
                .unwrap(),
                "./media/static/a.webp"
            );
        }
    }
    // tests.test_media_paths.LegacyConversionTests.test_opening_converts_once_and_keeps_every_file
    #[test]
    fn test_opening_converts_once_and_keeps_every_file() {
        let f = Legacy::new();
        {
            let cat = catalog(&f.data, &f.scratch.0);
            let a = cat.get("s:a").unwrap().unwrap();
            let p = cat.get("s:p").unwrap().unwrap();
            assert_eq!(
                std::path::Path::new(a["file_path"].as_str().unwrap()),
                f.inside
            );
            assert_eq!(
                std::fs::canonicalize(p["file_path"].as_str().unwrap()).unwrap(),
                f.project_file.canonicalize().unwrap()
            );
            assert_eq!(
                cat.meta("media_paths").unwrap().as_deref(),
                Some("data-relative-2")
            );
        }
        let raw = raw_paths(&f.data);
        assert_eq!(raw["s:a"], "./media/a.webp");
        assert!(std::path::Path::new(&raw["s:p"]).is_absolute());
    }
    // tests.test_media_paths.LegacyConversionTests.test_a_converted_catalog_is_not_converted_again
    #[test]
    fn test_a_converted_catalog_is_not_converted_again() {
        let f = Legacy::new();
        drop(catalog(&f.data, &f.scratch.0));
        let cat = catalog(&f.data, &f.scratch.0);
        assert_eq!(cat.converted_media_paths(), 0);
    }
    // tests.test_media_paths.LegacyConversionTests.test_a_catalog_marked_with_the_old_value_is_healed_on_open
    #[test]
    fn test_a_catalog_marked_with_the_old_value_is_healed_on_open() {
        let f = Legacy::new();
        {
            let db = rusqlite::Connection::open(f.data.join("catalog.db")).unwrap();
            db.execute(
                "INSERT OR REPLACE INTO meta(key,value) VALUES('media_paths','data-relative')",
                [],
            )
            .unwrap();
        }
        {
            let cat = catalog(&f.data, &f.scratch.0);
            assert!(cat.converted_media_paths() >= 1);
        }
        assert_eq!(raw_paths(&f.data)["s:a"], "./media/a.webp");
    }
    // tests.test_media_paths.LegacyConversionTests.test_a_legacy_relative_row_means_the_project_even_without_meta
    #[test]
    fn test_a_legacy_relative_row_means_the_project_even_without_meta() {
        let f = Legacy::new();
        let db = rusqlite::Connection::open(f.data.join("catalog.db")).unwrap();
        db.execute("DROP TABLE meta", []).unwrap();
        assert_eq!(
            rt::paths::resolve(&f.data, "tests/__media_paths_rel__/p.webp", &f.scratch.0)
                .canonicalize()
                .unwrap(),
            f.project_file.canonicalize().unwrap()
        );
        assert_eq!(
            rt::paths::resolve(&f.data, "./media/a.webp", &f.scratch.0),
            f.inside
        );
    }
}
mod native_similarity {
    use super::*;
    // tests.test_native_similarity.NativeSimilarity.test_greedy_matches_source_fixtures
    #[test]
    fn test_greedy_matches_source_fixtures() {
        let data = similarity();
        let cases = data["greedy"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let hashes: Vec<u64> = serde_json::from_value(case["hashes"].clone()).unwrap();
            assert_eq!(
                json!(rt::similarity::greedy(&hashes)),
                case["expected"],
                "{case}"
            );
        }
    }
    // tests.test_native_similarity.NativeSimilarity.test_near_matches_source_fixtures
    #[test]
    fn test_near_matches_source_fixtures() {
        let data = similarity();
        let cases = data["near"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let hashes: Vec<Option<u64>> = serde_json::from_value(case["hashes"].clone()).unwrap();
            assert_eq!(
                json!(
                    rt::similarity::near(
                        &hashes,
                        case["query"].as_u64().unwrap(),
                        case["threshold"].as_i64().unwrap() as i32
                    )
                    .unwrap()
                ),
                case["expected"],
                "{case}"
            );
        }
    }
    // tests.test_native_similarity.NativeSimilarity.test_independent_concurrent_calls_have_no_shared_state
    #[test]
    fn test_independent_concurrent_calls_have_no_shared_state() {
        let cases = similarity()["greedy"].as_array().unwrap().clone();
        let calls = cases
            .iter()
            .map(|case| {
                let hashes: Vec<u64> = serde_json::from_value(case["hashes"].clone()).unwrap();
                std::thread::spawn(move || rt::similarity::greedy(&hashes))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            calls
                .into_iter()
                .map(|h| json!(h.join().unwrap()))
                .collect::<Vec<_>>(),
            cases
                .iter()
                .map(|c| c["expected"].clone())
                .collect::<Vec<_>>()
        );
    }
}
mod native_similarity_prerequisites {
    use super::*;
    // tests.test_native_similarity.NativeSimilarity.test_source_fixtures_do_not_drift
    // Migration: pin the frozen original input/provenance bytes and execute their native results.
    #[test]
    fn test_source_fixtures_do_not_drift() {
        use sha2::{Digest, Sha256};
        let bytes = include_bytes!("../../tests/fixtures/similarity.json");
        let digest = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(
            digest,
            "34df25f072dc51a8812006223465259493c8499815e8e91d9df52c3f0a680dcb"
        );
        let data = similarity();
        assert_eq!(data["source"], "695c895");
        assert_eq!(data["greedy"].as_array().unwrap().len(), 6);
        assert_eq!(data["near"].as_array().unwrap().len(), 24);
        for case in data["greedy"].as_array().unwrap() {
            let hashes: Vec<u64> = serde_json::from_value(case["hashes"].clone()).unwrap();
            assert_eq!(json!(rt::similarity::greedy(&hashes)), case["expected"]);
        }
        for case in data["near"].as_array().unwrap() {
            let hashes: Vec<Option<u64>> = serde_json::from_value(case["hashes"].clone()).unwrap();
            assert_eq!(
                json!(
                    rt::similarity::near(
                        &hashes,
                        case["query"].as_u64().unwrap(),
                        case["threshold"].as_i64().unwrap() as i32
                    )
                    .unwrap()
                ),
                case["expected"]
            );
        }
    }
    // tests.test_native_similarity.NativeSimilarity.test_invalid_native_inputs_fail_before_computation
    // Migration: actual production similarity calls reject invalid u64 values after a linked/executed positive control.
    #[test]
    fn test_invalid_native_inputs_fail_before_computation() {
        let t = Scratch::new();
        let compiler = ProductionLink::new(&t.0);
        compiler.admitted_and_executed(&t.0,"valid_similarity", "fn main() { assert_eq!(numera_runtime::similarity::greedy(&[0,1,2,3]),vec![0,1,3,2]); }");
        for (i, (value, diagnostic)) in [
            ("-1", "E0600"),
            ("18446744073709551616", "literal out of range for `u64`"),
            ("1.5", "E0308"),
            ("\"1\"", "E0308"),
        ]
        .iter()
        .enumerate()
        {
            let stderr = compiler.refused(
                &t.0,
                &format!("invalid_similarity_{i}"),
                &format!("fn main() {{ let _ = numera_runtime::similarity::greedy(&[{value}]); }}"),
                true,
            );
            assert!(
                stderr.contains(diagnostic),
                "the actual production API must refuse {value} with {diagnostic}: {stderr}"
            );
            assert!(
                stderr.contains("numera_runtime::similarity::greedy"),
                "the diagnostic must identify the actual production call: {stderr}"
            );
            assert!(
                !stderr.contains("can't find crate") && !stderr.contains("extern location"),
                "invalid-input test must reach the admitted actual production API: {stderr}"
            );
        }
        for threshold in [-2, 17] {
            assert!(rt::similarity::near(&[Some(0)], 0, threshold).is_err());
        }
        for value in ["-1", "18446744073709551616", "1.5", "\"1\""] {
            assert!(
                serde_json::from_str::<Vec<u64>>(&format!("[{value}]")).is_err(),
                "dynamic u64 input accepted: {value}"
            );
        }
    }
    // tests.test_native_similarity.NativeSimilarity.test_missing_module_never_uses_a_python_fallback
    // Migration: the actual numera_runtime dependency links and executes positively; withholding it refuses the same consumer.
    #[test]
    fn test_missing_module_never_uses_a_python_fallback() {
        let t = Scratch::new();
        let compiler = ProductionLink::new(&t.0);
        let source =
            "fn main() { assert_eq!(numera_runtime::similarity::greedy(&[0,1]),vec![0,1]); }";
        compiler.admitted_and_executed(&t.0, "native_present", source);
        let stderr = compiler.refused(&t.0, "native_absent", source, false);
        assert!(
            stderr.contains("extern location for numera_runtime does not exist"),
            "{stderr}"
        );
    }
    // tests.test_native_similarity.NativeSimilarity.test_missing_native_cannot_create_catalog_state
    // Migration: identical actual Catalog consumer creates state with its real dependency and cannot link without it.
    #[test]
    fn test_missing_native_cannot_create_catalog_state() {
        let t = Scratch::new();
        let compiler = ProductionLink::new(&t.0);
        let allowed = t.join("admitted");
        let refused = t.join("refused");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&refused).unwrap();
        let source = "fn main() { let root=std::env::current_dir().unwrap(); let path=root.join(\"catalog.db\"); let catalog=numera_runtime::catalog::Catalog::open(&path,\"2026-01-01T00:00:00Z\",&root).unwrap(); assert_eq!(catalog.meta(\"media_paths\").unwrap().as_deref(),Some(\"data-relative-2\")); }";
        compiler.admitted_and_executed(&allowed, "catalog_with_native", source);
        assert!(
            allowed.join("catalog.db").is_file(),
            "same production Catalog consumer must create real state in positive control"
        );
        let stderr = compiler.refused(&refused, "catalog_without_native", source, false);
        assert!(
            stderr.contains("extern location for numera_runtime does not exist"),
            "{stderr}"
        );
        assert!(!refused.join("catalog.db").exists());
    }
    // tests.test_native_similarity.NativeSimilarity.test_installer_fails_usefully_without_rust
    // Migration: exercise the real Rust package build boundary in a subprocess with no Cargo on PATH.
    #[test]
    fn test_installer_fails_usefully_without_rust() {
        const MARKER: &str = "NUMERA_BASELINE_NO_RUST_ROOT";
        if let Some(root) = std::env::var_os(MARKER) {
            let root = std::path::PathBuf::from(root);
            let error = rt::package::build(&root, false).unwrap_err();
            assert!(
                error.contains("Rust/Cargo is required to build the runtime"),
                "{error}"
            );
            assert_eq!(
                std::fs::read_dir(root).unwrap().count(),
                0,
                "missing toolchain created artifacts"
            );
            println!("{error}");
            return;
        }
        let t = Scratch::new();
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "native_similarity_prerequisites::test_installer_fails_usefully_without_rust",
                "--nocapture",
            ])
            .env_clear()
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("PATH", "")
            .env(MARKER, &t.0)
            .current_dir(&t.0);
        for key in ["SystemRoot", "WINDIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        if let Some(marker) = std::env::var_os("NUMERA_RUNTIME_TEST_ADMISSION") {
            command.env("NUMERA_RUNTIME_TEST_ADMISSION", marker);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("Rust/Cargo is required to build the runtime")
        );
        assert_eq!(std::fs::read_dir(&t.0).unwrap().count(), 0);
    }
}
mod coin_keywords {
    use super::*;
    // tests.test_coin_keywords.KeywordsFileHasOneWriter.test_an_invalid_cached_png_is_not_listed
    #[test]
    fn test_an_invalid_cached_png_is_not_listed() {
        let t = Scratch::new();
        let csv = t.join("keywords.csv");
        file(
            &t.join("logos/png/bad.png"),
            b"<html>429 Too Many Requests</html>",
        );
        image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]))
            .save(t.join("logos/png/ok.png"))
            .unwrap();
        let rows = json!({"bad":{"ticker":"bad","name":"","format":"png","file":"logos/png/bad.png"},"ok":{"ticker":"ok","name":"","format":"png","file":"logos/png/ok.png"}});
        rt::coin_keywords::write(rows.as_object().unwrap(), &csv).unwrap();
        assert_eq!(
            csv_rows(&csv)
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["ok"]
        );
    }
    // tests.test_coin_keywords.KeywordsFileHasOneWriter.test_an_empty_new_name_keeps_the_old_one
    #[test]
    fn test_an_empty_new_name_keeps_the_old_one() {
        let t = Scratch::new();
        let csv = file(&t.join("keywords.csv"), OLD.as_bytes());
        rt::coin_keywords::write(
            json!({"btc":{"ticker":"btc","name":"","format":"svg","file":"logos/svg/btc.svg"}})
                .as_object()
                .unwrap(),
            &csv,
        )
        .unwrap();
        let rows = csv_rows(&csv);
        assert_eq!(
            rows["btc"]["name"], "Bitcoin",
            "a failed lookup must not erase an earlier name"
        );
        assert_eq!(rows["btc"]["keywords"], "btc, Bitcoin");
    }
}
mod pack_rows {
    use super::*;
    // tests.test_pack_rows.TheSharedRenderer.test_pipes_and_newlines_are_escaped_and_empty_is_a_dash
    #[test]
    fn test_pipes_and_newlines_are_escaped_and_empty_is_a_dash() {
        let table =
            rt::pack_rows::markdown(&[json!({"a":HOSTILE,"b":""})], &[("a", "A"), ("b", "B")]);
        assert_eq!(table.lines().nth(2).unwrap(), format!("| {ESCAPED} | — |"));
    }
    // tests.test_pack_rows.TheSharedRenderer.test_source_splitlines_and_alignment_match_native_table
    #[test]
    fn test_source_splitlines_and_alignment_match_native_table() {
        let rows = [
            json!({"a":"one\r\ntwo\u{1c}next","b":true}),
            json!({"a":"","b":123}),
        ];
        assert_eq!(
            rt::pack_rows::aligned(&rows, &[("a", "Label", false), ("b", "Count", true)]),
            "| Label | Count |\n|-------|-----:|\n| one two next | True |\n| — | 123 |"
        );
    }
    // tests.test_pack_rows.TheSharedRenderer.test_every_keyword_is_kept
    #[test]
    fn test_every_keyword_is_kept() {
        assert_eq!(
            rt::pack_rows::keywords(&json!({"keywords":["a","","b","c"]})),
            "a, b, c"
        );
    }
    // tests.test_pack_rows.EveryWriterUsesIt.test_the_publishers_manifest
    #[test]
    fn test_the_publishers_manifest() {
        let t = Scratch::new();
        let img = t.join("media/a.png");
        std::fs::create_dir_all(img.parent().unwrap()).unwrap();
        image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 0, 0, 255]))
            .save(&img)
            .unwrap();
        {
            let mut cat = catalog(&t.0, &t.0);
            cat.insert(&json!({"content_key":"s:a","fmt":"static","file_path":img,"emojis":["😀"],"keywords":["one","two",HOSTILE]}),UTC).unwrap();
            rt::collection_manifest::write(
                &t.0,
                &mut cat,
                &json!({"name":"pk1_by_bot","title":"P","fmt":"static","keys":["s:a"]}),
                "pk",
            )
            .unwrap();
        }
        let text = std::fs::read_to_string(t.join("manifests/pk1_by_bot.md")).unwrap();
        assert_row(&text, "| # | Name | Emoji ID |");
        assert!(
            text.contains("one, two, left"),
            "all keywords, not the first two"
        );
    }
    // tests.test_pack_rows.EveryWriterUsesIt.test_the_archive_manifest
    #[test]
    fn test_the_archive_manifest() {
        let text = rt::archive_rows::manifest(
            &json!({"name":"set1","title":"One","fmt":"mixed"}),
            &[json!({"content_key":"s:a","premium_id":"111"})],
            &json!({"s:a":{"keywords":[HOSTILE]}}),
        )
        .unwrap();
        assert_row(&text, "| # | Name | Emoji ID |");
    }
    // tests.test_pack_rows.EveryWriterUsesIt.test_the_roster
    #[test]
    fn test_the_roster() {
        let mut doc = rt::roster::pack(
            &json!({"name":"s","index":1,"logo":false}),
            &json!({"stickers":[{"custom_emoji_id":"111","emoji":"✅"}]}),
            "general",
            &json!({}),
            &json!({}),
            UTC,
        )
        .unwrap();
        doc["emoji"][0]["name"] = json!(HOSTILE);
        assert_row(
            &rt::roster_markdown::pack(&doc).unwrap(),
            "| # | slot | custom_emoji_id | ours before | glyph | format | name | was |",
        );
    }
    // tests.test_pack_rows.EveryWriterUsesIt.test_the_coin_manifest
    #[test]
    fn test_the_coin_manifest() {
        let text = rt::coin_manifest::render(
            &json!({"name":"coin1_by_bot","title":"C 1"}),
            &[json!({"custom_emoji_id":"111"})],
            &BTreeMap::from([("111".into(), vec![HOSTILE.into()])]),
        )
        .unwrap();
        assert_row(&text, "| # | Ticker(s) | Emoji ID |");
    }
}
mod contracts_shared {
    use super::*;
    // tests.test_contracts_shared.TheIdSyntaxIsSharedWithTheWorker.test_every_case
    #[test]
    fn test_every_case() {
        let cases: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/contracts/emoji_messages.json"
        ))
        .unwrap();
        for case in cases.as_array().unwrap() {
            let msg = &case["message"];
            assert_eq!(
                json!(rt::bot_payloads::extract(msg)),
                case["ids"],
                "{}",
                case["name"]
            );
            assert_eq!(
                json!(rt::bot_payloads::typed(msg["text"].as_str().unwrap_or(""))),
                case["typed"],
                "{}",
                case["name"]
            );
        }
    }
}
mod contracts_publish {
    use super::*;
    // tests.test_contracts_shared.ThePublishBodyIsWhatTheWorkerValidates.test_announce_via_worker_sends_the_fixture_body
    #[test]
    fn test_announce_via_worker_sends_the_fixture_body() {
        use std::io::{Read, Write};
        use std::time::{Duration, Instant};
        let want: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/contracts/publish_request.json"
        ))
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let peer = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(8);
            let mut stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("bounded local request accept: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            let (headers, body) = loop {
                let mut chunk = [0; 4096];
                let n = stream.read(&mut chunk).unwrap();
                assert_ne!(n, 0, "request ended early");
                bytes.extend_from_slice(&chunk[..n]);
                assert!(bytes.len() < 1024 * 1024);
                if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                    let length = headers
                        .lines()
                        .find_map(|l| {
                            l.split_once(':')
                                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + length {
                        break (headers, bytes[end + 4..end + 4 + length].to_vec());
                    }
                }
            };
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .unwrap();
            (headers, serde_json::from_slice::<Value>(&body).unwrap())
        });
        let t = Scratch::new();
        let url = format!("http://{address}/publish");
        let c = config(
            &t.0,
            &[
                ("WORKER_PUBLISH_URL", &url),
                ("WORKER_PUBLISH_SECRET", "publish-bearer"),
            ],
        );
        let result = rt::announce::worker(
            &c,
            want["packs"].as_array().unwrap(),
            want["bot"].as_str().unwrap(),
            want["note"].as_str().unwrap(),
            want["style"].as_str().unwrap(),
            5.0,
        );
        let (headers, body) = peer.join().unwrap();
        result.unwrap();
        assert!(
            headers
                .lines()
                .next()
                .unwrap()
                .starts_with("POST /publish HTTP/1.1")
        );
        assert_eq!(
            body, want,
            "the body changed: update the shared fixture and Worker's validator together"
        );
        let authorization = headers
            .lines()
            .find_map(|l| {
                l.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("Authorization"))
                    .map(|(_, v)| v.trim())
            })
            .unwrap();
        assert!(authorization.starts_with("Bearer "));
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
