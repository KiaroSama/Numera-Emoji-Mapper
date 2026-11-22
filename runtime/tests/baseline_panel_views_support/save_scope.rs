use super::*;

// OriginalID: test_panel_save_scope.SaveScope.test_an_ordinary_in_scope_save_still_works
#[test]
fn scope_an_ordinary_in_scope_save_still_works() {
    let s = Server::with_token(3, "scope-test-token");
    let (code, body) = s.post(
        "/api/save",
        json!({"excluded":[key(1)],"known":s.data.keys}),
    );
    assert_eq!(code, 200);
    assert_eq!(body["excluded"], 1);
    assert_eq!(s.data.excluded(), BTreeSet::from([key(1)]));
}
// OriginalID: test_panel_save_scope.SaveScope.test_an_in_scope_key_can_be_re_included
#[test]
fn scope_an_in_scope_key_can_be_re_included() {
    let s = Server::with_token(3, "scope-test-token");
    s.post(
        "/api/save",
        json!({"excluded":[key(1)],"known":s.data.keys}),
    );
    assert_eq!(
        s.post("/api/save", json!({"excluded":[],"known":s.data.keys}))
            .0,
        200
    );
    assert!(s.data.excluded().is_empty());
}
// OriginalID: test_panel_save_scope.SaveScope.test_a_key_outside_the_scope_keeps_its_exclusion
#[test]
fn scope_a_key_outside_the_scope_keeps_its_exclusion() {
    let s = Server::with_token(3, "scope-test-token");
    s.post(
        "/api/save",
        json!({"excluded":[key(2)],"known":s.data.keys}),
    );
    assert_eq!(s.data.excluded(), BTreeSet::from([key(2)]));
    assert_eq!(
        s.post("/api/save", json!({"excluded":[],"known":[key(0),key(1)]}))
            .0,
        200
    );
    assert_eq!(s.data.excluded(), BTreeSet::from([key(2)]));
}
// OriginalID: test_panel_save_scope.SaveScope.test_a_stale_tab_cannot_re_include_by_omission
#[test]
fn scope_a_stale_tab_cannot_re_include_by_omission() {
    let s = Server::with_token(3, "scope-test-token");
    s.post(
        "/api/save",
        json!({"excluded":[key(0),key(2)],"known":s.data.keys}),
    );
    let expected = BTreeSet::from([key(0), key(2)]);
    assert_eq!(s.data.excluded(), expected);
    assert_eq!(
        s.post(
            "/api/save",
            json!({"excluded":[key(0)],"known":[key(0),key(1)]})
        )
        .0,
        200
    );
    assert_eq!(s.data.excluded(), expected);
}
// OriginalID: test_panel_save_scope.SaveScope.test_a_page_that_cannot_say_what_it_saw_is_refused
#[test]
fn scope_a_page_that_cannot_say_what_it_saw_is_refused() {
    let s = Server::with_token(3, "scope-test-token");
    s.post(
        "/api/save",
        json!({"excluded":[key(2)],"known":s.data.keys}),
    );
    let before = s.data.excluded();
    let (code, body) = s.post("/api/save", json!({"excluded":[]}));
    assert_eq!(code, 409);
    assert!(body["error"].as_str().unwrap().contains("reload"));
    assert_eq!(s.data.excluded(), before);
}
// OriginalID: test_panel_save_scope.SaveScope.test_a_malformed_scope_is_refused_the_same_way
#[test]
fn scope_a_malformed_scope_is_refused_the_same_way() {
    let s = Server::with_token(3, "scope-test-token");
    for bad in [json!("everything"), json!(7), json!([1, 2]), json!({"a":1})] {
        assert_eq!(
            s.post("/api/save", json!({"excluded":[],"known":bad})).0,
            409
        );
    }
}
// OriginalID: test_panel_save_scope.SaveScope.test_a_stale_scope_is_refused_without_applying_even_its_valid_subset
#[test]
fn scope_a_stale_scope_is_refused_without_applying_even_its_valid_subset() {
    let s = Server::with_token(3, "scope-test-token");
    let (code, body) = s.post(
        "/api/save",
        json!({"excluded":[key(0),"s:vanished"],"known":[key(0),"s:vanished","nonsense"]}),
    );
    assert_eq!(code, 409);
    assert!(body["error"].as_str().unwrap().contains("reload"));
    assert!(s.data.excluded().is_empty());
}
// OriginalID: test_panel_save_scope.SaveScope.test_an_unknown_field_is_still_rejected
#[test]
fn scope_an_unknown_field_is_still_rejected() {
    let s = Server::with_token(3, "scope-test-token");
    let (code, body) = s.post("/api/save", json!({"excluded":[],"known":[],"wat":1}));
    assert_eq!(code, 400);
    assert!(body["error"].as_str().unwrap().contains("unknown keys"));
}
