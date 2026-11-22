use super::*;

// Original: tests.test_panel_plan.ThePlanSaysWhatTheOwnerDecided.test_an_emoji_dragged_into_another_pack_is_recorded_as_a_move
#[test]
#[allow(non_snake_case)]
fn panel_plan__ThePlanSaysWhatTheOwnerDecided__test_an_emoji_dragged_into_another_pack_is_recorded_as_a_move()
 {
    let p = build(
        json!([
            card("a", Some(2), true, false),
            card("b", Some(3), true, false)
        ]),
        json!({"a":3,"b":3}),
        200,
    );
    assert_eq!(
        p["moves"],
        json!([{"key":"a","label":"a","from_pack":2,"to_pack":3}])
    );
}

// Original: tests.test_panel_plan.ThePlanSaysWhatTheOwnerDecided.test_an_emoji_the_panel_has_no_opinion_about_is_absent
#[test]
#[allow(non_snake_case)]
fn panel_plan__ThePlanSaysWhatTheOwnerDecided__test_an_emoji_the_panel_has_no_opinion_about_is_absent()
 {
    let p = build(
        json!([
            card("a", Some(2), true, false),
            card("b", Some(2), true, false)
        ]),
        json!({"a":2}),
        200,
    );
    assert_eq!(p["moves"], json!([]));
    assert_eq!(p["counts"], json!({"2":1}));
}

// Original: tests.test_panel_plan.ThePlanSaysWhatTheOwnerDecided.test_a_parked_emoji_is_held_not_moved
#[test]
#[allow(non_snake_case)]
fn panel_plan__ThePlanSaysWhatTheOwnerDecided__test_a_parked_emoji_is_held_not_moved() {
    let p = build(
        json!([
            card("a", Some(3), false, false),
            card("b", Some(3), true, false)
        ]),
        json!({"a":3,"b":3}),
        200,
    );
    assert_eq!(p["held"], json!([{"key":"a","label":"a","from_pack":3}]));
    assert_eq!(p["moves"], json!([]));
    assert_eq!(p["counts"], json!({"3":1}));
}

// Original: tests.test_panel_plan.ThePlanSaysWhatTheOwnerDecided.test_the_brand_logo_is_not_part_of_the_plan
#[test]
#[allow(non_snake_case)]
fn panel_plan__ThePlanSaysWhatTheOwnerDecided__test_the_brand_logo_is_not_part_of_the_plan() {
    let p = build(
        json!([
            card("__logo_pack_1__", Some(1), true, true),
            card("a", Some(1), true, false)
        ]),
        json!({"__logo_pack_1__":2,"a":1}),
        200,
    );
    assert_eq!(p["moves"], json!([]));
    assert_eq!(p["counts"], json!({"1":1}));
}

// Original: tests.test_panel_plan.ThePlanSaysWhatTheOwnerDecided.test_a_candidate_that_was_never_live_is_counted_but_never_a_move
#[test]
#[allow(non_snake_case)]
fn panel_plan__ThePlanSaysWhatTheOwnerDecided__test_a_candidate_that_was_never_live_is_counted_but_never_a_move()
 {
    let p = build(
        json!([card("new", None, true, false)]),
        json!({"new":4}),
        200,
    );
    assert_eq!(p["moves"], json!([]));
    assert_eq!(p["counts"], json!({"4":1}));
}

// Original: tests.test_panel_plan.ThePlanSaysWhatTheOwnerDecided.test_an_over_full_pack_is_reported_rather_than_silently_fine
#[test]
#[allow(non_snake_case)]
fn panel_plan__ThePlanSaysWhatTheOwnerDecided__test_an_over_full_pack_is_reported_rather_than_silently_fine()
 {
    let p = build(
        json!(
            (0..3)
                .map(|i| card(&i.to_string(), Some(1), true, false))
                .collect::<Vec<_>>()
        ),
        json!({"0":1,"1":1,"2":1}),
        2,
    );
    assert_eq!(p["counts"], json!({"1":3}));
    assert_eq!(p["over_capacity"], json!({"1":3}));
}

