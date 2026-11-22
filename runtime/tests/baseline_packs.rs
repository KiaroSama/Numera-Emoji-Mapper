//! Individually named original Python cases; see support/case-map.json.
use numera_runtime as runtime;
use serde_json::json;
#[path = "baseline_packs_support/mod.rs"]
mod support;
use support::*;
#[path = "baseline_packs_support/integration.rs"]
mod integration;
use integration::*;

#[path = "baseline_packs_support/archive.rs"]
mod archive;
#[path = "baseline_packs_support/pack_plans.rs"]
mod pack_plans;

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

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
