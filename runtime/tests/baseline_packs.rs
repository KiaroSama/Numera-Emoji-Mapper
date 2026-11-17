//! Individually named original Python cases; see support/case-map.json.
use numera_runtime as runtime;
use serde_json::json;
#[path = "baseline_packs_support/mod.rs"]
mod support;
use support::*;
#[path = "baseline_packs_support/integration.rs"]
mod integration;
use integration::*;

// Original: tests.test_fetch_emoji_ids.ExtractRealIds.test_bare_entry_lines
#[test]
#[allow(non_snake_case)]
fn fetch_emoji_ids__ExtractRealIds__test_bare_entry_lines() {
    assert_eq!(
        runtime::emoji_ids::entries(
            "premium-id:5334673106202010226\npremium-id:6260129328881732024\n"
        ),
        Vec::<String>::from_iter(
            ["5334673106202010226", "6260129328881732024"]
                .into_iter()
                .map(str::to_owned)
        )
    );
}

// Original: tests.test_fetch_emoji_ids.ExtractRealIds.test_example_prose_line_is_ignored
#[test]
#[allow(non_snake_case)]
fn fetch_emoji_ids__ExtractRealIds__test_example_prose_line_is_ignored() {
    assert_eq!(
        runtime::emoji_ids::entries(
            "- Set a numeric custom_emoji_id (e.g. premium-id: 5472055112702629499) OR a full\npremium-id:5334673106202010226\n"
        ),
        Vec::<String>::from_iter(["5334673106202010226"].into_iter().map(str::to_owned))
    );
}

// Original: tests.test_fetch_emoji_ids.ExtractRealIds.test_trailing_label_and_emoji_still_captured
#[test]
#[allow(non_snake_case)]
fn fetch_emoji_ids__ExtractRealIds__test_trailing_label_and_emoji_still_captured() {
    assert_eq!(
        runtime::emoji_ids::entries(
            "premium-id:5334673106202010226 👋 waving hand\n- premium-id:6260129328881732024 | fire\n"
        ),
        Vec::<String>::from_iter(
            ["5334673106202010226", "6260129328881732024"]
                .into_iter()
                .map(str::to_owned)
        )
    );
}

// Original: tests.test_fetch_emoji_ids.ExtractRealIds.test_bullet_and_spacing_variants
#[test]
#[allow(non_snake_case)]
fn fetch_emoji_ids__ExtractRealIds__test_bullet_and_spacing_variants() {
    assert_eq!(
        runtime::emoji_ids::entries(
            "  - premium-id:5334673106202010226\n* PREMIUM-ID : 6260129328881732024\npremium-id:  5215313353706057331\n"
        ),
        Vec::<String>::from_iter(
            [
                "5334673106202010226",
                "6260129328881732024",
                "5215313353706057331"
            ]
            .into_iter()
            .map(str::to_owned)
        )
    );
}

// Original: tests.test_fetch_emoji_ids.ExtractRealIds.test_bare_number_list_fallback
#[test]
#[allow(non_snake_case)]
fn fetch_emoji_ids__ExtractRealIds__test_bare_number_list_fallback() {
    assert_eq!(
        runtime::emoji_ids::entries(
            "5334673106202010226\n6260129328881732024\n   5215313353706057331   \n"
        ),
        Vec::<String>::from_iter(
            [
                "5334673106202010226",
                "6260129328881732024",
                "5215313353706057331"
            ]
            .into_iter()
            .map(str::to_owned)
        )
    );
}

// Original: tests.test_fetch_emoji_ids.ExtractRealIds.test_short_or_nonnumeric_noise_ignored
#[test]
#[allow(non_snake_case)]
fn fetch_emoji_ids__ExtractRealIds__test_short_or_nonnumeric_noise_ignored() {
    assert_eq!(
        runtime::emoji_ids::entries(
            "premium-id:123\nrandom prose 5334673106202010226 in text\nhello\n"
        ),
        Vec::<String>::from_iter([].into_iter().map(str::to_owned))
    );
}

