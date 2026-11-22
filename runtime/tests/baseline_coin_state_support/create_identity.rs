use super::*;
use serde_json::json;
// tests.test_rebuild_dedup_resume.CreateIsAdoptedOnlyOnImageIdentity.test_a_foreign_first_image_is_never_adopted
#[test]
fn test_a_foreign_first_image_is_never_adopted() {
    let f = pending_create(&["aaa", "bbb"]);
    let before = f.saved();
    let mut script = vec![me(), listing(CREATED, 1)];
    download(&mut script, CREATED, 0, &noise("someone-elses-pack"));
    let server = Server::new(script);
    let completed = f.build_cli(&server);
    assert_eq!(
        completed.code, 3,
        "{}{}",
        completed.stdout, completed.stderr
    );
    no_mutations(&server.finish());
    assert_eq!(f.saved(), before);
    f.released();
}
// tests.test_rebuild_dedup_resume.CreateIsAdoptedOnlyOnImageIdentity.test_a_first_sticker_that_cannot_be_read_stops_the_run
#[test]
fn test_a_first_sticker_that_cannot_be_read_stops_the_run() {
    let f = pending_create(&["aaa", "bbb"]);
    let before = f.saved();
    let mut script = vec![me(), listing(CREATED, 1)];
    download(&mut script, CREATED, 0, b"not an image at all");
    let server = Server::new(script);
    let completed = f.build_cli(&server);
    assert_eq!(
        completed.code, 3,
        "{}{}",
        completed.stdout, completed.stderr
    );
    no_mutations(&server.finish());
    assert_eq!(f.saved(), before);
    f.released();
}
// tests.test_rebuild_dedup_resume.CreateIsAdoptedOnlyOnImageIdentity.test_an_empty_set_of_that_name_is_not_treated_as_ours
#[test]
fn test_an_empty_set_of_that_name_is_not_treated_as_ours() {
    let f = pending_create(&["aaa", "bbb"]);
    let before = f.saved();
    let server = Server::new(vec![me(), listing(CREATED, 0)]);
    assert_eq!(f.build_cli(&server).code, 3);
    no_mutations(&server.finish());
    assert_eq!(f.saved(), before);
    f.released();
}
// tests.test_rebuild_dedup_resume.CreateIsAdoptedOnlyOnImageIdentity.test_our_image_beside_a_foreign_one_is_not_our_create
#[test]
fn test_our_image_beside_a_foreign_one_is_not_our_create() {
    let f = pending_create(&["aaa", "bbb"]);
    let before = f.saved();
    let server = Server::new(vec![me(), listing(CREATED, 2)]);
    assert_eq!(f.build_cli(&server).code, 3);
    no_mutations(&server.finish());
    let after = f.saved();
    assert_eq!(after["sets"], json!([]));
    assert_eq!(after["cursor"], before["cursor"]);
    assert_eq!(after["order"], json!([]));
    assert_eq!(after["in_flight"], before["in_flight"]);
    assert_eq!(after, before);
    f.released();
}
// tests.test_rebuild_dedup_resume.CreateIsAdoptedOnlyOnImageIdentity.test_our_own_image_is_still_adopted
#[test]
fn test_our_own_image_is_still_adopted() {
    let f = pending_create(&["aaa", "bbb"]);
    let mut script = vec![listing(CREATED, 1)];
    download(&mut script, CREATED, 0, &noise("aaa"));
    script.extend([
        listing(CREATED, 1),
        f.mutate("bbb", CREATED, false, Body::Json(ok(json!(true)))),
        notify(),
    ]);
    let server = Server::new(script);
    f.run(&server).unwrap();
    let requests = server.finish();
    assert_eq!(
        f.saved()["sets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![CREATED]
    );
    assert_eq!(f.saved()["order"], json!(["aaa", "bbb"]));
    let adds = requests
        .iter()
        .filter(|r| r.method == "addStickerToSet")
        .collect::<Vec<_>>();
    assert_eq!(adds.len(), 1);
    assert_eq!(adds[0].files["file0"], noise("bbb"));
    assert!(requests.iter().all(|r| r.method != "createNewStickerSet"));
    f.released();
}
