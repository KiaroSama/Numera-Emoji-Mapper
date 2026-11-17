//! 95 authoritative original frozen-rebuild, canonical-map and coin argument identities.
#[path = "baseline_coin_providers_support/mod.rs"]
mod baseline_coin_provider_support;
#[path = "baseline_coin_state_support/mod.rs"]
mod support;
use support::*;
mod cli_args {
    use super::*;
    // tests.test_coin_cli_args.AnUnrecognisedArgumentCannotSelectTheLiveBranch.test_a_misspelled_dry_flag_is_refused_by_both_providers
    #[test]
    fn test_a_misspelled_dry_flag_is_refused_by_both_providers() {
        for provider in ["paprika", "cmc"] {
            for typo in ["--dryy", "--dry-run", "--dr", "-dry", "dry"] {
                let root = empty_provider();
                let before = doc(&root.0.join("ticker_to_id.json"));
                let server = Server::new(vec![]);
                let result = command(
                    &root,
                    &server.base,
                    &server.base,
                    &ids(&[&format!("coin-fetch-{provider}"), typo]),
                    &[],
                );
                assert_eq!(
                    result.code, 2,
                    "{provider} {typo}: {}{}",
                    result.stdout, result.stderr
                );
                assert!(result.stderr.to_lowercase().contains("usage:"));
                assert_eq!(doc(&root.0.join("ticker_to_id.json")), before);
                assert!(server.finish().is_empty());
            }
        }
    }
    // tests.test_coin_cli_args.AnUnrecognisedArgumentCannotSelectTheLiveBranch.test_the_real_dry_flag_still_parses
    #[test]
    fn test_the_real_dry_flag_still_parses() {
        for provider in ["paprika", "cmc"] {
            let root = empty_provider();
            let server = Server::new(vec![]);
            let result = command(
                &root,
                &server.base,
                &server.base,
                &ids(&[
                    &format!("coin-fetch-{provider}"),
                    "--coin-dir",
                    root.0.to_str().unwrap(),
                    "--dry",
                ]),
                &[("CMC_API_KEY", "k")],
            );
            assert_eq!(
                result.code, 0,
                "{provider}: {}{}",
                result.stdout, result.stderr
            );
            assert!(
                result
                    .stdout
                    .contains("dry run: no downloads/pack changes.")
            );
            assert!(server.finish().is_empty());
            assert_eq!(
                doc(&root.0.join("ticker_to_id.json")),
                serde_json::json!({})
            );
            assert!(!root.0.join("logos").exists());
            assert!(!root.0.join("rebuild_dedup_state.json").exists());
        }
    }
    // tests.test_coin_cli_args.AnUnrecognisedArgumentCannotSelectTheLiveBranch.test_an_unset_token_stops_a_live_run_before_any_download
    #[test]
    fn test_an_unset_token_stops_a_live_run_before_any_download() {
        for provider in ["paprika", "cmc"] {
            let root = empty_provider();
            let server = Server::new(vec![]);
            let result = command(
                &root,
                &server.base,
                &server.base,
                &ids(&[
                    &format!("coin-fetch-{provider}"),
                    "--coin-dir",
                    root.0.to_str().unwrap(),
                ]),
                &[("CMC_API_KEY", "k")],
            );
            assert_eq!(
                result.code, 2,
                "{provider}: {}{}",
                result.stdout, result.stderr
            );
            assert!(result.stderr.contains("TELEGRAM_BOT_TOKEN is not set"));
            assert!(server.finish().is_empty());
            assert!(!root.0.join("logos").exists());
            assert_eq!(
                doc(&root.0.join("ticker_to_id.json")),
                serde_json::json!({})
            );
        }
    }
    // tests.test_coin_cli_args.AnUnrecognisedArgumentCannotSelectTheLiveBranch.test_fetch_logos_page_count_is_not_read_at_import
    #[test]
    fn test_fetch_logos_page_count_is_not_read_at_import() {
        let root = empty_provider();
        let server = Server::new(vec![]);
        let result = command(
            &root,
            &server.base,
            &server.base,
            &ids(&["coin-fetch-logos", "--help"]),
            &[],
        );
        assert_eq!(result.code, 0);
        assert!(result.stdout.to_lowercase().contains("usage:"));
        assert!(result.stdout.contains("Default: 40 market pages"));
        for bad in ["notanumber", "0", "-3"] {
            let result = command(
                &root,
                &server.base,
                &server.base,
                &ids(&["coin-fetch-logos", bad]),
                &[],
            );
            assert_eq!(result.code, 2, "{bad}: {}{}", result.stdout, result.stderr);
        }
        assert!(server.finish().is_empty());
        assert!(!root.0.join("logos").exists());
    }
}

mod state_schema {
    use super::*;
    use serde_json::json;
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_a_negative_cursor_is_rejected_before_any_deletion
    #[test]
    fn test_a_negative_cursor_is_rejected_before_any_deletion() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        assert!(f.rejects(json!({"cursor":-1})).contains("cursor -1"));
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_an_oversized_cursor_is_rejected_instead_of_reporting_done
    #[test]
    fn test_an_oversized_cursor_is_rejected_instead_of_reporting_done() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        assert!(f.rejects(json!({"cursor":3})).contains("past the end"));
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_a_non_integer_cursor_is_rejected
    #[test]
    fn test_a_non_integer_cursor_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        assert!(f.rejects(json!({"cursor":"1"})).contains("cursor"));
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_a_boolean_cursor_is_not_read_as_a_number
    #[test]
    fn test_a_boolean_cursor_is_not_read_as_a_number() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        assert!(f.rejects(json!({"cursor":true})).contains("cursor"));
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_an_order_that_repeats_an_entry_is_rejected
    #[test]
    fn test_an_order_that_repeats_an_entry_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        assert!(
            f.rejects(json!({"order":["aaa","aaa"]}))
                .contains("more than once")
        );
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_a_repeated_set_name_is_rejected
    #[test]
    fn test_a_repeated_set_name_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        assert!(
            f.rejects(json!({"sets":[{"index":1,"name":"s1"},{"index":2,"name":"s1"}]}))
                .contains("repeats the set name")
        );
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_set_indexes_must_ascend
    #[test]
    fn test_set_indexes_must_ascend() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        assert!(
            f.rejects(json!({"sets":[{"index":2,"name":"s2"},{"index":1,"name":"s1"}]}))
                .contains("does not ascend")
        );
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_a_live_count_over_the_pack_limit_is_rejected
    #[test]
    fn test_a_live_count_over_the_pack_limit_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        assert!(
            f.rejects(json!({"sets":[{"index":1,"name":"s1","live":201}]}))
                .contains("outside 0..")
        );
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_an_in_flight_marker_missing_its_target_is_rejected
    #[test]
    fn test_an_in_flight_marker_missing_its_target_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        assert!(
            f.rejects(json!({"in_flight":{"key":"aaa","operation":"add"}}))
                .contains("set_name")
        );
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_an_unknown_in_flight_operation_is_rejected
    #[test]
    fn test_an_unknown_in_flight_operation_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        assert!(f.rejects(json!({"in_flight":{"key":"aaa","operation":"delete","set_name":"s1","set_index":1}})).contains("unknown"));
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_wrong_types_are_rejected
    #[test]
    fn test_wrong_types_are_rejected() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        assert!(f.rejects(json!({"order":"aaa"})).contains("'order'"));
        assert!(
            f.rejects(json!({"deleted_old":"no"}))
                .contains("'deleted_old'")
        );
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_the_legacy_bare_key_marker_is_rejected_here_not_at_recovery
    #[test]
    fn test_the_legacy_bare_key_marker_is_rejected_here_not_at_recovery() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        let problem = f.rejects(json!({"in_flight":"aaa"}));
        assert!(problem.contains("legacy bare key"));
        assert!(problem.contains("cursor"));
        assert!(problem.contains("'order'"));
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_a_marker_that_is_neither_object_nor_null_is_rejected
    #[test]
    fn test_a_marker_that_is_neither_object_nor_null_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        assert!(
            f.rejects(json!({"in_flight":["aaa"]}))
                .contains("'in_flight'")
        );
    }
}

