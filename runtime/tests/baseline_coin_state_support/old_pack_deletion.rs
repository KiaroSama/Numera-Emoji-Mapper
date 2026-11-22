use super::*;
use serde_json::json;
fn fixture() -> Rebuild {
    let f = Rebuild::new(&["aaa"]);
    f.state(json!({"deleted_old":false}));
    f.old(&["old1", "old2"]);
    f
}
fn deletion(name: &str, rejected: bool) -> Reply {
    let mut r = reply(
        "deleteStickerSet",
        if rejected {
            reject("deleteStickerSet failed: BOT_ACCESS_DENIED")
        } else {
            ok(json!(true))
        },
    );
    let name = name.to_owned();
    r.action = Some(Box::new(move |req| assert_eq!(req.form["name"], name)));
    r
}
fn no_uploads(requests: &[baseline_coin_provider_support::Request]) {
    assert!(
        requests
            .iter()
            .all(|r| r.method != "createNewStickerSet" && r.method != "addStickerToSet")
    );
}
// tests.test_rebuild_dedup_state.OldPackDeletionMustBeConfirmed.test_a_surviving_pack_leaves_the_phase_open
#[test]
fn test_a_surviving_pack_leaves_the_phase_open() {
    let f = fixture();
    let server = Server::new(vec![
        deletion("old1", true),
        listing("old1", 5),
        deletion("old2", true),
        listing("old2", 5),
    ]);
    assert!(f.run(&server).is_err());
    let requests = server.finish();
    assert_eq!(f.saved()["deleted_old"], false);
    assert_eq!(
        f.saved()
            .get("deleted_old_packs")
            .cloned()
            .unwrap_or(json!([])),
        json!([])
    );
    no_uploads(&requests);
    f.released();
}
// tests.test_rebuild_dedup_state.OldPackDeletionMustBeConfirmed.test_partial_deletion_records_only_what_is_gone
#[test]
fn test_partial_deletion_records_only_what_is_gone() {
    let f = fixture();
    let server = Server::new(vec![
        deletion("old1", false),
        missing("old1"),
        deletion("old2", true),
        listing("old2", 5),
    ]);
    assert!(f.run(&server).is_err());
    let requests = server.finish();
    assert_eq!(f.saved()["deleted_old_packs"], json!(["old1"]));
    assert_eq!(f.saved()["deleted_old"], false);
    no_uploads(&requests);
    f.released();
}
// tests.test_rebuild_dedup_state.OldPackDeletionMustBeConfirmed.test_an_unreadable_pack_stops_the_run
#[test]
fn test_an_unreadable_pack_stops_the_run() {
    let f = fixture();
    let server = Server::new(vec![deletion("old1", false), unknown("old1")]);
    assert!(f.run(&server).is_err());
    let requests = server.finish();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "deleteStickerSet")
            .map(|r| r.form["name"].as_str())
            .collect::<Vec<_>>(),
        vec!["old1"]
    );
    assert_eq!(f.saved()["deleted_old"], false);
    f.released();
}
// tests.test_rebuild_dedup_state.OldPackDeletionMustBeConfirmed.test_a_confirmed_deletion_completes_the_phase
#[test]
fn test_a_confirmed_deletion_completes_the_phase() {
    let f = fixture();
    let server = Server::new(vec![
        deletion("old1", false),
        missing("old1"),
        deletion("old2", false),
        missing("old2"),
        f.mutate(
            "aaa",
            "cryptoemoji1_by_bot",
            true,
            Body::Json(ok(json!(true))),
        ),
        notify(),
    ]);
    f.run(&server).unwrap();
    let requests = server.finish();
    assert_eq!(f.saved()["deleted_old"], true);
    assert_eq!(f.saved()["deleted_old_packs"], json!(["old1", "old2"]));
    let creates = requests
        .iter()
        .filter(|r| r.method == "createNewStickerSet")
        .collect::<Vec<_>>();
    assert_eq!(creates.len(), 1);
    assert_eq!(creates[0].files["file0"], noise("aaa"));
    f.released();
}
// tests.test_rebuild_dedup_state.OldPackDeletionMustBeConfirmed.test_deletion_is_not_repeated_after_a_resume
#[test]
fn test_deletion_is_not_repeated_after_a_resume() {
    let f = fixture();
    let server = Server::new(vec![
        deletion("old1", false),
        missing("old1"),
        deletion("old2", false),
        unknown("old2"),
    ]);
    assert!(f.run(&server).is_err());
    let first = server.finish();
    assert_eq!(
        first
            .iter()
            .filter(|r| r.method == "deleteStickerSet")
            .map(|r| r.form["name"].as_str())
            .collect::<Vec<_>>(),
        vec!["old1", "old2"]
    );
    f.released();
    let server = Server::new(vec![
        deletion("old2", false),
        missing("old2"),
        f.mutate(
            "aaa",
            "cryptoemoji1_by_bot",
            true,
            Body::Json(ok(json!(true))),
        ),
        notify(),
    ]);
    f.run(&server).unwrap();
    let last = server.finish();
    assert_eq!(
        first
            .iter()
            .chain(&last)
            .filter(|r| r.method == "deleteStickerSet")
            .map(|r| r.form["name"].as_str())
            .collect::<Vec<_>>(),
        vec!["old1", "old2", "old2"]
    );
    f.released();
}
