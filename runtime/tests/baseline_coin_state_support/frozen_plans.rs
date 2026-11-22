use super::*;

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