mod cursor_order {
    use super::*;
    use serde_json::json;
    // tests.test_rebuild_dedup_plan.CursorOrderAndPlanMustDescribeOneWalk.test_an_upload_the_cursor_never_reached_is_rejected
    #[test]
    fn test_an_upload_the_cursor_never_reached_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb", "ccc"]);
        assert!(f.rejects(json!({"cursor":0,"order":["aaa"],"sets":[{"index":1,"name":"s1","title":"T 1","live":1}]})).contains("'order' records 'aaa'"));
    }
    // tests.test_rebuild_dedup_plan.CursorOrderAndPlanMustDescribeOneWalk.test_an_order_that_is_not_in_plan_order_is_rejected
    #[test]
    fn test_an_order_that_is_not_in_plan_order_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb", "ccc"]);
        assert!(f.rejects(json!({"cursor":3,"order":["ccc","aaa"],"sets":[{"index":1,"name":"s1","title":"T 1","live":2}]})).contains("'order' records 'aaa'"));
    }
    // tests.test_rebuild_dedup_plan.CursorOrderAndPlanMustDescribeOneWalk.test_an_upload_that_is_not_a_plan_entry_at_all_is_rejected
    #[test]
    fn test_an_upload_that_is_not_a_plan_entry_at_all_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb", "ccc"]);
        assert!(f.rejects(json!({"cursor":3,"order":["zzz"],"sets":[{"index":1,"name":"s1","title":"T 1","live":1}]})).contains("'order' records 'zzz'"));
    }
    // tests.test_rebuild_dedup_plan.CursorOrderAndPlanMustDescribeOneWalk.test_uploads_with_no_set_to_live_in_are_rejected
    #[test]
    fn test_uploads_with_no_set_to_live_in_are_rejected() {
        let f = Rebuild::new(&["aaa", "bbb", "ccc"]);
        assert!(
            f.rejects(json!({"cursor":1,"order":["aaa"],"sets":[]}))
                .contains("no set holds them")
        );
    }
    // tests.test_rebuild_dedup_plan.CursorOrderAndPlanMustDescribeOneWalk.test_a_marker_for_a_different_plan_entry_is_rejected
    #[test]
    fn test_a_marker_for_a_different_plan_entry_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb", "ccc"]);
        assert!(f.rejects(json!({"cursor":1,"order":[],"sets":[{"index":1,"name":"s1","live":0}],"in_flight":{"key":"ccc","operation":"add","set_name":"s1","set_index":1,"expected_before":0}})).contains("is not plan entry"));
    }
    // tests.test_rebuild_dedup_plan.CursorOrderAndPlanMustDescribeOneWalk.test_a_marker_with_no_cursor_behind_it_is_rejected
    #[test]
    fn test_a_marker_with_no_cursor_behind_it_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb", "ccc"]);
        assert!(f.rejects(json!({"cursor":0,"order":[],"sets":[{"index":1,"name":"s1","live":0}],"in_flight":{"key":"aaa","operation":"add","set_name":"s1","set_index":1,"expected_before":0}})).contains("is not plan entry"));
    }
}

mod owner_id {
    use super::*;
    // tests.test_rebuild_dedup_state.OwnerIdIsParsedSafely.test_a_typo_falls_back_instead_of_killing_the_import
    #[test]
    fn test_a_typo_falls_back_instead_of_killing_the_import() {
        let root = Root::new();
        let config =
            baseline_coin_provider_support::config(&root.0, &[("PACK_OWNER_USER_ID", "42abc")]);
        assert_eq!(config.integer("PACK_OWNER_USER_ID", 0, 0, i64::MAX), 0);
    }
    // tests.test_rebuild_dedup_state.OwnerIdIsParsedSafely.test_a_valid_value_is_still_used
    #[test]
    fn test_a_valid_value_is_still_used() {
        let root = Root::new();
        let config =
            baseline_coin_provider_support::config(&root.0, &[("PACK_OWNER_USER_ID", "12345")]);
        assert_eq!(config.integer("PACK_OWNER_USER_ID", 0, 0, i64::MAX), 12345);
    }
}
mod shared_logo_guard {
    use numera_runtime as rt;
    use serde_json::{Map, json};
    // tests.test_rebuild_dedup_map.SharedLogoGuard.test_unreviewed_many_to_one_group_is_reported
    #[test]
    fn test_unreviewed_many_to_one_group_is_reported() {
        let mapping = json!({"apt":"SAME","arkm":"SAME","hbar":"SAME","near":"SAME","tao":"SAME","btc":"OWN"});
        let result = rt::coin_plan::replay(mapping.as_object().unwrap(), &Map::new(), &[]).unwrap();
        let bad = result["unapproved"].as_object().unwrap();
        assert_eq!(bad.keys().collect::<Vec<_>>(), vec!["SAME"]);
        assert_eq!(bad["SAME"].as_array().unwrap().len(), 5);
    }
    // tests.test_rebuild_dedup_map.SharedLogoGuard.test_reviewed_shared_group_is_accepted
    #[test]
    fn test_reviewed_shared_group_is_accepted() {
        let mapping = json!({"usdt":"T","usdtbsc":"T","usdterc20":"T"});
        let approved = super::ids(&["usdt", "usdtbsc", "usdterc20"]);
        let result =
            rt::coin_plan::replay(mapping.as_object().unwrap(), &Map::new(), &approved).unwrap();
        assert_eq!(result["unapproved"], json!({}));
    }
    // tests.test_rebuild_dedup_map.SharedLogoGuard.test_one_to_one_map_is_clean
    #[test]
    fn test_one_to_one_map_is_clean() {
        let mapping = json!({"btc":"1","eth":"2","sol":"3"});
        let result = rt::coin_plan::replay(mapping.as_object().unwrap(), &Map::new(), &[]).unwrap();
        assert_eq!(result["unapproved"], json!({}));
    }
}
mod local_shared_logo_guard {
    // tests.test_rebuild_dedup_map.SharedLogoGuard.test_the_real_committed_map_is_checked_against_the_real_groups
    #[test]
    #[ignore = "local-only: coins/ticker_to_id.json is not tracked"]
    fn test_the_real_committed_map_is_checked_against_the_real_groups() {
        use numera_runtime as rt;
        use serde_json::{Map, Value};
        let supplied =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/local-coin-map-fixture");
        assert!(
            supplied.is_dir(),
            "manual --ignored run requires owned copies of ticker_to_id.json and shared_logo_groups.json in runtime/target/local-coin-map-fixture; the guard intentionally forwards no operator environment"
        );
        let root = rt::tooling_environment::physical_root(&supplied).unwrap();
        let path = root.join("ticker_to_id.json");
        rt::tooling_environment::reject_links(&path, "local map fixture must be nonlinked")
            .unwrap();
        assert!(
            path.is_file(),
            "manual local map fixture must exist; absent operator maps remain explicitly ignored in normal CI"
        );
        let mapping: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let group_path = root.join("shared_logo_groups.json");
        rt::tooling_environment::reject_links(&group_path, "local group fixture must be nonlinked")
            .unwrap();
        let approved = if group_path.is_file() {
            let groups: Value =
                serde_json::from_slice(&std::fs::read(group_path).unwrap()).unwrap();
            groups
                .as_object()
                .unwrap()
                .values()
                .flat_map(|group| {
                    group
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|ticker| ticker.as_str().unwrap().to_owned())
                })
                .collect::<Vec<_>>()
        } else {
            vec![]
        };
        let result =
            rt::coin_plan::replay(mapping.as_object().unwrap(), &Map::new(), &approved).unwrap();
        let biggest = result["unapproved"]
            .as_object()
            .unwrap()
            .values()
            .map(|group| group.as_array().unwrap().len())
            .max()
            .unwrap_or(0);
        assert!(
            biggest < 20,
            "an emoji id is shared by {biggest} unreviewed tickers; this is the positional-drift signature"
        );
    }
}

