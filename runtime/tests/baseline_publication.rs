//! One Rust behavioral case per original publication/state contract.
use numera_runtime as production;
#[allow(dead_code)]
#[path = "baseline_catalog_support/mod.rs"]
mod baseline_catalog_support;
#[allow(dead_code)]
#[path = "baseline_publication_support/mod.rs"]
mod support;
use production::{reconcile, resume};
use serde_json::{Value, json};
use support::*;
fn checked(f: &Fixture, sets: Vec<Value>, top: Value) -> Result<Value, String> {
    f.write_state(sets, top);
    resume::collection(read_json(&f.state_path()), "pk")
}
fn live(stickers: Vec<Value>) -> Bot {
    let mut model = Model::ordinary();
    model.sets.insert(SET.into(), stickers);
    Bot::new(model)
}
fn usual() -> Vec<Value> {
    vec![sticker("UP-item0", "c0"), sticker("UP-item1", "c1")]
}

// tests.test_build_collection_state.StateFileContract.test_plan_of_the_wrong_shape_is_rejected
#[test]
fn state_plan_of_the_wrong_shape_is_rejected() {
    let f = Fixture::new();
    std::fs::write(f.plan_path(), r#"{"static":"s:a"}"#).unwrap();
    assert!(resume::frozen(&read_json(&f.plan_path())).is_err());
}
// tests.test_build_collection_state.StateFileContract.test_state_of_another_pack_family_is_rejected
#[test]
fn state_state_of_another_pack_family_is_rejected() {
    let f = Fixture::new();
    let result = checked(&f, vec![], json!({"base":"other"})).unwrap_err();
    assert!(result.contains("other"));
}
// tests.test_build_collection_state.StateFileContract.test_state_schema_is_checked
#[test]
fn state_state_schema_is_checked() {
    let f = Fixture::new();
    assert!(
        checked(
            &f,
            vec![json!({"name":SET,"fmt":"nope","index":1})],
            json!({})
        )
        .is_err()
    );
}
// tests.test_build_collection_state.StateFileContract.test_a_wholly_consistent_state_is_accepted
#[test]
fn state_a_wholly_consistent_state_is_accepted() {
    let f = Fixture::new();
    assert_eq!(
        checked(&f, vec![f.state_set(None, 2, false)], json!({})).unwrap()["sets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
// tests.test_build_collection_state.StateFileContract.test_the_same_index_in_two_formats_is_fine
#[test]
fn state_the_same_index_in_two_formats_is_fine() {
    let f = Fixture::new();
    let first = f.state_set(Some(&f.keys[..1]), 1, false);
    let mut second = f.state_set(Some(&f.keys[1..]), 1, false);
    second["fmt"] = json!("video");
    second["name"] = json!("pkv1_by_bot");
    assert_eq!(
        checked(&f, vec![first, second], json!({})).unwrap()["sets"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
// tests.test_build_collection_state.StateFileContract.test_a_negative_live_count_is_rejected
#[test]
fn state_a_negative_live_count_is_rejected() {
    let f = Fixture::new();
    let mut set = f.state_set(Some(&[]), 0, false);
    set["live"] = json!(-1);
    assert!(checked(&f, vec![set], json!({})).is_err());
}
// tests.test_build_collection_state.StateFileContract.test_a_live_count_above_telegrams_cap_is_rejected
#[test]
fn state_a_live_count_above_telegrams_cap_is_rejected() {
    let f = Fixture::new();
    assert!(checked(&f, vec![f.state_set(Some(&[]), 201, false)], json!({})).is_err());
}
// tests.test_build_collection_state.StateFileContract.test_more_recorded_keys_than_live_stickers_is_rejected
#[test]
fn state_more_recorded_keys_than_live_stickers_is_rejected() {
    let f = Fixture::new();
    assert!(
        checked(&f, vec![f.state_set(None, 1, false)], json!({}))
            .unwrap_err()
            .contains("records 2")
    );
}
// tests.test_build_collection_state.StateFileContract.test_the_logo_slot_counts_towards_that_bound
#[test]
fn state_the_logo_slot_counts_towards_that_bound() {
    let f = Fixture::new();
    assert!(checked(&f, vec![f.state_set(None, 2, true)], json!({})).is_err());
}
// tests.test_build_collection_state.StateFileContract.test_one_emoji_recorded_twice_in_a_set_is_rejected
#[test]
fn state_one_emoji_recorded_twice_in_a_set_is_rejected() {
    let f = Fixture::new();
    let repeated = vec![f.keys[0].clone(), f.keys[0].clone()];
    assert!(checked(&f, vec![f.state_set(Some(&repeated), 2, false)], json!({})).is_err());
}
// tests.test_build_collection_state.StateFileContract.test_one_emoji_recorded_in_two_sets_is_rejected
#[test]
fn state_one_emoji_recorded_in_two_sets_is_rejected() {
    let f = Fixture::new();
    let first = f.state_set(Some(&f.keys[..1]), 1, false);
    let mut second = first.clone();
    second["name"] = json!(SET2);
    second["index"] = json!(2);
    assert!(checked(&f, vec![first, second], json!({})).is_err());
}
// tests.test_build_collection_state.StateFileContract.test_two_sets_sharing_a_name_are_rejected
#[test]
fn state_two_sets_sharing_a_name_are_rejected() {
    let f = Fixture::new();
    let first = f.state_set(Some(&f.keys[..1]), 1, false);
    let mut second = f.state_set(Some(&f.keys[1..]), 1, false);
    second["index"] = json!(2);
    assert!(checked(&f, vec![first, second], json!({})).is_err());
}
// tests.test_build_collection_state.StateFileContract.test_a_repeated_or_backwards_index_is_rejected
#[test]
fn state_a_repeated_or_backwards_index_is_rejected() {
    let f = Fixture::new();
    for index in [1, 0] {
        let mut first = f.state_set(Some(&f.keys[..1]), 1, false);
        first["index"] = json!(2);
        let mut second = f.state_set(Some(&f.keys[1..]), 1, false);
        second["name"] = json!(SET2);
        second["index"] = json!(index);
        assert!(
            checked(&f, vec![first, second], json!({})).is_err(),
            "{index}"
        );
    }
}
// tests.test_build_collection_state.StateFileContract.test_malformed_field_types_are_rejected
#[test]
fn state_malformed_field_types_are_rejected() {
    let f = Fixture::new();
    for over in [
        json!({"keys":"not-a-list"}),
        json!({"keys":[1,2]}),
        json!({"logo":"yes"}),
        json!({"live":"two"}),
        json!({"live":true}),
        json!({"index":true}),
        json!({"title":""}),
        json!({"name":""}),
    ] {
        let mut set = f.state_set(None, 2, false);
        for (k, v) in over.as_object().unwrap() {
            set[k] = v.clone();
        }
        assert!(checked(&f, vec![set], json!({})).is_err(), "{over}");
    }
}
// tests.test_build_collection_state.StateFileContract.test_malformed_sent_and_skipped_entries_are_rejected
#[test]
fn state_malformed_sent_and_skipped_entries_are_rejected() {
    let f = Fixture::new();
    for top in [
        json!({"sent":[null]}),
        json!({"sent":[""]}),
        json!({"skipped":[{"k":1}]}),
    ] {
        assert!(checked(&f, vec![], top.clone()).is_err(), "{top}");
    }
}

// tests.test_build_collection_state.LiveSetDrift.test_unknown_live_state_aborts_instead_of_guessing
#[test]
fn state_unknown_live_state_aborts_instead_of_guessing() {
    let f = Fixture::new();
    let bot = live(usual());
    bot.edit(|m| {
        m.unknown.insert(SET.into());
    });
    let mut set = f.state_set(None, 2, false);
    assert!(
        f.reconcile(&bot, &Images::default(), &mut set)
            .unwrap_err()
            .contains("live state unknown")
    );
    assert_eq!(set["live"], 2);
}
// tests.test_build_collection_state.LiveSetDrift.test_deleted_set_is_reported_not_treated_as_the_recorded_count
#[test]
fn state_deleted_set_is_reported_not_treated_as_the_recorded_count() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    let mut set = f.state_set(None, 2, false);
    assert!(
        f.reconcile(&bot, &Images::default(), &mut set)
            .unwrap_err()
            .contains("no longer exists")
    );
}
// tests.test_build_collection_state.LiveSetDrift.test_shrunk_set_is_drift
#[test]
fn state_shrunk_set_is_drift() {
    let f = Fixture::new();
    let bot = live(vec![sticker("UP-item0", "c0")]);
    assert!(
        f.reconcile(&bot, &Images::default(), &mut f.state_set(None, 2, false))
            .is_err()
    );
}
// tests.test_build_collection_state.LiveSetDrift.test_reordered_set_is_drift
#[test]
fn state_reordered_set_is_drift() {
    let f = Fixture::new();
    let bot = live(vec![sticker("UP-item1", "c1"), sticker("UP-item0", "c0")]);
    assert!(
        f.reconcile(&bot, &Images::default(), &mut f.state_set(None, 2, false))
            .unwrap_err()
            .contains("position 0")
    );
}
// tests.test_build_collection_state.LiveSetDrift.test_insert_inside_the_recorded_prefix_is_drift
#[test]
fn state_insert_inside_the_recorded_prefix_is_drift() {
    let f = Fixture::new();
    let bot = live(vec![
        sticker("UP-item0", "c0"),
        sticker("MANUAL", "cx"),
        sticker("UP-item1", "c1"),
    ]);
    let mut set = f.state_set(Some(&f.keys[..1]), 2, false);
    assert!(f.reconcile(&bot, &Images::default(), &mut set).is_err());
    assert_eq!(set["keys"], json!(&f.keys[..1]));
}
// tests.test_build_collection_state.LiveSetDrift.test_the_same_emoji_live_twice_is_drift
#[test]
fn state_the_same_emoji_live_twice_is_drift() {
    let f = Fixture::new();
    let bot = live(vec![
        sticker("UP-item0", "c0"),
        sticker("UP-item1", "c1"),
        sticker("UP-item0", "c2"),
    ]);
    assert!(
        f.reconcile(&bot, &Images::default(), &mut f.state_set(None, 2, false))
            .unwrap_err()
            .contains("twice")
    );
}
// tests.test_build_collection_state.LiveSetDrift.test_a_sticker_appended_by_the_owner_only_stops_attribution
#[test]
fn state_a_sticker_appended_by_the_owner_only_stops_attribution() {
    let f = Fixture::new();
    let bot = live(vec![
        sticker("UP-item0", "c0"),
        sticker("UP-item1", "c1"),
        sticker("OWNER", "cx"),
    ]);
    let mut set = f.state_set(None, 2, false);
    f.reconcile(&bot, &Images::default(), &mut set).unwrap();
    assert_eq!(set["live"], 3);
    assert_eq!(set["keys"], json!(f.keys));
}
// tests.test_build_collection_state.LiveSetDrift.test_replaced_sticker_is_drift
#[test]
fn state_replaced_sticker_is_drift() {
    let f = Fixture::new();
    let bot = live(vec![sticker("UP-item0", "c0"), sticker("UP-item0", "cX")]);
    assert!(
        f.reconcile(&bot, &Images::default(), &mut f.state_set(None, 2, false))
            .is_err()
    );
}
// tests.test_build_collection_state.LiveSetDrift.test_matching_manifest_reconciles_the_tail
#[test]
fn state_matching_manifest_reconciles_the_tail() {
    let f = Fixture::new();
    let bot = live(usual());
    let mut set = f.state_set(Some(&f.keys[..1]), 2, false);
    f.reconcile(&bot, &Images::default(), &mut set).unwrap();
    assert_eq!(set["live"], 2);
    assert_eq!(set["keys"], json!(f.keys));
    assert!(f.published(&f.keys[1]));
}
// tests.test_build_collection_state.LiveSetDrift.test_cids_are_never_written_from_a_drifted_position
#[test]
fn state_cids_are_never_written_from_a_drifted_position() {
    let f = Fixture::new();
    let bot = live(vec![
        sticker("UP-item1", "wrong-0"),
        sticker("UP-item0", "wrong-1"),
    ]);
    assert!(
        f.reconcile(&bot, &Images::default(), &mut f.state_set(None, 2, false))
            .is_err()
    );
    for key in &f.keys {
        assert!(f.catalog().get(key).unwrap().unwrap()["custom_emoji_id"].is_null());
    }
}
// tests.test_build_collection_state.LiveSetDrift.test_cids_are_written_when_the_manifest_matches
#[test]
fn state_cids_are_written_when_the_manifest_matches() {
    let f = Fixture::new();
    let bot = live(usual());
    f.reconcile(&bot, &Images::default(), &mut f.state_set(None, 2, false))
        .unwrap();
    assert_eq!(
        f.catalog().get(&f.keys[0]).unwrap().unwrap()["custom_emoji_id"],
        "c0"
    );
    assert_eq!(
        f.catalog().get(&f.keys[1]).unwrap().unwrap()["custom_emoji_id"],
        "c1"
    );
}

// tests.test_build_collection_state.ForeignIdentityOnARecordedPosition.test_a_foreign_sticker_on_a_read_back_position_is_drift
#[test]
fn state_a_foreign_sticker_on_a_read_back_position_is_drift() {
    let f = Fixture::new();
    f.read_back(&f.keys);
    let bot = live(vec![
        sticker("NEVER-SEEN", "foreign-cid"),
        sticker("UP-item1", "c1"),
    ]);
    assert!(
        f.reconcile(&bot, &Images::default(), &mut f.state_set(None, 2, false))
            .unwrap_err()
            .contains("position 0")
    );
    assert_eq!(f.cid(&f.keys[0]), "c0");
}
// tests.test_build_collection_state.ForeignIdentityOnARecordedPosition.test_reconcile_also_refuses_a_replaced_recorded_position
#[test]
fn state_reconcile_also_refuses_a_replaced_recorded_position() {
    let f = Fixture::new();
    f.read_back(&f.keys);
    let bot = live(vec![
        sticker("UP-item0", "c0"),
        sticker("NEVER-SEEN", "foreign-cid"),
    ]);
    assert!(
        f.reconcile(&bot, &Images::default(), &mut f.state_set(None, 2, false))
            .is_err()
    );
}
// tests.test_build_collection_state.ForeignIdentityOnARecordedPosition.test_content_resolution_rescues_a_re_uploaded_identical_picture
#[test]
fn state_content_resolution_rescues_a_re_uploaded_identical_picture() {
    let f = Fixture::new();
    f.read_back(&f.keys);
    let bot = live(vec![
        sticker("RE-UPLOADED", "c9"),
        sticker("UP-item1", "c1"),
    ]);
    bot.edit(|m| {
        m.bodies
            .insert("RE-UPLOADED".into(), b"whatever telegram returned".to_vec());
    });
    let media = Images {
        forced_key: Some(f.keys[0].clone()),
        fail: None,
    };
    f.reconcile(&bot, &media, &mut f.state_set(None, 2, false))
        .unwrap();
    assert_eq!(bot.edit(|m| m.downloaded.len()), 1);
}
// tests.test_build_collection_state.ForeignIdentityOnARecordedPosition.test_a_fresh_upload_whose_identity_was_recorded_is_not_drift
#[test]
fn state_a_fresh_upload_whose_identity_was_recorded_is_not_drift() {
    let f = Fixture::new();
    assert_eq!(f.keys.len(), 2);
    {
        let mut cat = f.catalog();
        for (fuid, key) in ["BRAND-NEW-0", "BRAND-NEW-1"].iter().zip(&f.keys) {
            cat.record_seen(fuid, key).unwrap();
        }
    }
    let bot = live(vec![
        sticker("BRAND-NEW-0", "c0"),
        sticker("BRAND-NEW-1", "c1"),
    ]);
    f.reconcile(&bot, &Images::default(), &mut f.state_set(None, 2, false))
        .unwrap();
    assert_eq!(f.cid(&f.keys[0]), "c0");
}
// tests.test_build_collection_state.ForeignIdentityOnARecordedPosition.test_an_unidentifiable_fresh_position_never_falls_back_to_order
#[test]
fn state_an_unidentifiable_fresh_position_never_falls_back_to_order() {
    let f = Fixture::new();
    let bot = live(vec![sticker("FRESH-1", "c1"), sticker("FRESH-0", "c0")]);
    assert!(
        f.reconcile(&bot, &Images::default(), &mut f.state_set(None, 2, false))
            .is_err()
    );
    assert!(f.cid(&f.keys[0]).is_null());
    assert!(f.cid(&f.keys[1]).is_null());
}
// tests.test_build_collection_state.ForeignIdentityOnARecordedPosition.test_content_resolution_failure_is_drift_not_a_positional_guess
#[test]
fn state_content_resolution_failure_is_drift_not_a_positional_guess() {
    let f = Fixture::new();
    let bot = live(vec![sticker("FRESH-0", "c0"), sticker("FRESH-1", "c1")]);
    bot.edit(|m| {
        m.undownloadable.insert("FRESH-0".into());
    });
    assert!(
        f.reconcile(&bot, &Images::default(), &mut f.state_set(None, 2, false))
            .is_err()
    );
    assert_eq!(bot.edit(|m| m.downloaded.len()), 1);
}

// tests.test_build_collection_state.UnattributedTail.test_a_tail_we_cannot_attribute_closes_the_set
#[test]
fn state_a_tail_we_cannot_attribute_closes_the_set() {
    let f = Fixture::new();
    let bot = live(vec![sticker("UP-item0", "c0"), sticker("OWNER", "cx")]);
    let mut set = f.state_set(Some(&f.keys[..1]), 1, false);
    f.reconcile(&bot, &Images::default(), &mut set).unwrap();
    assert_eq!(set["live"], 2);
    assert_eq!(set["keys"], json!(&f.keys[..1]));
    assert!(!reconcile::open(&set));
}
// tests.test_build_collection_state.UnattributedTail.test_a_tail_whose_download_fails_is_refused_not_closed
#[test]
fn state_a_tail_whose_download_fails_is_refused_not_closed() {
    let f = Fixture::new();
    let bot = live(vec![sticker("UP-item0", "c0"), sticker("UNSEEN", "cx")]);
    bot.edit(|m| {
        m.undownloadable.insert("UNSEEN".into());
    });
    let mut set = f.state_set(Some(&f.keys[..1]), 1, false);
    assert!(
        f.reconcile(&bot, &Images::default(), &mut set)
            .unwrap_err()
            .contains("could not be examined")
    );
    assert_eq!(bot.edit(|m| m.downloaded.len()), 1);
    assert_eq!(set["keys"], json!(&f.keys[..1]));
}
// tests.test_build_collection_state.UnattributedTail.test_an_unreadable_sticker_behind_a_foreign_one_is_not_read_as_absent
#[test]
fn state_an_unreadable_sticker_behind_a_foreign_one_is_not_read_as_absent() {
    let f = Fixture::new();
    let bot = live(vec![
        sticker("UP-item0", "c0"),
        sticker("OWNER", "cx"),
        sticker("FRESH-item1", "c2"),
    ]);
    bot.edit(|m| {
        m.undownloadable.insert("FRESH-item1".into());
    });
    let mut set = f.state_set(Some(&f.keys[..1]), 1, false);
    assert!(
        f.reconcile(&bot, &Images::default(), &mut set)
            .unwrap_err()
            .contains("could not be examined")
    );
    assert_eq!(set["keys"], json!(&f.keys[..1]));
}
// tests.test_build_collection_state.UnattributedTail.test_a_fully_attributed_set_stays_open
#[test]
fn state_a_fully_attributed_set_stays_open() {
    let f = Fixture::new();
    let bot = live(usual());
    let mut set = f.state_set(Some(&f.keys[..1]), 1, false);
    f.reconcile(&bot, &Images::default(), &mut set).unwrap();
    assert!(reconcile::open(&set));
}
fn unrecorded(f: &Fixture, bot: &Bot, i: usize) -> Value {
    let fuid = format!("FRESH-item{i}");
    bot.edit(|m| {
        m.bodies.insert(
            fuid.clone(),
            std::fs::read(f.data.join(format!("media/static/item{i}.png"))).unwrap(),
        );
    });
    sticker(&fuid, &format!("fresh-c{i}"))
}
// tests.test_build_collection_state.OurEmojiBehindAForeignSticker.test_an_unrecorded_upload_behind_a_foreign_sticker_is_refused
#[test]
fn state_an_unrecorded_upload_behind_a_foreign_sticker_is_refused() {
    let f = Fixture::new();
    let bot = live(vec![
        sticker("UP-item0", "c0"),
        sticker("OWNER", "owner-cid"),
    ]);
    let item = unrecorded(&f, &bot, 1);
    bot.edit(|m| m.sets.get_mut(SET).unwrap().push(item));
    let mut set = f.state_set(Some(&f.keys[..1]), 1, false);
    let error = f.reconcile(&bot, &Images::default(), &mut set).unwrap_err();
    assert!(!f.published(&f.keys[1]));
    assert!(error.contains(&f.keys[1]));
    assert!(error.contains("position 2"));
    assert_eq!(set["keys"], json!(&f.keys[..1]));
}
// tests.test_build_collection_state.OurEmojiBehindAForeignSticker.test_the_look_behind_stops_at_the_first_of_ours
#[test]
fn state_the_look_behind_stops_at_the_first_of_ours() {
    let f = Fixture::new();
    let bot = live(vec![
        sticker("UP-item0", "c0"),
        sticker("OWNER", "owner-cid"),
    ]);
    let item = unrecorded(&f, &bot, 1);
    bot.edit(|m| {
        m.sets
            .get_mut(SET)
            .unwrap()
            .extend([item, sticker("OWNER2", "owner-cid2")])
    });
    assert!(
        f.reconcile(
            &bot,
            &Images::default(),
            &mut f.state_set(Some(&f.keys[..1]), 1, false)
        )
        .is_err()
    );
    assert_eq!(bot.edit(|m| m.downloaded.clone()), ["OWNER", "FRESH-item1"]);
}
// tests.test_build_collection_state.OurEmojiBehindAForeignSticker.test_a_foreign_sticker_at_the_very_end_still_stops_quietly
#[test]
fn state_a_foreign_sticker_at_the_very_end_still_stops_quietly() {
    let f = Fixture::new();
    let bot = live(vec![
        sticker("UP-item0", "c0"),
        sticker("UP-item1", "c1"),
        sticker("OWNER", "owner-cid"),
    ]);
    let mut set = f.state_set(None, 2, false);
    f.reconcile(&bot, &Images::default(), &mut set).unwrap();
    assert_eq!(set["live"], 3);
    assert_eq!(set["keys"], json!(f.keys));
    assert!(!reconcile::open(&set));
    assert_eq!(bot.edit(|m| m.downloaded.clone()), ["OWNER"]);
}

// tests.test_publish_dedup.PublishDedupTest.test_unrecorded_live_upload_is_reconciled_not_reuploaded
#[test]
fn dedup_unrecorded_live_upload_is_reconciled_not_reuploaded() {
    let mut f = Fixture::empty();
    seed(&mut f, 2, "SRC-");
    let mut model = Model::ordinary();
    model.prefix = "FU-".into();
    model
        .sets
        .insert(SET.into(), vec![sticker("SRC-item0", "cid-old-0")]);
    let bot = Bot::new(model);
    let mut state = f.state();
    state["sets"] = json!([f.state_set(Some(&[]), 1, false)]);
    f.publish(&bot, &Images::default(), &mut state, &f.options())
        .unwrap();
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 2);
        assert_eq!(m.adds, ["item1"]);
    });
    assert_eq!(state["sets"][0]["keys"], json!(f.keys));
    let row = f.catalog().get(&f.keys[0]).unwrap().unwrap();
    assert_eq!(row["uploaded"], true);
    assert_eq!(row["custom_emoji_id"], "cid-old-0");
}
// tests.test_publish_dedup.PublishDedupTest.test_ambiguous_add_is_reconciled_in_run_without_duplicate
#[test]
fn dedup_ambiguous_add_is_reconciled_in_run_without_duplicate() {
    let mut f = Fixture::empty();
    seed(&mut f, 2, "SRC-");
    let mut model = Model::ordinary();
    model.fuids.insert("item0".into(), "UP-item0".into());
    model.fuids.insert("item1".into(), "SRC-item1".into());
    model.mutation_mode = "after_apply".into();
    let bot = Bot::new(model);
    let mut state = f.state();
    f.publish(&bot, &Images::default(), &mut state, &f.options())
        .unwrap();
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 2);
        assert_eq!(m.adds, ["item1"]);
    });
    assert_eq!(state["sets"][0]["keys"], json!(f.keys));
    assert_eq!(
        f.catalog().get(&f.keys[1]).unwrap().unwrap()["uploaded"],
        true
    );
}
// tests.test_publish_dedup.PublishDedupTest.test_occupied_set_is_adopted_after_lost_state
#[test]
fn dedup_occupied_set_is_adopted_after_lost_state() {
    let mut f = Fixture::empty();
    seed(&mut f, 2, "SRC-");
    let mut model = Model::ordinary();
    model
        .sets
        .insert(SET.into(), vec![sticker("SRC-item0", "cid-a")]);
    let bot = Bot::new(model);
    let mut state = f.state();
    f.publish(&bot, &Images::default(), &mut state, &f.options())
        .unwrap();
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 2);
        assert_eq!(m.adds, ["item1"]);
    });
    assert_eq!(state["sets"][0]["keys"], json!(f.keys));
    assert_eq!(
        f.catalog().get(&f.keys[0]).unwrap().unwrap()["uploaded"],
        true
    );
}
// tests.test_publish_dedup.PublishDedupTest.test_publish_records_uploaded_fuids_so_fetch_skips_download
#[test]
fn dedup_publish_records_uploaded_fuids_so_fetch_skips_download() {
    let mut f = Fixture::empty();
    seed(&mut f, 2, "SRC-");
    let bot = Bot::new(Model::ordinary());
    let mut state = f.state();
    f.publish(&bot, &Images::default(), &mut state, &f.options())
        .unwrap();
    assert_eq!(f.seen("UP-item0"), f.keys[0]);
    assert_eq!(f.seen("UP-item1"), f.keys[1]);
    assert_eq!(f.seen("SRC-item0"), f.keys[0]);
}
fn closed_fixture() -> (Fixture, Bot, Value) {
    let mut f = Fixture::empty();
    seed(&mut f, 2, "SRC-");
    let bot = live(vec![
        sticker("SRC-item0", "cid-ours"),
        sticker("FOREIGN-1", "cid-foreign"),
    ]);
    let mut state = f.state();
    state["sets"] = json!([f.state_set(Some(&[]), 2, false)]);
    (f, bot, state)
}
// tests.test_publish_invariants.AClosedSetIsNeverAppendedTo.test_no_pending_item_is_added_to_a_set_with_a_foreign_sticker
#[test]
fn invariant_no_pending_item_is_added_to_a_set_with_a_foreign_sticker() {
    let (f, bot, mut state) = closed_fixture();
    f.publish(&bot, &Images::default(), &mut state, &f.options())
        .unwrap();
    assert_eq!(bot.edit(|m| m.sets[SET].len()), 2);
}
// tests.test_publish_invariants.AClosedSetIsNeverAppendedTo.test_the_run_continues_in_a_fresh_set_instead
#[test]
fn invariant_the_run_continues_in_a_fresh_set_instead() {
    let (f, bot, mut state) = closed_fixture();
    f.publish(&bot, &Images::default(), &mut state, &f.options())
        .unwrap();
    bot.edit(|m| {
        let fresh = m
            .sets
            .iter()
            .filter(|(name, _)| name.as_str() != SET)
            .collect::<Vec<_>>();
        assert!(!fresh.is_empty());
        assert!(fresh.iter().map(|(_, set)| set.len()).sum::<usize>() >= 1);
    });
}
// tests.test_publish_invariants.AClosedSetIsNeverAppendedTo.test_into_pack_refuses_rather_than_switching_targets
#[test]
fn invariant_into_pack_refuses_rather_than_switching_targets() {
    let (f, bot, mut state) = closed_fixture();
    let mut options = f.options();
    options.into_pack = Some(1);
    assert!(
        f.publish(&bot, &Images::default(), &mut state, &options)
            .unwrap_err()
            .contains("unattributed")
    );
}
fn blank(f: &Fixture) -> std::path::PathBuf {
    let path = f.data.join("blank.png");
    let mut rgba = image::RgbaImage::new(100, 100);
    rgba.put_pixel(5, 5, image::Rgba([255, 255, 255, 255]));
    rgba.save(&path).unwrap();
    path
}
// tests.test_publish_invariants.MixedFamiliesStillCheckEachItemsOwnFormat.test_a_blank_static_is_caught_when_asked_about_static
#[test]
fn invariant_a_blank_static_is_caught_when_asked_about_static() {
    let f = Fixture::empty();
    let path = blank(&f);
    assert!(
        !runtime()
            .block_on(production::publication_media::usable(
                &f.data, &path, "static"
            ))
            .unwrap()
    );
}
// tests.test_publish_invariants.MixedFamiliesStillCheckEachItemsOwnFormat.test_the_family_format_answers_yes_to_everything
#[test]
fn invariant_the_family_format_answers_yes_to_everything() {
    let f = Fixture::empty();
    let path = blank(&f);
    assert!(
        runtime()
            .block_on(production::publication_media::usable(
                &f.data, &path, "mixed"
            ))
            .unwrap()
    );
}
// tests.test_publish_invariants.MixedFamiliesStillCheckEachItemsOwnFormat.test_the_publisher_asks_about_the_item_not_the_family
#[test]
fn invariant_the_publisher_asks_about_the_item_not_the_family() {
    let mut f = Fixture::empty();
    let path = blank(&f);
    let key = key(&path);
    f.catalog().insert(&json!({"content_key":key,"fmt":"static","file_path":path,"emojis":["😀"],"keywords":["blank"]}),UTC).unwrap();
    f.keys.push(key.clone());
    let bot = Bot::new(Model::ordinary());
    let mut state = f.state();
    let mut options = f.options();
    options.fmt = "mixed".into();
    f.publish(&bot, &Images::default(), &mut state, &options)
        .unwrap();
    assert!(bot.edit(|m| m.uploaded.is_empty()));
    assert!(state["skipped"].as_array().unwrap().contains(&json!(key)));
}

