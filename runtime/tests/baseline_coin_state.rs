//! 95 authoritative original frozen-rebuild, canonical-map and coin argument identities.
#[path = "baseline_coin_providers_support/mod.rs"]
mod baseline_coin_provider_support;
#[path = "baseline_coin_state_support/mod.rs"]
mod support;
use support::*;
#[path = "baseline_coin_state_support/cli_args.rs"]
mod cli_args;

#[path = "baseline_coin_state_support/frozen_plans.rs"]
mod frozen_plans;

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

#[path = "baseline_coin_state_support/map_identity.rs"]
mod map_identity;

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
#[path = "baseline_coin_state_support/old_pack_deletion.rs"]
mod old_pack_deletion;
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
#[path = "baseline_coin_state_support/create_identity.rs"]
mod create_identity;
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

#[path = "baseline_coin_state_support/alias_commands.rs"]
mod alias_commands;

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