mod inventory {
    use numera_runtime as rt;
    use std::collections::{BTreeMap, BTreeSet};
    const INVENTORY: &str = "## 🟡 — Avalanche C-Chain\n   ticker: avaxc\n   premium-id:\n\n## 🟢 — BitTorrent Chain\n   ticker: bttc\n   premium-id:\n";
    // tests.test_coin_ticker_map.OneInventoryImplementation.test_the_explicit_aliases_now_reach_the_fetchers
    #[test]
    fn test_the_explicit_aliases_now_reach_the_fetchers() {
        assert_eq!(rt::coin_inventory::base_ticker("avaxc"), "avax");
        assert_eq!(rt::coin_inventory::base_ticker("bttc"), "btt");
        for source in [
            include_str!("../src/coin_cmc.rs"),
            include_str!("../src/coin_provider_policy.rs"),
        ] {
            assert!(
                source.contains("crate::coin_inventory::base_ticker(ticker)"),
                "actual CMC and Paprika classification must share the production resolver"
            );
        }
    }
    // tests.test_coin_ticker_map.OneInventoryImplementation.test_the_suffix_rules_are_unchanged
    #[test]
    fn test_the_suffix_rules_are_unchanged() {
        for (ticker, want) in [("kibabsc", "kiba"), ("btc", "btc"), ("ghc", "ghc")] {
            assert_eq!(rt::coin_inventory::base_ticker(ticker), want);
        }
    }
    // tests.test_coin_ticker_map.OneInventoryImplementation.test_the_re_fill_loop_has_exactly_one_definition
    #[test]
    fn test_the_re_fill_loop_has_exactly_one_definition() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("runtime/src");
        let mut copies = std::fs::read_dir(source)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "rs"))
            .filter(|p| {
                std::fs::read_to_string(p)
                    .unwrap()
                    .contains("premium-id: {id}")
            })
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        copies.sort();
        assert_eq!(copies, vec!["coin_inventory.rs"]);
    }
    // tests.test_coin_ticker_map.OneInventoryImplementation.test_every_tool_re_fills_through_the_same_function
    #[test]
    fn test_every_tool_re_fills_through_the_same_function() {
        for source in [
            include_str!("../src/coin_provider_cli.rs"),
            include_str!("../src/coin_map_cli.rs"),
        ] {
            assert!(source.contains("crate::coin_inventory::refill("));
        }
        assert!(
            include_str!("../src/coin_rebuild_map_disk.rs")
                .contains("crate::coin_provider_cli::refill(")
        );
        assert_eq!(
            include_str!("../src/coin_provider_cli.rs")
                .matches("crate::coin_inventory::refill(")
                .count(),
            1,
            "both provider branches share one real refill flow"
        );
    }
    // tests.test_coin_ticker_map.OneInventoryImplementation.test_an_unmapped_ticker_is_blanked_not_left_stale
    #[test]
    fn test_an_unmapped_ticker_is_blanked_not_left_stale() {
        let result =
            rt::coin_inventory::refill(&BTreeMap::new(), "   ticker: btc\n   premium-id: 999\n");
        assert_eq!((result.filled, result.total), (0, 1));
        assert_eq!(result.text, "   ticker: btc\n   premium-id:\n");
    }
    // tests.test_coin_ticker_map.OneInventoryImplementation.test_indentation_and_ids_survive_the_round_trip
    #[test]
    fn test_indentation_and_ids_survive_the_round_trip() {
        let result = rt::coin_inventory::refill(
            &BTreeMap::from([
                ("avaxc".into(), "111".into()),
                ("bttc".into(), "222".into()),
            ]),
            INVENTORY,
        );
        assert_eq!((result.filled, result.total), (2, 2));
        assert!(result.text.contains("   premium-id: 111"));
        assert!(result.text.contains("   premium-id: 222"));
    }
    // tests.test_coin_ticker_map.OneInventoryImplementation.test_parse_missing_skips_what_is_already_mapped
    #[test]
    fn test_parse_missing_skips_what_is_already_mapped() {
        let result =
            rt::coin_inventory_parse::missing(INVENTORY, &BTreeSet::from(["avaxc".into()]));
        assert_eq!(result, vec![("BitTorrent Chain".into(), "bttc".into())]);
    }
}

mod truncated_plan {
    use super::*;
    use serde_json::json;
    // tests.test_rebuild_dedup_plan.TruncatedPlanFailsClosed.test_a_truncated_plan_refuses_to_build
    #[test]
    fn test_a_truncated_plan_refuses_to_build() {
        let f = Rebuild::new(&["aaa"]);
        let path = f.files.directory.join("rebuild_dedup_plan.json");
        std::fs::write(&path, "[{\"rep\": \"aaa\", \"tickers\": [\"aa").unwrap();
        let server = Server::new(vec![]);
        let error = f.run(&server).unwrap_err();
        assert!(
            error.contains(path.file_name().unwrap().to_str().unwrap()),
            "the stop must name the malformed frozen plan: {error}"
        );
        assert!(server.finish().is_empty());
    }
    // tests.test_rebuild_dedup_plan.TruncatedPlanFailsClosed.test_a_plan_entry_missing_its_fields_is_rejected
    #[test]
    fn test_a_plan_entry_missing_its_fields_is_rejected() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        write(
            &f.files.directory.join("rebuild_dedup_plan.json"),
            &json!([{"rep":"aaa","tickers":["aaa"],"kw":"aaa"},{"rep":"bbb"}]),
        );
        let server = Server::new(vec![]);
        assert!(f.run(&server).is_err());
        assert!(server.finish().is_empty());
    }
    // tests.test_rebuild_dedup_plan.TruncatedPlanFailsClosed.test_a_sound_plan_still_loads
    #[test]
    fn test_a_sound_plan_still_loads() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        let plan = numera_runtime::coin_plan_disk::load(&f.files.directory).unwrap();
        assert_eq!(
            plan.as_array()
                .unwrap()
                .iter()
                .map(|g| g["rep"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["aaa", "bbb"]
        );
    }
}