// Original: tests.test_panel_plan.ThePlanSaysWhatTheOwnerDecided.test_it_round_trips_through_the_file_it_writes
#[test]
#[allow(non_snake_case)]
fn panel_plan__ThePlanSaysWhatTheOwnerDecided__test_it_round_trips_through_the_file_it_writes() {
    let r = Root::new();
    let p = build(
        json!([card("a", Some(2), true, false)]),
        json!({"a":5}),
        200,
    );
    let file = r.0.join("pack_plan.json");
    runtime::atomic::write_json(&file, &p).unwrap();
    assert_eq!(file.file_name().unwrap(), "pack_plan.json");
    assert_eq!(read(&file), p);
}

// Original: tests.test_panel_plan.TheMergeDropsDecisionsAboutEmojiThatAreGone.test_a_target_for_a_deleted_emoji_is_dropped
#[test]
#[allow(non_snake_case)]
fn panel_plan__TheMergeDropsDecisionsAboutEmojiThatAreGone__test_a_target_for_a_deleted_emoji_is_dropped()
 {
    let p = merge(
        json!({"version":1,"targets":[["gone",4],["stays",5]],"known":["gone","stays"],"excluded":[],"moves":[],"held":[]}),
        Some(json!(["here", "stays"])),
    );
    assert_eq!(
        runtime::intent::targets(&p).unwrap(),
        std::collections::BTreeMap::from([("here".into(), 1), ("stays".into(), 5)])
    );
    assert!(!p["known"].as_array().unwrap().contains(&json!("gone")));
    assert!(p["counts"].get("4").is_none());
}

// Original: tests.test_panel_plan.TheMergeDropsDecisionsAboutEmojiThatAreGone.test_without_the_catalog_it_still_only_adds
#[test]
#[allow(non_snake_case)]
fn panel_plan__TheMergeDropsDecisionsAboutEmojiThatAreGone__test_without_the_catalog_it_still_only_adds()
 {
    let p = merge(
        json!({"version":1,"targets":[["gone",4]],"known":["gone"],"excluded":[],"moves":[],"held":[]}),
        None,
    );
    assert_eq!(
        runtime::intent::targets(&p).unwrap(),
        std::collections::BTreeMap::from([("gone".into(), 4), ("here".into(), 1)])
    );
}

// Original: tests.test_panel_plan.TheMergeDropsDecisionsAboutEmojiThatAreGone.test_a_held_row_for_a_deleted_emoji_is_dropped
#[test]
#[allow(non_snake_case)]
fn panel_plan__TheMergeDropsDecisionsAboutEmojiThatAreGone__test_a_held_row_for_a_deleted_emoji_is_dropped()
 {
    let p = merge(
        json!({"version":1,"targets":[],"known":["gone"],"excluded":["gone"],"moves":[],"held":[{"key":"gone","label":"gone","from_pack":3}]}),
        Some(json!(["here"])),
    );
    assert_eq!(p["held"], json!([]));
    assert_eq!(p["excluded"], json!([]));
}

// Original: tests.test_panel_plan.TheMergeDropsDecisionsAboutEmojiThatAreGone.test_a_logo_slot_for_a_vanished_pack_is_dropped
#[test]
#[allow(non_snake_case)]
fn panel_plan__TheMergeDropsDecisionsAboutEmojiThatAreGone__test_a_logo_slot_for_a_vanished_pack_is_dropped()
 {
    let p = merge(
        json!({"version":1,"targets":[["gone",9]],"known":["gone"],"excluded":[],"moves":[],"held":[],"logo_slots":{"9":1,"1":1}}),
        Some(json!(["here"])),
    );
    assert_eq!(p["logo_slots"], json!({"1":1}));
    assert_eq!(p["over_capacity"], json!({}));
}