// Original: tests.test_fetch_emoji_ids.ExtractRealIds.test_overlong_number_not_truncated
#[test]
#[allow(non_snake_case)]
fn fetch_emoji_ids__ExtractRealIds__test_overlong_number_not_truncated() {
    assert_eq!(
        runtime::emoji_ids::entries(
            "premium-id:999999999999999999999999999999\npremium-id:5334673106202010226\n"
        ),
        Vec::<String>::from_iter(["5334673106202010226"].into_iter().map(str::to_owned))
    );
}

// Original: tests.test_fetch_emoji_ids.WithinFileDuplicates.test_detects_true_within_file_duplicate
#[test]
#[allow(non_snake_case)]
fn fetch_emoji_ids__WithinFileDuplicates__test_detects_true_within_file_duplicate() {
    assert_eq!(
        runtime::emoji_ids::within_file_duplicates(
            "premium-id:5334673106202010226\npremium-id:6260129328881732024\npremium-id:5334673106202010226\n"
        ),
        vec![("5334673106202010226".to_owned(), 2)]
    );
}

// Original: tests.test_fetch_emoji_ids.WithinFileDuplicates.test_example_line_is_not_a_duplicate
#[test]
#[allow(non_snake_case)]
fn fetch_emoji_ids__WithinFileDuplicates__test_example_line_is_not_a_duplicate() {
    assert_eq!(
        runtime::emoji_ids::within_file_duplicates(
            "- Set a numeric custom_emoji_id (e.g. premium-id: 5472055112702629499) OR a full\npremium-id:5472055112702629499\n"
        ),
        Vec::<(String, usize)>::new()
    );
}

// Original: tests.test_fetch_emoji_ids.CollectIdsDedup.test_cross_file_dedup_preserves_first_seen_order
#[test]
#[allow(non_snake_case)]
fn fetch_emoji_ids__CollectIdsDedup__test_cross_file_dedup_preserves_first_seen_order() {
    let r = Root::new();
    let files = vec![
        r.write(
            "a.md",
            "premium-id:5334673106202010226\npremium-id:6260129328881732024\n",
        )
        .to_str()
        .unwrap()
        .to_owned(),
        r.write(
            "b.md",
            "premium-id:6260129328881732024\npremium-id:5215313353706057331\n",
        )
        .to_str()
        .unwrap()
        .to_owned(),
    ];
    let inline: Vec<String> = [].into_iter().map(str::to_owned).collect();
    assert_eq!(
        runtime::emoji_ids::collect(&files, &inline).unwrap(),
        Vec::<String>::from_iter(
            [
                "5334673106202010226",
                "6260129328881732024",
                "5215313353706057331"
            ]
            .into_iter()
            .map(str::to_owned)
        )
    );
}

// Original: tests.test_fetch_emoji_ids.CollectIdsDedup.test_within_and_cross_file_and_inline_all_collapse
#[test]
#[allow(non_snake_case)]
fn fetch_emoji_ids__CollectIdsDedup__test_within_and_cross_file_and_inline_all_collapse() {
    let r = Root::new();
    let files = vec![
        r.write(
            "a.md",
            "premium-id:5334673106202010226\npremium-id:5334673106202010226\n",
        )
        .to_str()
        .unwrap()
        .to_owned(),
    ];
    let inline: Vec<String> = ["5334673106202010226", "6260129328881732024"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        runtime::emoji_ids::collect(&files, &inline).unwrap(),
        Vec::<String>::from_iter(
            ["5334673106202010226", "6260129328881732024"]
                .into_iter()
                .map(str::to_owned)
        )
    );
}

// Original: tests.test_fetch_emoji_ids.CollectIdsDedup.test_example_ids_never_downloaded
#[test]
#[allow(non_snake_case)]
fn fetch_emoji_ids__CollectIdsDedup__test_example_ids_never_downloaded() {
    let r = Root::new();
    let files=vec![r.write("a.md","- e.g. premium-id: 5472055112702629499 in the header\npremium-id:5334673106202010226\n").to_str().unwrap().to_owned()];
    let inline: Vec<String> = [].into_iter().map(str::to_owned).collect();
    assert_eq!(
        runtime::emoji_ids::collect(&files, &inline).unwrap(),
        Vec::<String>::from_iter(["5334673106202010226"].into_iter().map(str::to_owned))
    );
    assert!(
        !runtime::emoji_ids::collect(&files, &inline)
            .unwrap()
            .contains(&"5472055112702629499".to_owned())
    );
}

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

