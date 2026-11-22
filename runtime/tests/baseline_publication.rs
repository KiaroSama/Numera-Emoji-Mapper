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

#[path = "baseline_publication_support/publish_attempts.rs"]
mod publish_attempts;
#[path = "baseline_publication_support/resume_state.rs"]
mod resume_state;
fn live(stickers: Vec<Value>) -> Bot {
    let mut model = Model::ordinary();
    model.sets.insert(SET.into(), stickers);
    Bot::new(model)
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