fn retry_fixture(mode: &str) -> (Fixture, Bot, std::path::PathBuf) {
    let f = Fixture::empty();
    let path = png(&f.data.join("e.png"), [200, 30, 30, 255]);
    let mut model = Model::ordinary();
    model.sets.insert(
        "pack1".into(),
        vec![json!({"file_unique_id":"FU-seed","file_id":"FID-seed"})],
    );
    if mode == "before_apply" {
        model.transport_adds = 1;
    } else {
        model.mutation_mode = mode.into();
    }
    (f, Bot::new(model), path)
}
fn retry_add(
    f: &Fixture,
    bot: &Bot,
    path: &std::path::Path,
    before: Option<u64>,
) -> Result<Value, production::telegram::ApiError> {
    let tg = bot.telegram();
    let rt = runtime();
    let media = Images::default();
    let request = json!({"create":false,"user_id":1,"name":"pack1","source":path,"format":"static","emojis":["😀"],"keywords":["kw"],"expected_before":before});
    production::telegram_mutation::mutate_with_wait(
        &tg,
        &request,
        |tg, sticker, source, fmt| {
            let effect = production::upload_effect::UploadEffect {
                tg,
                media: &media,
                data: &f.data,
                runtime: &rt,
            };
            Ok(effect
                .same(sticker, std::path::Path::new(source), fmt)
                .unwrap_or(None))
        },
        |_| {},
    )
}
// tests.test_publish_dedup.VerifiedRetryTest.test_applied_then_network_error_is_not_resent
#[test]
fn retry_applied_then_network_error_is_not_resent() {
    let (f, bot, path) = retry_fixture("after_apply");
    retry_add(&f, &bot, &path, Some(1)).unwrap();
    bot.edit(|m| {
        assert_eq!(m.adds.len(), 1);
        assert_eq!(m.sets["pack1"].len(), 2);
    });
}
// tests.test_publish_dedup.VerifiedRetryTest.test_a_foreign_sticker_landing_is_not_read_as_our_upload
#[test]
fn retry_a_foreign_sticker_landing_is_not_read_as_our_upload() {
    let (f, bot, path) = retry_fixture("foreign_only");
    retry_add(&f, &bot, &path, Some(1)).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    bot.edit(|m| {
        assert_eq!(m.adds.len(), 2);
        let ours = m.sets["pack1"]
            .iter()
            .filter(|st| m.bodies.get(st["file_unique_id"].as_str().unwrap()) == Some(&bytes))
            .count();
        assert_eq!(ours, 1);
    });
}
// tests.test_publish_dedup.VerifiedRetryTest.test_an_unverifiable_new_sticker_is_never_claimed_as_ours
#[test]
fn retry_an_unverifiable_new_sticker_is_never_claimed_as_ours() {
    let (f, bot, path) = retry_fixture("after_apply");
    bot.edit(|m| {
        m.undownloadable.insert("UP-e".into());
    });
    assert!(matches!(
        retry_add(&f, &bot, &path, Some(1)),
        Err(production::telegram::ApiError::Ambiguous(_))
    ));
    assert_eq!(bot.edit(|m| m.adds.len()), 1);
}
// tests.test_publish_dedup.VerifiedRetryTest.test_not_applied_network_error_is_retried_with_full_body
#[test]
fn retry_not_applied_network_error_is_retried_with_full_body() {
    let (f, bot, path) = retry_fixture("before_apply");
    retry_add(&f, &bot, &path, Some(1)).unwrap();
    bot.edit(|m| {
        assert_eq!(m.adds.len(), 2);
        assert_eq!(m.sets["pack1"].len(), 2);
        let last = m
            .requests
            .iter()
            .rev()
            .find(|r| r.path.ends_with("/addStickerToSet"))
            .unwrap();
        assert_eq!(last.files["file0"].1, std::fs::read(&path).unwrap());
    });
}
// tests.test_publish_dedup.VerifiedRetryTest.test_unknown_live_state_raises_ambiguous
#[test]
fn retry_unknown_live_state_raises_ambiguous() {
    let (f, bot, path) = retry_fixture("after_apply");
    bot.edit(|m| {
        m.unknown.insert("pack1".into());
    });
    assert!(matches!(
        retry_add(&f, &bot, &path, Some(1)),
        Err(production::telegram::ApiError::Ambiguous(_))
    ));
    assert_eq!(bot.edit(|m| m.adds.len()), 1);
}
// tests.test_publish_dedup.VerifiedRetryTest.test_without_expected_before_keeps_legacy_retry
#[test]
fn retry_without_expected_before_stops_on_ambiguous_transport() {
    let (f, bot, path) = retry_fixture("before_apply");
    let before = std::fs::read(f.data.join("catalog.db")).unwrap();
    let live_before = bot.edit(|m| m.sets["pack1"].clone());
    let error = retry_add(&f, &bot, &path, None).unwrap_err();
    assert!(matches!(
        error,
        production::telegram::ApiError::Ambiguous(_)
    ));
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("live state unknown"), "{diagnostic}");
    assert!(!diagnostic.contains("test-only-token"));
    assert!(diagnostic.contains("[REDACTED]"));
    bot.edit(|m| {
        assert_eq!(m.adds.len(), 1);
        assert_eq!(m.sets["pack1"], live_before);
        assert!(m.downloaded.is_empty());
    });
    assert_eq!(std::fs::read(f.data.join("catalog.db")).unwrap(), before);
}