// Original: tests.test_pack_archive.OnlyAFinishedPackIsArchived.test_a_pack_still_being_filled_must_own_no_folder
#[test]
#[allow(non_snake_case)]
fn pack_archive__OnlyAFinishedPackIsArchived__test_a_pack_still_being_filled_must_own_no_folder() {
    let a = Archive::new(146, "1", true, true, &["1"], true);
    let (stale, why) = a.check();
    assert!(stale);
    assert!(why[0].contains("not full"));
}

// Original: tests.test_pack_archive.OnlyAFinishedPackIsArchived.test_a_full_pack_that_was_never_archived_is_stale
#[test]
#[allow(non_snake_case)]
fn pack_archive__OnlyAFinishedPackIsArchived__test_a_full_pack_that_was_never_archived_is_stale() {
    let a = Archive::new(200, "1", false, true, &["1"], false);
    let (stale, why) = a.check();
    assert!(stale);
    assert!(why[0].contains("never archived"));
}

// Original: tests.test_pack_archive.OnlyAFinishedPackIsArchived.test_a_half_full_pack_with_no_folder_is_simply_fine
#[test]
#[allow(non_snake_case)]
fn pack_archive__OnlyAFinishedPackIsArchived__test_a_half_full_pack_with_no_folder_is_simply_fine()
{
    let a = Archive::new(146, "1", false, true, &["1"], false);
    assert_eq!(a.check(), (false, vec![]));
}

// Original: tests.test_pack_archive.OnlyAFinishedPackIsArchived.test_a_synced_full_pack_is_fresh
#[test]
#[allow(non_snake_case)]
fn pack_archive__OnlyAFinishedPackIsArchived__test_a_synced_full_pack_is_fresh() {
    let a = Archive::new(200, "1", true, true, &["1"], true);
    assert_eq!(a.check(), (false, vec![]));
}

// Original: tests.test_pack_archive.TheArchiveHasToFollowThePack.test_a_replaced_id_makes_the_archive_stale
#[test]
#[allow(non_snake_case)]
fn pack_archive__TheArchiveHasToFollowThePack__test_a_replaced_id_makes_the_archive_stale() {
    let a = Archive::new(200, "NEW", true, true, &["OLD"], true);
    let (s, w) = a.check();
    assert!(s);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(w[0].contains("id(s) added/changed"));
}

// Original: tests.test_pack_archive.TheArchiveHasToFollowThePack.test_media_still_inside_the_project_is_stale
#[test]
#[allow(non_snake_case)]
fn pack_archive__TheArchiveHasToFollowThePack__test_media_still_inside_the_project_is_stale() {
    let a = Archive::new(200, "1", true, true, &["1"], false);
    let (s, w) = a.check();
    assert!(s);
    assert!(
        w.iter().any(|s| s.contains("still in the project")),
        "{w:?}"
    );
}

// Original: tests.test_pack_archive.TheArchiveHasToFollowThePack.test_a_missing_metadata_file_is_stale
#[test]
#[allow(non_snake_case)]
fn pack_archive__TheArchiveHasToFollowThePack__test_a_missing_metadata_file_is_stale() {
    let a = Archive::new(200, "1", true, false, &["1"], true);
    let (s, w) = a.check();
    assert!(s);
    assert!(
        w.iter()
            .any(|s| s.contains(runtime::archive_rows::LOGO_NAME)),
        "{w:?}"
    );
}

// Original: tests.test_pack_archive.TheNameCarriesTheSlot.test_the_filename_matches_the_convention_already_on_disk
#[test]
#[allow(non_snake_case)]
fn pack_archive__TheNameCarriesTheSlot__test_the_filename_matches_the_convention_already_on_disk() {
    let n =
        runtime::archive_rows::name(7, "animated", "a:169f06002d30a194aa2a4d6ab7cabd87", ".tgs")
            .unwrap();
    assert_eq!(n, "007_animated_169f06002d30.tgs");
    let parts = n.split(['_', '.']).collect::<Vec<_>>();
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[0].len(), 3);
    assert!(parts[0].bytes().all(|b| b.is_ascii_digit()));
    assert!(["static", "animated", "video"].contains(&parts[1]));
    assert_eq!(parts[2].len(), 12);
    assert!(
        parts[2]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
}