// Original: tests.test_panel_plan.TheMergeDropsDecisionsAboutEmojiThatAreGone.test_a_decision_about_a_live_emoji_outside_the_scope_survives
#[test]
#[allow(non_snake_case)]
fn panel_plan__TheMergeDropsDecisionsAboutEmojiThatAreGone__test_a_decision_about_a_live_emoji_outside_the_scope_survives()
 {
    let p = merge(
        json!({"version":1,"targets":[["elsewhere",6]],"known":["elsewhere"],"excluded":[],"moves":[],"held":[]}),
        Some(json!(["here", "elsewhere"])),
    );
    assert_eq!(
        runtime::intent::targets(&p).unwrap(),
        std::collections::BTreeMap::from([("elsewhere".into(), 6), ("here".into(), 1)])
    );
}

// Original: tests.test_sync_order.PlanMoves.test_an_already_correct_set_needs_no_calls
#[test]
#[allow(non_snake_case)]
fn sync_order__PlanMoves__test_an_already_correct_set_needs_no_calls() {
    assert!(
        runtime::sync_order::moves(
            &live(&["logo", "a", "b", "c"]),
            &["a", "b", "c"].map(str::to_owned),
            1
        )
        .unwrap()
        .is_empty()
    );
}

// Original: tests.test_sync_order.PlanMoves.test_it_sorts_into_the_wanted_order
#[test]
#[allow(non_snake_case)]
fn sync_order__PlanMoves__test_it_sorts_into_the_wanted_order() {
    let ids = ["logo", "c", "a", "b"];
    let moves =
        runtime::sync_order::moves(&live(&ids), &["a", "b", "c"].map(str::to_owned), 1).unwrap();
    assert_eq!(
        apply_moves(ids.map(str::to_owned).to_vec(), &moves),
        ["logo", "a", "b", "c"]
    );
}

// Original: tests.test_sync_order.PlanMoves.test_the_pinned_logo_is_never_moved
#[test]
#[allow(non_snake_case)]
fn sync_order__PlanMoves__test_the_pinned_logo_is_never_moved() {
    let m = runtime::sync_order::moves(
        &live(&["logo", "c", "b", "a"]),
        &["a", "b", "c"].map(str::to_owned),
        1,
    )
    .unwrap();
    assert!(m.iter().all(|r| r[0].as_u64().unwrap() >= 1));
    assert!(m.iter().all(|r| r[1] != "logo"));
}

// Original: tests.test_sync_order.PlanMoves.test_a_full_reversal_still_lands_exactly
#[test]
#[allow(non_snake_case)]
fn sync_order__PlanMoves__test_a_full_reversal_still_lands_exactly() {
    let desired = (0..30).map(|i| format!("e{i}")).collect::<Vec<_>>();
    let ids = std::iter::once("logo".to_owned())
        .chain(desired.iter().rev().cloned())
        .collect::<Vec<_>>();
    let refs = ids.iter().map(String::as_str).collect::<Vec<_>>();
    let m = runtime::sync_order::moves(&live(&refs), &desired, 1).unwrap();
    let wanted = std::iter::once("logo".to_owned())
        .chain(desired)
        .collect::<Vec<_>>();
    assert_eq!(apply_moves(ids, &m), wanted);
    assert!(m.len() <= 30);
}

// Original: tests.test_sync_order.ReorderIsIdempotent.test_replaying_a_plan_twice_changes_nothing_the_second_time
#[test]
#[allow(non_snake_case)]
fn sync_order__ReorderIsIdempotent__test_replaying_a_plan_twice_changes_nothing_the_second_time() {
    let ids = ["logo", "b", "c", "a"].map(str::to_owned).to_vec();
    let desired = ["a", "b", "c"].map(str::to_owned);
    let refs = ids.iter().map(String::as_str).collect::<Vec<_>>();
    let m = runtime::sync_order::moves(&live(&refs), &desired, 1).unwrap();
    let order = apply_moves(ids, &m);
    let refs = order.iter().map(String::as_str).collect::<Vec<_>>();
    assert!(
        runtime::sync_order::moves(&live(&refs), &desired, 1)
            .unwrap()
            .is_empty()
    );
}

