use super::*;

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
