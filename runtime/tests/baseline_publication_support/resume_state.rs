use super::*;

fn checked(f: &Fixture, sets: Vec<Value>, top: Value) -> Result<Value, String> {
    f.write_state(sets, top);
    resume::collection(read_json(&f.state_path()), "pk")
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