mod positive_walks {
    use super::*;
    use serde_json::json;
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_a_sound_state_still_builds
    #[test]
    fn test_a_sound_state_still_builds() {
        let f = Rebuild::new(&["aaa", "bbb"]);
        f.old(&["old1"]);
        f.state(json!({"deleted_old":false,"cursor":0}));
        let created = "cryptoemoji1_by_bot";
        let server = Server::new(vec![
            reply("deleteStickerSet", ok(json!(true))),
            missing("old1"),
            f.mutate("aaa", created, true, Body::Json(ok(json!(true)))),
            listing(created, 1),
            f.mutate("bbb", created, false, Body::Json(ok(json!(true)))),
            notify(),
        ]);
        f.run(&server).unwrap();
        let requests = server.finish();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "deleteStickerSet")
                .map(|r| r.form["name"].as_str())
                .collect::<Vec<_>>(),
            vec!["old1"]
        );
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "createNewStickerSet")
                .count(),
            1
        );
        assert_eq!(f.saved()["cursor"], 2);
        f.released();
    }
    // tests.test_rebuild_dedup_plan.StateSchemaIsValidatedBeforeAnyMutation.test_following_the_legacy_repair_actually_clears_the_stop
    #[test]
    fn test_following_the_legacy_repair_actually_clears_the_stop() {
        let f = Rebuild::new(&["seed", "aaa", "bbb"]);
        f.old(&["old1"]);
        f.state(json!({"deleted_old":true,"cursor":2,"order":["seed"],"in_flight":"aaa","sets":[{"index":1,"name":"s1","title":"T 1"}]}));
        let server = Server::new(vec![]);
        let prescribed = f.run(&server).unwrap_err();
        assert!(server.finish().is_empty());
        assert!(prescribed.contains("'in_flight' to null"));
        assert!(prescribed.contains("append 'aaa' to 'order'"));
        let mut state = f.saved();
        state["in_flight"] = json!(null);
        state["order"].as_array_mut().unwrap().push(json!("aaa"));
        write(&f.files.directory.join("rebuild_dedup_state.json"), &state);
        let server = Server::new(vec![
            listing("s1", 2),
            listing("s1", 2),
            f.mutate("bbb", "s1", false, Body::Json(ok(json!(true)))),
            notify(),
        ]);
        f.run(&server).unwrap();
        let requests = server.finish();
        assert!(f.saved()["in_flight"].is_null());
        assert!(
            f.saved()["order"]
                .as_array()
                .unwrap()
                .contains(&json!("aaa"))
        );
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "addStickerToSet" || r.method == "createNewStickerSet")
                .count(),
            1
        );
        assert_eq!(
            requests
                .iter()
                .find(|r| r.method == "addStickerToSet")
                .unwrap()
                .files["file0"],
            noise("bbb")
        );
        f.released();
    }
    // tests.test_rebuild_dedup_plan.CursorOrderAndPlanMustDescribeOneWalk.test_live_drift_stops_before_the_old_packs_are_deleted
    #[test]
    fn test_live_drift_stops_before_the_old_packs_are_deleted() {
        let f = Rebuild::new(&["aaa", "bbb", "ccc"]);
        f.old(&["old1"]);
        f.state(json!({"deleted_old":false,"cursor":1,"order":["aaa"],"sets":[{"index":1,"name":"s1","title":"T 1","live":1}]}));
        let server = Server::new(vec![listing("s1", 2)]);
        let error = f.run(&server).unwrap_err();
        assert!(error.contains("uploads are recorded"));
        let requests = server.finish();
        assert!(requests.iter().all(|r| r.method != "deleteStickerSet"
            && r.method != "addStickerToSet"
            && r.method != "createNewStickerSet"));
        f.released();
    }
    // tests.test_rebuild_dedup_plan.CursorOrderAndPlanMustDescribeOneWalk.test_a_walk_with_skipped_entries_is_still_accepted
    #[test]
    fn test_a_walk_with_skipped_entries_is_still_accepted() {
        let f = Rebuild::new(&["aaa", "bbb", "ccc"]);
        f.state(json!({"order":["aaa","ccc"],"cursor":3,"sets":[{"index":1,"name":"s1","title":"T 1","live":2}]}));
        let server = Server::new(vec![listing("s1", 2), notify()]);
        f.run(&server).unwrap();
        let requests = server.finish();
        assert!(
            requests
                .iter()
                .all(|r| r.method != "addStickerToSet" && r.method != "createNewStickerSet")
        );
        assert_eq!(f.saved()["cursor"], 3);
        f.released();
    }
    // tests.test_rebuild_dedup_plan.BuildFinishesWhenThePlanIsWalked.test_trailing_skips_are_saved_and_the_walk_is_done
    #[test]
    fn test_trailing_skips_are_saved_and_the_walk_is_done() {
        let f = Rebuild::new(&["seed", "aaa", "bbb"]);
        f.state(
            json!({"sets":[{"index":1,"name":"s1","title":"T 1"}],"order":["seed"],"cursor":1}),
        );
        std::fs::remove_file(f.files.images.join("aaa.png")).unwrap();
        std::fs::write(f.files.images.join("bbb.png"), b"").unwrap();
        let server = Server::new(vec![me(), listing("s1", 1), notify()]);
        let completed = f.build_cli(&server);
        assert_eq!(
            completed.code, 0,
            "{}{}",
            completed.stdout, completed.stderr
        );
        assert_eq!(f.saved()["cursor"], 3);
        assert!(
            server
                .finish()
                .iter()
                .all(|r| r.method != "addStickerToSet" && r.method != "createNewStickerSet")
        );
        f.released();
    }
    // tests.test_rebuild_dedup_plan.BuildFinishesWhenThePlanIsWalked.test_a_clean_walk_is_done
    #[test]
    fn test_a_clean_walk_is_done() {
        let f = Rebuild::new(&["seed", "aaa"]);
        f.state(
            json!({"sets":[{"index":1,"name":"s1","title":"T 1"}],"order":["seed"],"cursor":1}),
        );
        let server = Server::new(vec![
            me(),
            listing("s1", 1),
            listing("s1", 1),
            f.mutate("aaa", "s1", false, Body::Json(ok(json!(true)))),
            notify(),
        ]);
        let completed = f.build_cli(&server);
        assert_eq!(
            completed.code, 0,
            "{}{}",
            completed.stdout, completed.stderr
        );
        assert_eq!(f.saved()["order"], json!(["seed", "aaa"]));
        server.finish();
        f.released();
    }
}

