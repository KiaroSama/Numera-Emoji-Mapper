use super::*;

// OriginalID: test_panel_guard.MutationGuard.test_missing_token_is_rejected
#[test]
#[allow(non_snake_case)]
fn guard__MutationGuard__test_missing_token_is_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            json!({"excluded":[],"known":[]}).to_string(),
            None,
            "application/json",
            None
        )
        .0,
        403
    );
}

// OriginalID: test_panel_guard.MutationGuard.test_wrong_token_is_rejected
#[test]
#[allow(non_snake_case)]
fn guard__MutationGuard__test_wrong_token_is_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            json!({"excluded":[],"known":[]}).to_string(),
            Some("nope"),
            "application/json",
            None
        )
        .0,
        403
    );
}

// OriginalID: test_panel_guard.MutationGuard.test_non_json_content_type_is_rejected
#[test]
#[allow(non_snake_case)]
fn guard__MutationGuard__test_non_json_content_type_is_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            json!({"excluded":[],"known":[]}).to_string(),
            Some(TOKEN),
            "text/plain",
            None
        )
        .0,
        403
    );
}

// OriginalID: test_panel_guard.MutationGuard.test_foreign_origin_is_rejected
#[test]
#[allow(non_snake_case)]
fn guard__MutationGuard__test_foreign_origin_is_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            json!({"excluded":[],"known":[]}).to_string(),
            Some(TOKEN),
            "application/json",
            Some("http://evil.com")
        )
        .0,
        403
    );
}

// OriginalID: test_panel_guard.MutationGuard.test_malformed_json_is_400
#[test]
#[allow(non_snake_case)]
fn guard__MutationGuard__test_malformed_json_is_400() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            "{not json".into(),
            Some(TOKEN),
            "application/json",
            None
        )
        .0,
        400
    );
}

// OriginalID: test_panel_guard.MutationGuard.test_unknown_keys_rejected
#[test]
#[allow(non_snake_case)]
fn guard__MutationGuard__test_unknown_keys_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.post("/api/save", json!({"excluded":[],"known":[],"wat":1}))
            .0,
        400
    );
}

// OriginalID: test_panel_guard.MutationGuard.test_order_must_be_a_permutation
#[test]
#[allow(non_snake_case)]
fn guard__MutationGuard__test_order_must_be_a_permutation() {
    let f = Fixture::guard();
    assert_eq!(
        f.post("/api/order", json!({"order":f.keys[..1]})).0,
        400,
        "a partial order would silently drop items"
    );
    let mut extra = f.keys.clone();
    extra.push("s:bogus".into());
    assert_eq!(f.post("/api/order", json!({"order":extra})).0, 400);
}

// OriginalID: test_panel_guard.MutationGuard.test_valid_requests_still_work
#[test]
#[allow(non_snake_case)]
fn guard__MutationGuard__test_valid_requests_still_work() {
    let f = Fixture::guard();
    let reversed = f.keys.iter().rev().collect::<Vec<_>>();
    let (code, body) = f.post("/api/order", json!({"order":reversed}));
    assert_eq!((code, body["ok"].clone()), (200, json!(true)));
    let (code, body) = f.post("/api/save", json!({"excluded":[f.keys[0]],"known":f.keys}));
    assert_eq!(code, 200);
    assert_eq!(body["excluded"], 1);
}

// OriginalID: test_panel_guard.MutationGuard.test_get_rejects_a_foreign_host
#[test]
#[allow(non_snake_case)]
fn guard__MutationGuard__test_get_rejects_a_foreign_host() {
    let f = Fixture::guard();
    for path in ["/".into(), format!("/img/{}", f.keys[0])] {
        assert_eq!(f.get(&path, Some("evil.com")).0, 403);
        assert_eq!(f.get(&path, None).0, 200, "loopback still works");
    }
}

// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_missing_token_is_rejected
#[test]
#[allow(non_snake_case)]
fn guard__LosingTheServerIsNeverSilent__test_missing_token_is_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            json!({"excluded":[],"known":[]}).to_string(),
            None,
            "application/json",
            None
        )
        .0,
        403
    );
}

// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_wrong_token_is_rejected
#[test]
#[allow(non_snake_case)]
fn guard__LosingTheServerIsNeverSilent__test_wrong_token_is_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            json!({"excluded":[],"known":[]}).to_string(),
            Some("nope"),
            "application/json",
            None
        )
        .0,
        403
    );
}

// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_non_json_content_type_is_rejected
#[test]
#[allow(non_snake_case)]
fn guard__LosingTheServerIsNeverSilent__test_non_json_content_type_is_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            json!({"excluded":[],"known":[]}).to_string(),
            Some(TOKEN),
            "text/plain",
            None
        )
        .0,
        403
    );
}

// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_foreign_origin_is_rejected
#[test]
#[allow(non_snake_case)]
fn guard__LosingTheServerIsNeverSilent__test_foreign_origin_is_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            json!({"excluded":[],"known":[]}).to_string(),
            Some(TOKEN),
            "application/json",
            Some("http://evil.com")
        )
        .0,
        403
    );
}

// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_malformed_json_is_400
#[test]
#[allow(non_snake_case)]
fn guard__LosingTheServerIsNeverSilent__test_malformed_json_is_400() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            "{not json".into(),
            Some(TOKEN),
            "application/json",
            None
        )
        .0,
        400
    );
}

// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_unknown_keys_rejected
#[test]
#[allow(non_snake_case)]
fn guard__LosingTheServerIsNeverSilent__test_unknown_keys_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.post("/api/save", json!({"excluded":[],"known":[],"wat":1}))
            .0,
        400
    );
}

// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_order_must_be_a_permutation
#[test]
#[allow(non_snake_case)]
fn guard__LosingTheServerIsNeverSilent__test_order_must_be_a_permutation() {
    let f = Fixture::guard();
    assert_eq!(
        f.post("/api/order", json!({"order":f.keys[..1]})).0,
        400,
        "a partial order would silently drop items"
    );
    let mut extra = f.keys.clone();
    extra.push("s:bogus".into());
    assert_eq!(f.post("/api/order", json!({"order":extra})).0, 400);
}

// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_valid_requests_still_work
#[test]
#[allow(non_snake_case)]
fn guard__LosingTheServerIsNeverSilent__test_valid_requests_still_work() {
    let f = Fixture::guard();
    let reversed = f.keys.iter().rev().collect::<Vec<_>>();
    let (code, body) = f.post("/api/order", json!({"order":reversed}));
    assert_eq!((code, body["ok"].clone()), (200, json!(true)));
    let (code, body) = f.post("/api/save", json!({"excluded":[f.keys[0]],"known":f.keys}));
    assert_eq!(code, 200);
    assert_eq!(body["excluded"], 1);
}

// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_get_rejects_a_foreign_host
#[test]
#[allow(non_snake_case)]
fn guard__LosingTheServerIsNeverSilent__test_get_rejects_a_foreign_host() {
    let f = Fixture::guard();
    for path in ["/".into(), format!("/img/{}", f.keys[0])] {
        assert_eq!(f.get(&path, Some("evil.com")).0, 403);
        assert_eq!(f.get(&path, None).0, 200, "loopback still works");
    }
}

// OriginalID: test_panel_guard.RefreshMustActuallyRefresh.test_missing_token_is_rejected
#[test]
#[allow(non_snake_case)]
fn guard__RefreshMustActuallyRefresh__test_missing_token_is_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            json!({"excluded":[],"known":[]}).to_string(),
            None,
            "application/json",
            None
        )
        .0,
        403
    );
}

