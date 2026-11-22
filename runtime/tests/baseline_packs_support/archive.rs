use super::*;

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