// Original: tests.test_pack_archive.TheNameCarriesTheSlot.test_the_logo_leads_and_carries_no_id
#[test]
#[allow(non_snake_case)]
fn pack_archive__TheNameCarriesTheSlot__test_the_logo_leads_and_carries_no_id() {
    let (_, rows) = archive_rows();
    assert_eq!(rows[0]["file"], runtime::archive_rows::LOGO_NAME);
    assert!(rows[0]["premium_id"].is_null());
    assert_eq!(rows[1]["position"], 2);
    assert_eq!(rows[1]["file"], ARCHIVED_NAME);
}

// Original: tests.test_pack_archive.TheNameCarriesTheSlot.test_the_two_human_files_count_the_logo_differently_on_purpose
#[test]
#[allow(non_snake_case)]
fn pack_archive__TheNameCarriesTheSlot__test_the_two_human_files_count_the_logo_differently_on_purpose()
 {
    let (items, rows) = archive_rows();
    let rec = json!({"name":"set1","title":"Pack One","fmt":"mixed"});
    let h = runtime::archive_rows::history(&rec, &rows).unwrap();
    let m = runtime::archive_rows::manifest(&rec, &rows, &items).unwrap();
    assert!(h.contains("Emoji in the pack: **2**"));
    assert!(m.contains("|  1 emoji"));
    assert!(h.contains(runtime::archive_rows::LOGO_NAME));
    assert!(!m.contains(runtime::archive_rows::LOGO_NAME));
    assert!(m.contains("lock"));
}

// Original: tests.test_pack_archive.FolderNames.test_reserved_characters_become_underscores
#[test]
#[allow(non_snake_case)]
fn pack_archive__FolderNames__test_reserved_characters_become_underscores() {
    assert_eq!(runtime::archive_rows::folder("A: B?"), "A_ B_");
}

// Original: tests.test_pack_archive.FolderNames.test_a_plain_title_is_unchanged
#[test]
#[allow(non_snake_case)]
fn pack_archive__FolderNames__test_a_plain_title_is_unchanged() {
    assert_eq!(runtime::archive_rows::folder("Pack One"), "Pack One");
}