// OriginalID: test_panel_guard.RefreshMustActuallyRefresh.test_wrong_token_is_rejected
#[test]
#[allow(non_snake_case)]
fn guard__RefreshMustActuallyRefresh__test_wrong_token_is_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            json!({"excluded":[],"known":[]}).to_string(),
            Some("nope"),
            "application/json",
            None
        )
        .0,
        403
    );
}

// OriginalID: test_panel_guard.RefreshMustActuallyRefresh.test_non_json_content_type_is_rejected
#[test]
#[allow(non_snake_case)]
fn guard__RefreshMustActuallyRefresh__test_non_json_content_type_is_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            json!({"excluded":[],"known":[]}).to_string(),
            Some(TOKEN),
            "text/plain",
            None
        )
        .0,
        403
    );
}

// OriginalID: test_panel_guard.RefreshMustActuallyRefresh.test_foreign_origin_is_rejected
#[test]
#[allow(non_snake_case)]
fn guard__RefreshMustActuallyRefresh__test_foreign_origin_is_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            json!({"excluded":[],"known":[]}).to_string(),
            Some(TOKEN),
            "application/json",
            Some("http://evil.com")
        )
        .0,
        403
    );
}

// OriginalID: test_panel_guard.RefreshMustActuallyRefresh.test_malformed_json_is_400
#[test]
#[allow(non_snake_case)]
fn guard__RefreshMustActuallyRefresh__test_malformed_json_is_400() {
    let f = Fixture::guard();
    assert_eq!(
        f.raw_post(
            "/api/save",
            "{not json".into(),
            Some(TOKEN),
            "application/json",
            None
        )
        .0,
        400
    );
}

// OriginalID: test_panel_guard.RefreshMustActuallyRefresh.test_unknown_keys_rejected
#[test]
#[allow(non_snake_case)]
fn guard__RefreshMustActuallyRefresh__test_unknown_keys_rejected() {
    let f = Fixture::guard();
    assert_eq!(
        f.post("/api/save", json!({"excluded":[],"known":[],"wat":1}))
            .0,
        400
    );
}

// OriginalID: test_panel_guard.RefreshMustActuallyRefresh.test_order_must_be_a_permutation
#[test]
#[allow(non_snake_case)]
fn guard__RefreshMustActuallyRefresh__test_order_must_be_a_permutation() {
    let f = Fixture::guard();
    assert_eq!(
        f.post("/api/order", json!({"order":f.keys[..1]})).0,
        400,
        "a partial order would silently drop items"
    );
    let mut extra = f.keys.clone();
    extra.push("s:bogus".into());
    assert_eq!(f.post("/api/order", json!({"order":extra})).0, 400);
}

// OriginalID: test_panel_guard.RefreshMustActuallyRefresh.test_valid_requests_still_work
#[test]
#[allow(non_snake_case)]
fn guard__RefreshMustActuallyRefresh__test_valid_requests_still_work() {
    let f = Fixture::guard();
    let reversed = f.keys.iter().rev().collect::<Vec<_>>();
    let (code, body) = f.post("/api/order", json!({"order":reversed}));
    assert_eq!((code, body["ok"].clone()), (200, json!(true)));
    let (code, body) = f.post("/api/save", json!({"excluded":[f.keys[0]],"known":f.keys}));
    assert_eq!(code, 200);
    assert_eq!(body["excluded"], 1);
}

// OriginalID: test_panel_guard.RefreshMustActuallyRefresh.test_get_rejects_a_foreign_host
#[test]
#[allow(non_snake_case)]
fn guard__RefreshMustActuallyRefresh__test_get_rejects_a_foreign_host() {
    let f = Fixture::guard();
    for path in ["/".into(), format!("/img/{}", f.keys[0])] {
        assert_eq!(f.get(&path, Some("evil.com")).0, 403);
        assert_eq!(f.get(&path, None).0, 200, "loopback still works");
    }
}

// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_ping_answers_without_a_token_and_touches_nothing
#[test]
fn guard_ping_answers_without_a_token_and_touches_nothing() {
    let f = Fixture::guard();
    let before = f.signature();
    let (status, body) = f.get("/api/ping", None);
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"ok":true})
    );
    assert_eq!(f.signature(), before);
}
// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_a_rejected_order_is_an_error_status_not_a_200
#[test]
fn guard_a_rejected_order_is_an_error_status_not_a_200() {
    let f = Fixture::guard();
    assert_eq!(
        f.post(
            "/api/order",
            json!({"order":[format!("s:{}","0".repeat(30))]})
        )
        .0,
        400
    );
}
// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_the_page_polls_and_keeps_the_warning_up
#[test]
fn guard_the_page_polls_and_keeps_the_warning_up() {
    let script = support::script();
    let page = std::fs::read_to_string(support::project().join("assets/panel.html")).unwrap();
    assert!(script.contains("/api/ping"));
    assert!(script.contains("setInterval("));
    assert!(page.contains("id=\"alert\""));
    assert!(!script.contains("setTimeout(()=>a.classList.remove('show')"));
}
// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_unsaved_work_is_remembered_and_retried
#[test]
fn guard_unsaved_work_is_remembered_and_retried() {
    let script = support::script();
    assert!(script.contains("pendingOrder"));
    let beat = &script[script.find("setInterval(async ()=>{").unwrap()..];
    assert!(
        beat.chars()
            .take(600)
            .collect::<String>()
            .contains("flushOrder(pendingOrder)")
    );
}
// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_a_restarted_panel_does_not_strand_the_page
#[test]
fn guard_a_restarted_panel_does_not_strand_the_page() {
    let script = support::script();
    let body = support::block(
        &script,
        "async function apiPost(",
        "async function flushOrder",
    );
    assert!(body.contains("r.status === 403"));
    assert!(body.contains("const TOKEN ="));
}
// OriginalID: test_panel_guard.LosingTheServerIsNeverSilent.test_the_browser_asks_before_closing_on_unsaved_work
#[test]
fn guard_the_browser_asks_before_closing_on_unsaved_work() {
    let script = support::script();
    assert!(script.contains("beforeunload"));
    let guard = &script[script.find("addEventListener('beforeunload'").unwrap()..];
    assert!(
        guard
            .chars()
            .take(200)
            .collect::<String>()
            .contains("if(pendingOrder)")
    );
}
// OriginalID: test_panel_guard.RefreshMustActuallyRefresh.test_a_reload_sees_a_change_another_process_made
#[test]
fn guard_a_reload_sees_a_change_another_process_made() {
    let f = Fixture::guard();
    let before = f.page();
    let key = before[0]["key"].as_str().unwrap();
    {
        let db = rusqlite::Connection::open(&f.db).unwrap();
        db.execute(
            "UPDATE items SET keywords=? WHERE content_key=?",
            rusqlite::params![json!(["premium-id:9999999999999999999"]).to_string(), key],
        )
        .unwrap();
    }
    let after = f.page();
    assert_ne!(
        before[0]["label"], after[0]["label"],
        "the page still served the startup snapshot"
    );
    assert_eq!(
        after[0]["copyId"], "9999999999999999999",
        "derived fields must be rebuilt too"
    );
}
// OriginalID: test_panel_guard.RefreshMustActuallyRefresh.test_the_reload_replaces_the_shared_view_in_place
#[test]
fn guard_the_reload_replaces_the_shared_view_in_place() {
    // This remains a source contract, checking the Rust equivalent of slice/map replacement
    // inside the shared object rather than replacing the Arc<Panel> held by the routes.
    let src =
        std::fs::read_to_string(support::project().join("runtime/src/panel_http.rs")).unwrap();
    let body = support::block(&src, "fn get_page(panel: &Panel)", "fn file_response");
    assert!(body.contains("let mut guard = panel.view.lock()"));
    assert!(body.contains("*guard = fresh;"));
    assert!(!body.contains("panel = "));
    assert!(!body.contains("panel.view = "));
}