mod map_identity {
    use super::*;
    use numera_runtime as rt;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_the_untouched_pack_maps_by_content
    #[test]
    fn test_the_untouched_pack_maps_by_content() {
        let f = mapping_fixture();
        let server = Server::new(mapping_script(&["aaa", "bbb", "ccc"]));
        f.map(&server).unwrap();
        server.finish();
        assert_eq!(f.mapping(), json!({"aaa":"s1-0","bbb":"s1-1","ccc":"s1-2"}));
        f.released();
    }
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_same_length_reorder_is_never_mapped_by_position
    #[test]
    fn test_a_same_length_reorder_is_never_mapped_by_position() {
        let f = mapping_fixture();
        let server = Server::new(mapping_script(&["ccc", "bbb", "aaa"]));
        f.map(&server).unwrap();
        server.finish();
        assert_eq!(f.mapping()["aaa"], "s1-2");
        assert_eq!(f.mapping()["ccc"], "s1-0");
        assert_ne!(f.mapping()["aaa"], "s1-0");
        f.released();
    }
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_same_length_replacement_does_not_rewrite_the_map
    #[test]
    fn test_a_same_length_replacement_does_not_rewrite_the_map() {
        let f = mapping_fixture();
        f.write_mapping(json!({"aaa":"keep-me"}));
        let mut script = vec![listing("s1", 3)];
        download(&mut script, "s1", 0, &noise("aaa"));
        download(&mut script, "s1", 1, &noise("someone-elses-logo"));
        let server = Server::new(script);
        let error = f.map(&server).unwrap_err();
        server.finish();
        assert_eq!(f.mapping(), json!({"aaa":"keep-me"}));
        assert!(error.contains("s1-1"));
        assert!(
            f.files
                .directory
                .join("ticker_to_id.candidate.json")
                .is_file()
        );
        f.released();
    }
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_live_sticker_that_cannot_be_read_refuses_to_map
    #[test]
    fn test_a_live_sticker_that_cannot_be_read_refuses_to_map() {
        let f = mapping_fixture();
        f.write_mapping(json!({"aaa":"keep-me"}));
        let mut script = vec![listing("s1", 3)];
        download(&mut script, "s1", 0, &noise("aaa"));
        download(&mut script, "s1", 1, b"not an image at all");
        let server = Server::new(script);
        assert!(f.map(&server).is_err());
        server.finish();
        assert_eq!(f.mapping(), json!({"aaa":"keep-me"}));
        f.released();
    }
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_missing_source_image_refuses_to_map
    #[test]
    fn test_a_missing_source_image_refuses_to_map() {
        let f = mapping_fixture();
        f.write_mapping(json!({"aaa":"keep-me"}));
        std::fs::remove_file(f.files.images.join("bbb.png")).unwrap();
        let server = Server::new(vec![listing("s1", 3)]);
        assert!(f.map(&server).is_err());
        server.finish();
        assert_eq!(f.mapping(), json!({"aaa":"keep-me"}));
        f.released();
    }
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_an_extra_live_sticker_refuses_to_map
    #[test]
    fn test_an_extra_live_sticker_refuses_to_map() {
        let f = mapping_fixture();
        f.write_mapping(json!({"aaa":"keep-me"}));
        let server = Server::new(mapping_script(&[
            "aaa",
            "bbb",
            "ccc",
            "appended-by-a-concurrent-tool",
        ]));
        assert!(f.map(&server).is_err());
        server.finish();
        assert_eq!(f.mapping(), json!({"aaa":"keep-me"}));
        f.released();
    }
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_the_canonical_map_is_written_under_the_shared_lock
    #[test]
    fn test_the_canonical_map_is_written_under_the_shared_lock() {
        let f = mapping_fixture();
        let hold = rt::locks::Lease::acquire(
            &rt::locks::canonical_map_path(&f.root.0),
            "actual competing canonical map writer",
        )
        .unwrap();
        let server = Server::new(vec![]);
        assert!(f.map(&server).is_err());
        assert!(server.finish().is_empty());
        assert!(!f.files.directory.join("ticker_to_id.json").exists());
        f.released();
        drop(hold);
        drop(
            rt::locks::Lease::acquire(
                &rt::locks::canonical_map_path(&f.root.0),
                "released canonical positive control",
            )
            .unwrap(),
        );
    }
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_provider_cannot_change_the_map_during_the_snapshot
    #[test]
    fn test_a_provider_cannot_change_the_map_during_the_snapshot() {
        let f = mapping_fixture();
        let outcomes = Arc::new(Mutex::new(vec![]));
        let seen = outcomes.clone();
        let root = f.root.0.clone();
        let map = f.files.directory.join("ticker_to_id.json");
        let mut script = mapping_script(&["aaa", "bbb", "ccc"]);
        script[0].action = Some(Box::new(move |r| {
            assert_eq!(r.form["name"], "s1");
            let attempt = match rt::locks::Lease::acquire(
                &rt::locks::canonical_map_path(&root),
                "actual provider during snapshot",
            ) {
                Ok(lease) => {
                    write(&map, &json!({"prov":"provider-cid"}));
                    drop(lease);
                    Ok(())
                }
                Err(e) => Err(e.to_string()),
            };
            seen.lock().unwrap().push(attempt);
        }));
        let server = Server::new(script);
        f.map(&server).unwrap();
        server.finish();
        let results = outcomes.lock().unwrap();
        assert_eq!(results.len(), 1);
        assert!(
            results[0]
                .as_ref()
                .unwrap_err()
                .contains("OS lock acquisition failed")
        );
        assert_eq!(f.mapping(), json!({"aaa":"s1-0","bbb":"s1-1","ccc":"s1-2"}));
        f.released();
        drop(
            rt::locks::Lease::acquire(
                &rt::locks::canonical_map_path(&f.root.0),
                "post-snapshot map availability",
            )
            .unwrap(),
        );
    }
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_the_pack_family_lock_is_held_across_the_mapping
    #[test]
    fn test_the_pack_family_lock_is_held_across_the_mapping() {
        let f = mapping_fixture();
        let hold = rt::locks::Lease::acquire(
            &rt::locks::pack_family_path(&f.root.0, &f.files.base),
            "actual concurrent rebuild",
        )
        .unwrap();
        let server = Server::new(vec![]);
        assert!(f.map(&server).is_err());
        assert!(server.finish().is_empty());
        assert!(!f.files.directory.join("ticker_to_id.json").exists());
        drop(hold);
        f.released();
    }
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_provider_top_up_is_mapped_from_its_own_image
    #[test]
    fn test_a_provider_top_up_is_mapped_from_its_own_image() {
        let f = mapping_fixture();
        std::fs::write(f.files.images.join("ddd.png"), noise("ddd")).unwrap();
        let mut state = f.saved();
        state["provider_added"] = json!(["ddd"]);
        write(&f.files.directory.join("rebuild_dedup_state.json"), &state);
        let server = Server::new(mapping_script(&["aaa", "bbb", "ccc", "ddd"]));
        f.map(&server).unwrap();
        server.finish();
        assert_eq!(f.mapping()["ddd"], "s1-3");
        assert_eq!(f.mapping()["aaa"], "s1-0");
        f.released();
    }
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_provider_top_up_without_its_image_still_refuses
    #[test]
    fn test_a_provider_top_up_without_its_image_still_refuses() {
        let f = mapping_fixture();
        let mut state = f.saved();
        state["provider_added"] = json!(["ddd"]);
        write(&f.files.directory.join("rebuild_dedup_state.json"), &state);
        let server = Server::new(vec![listing("s1", 4)]);
        assert!(f.map(&server).is_err());
        server.finish();
        assert!(!f.files.directory.join("ticker_to_id.json").exists());
        f.released();
    }
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_aliases_in_the_current_map_survive_the_rebuild
    #[test]
    fn test_aliases_in_the_current_map_survive_the_rebuild() {
        let f = mapping_fixture();
        f.write_mapping(json!({"aaa":"old-a","aaabsc":"old-a","bbb":"old-b","ccc":"old-c"}));
        let server = Server::new(mapping_script(&["aaa", "bbb", "ccc"]));
        f.map(&server).unwrap();
        server.finish();
        assert_eq!(
            f.mapping(),
            json!({"aaa":"s1-0","aaabsc":"s1-0","bbb":"s1-1","ccc":"s1-2"})
        );
        f.released();
    }
    // tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_an_alias_that_cannot_be_reapplied_stops_the_write
    #[test]
    fn test_an_alias_that_cannot_be_reapplied_stops_the_write() {
        let f = mapping_fixture();
        let before = json!({"aaa":"old-a","zzz":"old-z","zzzbsc":"old-z"});
        f.write_mapping(before.clone());
        let server = Server::new(mapping_script(&["aaa", "bbb", "ccc"]));
        let error = f.map(&server).unwrap_err();
        server.finish();
        assert!(error.contains("zzzbsc"));
        assert_eq!(f.mapping(), before);
        f.released();
    }
}

