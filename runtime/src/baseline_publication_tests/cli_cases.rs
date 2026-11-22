use super::*;

// tests.test_publish_cli.PublishThroughMain.test_publish_then_resume_uploads_each_emoji_once
#[test]
fn cli_publish_then_resume_uploads_each_emoji_once() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(bot.edit(|m| m.uploaded.clone()), ["item0", "item1"]);
    assert_eq!(read_json(&f.state_path())["sets"][0]["keys"], json!(f.keys));
    for i in 0..2 {
        assert_eq!(
            f.catalog().get(&f.keys[i]).unwrap().unwrap()["custom_emoji_id"],
            format!("{SET}-{i}")
        );
    }
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    bot.edit(|m| {
        assert_eq!(m.uploaded, ["item0", "item1"]);
        assert_eq!(m.sets[SET].len(), 2);
    });
}
// tests.test_publish_cli.PublishThroughMain.test_a_set_emptied_between_runs_stops_the_resume
#[test]
fn cli_a_set_emptied_between_runs_stops_the_resume() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    bot.edit(|m| m.sets.get_mut(SET).unwrap().clear());
    assert_eq!(cli(&f, &bot, &[]).code, 4);
    assert_eq!(bot.edit(|m| m.uploaded.clone()), ["item0", "item1"]);
}
// tests.test_publish_cli.PublishThroughMain.test_unreachable_telegram_is_retryable_not_a_failure
#[test]
fn cli_unreachable_telegram_is_retryable_not_a_failure() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    bot.edit(|m| {
        m.unknown.insert(SET.into());
    });
    assert_eq!(cli(&f, &bot, &[]).code, 3);
    assert_eq!(bot.edit(|m| m.uploaded.clone()), ["item0", "item1"]);
}
// tests.test_publish_cli.PublishThroughMain.test_a_foreign_tail_sticker_sends_the_next_emoji_to_a_new_set
#[test]
fn cli_a_foreign_tail_sticker_sends_the_next_emoji_to_a_new_set() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    bot.edit(|m| {
        m.sets
            .get_mut(SET)
            .unwrap()
            .push(sticker("OWNER", "owner-cid"))
    });
    let key = f.add(2);
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(f.cid(&key), format!("{SET2}-0"));
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 3);
        assert_eq!(m.sets[SET2].len(), 1);
    });
}
// tests.test_publish_cli.PublishThroughMain.test_new_set_starts_the_next_pack_while_this_one_is_half_empty
#[test]
fn cli_new_set_starts_the_next_pack_while_this_one_is_half_empty() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(bot.edit(|m| m.sets[SET].len()), 2);
    let key = f.add(2);
    assert_eq!(cli(&f, &bot, &["--new-set"]).code, 0);
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 2);
        assert_eq!(m.sets[SET2].len(), 1);
    });
    assert_eq!(f.cid(&key), format!("{SET2}-0"));
    let state = read_json(&f.state_path());
    assert_eq!(
        state["sets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(state["sets"][1]["keys"], json!([key]));
}
// tests.test_publish_cli.PublishThroughMain.test_without_new_set_the_next_emoji_still_fills_the_current_pack
#[test]
fn cli_without_new_set_the_next_emoji_still_fills_the_current_pack() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    f.add(2);
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 3);
        assert!(!m.sets.contains_key(SET2));
    });
}
fn two_packs(f: &mut Fixture, bot: &Bot) {
    assert_eq!(cli(f, bot, &[]).code, 0);
    f.add(2);
    assert_eq!(cli(f, bot, &["--new-set"]).code, 0);
}
// tests.test_publish_cli.PublishThroughMain.test_into_pack_tops_up_an_older_pack_that_still_has_room
#[test]
fn cli_into_pack_tops_up_an_older_pack_that_still_has_room() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    two_packs(&mut f, &bot);
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 2);
        assert_eq!(m.sets[SET2].len(), 1);
    });
    let key = f.add(3);
    assert_eq!(cli(&f, &bot, &["--into-pack", "1"]).code, 0);
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 3);
        assert_eq!(m.sets[SET2].len(), 1);
    });
    assert_eq!(f.cid(&key), format!("{SET}-2"));
}
// tests.test_publish_cli.PublishThroughMain.test_the_record_written_is_the_pack_actually_uploaded_to
#[test]
fn cli_the_record_written_is_the_pack_actually_uploaded_to() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    two_packs(&mut f, &bot);
    let key = f.add(3);
    assert_eq!(cli(&f, &bot, &["--into-pack", "1"]).code, 0);
    let state = read_json(&f.state_path());
    let first = &state["sets"][0];
    let second = &state["sets"][1];
    assert!(first["keys"].as_array().unwrap().contains(&json!(key)));
    assert!(!second["keys"].as_array().unwrap().contains(&json!(key)));
    bot.edit(|m| {
        assert_eq!(first["live"], m.sets[SET].len());
        assert_eq!(second["live"], m.sets[SET2].len());
    });
}
// tests.test_publish_cli.PublishThroughMain.test_into_pack_refuses_a_number_that_is_not_there
#[test]
fn cli_into_pack_refuses_a_number_that_is_not_there() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    two_packs(&mut f, &bot);
    f.add(3);
    assert_eq!(cli(&f, &bot, &["--into-pack", "9"]).code, 4);
    bot.edit(|m| {
        assert_eq!(m.sets[SET].len(), 2);
        assert_eq!(m.sets[SET2].len(), 1);
    });
}
// tests.test_publish_cli.PublishThroughMain.test_into_pack_refuses_a_full_pack
#[test]
fn cli_into_pack_refuses_a_full_pack() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &["--per-set", "2"]).code, 0);
    f.add(2);
    assert_eq!(
        cli(&f, &bot, &["--per-set", "2", "--into-pack", "1"]).code,
        4
    );
}
// tests.test_publish_cli.PublishThroughMain.test_into_pack_and_new_set_together_are_rejected
#[test]
fn cli_into_pack_and_new_set_together_are_rejected() {
    let mut f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    two_packs(&mut f, &bot);
    assert_eq!(cli(&f, &bot, &["--new-set", "--into-pack", "1"]).code, 2);
}
// tests.test_publish_cli.PublishThroughMain.test_an_older_recorded_set_that_disappeared_fails_closed
#[test]
fn cli_an_older_recorded_set_that_disappeared_fails_closed() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &["--per-set", "1"]).code, 0);
    bot.edit(|m| {
        assert_eq!(m.sets.keys().cloned().collect::<Vec<_>>(), [SET, SET2]);
        m.sets.remove(SET);
    });
    assert_eq!(cli(&f, &bot, &["--per-set", "1"]).code, 4);
}
// tests.test_publish_cli.PublishThroughMain.test_an_older_set_that_cannot_be_read_is_retryable
#[test]
fn cli_an_older_set_that_cannot_be_read_is_retryable() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &["--per-set", "1"]).code, 0);
    bot.edit(|m| {
        m.unknown.insert(SET.into());
    });
    assert_eq!(cli(&f, &bot, &["--per-set", "1"]).code, 3);
}
// tests.test_publish_cli.PublishThroughMain.test_each_successful_upload_is_logged
#[test]
fn cli_each_successful_upload_is_logged() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    let run = cli(&f, &bot, &[]);
    assert_eq!(run.code, 0);
    let uploads = run
        .log
        .lines()
        .filter(|line| line.contains("] uploaded "))
        .collect::<Vec<_>>();
    assert_eq!(uploads.len(), 2, "{}", run.log);
    assert!(uploads[0].contains("1/2"));
    assert!(uploads[1].contains("2/2"));
    assert!(uploads.iter().all(|line| line.contains("pk")));
}
// tests.test_publish_cli.PublishThroughMain.test_a_file_telegram_will_never_accept_stops_being_retried
#[test]
fn cli_a_file_telegram_will_never_accept_stops_being_retried() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.reject_adds = 1;
    let bot = Bot::new(model);
    cli(&f, &bot, &[]);
    assert_eq!(
        read_json(&f.state_path())["skipped"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let before = bot.edit(|m| m.adds.len());
    cli(&f, &bot, &[]);
    assert_eq!(bot.edit(|m| m.adds.len()), before);
}
// tests.test_publish_cli.PublishThroughMain.test_the_dry_run_counts_what_will_publish_not_the_plan
#[test]
fn cli_the_dry_run_counts_what_will_publish_not_the_plan() {
    let f = Fixture::new();
    let mut cat = f.catalog();
    let keys = cat
        .all(None)
        .unwrap()
        .iter()
        .map(|row| row["content_key"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    cat.inclusion(&[keys[0].clone()]).unwrap();
    let plan =
        crate::resume::freeze(json!({}), &cat.all(None).unwrap(), &["static".into()]).unwrap();
    let queued = super::pending(
        &mut cat,
        &plan,
        "static",
        "pk",
        &std::collections::BTreeSet::new(),
    )
    .unwrap();
    assert!(!queued.contains(&keys[0]));
    assert_eq!(queued.len(), keys.len() - 1);
    cat.uploaded(
        &json!({"content_key":queued[0],"custom_emoji_id":"1","base":"pk","set_name":"pks1"}),
        UTC,
    )
    .unwrap();
    assert!(
        !super::pending(
            &mut cat,
            &plan,
            "static",
            "pk",
            &std::collections::BTreeSet::new()
        )
        .unwrap()
        .contains(&queued[0])
    );
}
// tests.test_publish_cli.PublishThroughMain.test_preflight_refuses_early_and_publishes_nothing
#[test]
fn cli_preflight_refuses_early_and_publishes_nothing() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.refuse.insert("item0.png".into());
    let bot = Bot::new(model);
    let run = cli(&f, &bot, &["--preflight"]);
    assert_ne!(run.code, 0);
    assert!(run.out.contains("REFUSED"));
    bot.edit(|m| {
        assert!(m.uploaded.is_empty());
        assert!(m.sets.is_empty());
    });
}
// tests.test_publish_cli.PublishThroughMain.test_preflight_passes_a_clean_queue_and_still_uploads_nothing
#[test]
fn cli_preflight_passes_a_clean_queue_and_still_uploads_nothing() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    let run = cli(&f, &bot, &["--preflight"]);
    assert_eq!(run.code, 0);
    assert_eq!(run.receipt["factories"], json!({"telegram":1,"media":0}));
    assert!(run.out.contains("acceptable"));
    bot.edit(|m| {
        assert_eq!(m.checked.len(), 2);
        assert!(m.uploaded.is_empty());
        assert!(m.sets.is_empty());
    });
}
// tests.test_publish_cli.PublishThroughMain.test_a_transport_error_is_not_reported_as_a_bad_file
#[test]
fn cli_a_transport_error_is_not_reported_as_a_bad_file() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model
        .unknown_upload
        .extend(["item0.png".into(), "item1.png".into()]);
    let bot = Bot::new(model);
    let run = cli(&f, &bot, &["--preflight"]);
    assert_eq!(run.code, 3);
    assert!(!run.out.contains("REFUSED"));
}
// tests.test_publish_cli.PublishThroughMain.test_a_run_that_lost_an_emoji_does_not_announce_the_pack
#[test]
fn cli_a_run_that_lost_an_emoji_does_not_announce_the_pack() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.reject_adds = 1;
    let bot = Bot::new(model);
    cli(&f, &bot, &[]);
    assert_eq!(read_json(&f.state_path())["sent"], json!([]));
    cli(&f, &bot, &[]);
    assert_eq!(
        read_json(&f.state_path())["sent"].as_array().unwrap().len(),
        1
    );
}
// tests.test_publish_cli.PublishThroughMain.test_a_clean_run_announces_normally
#[test]
fn cli_a_clean_run_announces_normally() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(
        read_json(&f.state_path())["sent"].as_array().unwrap().len(),
        1
    );
}
// tests.test_publish_cli.PublishThroughMain.test_a_transient_failure_is_still_retried
#[test]
fn cli_a_transient_failure_is_still_retried() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.transport_adds = 5;
    let bot = Bot::new(model);
    cli(&f, &bot, &[]);
    assert_eq!(read_json(&f.state_path())["skipped"], json!([]));
}
// tests.test_publish_cli.PublishThroughMain.test_a_run_where_every_upload_failed_exits_non_zero
#[test]
fn cli_a_run_where_every_upload_failed_exits_non_zero() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.fail_after = Some(0);
    let bot = Bot::new(model);
    assert_eq!(cli(&f, &bot, &[]).code, 4);
    assert!(bot.edit(|m| m.uploaded.is_empty()));
}
// tests.test_publish_cli.PublishThroughMain.test_a_partly_failed_run_is_partial
#[test]
fn cli_a_partly_failed_run_is_partial() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model.fail_after = Some(1);
    let bot = Bot::new(model);
    assert_eq!(cli(&f, &bot, &[]).code, 3);
    assert_eq!(bot.edit(|m| m.uploaded.clone()), ["item0"]);
}
// tests.test_publish_cli.PublishThroughMain.test_drift_while_adopting_a_set_stops_the_run_instead_of_retrying
#[test]
fn cli_drift_while_adopting_a_set_stops_the_run_instead_of_retrying() {
    let f = Fixture::new();
    let mut model = Model::ordinary();
    model
        .sets
        .insert(SET.into(), vec![sticker("GHOST", "ghost-cid")]);
    model.undownloadable.insert("GHOST".into());
    let bot = Bot::new(model);
    let run = cli(&f, &bot, &[]);
    assert_eq!(run.code, 4);
    assert!(run.log.contains("could not be examined"), "{}", run.log);
    assert!(!run.log.contains("will retry"));
    assert!(!run.out.contains("DONE"));
    let state = read_json(&f.state_path());
    assert_eq!(
        state["sets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|set| set["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [SET]
    );
    crate::resume::collection(state, "pk").unwrap();
}
// tests.test_publish_cli.PublishThroughMain.test_skipped_blank_media_is_not_counted_as_a_failure
#[test]
fn cli_skipped_blank_media_is_not_counted_as_a_failure() {
    let f = Fixture::new();
    let bot = Bot::new(Model::ordinary());
    let path = png(&f.data.join("media/static/blank.png"), [0, 0, 0, 0]);
    let key = format!("s:blank{}", "0".repeat(25));
    f.catalog().insert(&json!({"content_key":key,"fmt":"static","file_path":path,"emojis":["😀"],"keywords":["blank"]}),UTC).unwrap();
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(bot.edit(|m| m.uploaded.clone()), ["item0", "item1"]);
}
// tests.test_publish_contracts.ThePublishFollowsThePanelOrderNow.test_a_reorder_after_the_plan_was_frozen_is_what_gets_uploaded
#[test]
fn contract_a_reorder_after_the_plan_was_frozen_is_what_gets_uploaded() {
    let f = Fixture::new();
    {
        let mut cat = f.catalog();
        let plan =
            crate::resume::freeze(json!({}), &cat.all(None).unwrap(), &["static".into()]).unwrap();
        atomic::write_json(&f.plan_path(), &plan).unwrap();
        cat.order(&f.keys.iter().rev().cloned().collect::<Vec<_>>())
            .unwrap();
    }
    let bot = Bot::new(Model::ordinary());
    assert_eq!(cli(&f, &bot, &[]).code, 0);
    assert_eq!(bot.edit(|m| m.uploaded.clone()), ["item1", "item0"]);
}
// tests.test_publish_contracts.ThePublishFollowsThePanelOrderNow.test_a_dry_run_writes_no_plan
#[test]
fn contract_a_dry_run_writes_no_plan() {
    let f = Fixture::new();
    let run = dry(&f, &[], &[]);
    assert_eq!(run.code, 0);
    assert_eq!(run.receipt["factories"], json!({"telegram":0,"media":0}));
    assert!(!f.plan_path().exists());
}