// tests.test_publish_cli.PublishThroughMain.test_the_manifest_counts_the_pack_not_the_catalog_rows
#[test]
fn cli_the_manifest_counts_the_pack_not_the_catalog_rows() {
    let f = Fixture::new();
    let mut cat = f.catalog();
    production::collection_manifest::write(
        &f.data,
        &mut cat,
        &json!({"name":"pks1_by_bot","title":"Pack 1","fmt":"static","keys":f.keys,"logo":true}),
        "pk",
    )
    .unwrap();
    let text = std::fs::read_to_string(f.data.join("manifests/pks1_by_bot.md")).unwrap();
    assert!(text.contains(&format!("{} emoji", f.keys.len() + 1)));
    assert!(!text.contains(&format!("{} emoji", f.keys.len())));
    assert!(text.contains("| 1 | brand logo |"));
    assert!(text.contains("| 2 |"));
}
// tests.test_publish_cli.PublishThroughMain.test_a_set_without_a_logo_still_counts_plainly
#[test]
fn cli_a_set_without_a_logo_still_counts_plainly() {
    let f = Fixture::new();
    let mut cat = f.catalog();
    production::collection_manifest::write(
        &f.data,
        &mut cat,
        &json!({"name":"nologo_by_bot","title":"No logo","fmt":"static","keys":f.keys}),
        "pk",
    )
    .unwrap();
    let text = std::fs::read_to_string(f.data.join("manifests/nologo_by_bot.md")).unwrap();
    assert!(text.contains(&format!("{} emoji", f.keys.len())));
    assert!(!text.contains("brand logo"));
}
// tests.test_publish_contracts.MixedPublishesOneFamily.test_the_base_name_accepts_telegram_s_actual_rule
#[test]
fn contract_the_base_name_accepts_telegram_s_actual_rule() {
    for base in ["YourBrand_Emoji_Packs", "mypack"] {
        production::collection_names::valid_base(base).unwrap();
    }
    for bad in ["bad__two", "_lead", "9start", "trail_", "has space", ""] {
        assert!(
            production::collection_names::valid_base(bad).is_err(),
            "{bad}"
        );
    }
}
// tests.test_publish_contracts.MixedPublishesOneFamily.test_a_base_too_long_for_the_64_char_name_is_refused_up_front
#[test]
fn contract_a_base_too_long_for_the_64_char_name_is_refused_up_front() {
    production::collection_names::name_length("short", "YourEmojiBot", "s").unwrap();
    assert!(
        production::collection_names::name_length(&"x".repeat(50), "YourEmojiBot", "s").is_err()
    );
}
// tests.test_publish_contracts.MixedPublishesOneFamily.test_mixed_orders_across_formats_and_drops_the_format_letter
#[test]
fn contract_mixed_orders_across_formats_and_drops_the_format_letter() {
    let f = Fixture::empty();
    let mut cat = f.catalog();
    let formats = ["static", "animated", "static", "video"];
    let mut order = Vec::new();
    for (i, fmt) in formats.iter().enumerate() {
        let path = png(
            &f.data.join(format!("media/{fmt}/i{i}.png")),
            [200, 30, 30, 255],
        );
        let key = format!("{}:item{i:030}", &fmt[..1]);
        cat.insert(&json!({"content_key":key,"fmt":fmt,"file_path":path,"emojis":["😀"],"keywords":[format!("i{i}")]}),UTC).unwrap();
        order.push(key);
    }
    cat.order(&order).unwrap();
    let plan = resume::freeze(json!({}), &cat.all(None).unwrap(), &["mixed".into()]).unwrap();
    let keys = plan["mixed"].as_array().unwrap();
    assert_eq!(keys.len(), 4);
    assert_eq!(
        keys.iter()
            .map(|k| k.as_str().unwrap().chars().next().unwrap())
            .collect::<Vec<_>>(),
        ['s', 'a', 's', 'v']
    );
    let source = include_str!("../src/publish_cli.rs");
    let tag = source
        .split("fn tag(fmt:")
        .nth(1)
        .unwrap()
        .split("fn pending")
        .next()
        .unwrap();
    assert!(tag.contains("_ => \"\""));
}
// tests.test_publish_contracts.MixedPublishesOneFamily.test_a_family_started_per_format_cannot_switch_to_mixed
#[test]
fn contract_a_family_started_per_format_cannot_switch_to_mixed() {
    let source = include_str!("../src/publish_cli.rs");
    let state = source.split("let started = state").nth(1).unwrap();
    let block = state.split("let mut cat = Catalog::open").next().unwrap();
    assert!(block.contains("formats.iter()"));
    assert!(block.contains("cannot change this family's mixed/per-format publication mode"));
    assert!(block.contains("return Err"));
}
// tests.test_publish_contracts.MixedPublishesOneFamily.test_state_written_by_mixed_can_be_read_back
#[test]
fn contract_state_written_by_mixed_can_be_read_back() {
    let f = Fixture::empty();
    let path = f.data.join("publish_b.json");
    production::atomic::write_json(&path,&json!({"base":"b","sent":[],"skipped":[],"sets":[{"fmt":"mixed","index":1,"name":"b1_by_bot","title":"T 1","live":1,"logo":true,"keys":[]}]})).unwrap();
    let state = resume::collection(read_json(&path), "b").unwrap();
    assert_eq!(state["sets"][0]["fmt"], "mixed");
    let path = f.data.join("publish_c.json");
    production::atomic::write_json(&path,&json!({"base":"c","sent":[],"skipped":[],"sets":[{"fmt":"sideways","index":1,"name":"c1_by_bot","title":"T","live":0,"keys":[]}]})).unwrap();
    assert!(resume::collection(read_json(&path), "c").is_err());
}
// tests.test_publish_contracts.PackTitlesAreOneSequence.test_the_number_counts_all_formats_not_just_this_one
#[test]
fn contract_the_number_counts_all_formats_not_just_this_one() {
    let source = include_str!("../src/publish_collection.rs");
    let title = source
        .split("let title = format!(")
        .nth(1)
        .unwrap()
        .split("let target_index")
        .next()
        .unwrap();
    assert!(title.contains("state[\"sets\"]"));
    assert!(title.contains(".len() + 1"));
    assert!(!source.contains("FMT_WORD"));
}
// tests.test_publish_contracts.PackTitlesAreOneSequence.test_the_announcement_uses_the_recorded_title
#[test]
fn contract_the_announcement_uses_the_recorded_title() {
    let source = include_str!("../src/publish_collection.rs");
    let branch = source
        .split(".is_some_and(|n| n >= options.per_set)")
        .nth(1)
        .unwrap()
        .split("break;")
        .next()
        .unwrap();
    assert!(branch.contains("let set = state[\"sets\"][i].clone()"));
    assert!(branch.contains("&set,"));
    assert!(!branch.contains("format!("));
    assert!(!branch.contains("let title"));
    let notify = include_str!("../src/collection_notify.rs");
    assert!(notify.contains("let title = set[\"title\"]"));
}
// tests.test_publish_contracts.EveryPublisherSharesOneAnnouncer.test_the_single_pack_build_and_the_coin_rebuild_both_import_it
#[test]
fn contract_the_single_pack_build_and_the_coin_rebuild_both_import_it() {
    for (name, source) in [
        ("collection", include_str!("../src/collection_notify.rs")),
        ("single pack", include_str!("../src/ticker_build.rs")),
        ("coin rebuild", include_str!("../src/coin_rebuild_links.rs")),
    ] {
        assert!(
            source.contains("crate::announce::packs("),
            "{name} does not call the shared production announcer"
        );
    }
}
// tests.test_publish_contracts.EveryPublisherSharesOneAnnouncer.test_half_a_worker_config_takes_the_direct_path
#[test]
fn contract_half_a_worker_config_takes_the_direct_path() {
    let f = Fixture::empty();
    let bot = Bot::new(Model::ordinary());
    let cfg = f.config(&[
        ("WORKER_PUBLISH_URL", "https://w.dev/publish".into()),
        ("WORKER_PUBLISH_SECRET", "".into()),
    ]);
    let destination = production::announce::packs(
        &cfg,
        &bot.telegram(),
        7,
        &[json!({"name":"a_by_bot","title":"A"})],
        "general",
        "",
        "cards",
    )
    .unwrap();
    assert_eq!(destination, "7");
    bot.edit(|m| {
        assert_eq!(m.messages.len(), 1);
        assert!(m.worker.is_empty());
    });
}
// tests.test_publish_contracts.EveryPublisherSharesOneAnnouncer.test_the_direct_path_disables_link_previews
#[test]
fn contract_the_direct_path_disables_link_previews() {
    let f = Fixture::empty();
    let bot = Bot::new(Model::ordinary());
    production::announce::packs(
        &f.config(&[]),
        &bot.telegram(),
        7,
        &[json!({"name":"a_by_bot","title":"A"})],
        "coin",
        "",
        "cards",
    )
    .unwrap();
    bot.edit(|m| {
        assert!(!m.messages.is_empty());
        assert!(
            m.messages
                .iter()
                .all(|msg| msg["disable_web_page_preview"] == "true")
        );
    });
}
// tests.test_publish_contracts.EveryPublisherSharesOneAnnouncer.test_a_whole_family_goes_in_ONE_worker_call
#[test]
fn contract_a_whole_family_goes_in_one_worker_call() {
    let f = Fixture::empty();
    let bot = Bot::new(Model::ordinary());
    let packs = (0..30)
        .map(|i| json!({"name":format!("p{i}_by_bot"),"title":i.to_string()}))
        .collect::<Vec<_>>();
    production::announce::packs(
        &f.config(&[
            ("WORKER_PUBLISH_URL", format!("{}/worker", bot.base)),
            ("WORKER_PUBLISH_SECRET", "s".into()),
        ]),
        &bot.telegram(),
        7,
        &packs,
        "coin",
        "all packs:",
        "cards",
    )
    .unwrap();
    bot.edit(|m| {
        assert_eq!(m.worker.len(), 1);
        assert_eq!(m.worker[0]["packs"].as_array().unwrap().len(), 30);
        assert_eq!(m.worker[0]["note"], "all packs:");
        assert_eq!(m.worker[0]["bot"], "coin");
        assert!(m.messages.is_empty());
    });
}
// tests.test_publish_contracts.EveryPublisherSharesOneAnnouncer.test_the_note_is_sent_too_on_the_direct_path
#[test]
fn contract_the_note_is_sent_too_on_the_direct_path() {
    let f = Fixture::empty();
    let bot = Bot::new(Model::ordinary());
    production::announce::packs(
        &f.config(&[]),
        &bot.telegram(),
        7,
        &[json!({"name":"a_by_bot","title":"A"})],
        "coin",
        "header",
        "cards",
    )
    .unwrap();
    bot.edit(|m| {
        assert_eq!(m.messages[0]["text"], "header");
        assert!(
            m.messages[1]["text"]
                .as_str()
                .unwrap()
                .contains("t.me/addemoji/a_by_bot")
        );
    });
}
fn notify(
    f: &Fixture,
    bot: &Bot,
    state: &mut Value,
    name: &str,
    title: &str,
    full: bool,
    extra: &[(&str, String)],
) -> Result<(), String> {
    *state = resume::collection(state.clone(), "b")?;
    let cfg = f.config(extra);
    let mut cat = f.catalog();
    let rt = runtime();
    let tg = bot.telegram();
    let media = Images::default();
    let publisher = production::publication::Publisher {
        tg: &tg,
        cat: &mut cat,
        media: &media,
        data: f.data.clone(),
        runtime: &rt,
        config: Some(&cfg),
    };
    production::collection_notify::notify(
        &publisher,
        1,
        state,
        &f.data.join("publish_b.json"),
        &json!({"name":name,"title":title}),
        full,
    )
}
// tests.test_publish_contracts.AFilledPackAnnouncesAgain.test_filling_announces_even_though_the_link_already_went_out
#[test]
fn contract_filling_announces_even_though_the_link_already_went_out() {
    let f = Fixture::empty();
    let bot = Bot::new(Model::ordinary());
    let mut state = json!({"sent":["pack1"],"skipped":[],"sets":[]});
    notify(&f, &bot, &mut state, "pack1", "Pack 1", true, &[]).unwrap();
    assert_eq!(bot.edit(|m| m.messages.len()), 1);
    assert_eq!(state["sent_full"], json!(["pack1"]));
}
// tests.test_publish_contracts.AFilledPackAnnouncesAgain.test_neither_milestone_fires_twice
#[test]
fn contract_neither_milestone_fires_twice() {
    let f = Fixture::empty();
    let bot = Bot::new(Model::ordinary());
    let mut state = json!({"sent":[],"skipped":[],"sets":[]});
    notify(&f, &bot, &mut state, "pack1", "Pack 1", true, &[]).unwrap();
    assert_eq!(bot.edit(|m| m.messages.len()), 1);
    notify(&f, &bot, &mut state, "pack1", "Pack 1", true, &[]).unwrap();
    assert_eq!(bot.edit(|m| m.messages.len()), 1);
    notify(&f, &bot, &mut state, "pack1", "Pack 1", false, &[]).unwrap();
    assert_eq!(bot.edit(|m| m.messages.len()), 2);
    notify(&f, &bot, &mut state, "pack1", "Pack 1", false, &[]).unwrap();
    assert_eq!(bot.edit(|m| m.messages.len()), 2);
}
// tests.test_publish_contracts.AFilledPackAnnouncesAgain.test_a_failed_send_is_not_recorded_as_sent
#[test]
fn contract_a_failed_send_is_not_recorded_as_sent() {
    let f = Fixture::empty();
    let mut model = Model::ordinary();
    model.send_failure = true;
    let bot = Bot::new(model);
    let mut state = json!({"sent":[],"skipped":[],"sets":[]});
    notify(&f, &bot, &mut state, "pack1", "Pack 1", true, &[]).unwrap();
    assert_eq!(state["sent_full"], json!([]));
}
// tests.test_publish_contracts.AFilledPackAnnouncesAgain.test_the_capacity_call_site_asks_for_the_full_milestone
#[test]
fn contract_the_capacity_call_site_asks_for_the_full_milestone() {
    let source = include_str!("../src/publish_collection.rs");
    let branch = source
        .split(".is_some_and(|n| n >= options.per_set)")
        .nth(1)
        .unwrap()
        .split("break;")
        .next()
        .unwrap();
    let compact = branch
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    assert!(compact.contains("&set,true,"));
    assert!(branch.contains("crate::collection_notify::notify("));
}
// tests.test_publish_contracts.AFilledPackAnnouncesAgain.test_the_state_loader_keeps_the_new_list_a_list
#[test]
fn contract_the_state_loader_keeps_the_new_list_a_list() {
    let f = Fixture::empty();
    let path = f.data.join("publish_b.json");
    std::fs::write(
        &path,
        r#"{"base":"b","sets":[],"sent":[],"sent_full":"nope"}"#,
    )
    .unwrap();
    assert!(resume::collection(read_json(&path), "b").is_err());
}
// tests.test_publish_contracts.AnnouncementRoutesThroughTheWorker.test_worker_is_used_and_telegram_is_not_touched
#[test]
fn contract_worker_is_used_and_telegram_is_not_touched() {
    let f = Fixture::empty();
    let bot = Bot::new(Model::ordinary());
    let mut state = json!({"sent":[]});
    notify(
        &f,
        &bot,
        &mut state,
        "clos1_by_bot",
        "Closure 1",
        false,
        &[
            ("WORKER_PUBLISH_URL", format!("{}/worker", bot.base)),
            ("WORKER_PUBLISH_SECRET", "s".into()),
        ],
    )
    .unwrap();
    bot.edit(|m| {
        assert_eq!(m.worker.len(), 1);
        assert_eq!(m.worker[0]["packs"][0]["name"], "clos1_by_bot");
        assert!(m.messages.is_empty());
    });
    assert!(
        state["sent"]
            .as_array()
            .unwrap()
            .contains(&json!("clos1_by_bot"))
    );
}
// tests.test_publish_contracts.AnnouncementRoutesThroughTheWorker.test_without_a_worker_it_still_posts_directly
#[test]
fn contract_without_a_worker_it_still_posts_directly() {
    let f = Fixture::empty();
    let bot = Bot::new(Model::ordinary());
    let mut state = json!({"sent":[]});
    notify(
        &f,
        &bot,
        &mut state,
        "clos1_by_bot",
        "Closure 1",
        false,
        &[],
    )
    .unwrap();
    assert_eq!(bot.edit(|m| m.messages.len()), 1);
    assert!(
        state["sent"]
            .as_array()
            .unwrap()
            .contains(&json!("clos1_by_bot"))
    );
}
// tests.test_publish_contracts.AnnouncementRoutesThroughTheWorker.test_a_failed_worker_call_does_not_record_it_as_sent
#[test]
fn contract_a_failed_worker_call_does_not_record_it_as_sent() {
    let f = Fixture::empty();
    let mut model = Model::ordinary();
    model.worker_failure = true;
    let bot = Bot::new(model);
    let mut state = json!({"sent":[]});
    notify(
        &f,
        &bot,
        &mut state,
        "clos1_by_bot",
        "Closure 1",
        false,
        &[
            ("WORKER_PUBLISH_URL", format!("{}/worker", bot.base)),
            ("WORKER_PUBLISH_SECRET", "s".into()),
        ],
    )
    .unwrap();
    assert_eq!(state["sent"], json!([]));
}
// tests.test_publish_contracts.AnnouncementRoutesThroughTheWorker.test_an_already_sent_pack_is_not_announced_again
#[test]
fn contract_an_already_sent_pack_is_not_announced_again() {
    let f = Fixture::empty();
    let bot = Bot::new(Model::ordinary());
    let mut state = json!({"sent":["clos1_by_bot"]});
    notify(
        &f,
        &bot,
        &mut state,
        "clos1_by_bot",
        "Closure 1",
        false,
        &[
            ("WORKER_PUBLISH_URL", format!("{}/worker", bot.base)),
            ("WORKER_PUBLISH_SECRET", "s".into()),
        ],
    )
    .unwrap();
    bot.edit(|m| {
        assert!(m.worker.is_empty());
        assert!(m.messages.is_empty());
        assert!(m.requests.is_empty());
    });
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