// Original: tests.test_sync_order.TheRecordedOrderFollowsTheLiveOne.test_desired_order_is_the_panel_order_of_the_live_ids
#[test]
#[allow(non_snake_case)]
fn sync_order__TheRecordedOrderFollowsTheLiveOne__test_desired_order_is_the_panel_order_of_the_live_ids()
 {
    let r = Root::new();
    let c = sync_catalog(
        &r,
        &[
            (Some("b"), "s:b"),
            (None, "s:unpublished"),
            (Some("x"), "s:x"),
            (Some("a"), "s:a"),
        ],
    );
    let b = sync_fixture(&["logo", "a", "b"]);
    let mut rec = json!({"name":"pk1_by_bot","logo":true,"keys":[]});
    runtime::sync_order::sync(&b.telegram(), &c, &mut rec, true).unwrap();
    assert_eq!(rec["keys"], json!(["s:b", "s:a"]));
    let moves = b
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.method == "setStickerPositionInSet")
        .map(|r| r.form["sticker"].clone())
        .collect::<Vec<_>>();
    assert_eq!(moves, vec!["f-b"]);
}

// Original: tests.test_sync_order.TheRecordedOrderFollowsTheLiveOne.test_a_reorder_rewrites_the_recorded_keys
#[test]
#[allow(non_snake_case)]
fn sync_order__TheRecordedOrderFollowsTheLiveOne__test_a_reorder_rewrites_the_recorded_keys() {
    let r = Root::new();
    let c = sync_catalog(
        &r,
        &[(Some("a"), "s:a"), (Some("b"), "s:b"), (Some("c"), "s:c")],
    );
    let b = sync_fixture(&["logo", "c", "a", "b"]);
    let mut rec = json!({"name":"pk1_by_bot","logo":true,"keys":["s:c","s:a","s:b"]});
    runtime::sync_order::sync(&b.telegram(), &c, &mut rec, true).unwrap();
    assert_eq!(rec["keys"], json!(["s:a", "s:b", "s:c"]));
}

// Original: tests.test_sync_order.TheRecordedOrderFollowsTheLiveOne.test_a_stale_record_is_repaired_even_with_nothing_to_move
#[test]
#[allow(non_snake_case)]
fn sync_order__TheRecordedOrderFollowsTheLiveOne__test_a_stale_record_is_repaired_even_with_nothing_to_move()
 {
    let r = Root::new();
    let c = sync_catalog(&r, &[(Some("a"), "s:a"), (Some("b"), "s:b")]);
    let b = sync_fixture(&["logo", "a", "b"]);
    let mut rec = json!({"name":"pk1_by_bot","logo":true,"keys":["s:b","s:a"]});
    assert_eq!(
        runtime::sync_order::sync(&b.telegram(), &c, &mut rec, true).unwrap(),
        0
    );
    assert!(!b.methods().contains(&"setStickerPositionInSet".into()));
    assert_eq!(rec["keys"], json!(["s:a", "s:b"]));
}

// Original: tests.test_sync_order.TheRecordedOrderFollowsTheLiveOne.test_a_report_only_run_leaves_the_record_alone
#[test]
#[allow(non_snake_case)]
fn sync_order__TheRecordedOrderFollowsTheLiveOne__test_a_report_only_run_leaves_the_record_alone() {
    let r = Root::new();
    let c = sync_catalog(&r, &[(Some("a"), "s:a"), (Some("b"), "s:b")]);
    let b = sync_fixture(&["logo", "b", "a"]);
    let mut rec = json!({"name":"pk1_by_bot","logo":true,"keys":["s:b","s:a"]});
    runtime::sync_order::sync(&b.telegram(), &c, &mut rec, false).unwrap();
    assert_eq!(rec["keys"], json!(["s:b", "s:a"]));
    assert!(!b.methods().contains(&"setStickerPositionInSet".into()));
}