mod rebuild_locks {
    use super::*;
    use numera_runtime as rt;
    use serde_json::json;
    // tests.test_rebuild_dedup_locks.RebuildTakesThePackFamilyLock.test_the_lock_is_keyed_on_the_pack_base
    #[test]
    fn test_the_lock_is_keyed_on_the_pack_base() {
        let f = Rebuild::new(&["aaa"]);
        let family = rt::locks::pack_family_path(&f.root.0, "cryptoemoji");
        assert_eq!(family, f.root.0.join(".locks/pack_cryptoemoji.lock"));
        let hold = rt::locks::Lease::acquire(&family, "actual other coin tool").unwrap();
        let server = Server::new(vec![]);
        let error = f.run(&server).unwrap_err();
        assert!(error.contains("pack family is busy"));
        assert!(server.finish().is_empty());
        drop(hold);
        f.released();
    }
    // tests.test_rebuild_dedup_locks.ConcurrentRunsAreLockedOut.test_a_second_run_refuses_to_start
    #[test]
    fn test_a_second_run_refuses_to_start() {
        let f = Rebuild::new(&["aaa"]);
        let hold = rt::locks::Lease::acquire(
            &rt::locks::pack_family_path(&f.root.0, &f.files.base),
            "actual first rebuild",
        )
        .unwrap();
        let server = Server::new(vec![]);
        assert!(f.run(&server).is_err());
        assert!(server.finish().is_empty());
        drop(hold);
        f.released();
    }
    // tests.test_rebuild_dedup_locks.ConcurrentRunsAreLockedOut.test_the_lock_is_released_after_a_run
    #[test]
    fn test_the_lock_is_released_after_a_run() {
        let f = Rebuild::new(&["aaa"]);
        let server = Server::new(vec![
            f.mutate(
                "aaa",
                "cryptoemoji1_by_bot",
                true,
                Body::Json(ok(json!(true))),
            ),
            notify(),
        ]);
        f.run(&server).unwrap();
        let requests = server.finish();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "createNewStickerSet")
                .count(),
            1
        );
        f.released();
    }
}
mod old_pack_deletion {
    use super::*;
    use serde_json::json;
    fn fixture() -> Rebuild {
        let f = Rebuild::new(&["aaa"]);
        f.state(json!({"deleted_old":false}));
        f.old(&["old1", "old2"]);
        f
    }
    fn deletion(name: &str, rejected: bool) -> Reply {
        let mut r = reply(
            "deleteStickerSet",
            if rejected {
                reject("deleteStickerSet failed: BOT_ACCESS_DENIED")
            } else {
                ok(json!(true))
            },
        );
        let name = name.to_owned();
        r.action = Some(Box::new(move |req| assert_eq!(req.form["name"], name)));
        r
    }
    fn no_uploads(requests: &[baseline_coin_provider_support::Request]) {
        assert!(
            requests
                .iter()
                .all(|r| r.method != "createNewStickerSet" && r.method != "addStickerToSet")
        );
    }
    // tests.test_rebuild_dedup_state.OldPackDeletionMustBeConfirmed.test_a_surviving_pack_leaves_the_phase_open
    #[test]
    fn test_a_surviving_pack_leaves_the_phase_open() {
        let f = fixture();
        let server = Server::new(vec![
            deletion("old1", true),
            listing("old1", 5),
            deletion("old2", true),
            listing("old2", 5),
        ]);
        assert!(f.run(&server).is_err());
        let requests = server.finish();
        assert_eq!(f.saved()["deleted_old"], false);
        assert_eq!(
            f.saved()
                .get("deleted_old_packs")
                .cloned()
                .unwrap_or(json!([])),
            json!([])
        );
        no_uploads(&requests);
        f.released();
    }
    // tests.test_rebuild_dedup_state.OldPackDeletionMustBeConfirmed.test_partial_deletion_records_only_what_is_gone
    #[test]
    fn test_partial_deletion_records_only_what_is_gone() {
        let f = fixture();
        let server = Server::new(vec![
            deletion("old1", false),
            missing("old1"),
            deletion("old2", true),
            listing("old2", 5),
        ]);
        assert!(f.run(&server).is_err());
        let requests = server.finish();
        assert_eq!(f.saved()["deleted_old_packs"], json!(["old1"]));
        assert_eq!(f.saved()["deleted_old"], false);
        no_uploads(&requests);
        f.released();
    }
    // tests.test_rebuild_dedup_state.OldPackDeletionMustBeConfirmed.test_an_unreadable_pack_stops_the_run
    #[test]
    fn test_an_unreadable_pack_stops_the_run() {
        let f = fixture();
        let server = Server::new(vec![deletion("old1", false), unknown("old1")]);
        assert!(f.run(&server).is_err());
        let requests = server.finish();
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "deleteStickerSet")
                .map(|r| r.form["name"].as_str())
                .collect::<Vec<_>>(),
            vec!["old1"]
        );
        assert_eq!(f.saved()["deleted_old"], false);
        f.released();
    }
    // tests.test_rebuild_dedup_state.OldPackDeletionMustBeConfirmed.test_a_confirmed_deletion_completes_the_phase
    #[test]
    fn test_a_confirmed_deletion_completes_the_phase() {
        let f = fixture();
        let server = Server::new(vec![
            deletion("old1", false),
            missing("old1"),
            deletion("old2", false),
            missing("old2"),
            f.mutate(
                "aaa",
                "cryptoemoji1_by_bot",
                true,
                Body::Json(ok(json!(true))),
            ),
            notify(),
        ]);
        f.run(&server).unwrap();
        let requests = server.finish();
        assert_eq!(f.saved()["deleted_old"], true);
        assert_eq!(f.saved()["deleted_old_packs"], json!(["old1", "old2"]));
        let creates = requests
            .iter()
            .filter(|r| r.method == "createNewStickerSet")
            .collect::<Vec<_>>();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].files["file0"], noise("aaa"));
        f.released();
    }
    // tests.test_rebuild_dedup_state.OldPackDeletionMustBeConfirmed.test_deletion_is_not_repeated_after_a_resume
    #[test]
    fn test_deletion_is_not_repeated_after_a_resume() {
        let f = fixture();
        let server = Server::new(vec![
            deletion("old1", false),
            missing("old1"),
            deletion("old2", false),
            unknown("old2"),
        ]);
        assert!(f.run(&server).is_err());
        let first = server.finish();
        assert_eq!(
            first
                .iter()
                .filter(|r| r.method == "deleteStickerSet")
                .map(|r| r.form["name"].as_str())
                .collect::<Vec<_>>(),
            vec!["old1", "old2"]
        );
        f.released();
        let server = Server::new(vec![
            deletion("old2", false),
            missing("old2"),
            f.mutate(
                "aaa",
                "cryptoemoji1_by_bot",
                true,
                Body::Json(ok(json!(true))),
            ),
            notify(),
        ]);
        f.run(&server).unwrap();
        let last = server.finish();
        assert_eq!(
            first
                .iter()
                .chain(&last)
                .filter(|r| r.method == "deleteStickerSet")
                .map(|r| r.form["name"].as_str())
                .collect::<Vec<_>>(),
            vec!["old1", "old2", "old2"]
        );
        f.released();
    }
}
mod bounded_links {
    use super::*;
    use serde_json::json;
    // tests.test_rebuild_dedup_state.LinkMessagesAreBounded.test_the_link_message_retries_at_most_twice
    #[test]
    fn test_the_link_message_retries_at_most_twice() {
        let f = Rebuild::new(&["aaa"]);
        let server = Server::new(vec![
            f.mutate(
                "aaa",
                "cryptoemoji1_by_bot",
                true,
                Body::Json(ok(json!(true))),
            ),
            notify(),
        ]);
        f.run(&server).unwrap();
        let successful = server.finish();
        let messages = successful
            .iter()
            .filter(|r| r.method == "sendMessage")
            .collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].form["disable_web_page_preview"], "true");
        f.released();
        // The original spy observed retries=2. Observe the actual wire retry budget as an additional control.
        let f = Rebuild::new(&["aaa"]);
        let mut script = vec![f.mutate(
            "aaa",
            "cryptoemoji1_by_bot",
            true,
            Body::Json(ok(json!(true))),
        )];
        for _ in 0..2 {
            let mut r = notify();
            r.body = Body::Disconnect;
            script.push(r);
        }
        let server = Server::new(script);
        f.run(&server).unwrap();
        let failed = server.finish();
        assert_eq!(
            failed.iter().filter(|r| r.method == "sendMessage").count(),
            2,
            "the actual full rebuild path must exhaust after two posts"
        );
        assert_eq!(f.saved()["sent"], json!([]));
        f.released();
    }
}