// Original: tests.test_pack_archive.AMovedFileStaysRecorded.test_a_missing_second_source_keeps_the_first_move
#[test]
#[allow(non_snake_case)]
fn pack_archive__AMovedFileStaysRecorded__test_a_missing_second_source_keeps_the_first_move() {
    let a = Archive::new(200, "1", true, false, &["1"], false);
    let first = a.root.0.join("collection/media/x.webp");
    std::fs::write(&first, b"a").unwrap();
    let missing = a.root.0.join("collection/media/gone.webp");
    let key2 = "s:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let db = a.root.0.join("collection/catalog.db");
    let con = rusqlite::Connection::open(&db).unwrap();
    con.execute(
        "INSERT INTO items VALUES(?,?,?,?)",
        rusqlite::params![key2, missing.to_str().unwrap(), "static", "[]"],
    )
    .unwrap();
    con.execute(
        "INSERT INTO publications VALUES(?,?,?,?)",
        rusqlite::params!["pk", key2, "set1", "2"],
    )
    .unwrap();
    drop(con);
    a.root.write("logo.png", b"logo");
    let config = a.root.config(&[
        ("COLLECTION_PACK_BASE", "pk"),
        ("EMOJI_ARCHIVE_DIR", "archive"),
        ("BRAND_LOGO_PATH", "logo.png"),
    ]);
    let bot = http::Bot::new(|r| {
        assert_eq!(r.method, "getStickerSet");
        http::ok(json!({"stickers":live(&["logo","1","2"])}))
    });
    let log = runtime::logging::RunLog::open(&a.root.0, "archive-case", vec![]);
    assert_eq!(
        runtime::archive_sync::run(&config, &bot.telegram(), &log).unwrap(),
        4
    );
    let moved = a
        .root
        .0
        .join("archive/Pack One")
        .join(runtime::archive_rows::name(2, "static", ARCHIVE_KEY, ".webp").unwrap());
    let con = rusqlite::Connection::open(db).unwrap();
    let stored: String = con
        .query_row(
            "SELECT file_path FROM items WHERE content_key=?",
            [ARCHIVE_KEY],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        runtime::paths::resolve(&a.root.0.join("collection"), &stored, &a.root.0),
        moved
    );
    assert!(moved.is_file());
    assert!(!first.exists());
    let logs = std::fs::read_dir(a.root.0.join("logs"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap_or_default())
        .collect::<String>();
    assert!(
        logs.contains("ERROR") && logs.contains("prior moved files remain committed"),
        "{logs}"
    );
}

// Original: tests.test_pack_manifest.TheRosterDescribesTheLiveSet.test_the_brand_logo_keeps_its_id_and_is_emoji_zero
#[test]
#[allow(non_snake_case)]
fn pack_manifest__TheRosterDescribesTheLiveSet__test_the_brand_logo_keeps_its_id_and_is_emoji_zero()
{
    let d = roster(
        vec![
            sticker("111", "😀", false, false),
            sticker("222", "😀", false, false),
        ],
        "general",
        json!({}),
        json!({}),
        true,
    );
    let r = &d["emoji"][0];
    assert_eq!(r["custom_emoji_id"], "111");
    assert_eq!(r["index"], 0);
    assert_eq!(r["slot"], 1);
    assert_eq!(r["role"], "brand-logo");
}

// Original: tests.test_pack_manifest.TheRosterDescribesTheLiveSet.test_a_coin_pack_has_no_logo_row
#[test]
#[allow(non_snake_case)]
fn pack_manifest__TheRosterDescribesTheLiveSet__test_a_coin_pack_has_no_logo_row() {
    let d = roster(
        vec![sticker("111", "😀", false, false)],
        "coins",
        json!({"111":["btc"]}),
        json!({}),
        true,
    );
    assert_eq!(d["emoji"][0]["role"], "emoji");
    assert_eq!(d["emoji"][0]["name"], "btc");
}

// Original: tests.test_pack_manifest.TheRosterDescribesTheLiveSet.test_our_own_id_is_not_reported_as_a_source
#[test]
#[allow(non_snake_case)]
fn pack_manifest__TheRosterDescribesTheLiveSet__test_our_own_id_is_not_reported_as_a_source() {
    let r = Root::new();
    let db = r.0.join("catalog.db");
    let c = rusqlite::Connection::open(&db).unwrap();
    c.execute_batch("CREATE TABLE items(content_key TEXT,keywords TEXT,emojis TEXT,format TEXT);CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);").unwrap();
    c.execute(
        "INSERT INTO items VALUES(?,?,?,?)",
        rusqlite::params![
            "k",
            json!(["premium-id:999", "premium-id:555", "lock"]).to_string(),
            "[]",
            "static"
        ],
    )
    .unwrap();
    c.execute("INSERT INTO publications VALUES(?,?)", ["k", "555"])
        .unwrap();
    drop(c);
    let p =
        runtime::roster_provenance::general(&db, &std::collections::BTreeSet::from(["555".into()]))
            .unwrap();
    assert_eq!(p["555"]["source_emoji_ids"], json!(["999"]));
    assert_eq!(p["555"]["name"], "lock");
}

// Original: tests.test_pack_manifest.TheRosterDescribesTheLiveSet.test_one_ticker_per_line_is_not_enough
#[test]
#[allow(non_snake_case)]
fn pack_manifest__TheRosterDescribesTheLiveSet__test_one_ticker_per_line_is_not_enough() {
    let r = Root::new();
    let f = r.write("t.json", json!({"btc":"1","xbt":"1","eth":"2"}).to_string());
    let p = runtime::roster_provenance::coins(&read(&f));
    assert_eq!(p["1"], json!(["btc", "xbt"]));
    assert_eq!(p["2"], json!(["eth"]));
}

// Original: tests.test_pack_manifest.TheRosterRemembersRetiredIds.test_a_changed_id_pushes_the_old_one_into_the_history
#[test]
#[allow(non_snake_case)]
fn pack_manifest__TheRosterRemembersRetiredIds__test_a_changed_id_pushes_the_old_one_into_the_history()
 {
    let first = roster_doc("111", json!({}));
    let prior = json!(runtime::roster_provenance::history(&[first]));
    let second = roster_doc("222", prior);
    let r = &second["emoji"][1];
    assert_eq!(r["source_emoji_ids"], json!(["999"]));
    assert_eq!(r["previous_custom_emoji_ids"], json!(["111"]));
    assert_eq!(r["custom_emoji_id"], "222");
}

// Original: tests.test_pack_manifest.TheRosterRemembersRetiredIds.test_an_unchanged_id_invents_no_history
#[test]
#[allow(non_snake_case)]
fn pack_manifest__TheRosterRemembersRetiredIds__test_an_unchanged_id_invents_no_history() {
    let d = roster_doc("111", json!({"ck:k":{"id":"111","history":[]}}));
    assert_eq!(d["emoji"][1]["previous_custom_emoji_ids"], json!([]));
}

// Original: tests.test_pack_manifest.TheRosterRemembersRetiredIds.test_the_trail_keeps_growing_and_never_repeats
#[test]
#[allow(non_snake_case)]
fn pack_manifest__TheRosterRemembersRetiredIds__test_the_trail_keeps_growing_and_never_repeats() {
    let d = roster_doc("333", json!({"ck:k":{"id":"222","history":["111"]}}));
    assert_eq!(
        d["emoji"][1]["previous_custom_emoji_ids"],
        json!(["111", "222"])
    );
    let d = roster_doc("333", json!({"ck:k":{"id":"333","history":["111","222"]}}));
    assert_eq!(
        d["emoji"][1]["previous_custom_emoji_ids"],
        json!(["111", "222"])
    );
}

// Original: tests.test_pack_manifest.TheRosterRemembersRetiredIds.test_history_follows_the_emoji_not_the_id_or_the_slot
#[test]
#[allow(non_snake_case)]
fn pack_manifest__TheRosterRemembersRetiredIds__test_history_follows_the_emoji_not_the_id_or_the_slot()
 {
    let d = roster_doc("111", json!({}));
    assert_eq!(d["emoji"][1]["history_key"], "ck:k");
    assert!(
        d["emoji"][0]["history_key"]
            .as_str()
            .unwrap()
            .starts_with("logo:")
    );
}

// Original: tests.test_pack_manifest.TheRosterRemembersRetiredIds.test_the_page_shows_the_retired_id_and_lets_it_be_copied
#[test]
#[allow(non_snake_case)]
fn pack_manifest__TheRosterRemembersRetiredIds__test_the_page_shows_the_retired_id_and_lets_it_be_copied()
 {
    let d = roster_doc("222", json!({"ck:k":{"id":"111","history":[]}}));
    let p = page(&d);
    assert!(p.contains("data-id=\"111\""));
    assert!(p.contains("this pack, before"));
}

// Original: tests.test_pack_manifest.StalenessIsNotOptimistic.test_a_missing_roster_is_stale
#[test]
#[allow(non_snake_case)]
fn pack_manifest__StalenessIsNotOptimistic__test_a_missing_roster_is_stale() {
    let r = Root::new();
    let (s, w) = runtime::roster_cli::stale(&r.config(&[])).unwrap();
    assert!(s);
    assert!(w.contains("index.json"));
}

// Original: tests.test_pack_manifest.StalenessIsNotOptimistic.test_a_changed_input_is_stale_and_names_the_file
#[test]
#[allow(non_snake_case)]
fn pack_manifest__StalenessIsNotOptimistic__test_a_changed_input_is_stale_and_names_the_file() {
    let r = Root::new();
    let c = r.config(&[("COLLECTION_PACK_BASE", "state")]);
    r.write("collection/catalog.db", b"one");
    let f = runtime::roster_cli::fingerprint(&c).unwrap();
    r.write("packs/index.json", json!({"inputs":f}).to_string());
    assert!(!runtime::roster_cli::stale(&c).unwrap().0);
    r.write("collection/catalog.db", b"two but longer");
    let (s, w) = runtime::roster_cli::stale(&c).unwrap();
    assert!(s);
    assert!(w.contains("catalog.db"));
}

// Original: tests.test_pack_manifest.StalenessIsNotOptimistic.test_an_unreadable_roster_is_stale_not_fresh
#[test]
#[allow(non_snake_case)]
fn pack_manifest__StalenessIsNotOptimistic__test_an_unreadable_roster_is_stale_not_fresh() {
    let r = Root::new();
    r.write("packs/index.json", b"{not json");
    assert!(runtime::roster_cli::stale(&r.config(&[])).unwrap().0);
}

// Original: tests.test_pack_manifest.ThePageCarriesTheDataToo.test_the_html_repeats_the_roster_for_machines
#[test]
#[allow(non_snake_case)]
fn pack_manifest__ThePageCarriesTheDataToo__test_the_html_repeats_the_roster_for_machines() {
    let d = roster(
        vec![sticker("111", "✅", false, false)],
        "general",
        json!({}),
        json!({}),
        true,
    );
    let p = page(&d);
    assert!(p.contains("<script type=\"application/json\" id=\"roster\">"));
    assert_eq!(roster_blob(&p)["emoji"][0]["custom_emoji_id"], "111");
    assert!(p.contains("data-custom-emoji-id=\"111\""));
    assert!(p.contains("data-index=\"0\""));
}

// Original: tests.test_pack_manifest.ThePageCarriesTheDataToo.test_a_label_cannot_close_the_roster_block
#[test]
#[allow(non_snake_case)]
fn pack_manifest__ThePageCarriesTheDataToo__test_a_label_cannot_close_the_roster_block() {
    let mut d = roster(
        vec![sticker("111", "✅", false, false)],
        "general",
        json!({}),
        json!({}),
        true,
    );
    d["emoji"][0]["name"] = json!("</script><b>x");
    let p = page(&d);
    assert!(!p.contains("</script><b>x"));
    assert_eq!(roster_blob(&p)["emoji"][0]["name"], "</script><b>x");
}

// Original: tests.test_pack_manifest.ThePageCarriesTheDataToo.test_a_missing_art_file_leaves_a_hole_not_an_exception
#[test]
#[allow(non_snake_case)]
fn pack_manifest__ThePageCarriesTheDataToo__test_a_missing_art_file_leaves_a_hole_not_an_exception()
{
    let r = Root::new();
    let d = roster(
        vec![sticker("1", "😀", false, false)],
        "general",
        json!({}),
        json!({}),
        false,
    );
    let art = std::collections::BTreeMap::from([("1".into(), r.0.join("nope.png"))]);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let p = rt
        .block_on(runtime::pack_gallery::render(
            &d,
            &media(&r.0),
            &art,
            &r.0.join("thumbs"),
        ))
        .unwrap();
    assert!(p.contains("no preview"));
    assert!(!r.0.join("thumbs/1.webp").exists());
}

// Original: tests.test_pack_manifest.ThePageCarriesTheDataToo.test_both_ids_are_copyable_and_the_glyph_is_shown
#[test]
#[allow(non_snake_case)]
fn pack_manifest__ThePageCarriesTheDataToo__test_both_ids_are_copyable_and_the_glyph_is_shown() {
    let d = roster(
        vec![
            sticker("111", "✅", false, false),
            sticker("222", "🔒", false, false),
        ],
        "general",
        json!({"222":{"name":"lock","source_emoji_ids":["999"]}}),
        json!({}),
        true,
    );
    let p = page(&d);
    assert!(p.contains("data-id=\"222\""));
    assert!(p.contains("data-id=\"999\""));
    assert!(p.contains("class=\"glyph\""));
    assert!(p.contains("🔒"));
}

// Original: tests.test_pack_manifest.ThePageCarriesTheDataToo.test_the_page_offers_nothing_that_changes_anything
#[test]
#[allow(non_snake_case)]
fn pack_manifest__ThePageCarriesTheDataToo__test_the_page_offers_nothing_that_changes_anything() {
    let d = roster(
        vec![sticker("111", "✅", false, false)],
        "general",
        json!({}),
        json!({}),
        true,
    );
    let p = page(&d);
    let body = p.split_once("</style>").unwrap().1;
    for s in [
        "draggable",
        "/api/save",
        "/api/order",
        "class=\"tick\"",
        "Save selection",
        "contenteditable",
    ] {
        assert!(!body.contains(s), "{s}");
    }
}

// Original: tests.test_pack_manifest.ThePageCarriesTheDataToo.test_the_view_only_controls_survive_the_read_only_stripping
#[test]
#[allow(non_snake_case)]
fn pack_manifest__ThePageCarriesTheDataToo__test_the_view_only_controls_survive_the_read_only_stripping()
 {
    let d = roster(
        vec![sticker("111", "✅", false, false)],
        "general",
        json!({}),
        json!({}),
        true,
    );
    let p = page(&d);
    for s in [
        "id=\"top\"",
        "id=\"bot\"",
        "id=\"bg\"",
        "id=\"anim\"",
        "bg-checker",
    ] {
        assert!(p.contains(s), "{s}");
    }
}

// Original: tests.test_pack_manifest.ThePageCarriesTheDataToo.test_an_animated_card_starts_frozen
#[test]
#[allow(non_snake_case)]
fn pack_manifest__ThePageCarriesTheDataToo__test_an_animated_card_starts_frozen() {
    let src = std::fs::read_to_string(project().join("runtime/src/pack_gallery.rs")).unwrap();
    assert!(src.contains(r#"data-still=\"{still}\""#));
    assert!(src.contains(r#"src=\"{still}\""#));
    let js = std::fs::read_to_string(project().join("assets/pack-gallery.js")).unwrap();
    assert!(js.contains("freezeAll()"));
    assert!(js.contains("visibilitychange"));
}

// Original: tests.test_pack_manifest.ThePageCarriesTheDataToo.test_the_zero_note_matches_the_family
#[test]
#[allow(non_snake_case)]
fn pack_manifest__ThePageCarriesTheDataToo__test_the_zero_note_matches_the_family() {
    let g = roster(vec![], "general", json!({}), json!({}), true);
    let c = roster(vec![], "coins", json!({}), json!({}), false);
    assert!(page(&g).contains("brand logo"));
    assert!(page(&c).contains("no brand logo"));
}

// Original: tests.test_pack_manifest.EveryFormatGetsAThumbnail.test_each_format_is_inlined_and_the_second_render_hits_the_cache
#[test]
#[allow(non_snake_case)]
fn pack_manifest__EveryFormatGetsAThumbnail__test_each_format_is_inlined_and_the_second_render_hits_the_cache()
 {
    use std::{collections::BTreeMap, io::Write};
    let r = Root::new();
    let st = r.0.join("static.png");
    png(&st, [200, 30, 30, 255]);
    let tgs = r.0.join("anim.tgs");
    let mut gz = flate2::GzBuilder::new().mtime(0).write(
        std::fs::File::create(&tgs).unwrap(),
        flate2::Compression::default(),
    );
    gz.write_all(
        &std::fs::read(project().join("tests/fixtures/lottie/red_circle_512.json")).unwrap(),
    )
    .unwrap();
    gz.finish().unwrap();
    let video = r.write(
        "clip.webm",
        include_bytes!("baseline_packs_support/fixtures/gallery-video.webm"),
    );
    let art = BTreeMap::from([
        ("111".into(), st),
        ("222".into(), tgs),
        ("333".into(), video),
    ]);
    let d = roster(
        vec![
            sticker("111", "😀", false, false),
            sticker("222", "😀", true, false),
            sticker("333", "😀", false, true),
        ],
        "general",
        json!({}),
        json!({}),
        false,
    );
    let cache = r.0.join("thumbs");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let m = media(&r.0);
    let p = rt
        .block_on(runtime::pack_gallery::render(&d, &m, &art, &cache))
        .unwrap();
    assert!(p.contains("data:image/webp;base64,"));
    assert!(cache.join("222_still.webp").is_file());
    assert!(p.contains("data:video/webm;base64,"));
    let snapshot = || {
        std::fs::read_dir(&cache)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (e.file_name(), e.metadata().unwrap().modified().unwrap())
            })
            .collect::<BTreeMap<_, _>>()
    };
    let before = snapshot();
    let again = rt
        .block_on(runtime::pack_gallery::render(&d, &m, &art, &cache))
        .unwrap();
    assert_eq!(snapshot(), before);
    assert!(!p.contains("no preview"));
    assert_eq!(again.matches("data:").count(), p.matches("data:").count());
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

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