// Original: tests.test_plan_apply.CatalogUnpublish.test_forgets_one_item_in_one_family_only
#[test]
#[allow(non_snake_case)]
fn plan_apply__CatalogUnpublish__test_forgets_one_item_in_one_family_only() {
    let root = Root::new();
    let mut cat =
        runtime::catalog::Catalog::open(&root.0.join("catalog.db"), UTC, &root.0).unwrap();
    let a = "s:a";
    let b = "s:b";
    for (index, (key, cid)) in [(a, "cid-1-1"), (b, "cid-1-2")].into_iter().enumerate() {
        let file = root.write(
            &format!("item{index}.png"),
            b"not decoded by catalog unpublish",
        );
        cat.insert(&json!({"content_key":key,"fmt":"static","file_path":file,"emojis":["😀"],"keywords":[]}),UTC).unwrap();
        cat.uploaded(&json!({"content_key":key,"custom_emoji_id":cid,"base":"cryptoemoji","set_name":"cryptoemoji1_by_YourEmojiBot"}),UTC).unwrap();
    }

    cat.uploaded(&json!({"content_key":a,"custom_emoji_id":"cid-other","base":"otherbase","set_name":"other1_by_x"}),UTC).unwrap();
    assert_eq!(
        cat.dispatch(json!({"operation":"unpublish","base":"cryptoemoji","content_key":a}))
            .unwrap(),
        true
    );
    assert_eq!(
        cat.dispatch(json!({"operation":"unpublish","base":"cryptoemoji","content_key":a}))
            .unwrap(),
        false
    );
    let id = |cat: &mut runtime::catalog::Catalog, base: &str, key: &str| {
        cat.dispatch(json!({"operation":"custom_id","base":base,"content_key":key}))
            .unwrap()
    };
    assert!(id(&mut cat, "cryptoemoji", a).is_null());
    assert!(!id(&mut cat, "cryptoemoji", b).is_null());
    assert_eq!(id(&mut cat, "otherbase", a), "cid-other");
}

// Original: tests.test_plan_apply.ClientFloodCeiling.test_a_wait_over_the_ceiling_raises_without_sleeping
#[test]
#[allow(non_snake_case)]
fn plan_apply__ClientFloodCeiling__test_a_wait_over_the_ceiling_raises_without_sleeping() {
    let mut calls = 0;
    let bot = http::Bot::new(move |q| {
        assert_eq!(q.method, "deleteStickerFromSet");
        calls += 1;
        if calls == 1 {
            http::payload(
                json!({"ok":false,"description":"Too Many Requests: retry after 900","parameters":{"retry_after":900}}),
            )
        } else {
            http::ok(json!(true))
        }
    });
    let mut tg = bot.telegram();
    tg.max_flood_wait = Some(300);
    let mut waits = vec![];
    let result = tg.call_with_wait(
        "deleteStickerFromSet",
        &std::collections::BTreeMap::from([("sticker".into(), "x".into())]),
        &[],
        5,
        None::<fn() -> runtime::telegram::Result<Option<bool>>>,
        |d| waits.push(d),
    );
    assert!(matches!(
        result,
        Err(runtime::telegram::ApiError::FloodWait { seconds: 900, .. })
    ));
    assert!(waits.is_empty());
    assert_eq!(bot.methods().len(), 1);
}

// Original: tests.test_plan_apply.ClientFloodCeiling.test_a_wait_under_the_ceiling_is_still_honoured
#[test]
#[allow(non_snake_case)]
fn plan_apply__ClientFloodCeiling__test_a_wait_under_the_ceiling_is_still_honoured() {
    let mut calls = 0;
    let bot = http::Bot::new(move |q| {
        assert_eq!(q.method, "deleteStickerFromSet");
        calls += 1;
        if calls == 1 {
            http::payload(
                json!({"ok":false,"description":"Too Many Requests: retry after 20","parameters":{"retry_after":20}}),
            )
        } else {
            http::ok(json!(true))
        }
    });
    let mut tg = bot.telegram();
    tg.max_flood_wait = Some(300);
    let mut waits = vec![];
    let result = tg.call_with_wait(
        "deleteStickerFromSet",
        &std::collections::BTreeMap::from([("sticker".into(), "x".into())]),
        &[],
        5,
        None::<fn() -> runtime::telegram::Result<Option<bool>>>,
        |d| waits.push(d),
    );
    assert_eq!(result.unwrap(), true);
    assert_eq!(waits, vec![std::time::Duration::from_secs(21)]);
    assert_eq!(bot.methods().len(), 2);
}