mod resume_marker {
    use super::*;
    use serde_json::json;
    // tests.test_rebuild_dedup_resume.ResumeReconcilesTheMarker.test_a_create_that_landed_is_adopted_from_the_marker
    #[test]
    fn test_a_create_that_landed_is_adopted_from_the_marker() {
        let f = pending_create(&["aaa"]);
        let mut script = vec![listing(CREATED, 1)];
        download(&mut script, CREATED, 0, &noise("aaa"));
        script.push(notify());
        let server = Server::new(script);
        f.run(&server).unwrap();
        let requests = server.finish();
        assert_eq!(f.saved()["order"], json!(["aaa"]));
        assert_eq!(
            f.saved()["sets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![CREATED]
        );
        assert!(f.saved()["in_flight"].is_null());
        no_mutations(&requests);
        f.released();
    }
    // tests.test_rebuild_dedup_resume.ResumeReconcilesTheMarker.test_a_create_that_did_not_land_is_retried_once
    #[test]
    fn test_a_create_that_did_not_land_is_retried_once() {
        let f = pending_create(&["aaa"]);
        let server = Server::new(vec![
            missing(CREATED),
            f.mutate("aaa", CREATED, true, Body::Json(ok(json!(true)))),
            notify(),
        ]);
        f.run(&server).unwrap();
        let requests = server.finish();
        let creates = requests
            .iter()
            .filter(|r| r.method == "createNewStickerSet")
            .collect::<Vec<_>>();
        assert_eq!(creates.len(), 1);
        assert_eq!(creates[0].files["file0"], noise("aaa"));
        assert_eq!(f.saved()["order"], json!(["aaa"]));
        f.released();
    }
    // tests.test_rebuild_dedup_resume.ResumeReconcilesTheMarker.test_an_unreadable_set_stops_instead_of_deciding
    #[test]
    fn test_an_unreadable_set_stops_instead_of_deciding() {
        let f = pending_create(&["aaa"]);
        let before = f.saved();
        let server = Server::new(vec![me(), unknown(CREATED)]);
        let completed = f.build_cli(&server);
        assert_eq!(
            completed.code, 3,
            "{}{}",
            completed.stdout, completed.stderr
        );
        no_mutations(&server.finish());
        assert_eq!(f.saved(), before);
        f.released();
    }
}
mod add_identity {
    use super::*;
    use serde_json::json;
    // tests.test_rebuild_dedup_resume.InFlightIsResolvedByIdentity.test_a_manual_sticker_cannot_satisfy_the_marker
    #[test]
    fn test_a_manual_sticker_cannot_satisfy_the_marker() {
        let f = pending_add();
        let mut script = vec![listing("s1", 2), listing("s1", 2)];
        download(&mut script, "s1", 1, &noise("added-by-hand"));
        let server = Server::new(script);
        assert!(f.run(&server).is_err());
        let requests = server.finish();
        assert!(
            !f.saved()["order"]
                .as_array()
                .unwrap()
                .contains(&json!("aaa"))
        );
        assert_eq!(f.saved()["cursor"], 1);
        assert!(f.saved()["in_flight"].is_null());
        no_mutations(&requests);
        f.released();
    }
    // tests.test_rebuild_dedup_resume.InFlightIsResolvedByIdentity.test_our_own_image_is_recognised
    #[test]
    fn test_our_own_image_is_recognised() {
        let f = pending_add();
        let mut script = vec![listing("s1", 2), listing("s1", 2)];
        download(&mut script, "s1", 1, &noise("aaa"));
        script.push(notify());
        let server = Server::new(script);
        f.run(&server).unwrap();
        let requests = server.finish();
        assert_eq!(f.saved()["order"], json!(["seed", "aaa"]));
        assert!(f.saved()["in_flight"].is_null());
        no_mutations(&requests);
        f.released();
    }
    // tests.test_rebuild_dedup_resume.InFlightIsResolvedByIdentity.test_an_upload_that_never_landed_is_retried
    #[test]
    fn test_an_upload_that_never_landed_is_retried() {
        let f = pending_add();
        let server = Server::new(vec![
            listing("s1", 1),
            listing("s1", 1),
            listing("s1", 1),
            f.mutate("aaa", "s1", false, Body::Json(ok(json!(true)))),
            notify(),
        ]);
        f.run(&server).unwrap();
        let requests = server.finish();
        let adds = requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .collect::<Vec<_>>();
        assert_eq!(adds.len(), 1);
        assert_eq!(adds[0].files["file0"], noise("aaa"));
        assert_eq!(f.saved()["order"], json!(["seed", "aaa"]));
        f.released();
    }
    // tests.test_rebuild_dedup_resume.InFlightIsResolvedByIdentity.test_an_unreadable_set_stops_instead_of_deciding
    #[test]
    fn test_an_unreadable_set_stops_instead_of_deciding() {
        let f = pending_add();
        let before = f.saved();
        let server = Server::new(vec![me(), unknown("s1")]);
        let completed = f.build_cli(&server);
        assert_eq!(
            completed.code, 3,
            "{}{}",
            completed.stdout, completed.stderr
        );
        no_mutations(&server.finish());
        assert_eq!(f.saved(), before);
        f.released();
    }
}
mod create_identity {
    use super::*;
    use serde_json::json;
    // tests.test_rebuild_dedup_resume.CreateIsAdoptedOnlyOnImageIdentity.test_a_foreign_first_image_is_never_adopted
    #[test]
    fn test_a_foreign_first_image_is_never_adopted() {
        let f = pending_create(&["aaa", "bbb"]);
        let before = f.saved();
        let mut script = vec![me(), listing(CREATED, 1)];
        download(&mut script, CREATED, 0, &noise("someone-elses-pack"));
        let server = Server::new(script);
        let completed = f.build_cli(&server);
        assert_eq!(
            completed.code, 3,
            "{}{}",
            completed.stdout, completed.stderr
        );
        no_mutations(&server.finish());
        assert_eq!(f.saved(), before);
        f.released();
    }
    // tests.test_rebuild_dedup_resume.CreateIsAdoptedOnlyOnImageIdentity.test_a_first_sticker_that_cannot_be_read_stops_the_run
    #[test]
    fn test_a_first_sticker_that_cannot_be_read_stops_the_run() {
        let f = pending_create(&["aaa", "bbb"]);
        let before = f.saved();
        let mut script = vec![me(), listing(CREATED, 1)];
        download(&mut script, CREATED, 0, b"not an image at all");
        let server = Server::new(script);
        let completed = f.build_cli(&server);
        assert_eq!(
            completed.code, 3,
            "{}{}",
            completed.stdout, completed.stderr
        );
        no_mutations(&server.finish());
        assert_eq!(f.saved(), before);
        f.released();
    }
    // tests.test_rebuild_dedup_resume.CreateIsAdoptedOnlyOnImageIdentity.test_an_empty_set_of_that_name_is_not_treated_as_ours
    #[test]
    fn test_an_empty_set_of_that_name_is_not_treated_as_ours() {
        let f = pending_create(&["aaa", "bbb"]);
        let before = f.saved();
        let server = Server::new(vec![me(), listing(CREATED, 0)]);
        assert_eq!(f.build_cli(&server).code, 3);
        no_mutations(&server.finish());
        assert_eq!(f.saved(), before);
        f.released();
    }
    // tests.test_rebuild_dedup_resume.CreateIsAdoptedOnlyOnImageIdentity.test_our_image_beside_a_foreign_one_is_not_our_create
    #[test]
    fn test_our_image_beside_a_foreign_one_is_not_our_create() {
        let f = pending_create(&["aaa", "bbb"]);
        let before = f.saved();
        let server = Server::new(vec![me(), listing(CREATED, 2)]);
        assert_eq!(f.build_cli(&server).code, 3);
        no_mutations(&server.finish());
        let after = f.saved();
        assert_eq!(after["sets"], json!([]));
        assert_eq!(after["cursor"], before["cursor"]);
        assert_eq!(after["order"], json!([]));
        assert_eq!(after["in_flight"], before["in_flight"]);
        assert_eq!(after, before);
        f.released();
    }
    // tests.test_rebuild_dedup_resume.CreateIsAdoptedOnlyOnImageIdentity.test_our_own_image_is_still_adopted
    #[test]
    fn test_our_own_image_is_still_adopted() {
        let f = pending_create(&["aaa", "bbb"]);
        let mut script = vec![listing(CREATED, 1)];
        download(&mut script, CREATED, 0, &noise("aaa"));
        script.extend([
            listing(CREATED, 1),
            f.mutate("bbb", CREATED, false, Body::Json(ok(json!(true)))),
            notify(),
        ]);
        let server = Server::new(script);
        f.run(&server).unwrap();
        let requests = server.finish();
        assert_eq!(
            f.saved()["sets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec![CREATED]
        );
        assert_eq!(f.saved()["order"], json!(["aaa", "bbb"]));
        let adds = requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .collect::<Vec<_>>();
        assert_eq!(adds.len(), 1);
        assert_eq!(adds[0].files["file0"], noise("bbb"));
        assert!(requests.iter().all(|r| r.method != "createNewStickerSet"));
        f.released();
    }
}
mod live_unknown {
    use super::*;
    use serde_json::json;
    // tests.test_rebuild_dedup_resume.LiveStateUnknownStopsTheRun.test_cursor_order_and_marker_are_left_untouched
    #[test]
    fn test_cursor_order_and_marker_are_left_untouched() {
        let f = Rebuild::new(&["seed", "aaa", "bbb"]);
        f.state(
            json!({"sets":[{"index":1,"name":"s1","title":"T 1"}],"order":["seed"],"cursor":1}),
        );
        let before = f.saved();
        let server = Server::new(vec![me(), unknown("s1")]);
        assert_eq!(f.build_cli(&server).code, 3);
        no_mutations(&server.finish());
        let after = f.saved();
        assert_eq!(after["cursor"], before["cursor"]);
        assert_eq!(after["order"], before["order"]);
        assert!(after["in_flight"].is_null());
        assert_eq!(after, before);
        f.released();
    }
}

mod alias_commands {
    use super::*;
    use numera_runtime as rt;
    use serde_json::json;
    // tests.test_coin_ticker_map.AliasMapAmbiguity.test_ambiguous_name_is_reported_not_guessed
    #[test]
    fn test_ambiguous_name_is_reported_not_guessed() {
        let root = alias_fixture();
        let server = Server::new(vec![]);
        let completed = command(
            &root,
            &server.base,
            &server.base,
            &ids(&["coin-alias-map", "--coin-dir", root.0.to_str().unwrap()]),
            &[],
        );
        assert_eq!(
            completed.code, 0,
            "{}{}",
            completed.stdout, completed.stderr
        );
        let mapping = doc(&root.0.join("ticker_to_id.json"));
        assert!(mapping.get("ccc").is_none());
        assert!(completed.stdout.contains("AMBIGUOUS ccc"));
        assert!(completed.stdout.contains("aaa, bbb"));
        assert!(filled(&root).contains("ticker: ccc\n   premium-id:\n"));
        assert!(server.finish().is_empty());
    }
    // tests.test_coin_ticker_map.AliasMapAmbiguity.test_unique_name_is_still_applied
    #[test]
    fn test_unique_name_is_still_applied() {
        let root = alias_fixture();
        let server = Server::new(vec![]);
        let completed = command(
            &root,
            &server.base,
            &server.base,
            &ids(&["coin-alias-map", "--coin-dir", root.0.to_str().unwrap()]),
            &[],
        );
        assert_eq!(completed.code, 0);
        assert_eq!(doc(&root.0.join("ticker_to_id.json"))["eee"], "333");
        assert!(filled(&root).contains("premium-id: 333"));
        assert!(server.finish().is_empty());
    }
    // tests.test_coin_ticker_map.CanonicalMapWritersCannotLoseAnUpdate.test_a_held_lock_stops_the_map_tools_without_writing
    #[test]
    fn test_a_held_lock_stops_the_map_tools_without_writing() {
        let root = Root::new();
        write(&root.0.join("ticker_to_id.json"), &json!({"btc":"111"}));
        std::fs::write(
            root.0.join("currency-emoji-inventory.md"),
            "   ticker: btcbsc\n   premium-id:\n",
        )
        .unwrap();
        std::fs::write(root.0.join("keywords.csv"), "ticker,name\nbtc,Bitcoin\n").unwrap();
        let before = std::fs::read(root.0.join("ticker_to_id.json")).unwrap();
        let server = Server::new(vec![]);
        for tool in ["coin-alias-map", "coin-enhance-map"] {
            let hold = rt::locks::Lease::acquire(
                &rt::locks::canonical_map_path(&root.0),
                "actual other map tool",
            )
            .unwrap();
            let completed = command(
                &root,
                &server.base,
                &server.base,
                &ids(&[tool, "--coin-dir", root.0.to_str().unwrap()]),
                &[],
            );
            assert_eq!(
                completed.code, 4,
                "{tool}: {}{}",
                completed.stdout, completed.stderr
            );
            assert_eq!(
                std::fs::read(root.0.join("ticker_to_id.json")).unwrap(),
                before
            );
            drop(hold);
        }
        assert!(server.finish().is_empty());
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
