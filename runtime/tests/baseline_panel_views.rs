//! Direct Rust ports of the remaining panel model, scope, server, preview and asset cases.
use numera_runtime as runtime;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
#[path = "baseline_panel_views_support/mod.rs"]
mod support;
use support::{Data, Server, key};

// OriginalID: test_panel_view.SimilarityOrder.test_matches_the_reference_walk
#[test]
fn view_matches_the_reference_walk() {
    let cases: Value = serde_json::from_str(include_str!(
        "baseline_panel_views_support/similarity-inputs.json"
    ))
    .unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 3);
    for (case, seed) in cases.as_array().unwrap().iter().zip([1, 1234, 99999]) {
        assert_eq!(case["seed"], seed);
        let items = case["items"].as_array().unwrap();
        let before = items.clone();
        assert_eq!(items.len(), 220);
        let got = runtime::panel_view::similarity(items)
            .unwrap()
            .iter()
            .map(|x| x["identity"].clone())
            .collect::<Vec<_>>();
        assert_eq!(json!(got), case["expected"], "original seed {seed}");
        assert_eq!(*items, before);
    }
}
// OriginalID: test_panel_view.SimilarityOrder.test_unhashed_items_follow_their_format_group
#[test]
fn view_unhashed_items_follow_their_format_group() {
    let items = json!([{"fmt":"animated","phash":null},{"fmt":"static","phash":1},{"fmt":"static","phash":2}]);
    let got = runtime::panel_view::similarity(items.as_array().unwrap()).unwrap();
    assert_eq!(
        got.iter()
            .map(|x| x["fmt"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["static", "static", "animated"]
    );
}
// OriginalID: test_panel_view.CopyTheEmojiId.test_only_a_whole_premium_id_label_is_copyable
#[test]
fn view_only_a_whole_premium_id_label_is_copyable() {
    assert_eq!(
        runtime::panel_view::copy_id("premium-id:5406926593698312391"),
        "5406926593698312391"
    );
    for junk in [
        "xpremium-id:12",
        "premium-id:12x",
        "premium-id:",
        "premium-id:12 34",
        "",
        "coin logo",
        "premium-id:abc",
    ] {
        assert_eq!(runtime::panel_view::copy_id(junk), "", "{junk}");
    }
}
// OriginalID: test_panel_view.CopyTheEmojiId.test_the_view_carries_the_id_for_the_page
#[test]
fn view_the_view_carries_the_id_for_the_page() {
    let d = Data::new(2);
    {
        let _cat = d.cat();
        let db = rusqlite::Connection::open(&d.db).unwrap();
        for (i, label) in ["premium-id:5406926593698312391", "hand drawn"]
            .iter()
            .enumerate()
        {
            db.execute(
                "UPDATE items SET keywords=? WHERE content_key=?",
                rusqlite::params![json!([label]).to_string(), d.keys[i]],
            )
            .unwrap();
        }
    }
    let v = d.view(false, json!({}), false, None);
    let rows = v["view"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .find(|x| x["label"] == "premium-id:5406926593698312391")
            .unwrap()["copyId"],
        "5406926593698312391"
    );
    assert_eq!(
        rows.iter().find(|x| x["label"] == "hand drawn").unwrap()["copyId"],
        ""
    );
}
// OriginalID: test_panel_view.CopyTheEmojiId.test_the_page_stops_the_click_before_the_toggle
#[test]
fn view_the_page_stops_the_click_before_the_toggle() {
    let script = support::scripts(&support::project());
    let copy = script.find("closest('.copyable')").unwrap();
    let toggle = copy - 400 + script[copy - 400..].find("closest('.card')").unwrap();
    assert!(copy < toggle);
    assert!(
        script[copy..]
            .chars()
            .take(200)
            .collect::<String>()
            .contains("e.stopPropagation()")
    );
}
// OriginalID: test_panel_view.CopyTheEmojiId.test_video_plays_without_hover
#[test]
fn view_video_plays_without_hover() {
    let script = support::scripts(&support::project());
    assert!(
        script
            .replace("dataset.play", "data-play")
            .contains("data-play")
    );
    assert!(script.contains("video[data-play]"));
    assert!(script.contains("if (RM) {"));
}
// OriginalID: test_panel_view.PublishedEmojiLeaveTheGrid.test_a_half_empty_pack_is_hidden_too_once_it_is_published
#[test]
fn view_a_half_empty_pack_is_hidden_too_once_it_is_published() {
    let d = Data::published_fixture();
    let v = d.view(false, json!({}), false, None);
    let keys = support::keys(&v);
    assert_eq!(v["hidden"], 3);
    assert!(!keys.contains(&key(2)));
    for i in [3, 4] {
        assert!(keys.contains(&key(i)));
    }
}
// OriginalID: test_panel_view.PublishedEmojiLeaveTheGrid.test_no_publish_state_file_is_needed
#[test]
fn view_no_publish_state_file_is_needed() {
    let d = Data::published_fixture();
    assert!(!std::fs::read_dir(&d.root).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("publish_")
    }));
    assert_eq!(d.view(false, json!({}), false, None)["hidden"], 3);
}
// OriginalID: test_panel_view.PublishedEmojiLeaveTheGrid.test_a_row_with_no_recorded_set_name_still_counts_as_published
#[test]
fn view_a_row_with_no_recorded_set_name_still_counts_as_published() {
    let d = Data::published_fixture();
    d.publish(3, None);
    assert!(!support::keys(&d.view(false, json!({}), false, None)).contains(&key(3)));
}
// OriginalID: test_panel_view.PublishedEmojiLeaveTheGrid.test_all_shows_everything
#[test]
fn view_all_shows_everything() {
    let d = Data::published_fixture();
    let v = d.view(true, json!({}), false, None);
    assert_eq!(v["hidden"], 0);
    assert_eq!(v["view"].as_array().unwrap().len(), 5);
}
// OriginalID: test_panel_view.PublishedEmojiLeaveTheGrid.test_hiding_deletes_nothing
#[test]
fn view_hiding_deletes_nothing() {
    let d = Data::published_fixture();
    d.view(false, json!({}), false, None);
    let mut cat = d.cat();
    assert_eq!(cat.all(None).unwrap().len(), 5);
    assert_eq!(
        cat.dispatch(json!({"operation":"published","base":"pk","content_key":key(0)}))
            .unwrap(),
        true
    );
}
// OriginalID: test_panel_view.OnePackCanBeUnhidden.test_an_index_resolves_through_the_publishers_own_state
#[test]
fn view_an_index_resolves_through_the_publishers_own_state() {
    let d = Data::pack_fixture();
    assert_eq!(d.keep(&[2]), json!({"pk2_by_bot":2}));
    assert_eq!(d.keep(&[1, 2]), json!({"pk1_by_bot":1,"pk2_by_bot":2}));
    assert_eq!(d.keep(&[9]), json!({}));
}
// OriginalID: test_panel_view.OnePackCanBeUnhidden.test_the_index_travels_with_the_name_onto_the_card
#[test]
fn view_the_index_travels_with_the_name_onto_the_card() {
    let d = Data::pack_fixture();
    let v = d.view(false, d.keep(&[1, 2]), false, None);
    let rows = v["view"].as_array().unwrap();
    assert_eq!(rows.iter().find(|x| x["key"] == key(0)).unwrap()["pack"], 1);
    assert_eq!(rows.iter().find(|x| x["key"] == key(2)).unwrap()["pack"], 2);
    assert!(rows.iter().find(|x| x["key"] == key(4)).unwrap()["pack"].is_null());
}
// OriginalID: test_panel_view.OnePackCanBeUnhidden.test_every_shown_pack_gets_its_own_live_logo_card
#[test]
fn view_every_shown_pack_gets_its_own_live_logo_card() {
    let d = Data::pack_fixture();
    let v = d.view(
        false,
        d.keep(&[1, 2]),
        true,
        Some(&d.root.join("brand.png")),
    );
    let rows = v["view"].as_array().unwrap();
    let logos = rows
        .iter()
        .filter(|x| x["isLogo"] == true)
        .collect::<Vec<_>>();
    assert_eq!(
        logos.iter().map(|x| x["pack"].clone()).collect::<Vec<_>>(),
        [json!(1), json!(2)]
    );
    for logo in logos {
        assert_ne!(logo["key"], "__brand_logo__");
        assert!(v["by_key"].get(logo["key"].as_str().unwrap()).is_some());
        let at = rows.iter().position(|x| x == logo).unwrap();
        let head = rows
            .iter()
            .position(|x| x["pack"] == logo["pack"] && x["isLogo"] != true)
            .unwrap();
        assert!(at < head);
    }
}
// OriginalID: test_panel_view.OnePackCanBeUnhidden.test_without_with_pack_the_single_publish_preview_is_unchanged
#[test]
fn view_without_with_pack_the_single_publish_preview_is_unchanged() {
    let d = Data::pack_fixture();
    let v = d.view(false, json!({}), true, Some(&d.root.join("brand.png")));
    let logos = v["view"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["isLogo"] == true)
        .map(|x| x["key"].clone())
        .collect::<Vec<_>>();
    assert_eq!(logos, [json!("__brand_logo__")]);
    assert_eq!(v["view"][0]["key"], "__brand_logo__");
}
// OriginalID: test_panel_view.OnePackCanBeUnhidden.test_an_unnamed_pack_tags_nothing
#[test]
fn view_an_unnamed_pack_tags_nothing() {
    let d = Data::pack_fixture();
    let v = d.view(false, json!({}), false, None);
    assert!(
        v["view"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x.get("pack").is_none())
    );
}
// OriginalID: test_panel_view.OnePackCanBeUnhidden.test_only_the_named_pack_comes_back
#[test]
fn view_only_the_named_pack_comes_back() {
    let d = Data::pack_fixture();
    let v = d.view(false, d.keep(&[2]), false, None);
    let keys = support::keys(&v);
    assert_eq!(v["hidden"], 2);
    for i in [2, 3, 4, 5] {
        assert!(keys.contains(&key(i)));
    }
    for i in [0, 1] {
        assert!(!keys.contains(&key(i)));
    }
}
// OriginalID: test_panel_view.OnePackCanBeUnhidden.test_without_the_flag_every_published_pack_stays_hidden
#[test]
fn view_without_the_flag_every_published_pack_stays_hidden() {
    let d = Data::pack_fixture();
    let v = d.view(false, json!({}), false, None);
    assert_eq!(v["hidden"], 4);
    assert_eq!(support::keys(&v), BTreeSet::from([key(4), key(5)]));
}
// OriginalID: test_panel_view.OnePackCanBeUnhidden.test_a_row_with_no_recorded_set_is_not_unhidden_by_guesswork
#[test]
fn view_a_row_with_no_recorded_set_is_not_unhidden_by_guesswork() {
    let d = Data::pack_fixture();
    d.publish(4, None);
    let v = d.view(false, d.keep(&[2]), false, None);
    assert!(!support::keys(&v).contains(&key(4)));
}
// OriginalID: test_panel_assets.ThePanelScriptsShip.test_every_script_is_a_real_file_inside_the_repo
#[test]
fn assets_every_script_is_a_real_file_inside_the_repo() {
    let root = support::project().canonicalize().unwrap();
    for name in runtime::asset_version::PANEL_SCRIPTS {
        let asset = root.join("assets").join(name);
        assert!(asset.is_file(), "missing {name}");
        assert!(asset.canonicalize().unwrap().starts_with(&root));
    }
}
// OriginalID: test_panel_assets.ThePanelScriptsShip.test_the_page_loads_every_script_in_order_and_versioned
#[test]
fn assets_the_page_loads_every_script_in_order_and_versioned() {
    let page = std::fs::read_to_string(support::project().join("assets/panel.html")).unwrap();
    let tags = runtime::asset_version::PANEL_SCRIPTS
        .iter()
        .map(|name| format!("<script src=\"/static/{name}?v=__ASSET_VER__\"></script>"))
        .collect::<Vec<_>>();
    for tag in &tags {
        assert!(page.contains(tag));
    }
    for pair in tags.windows(2) {
        assert!(page.find(&pair[0]).unwrap() < page.find(&pair[1]).unwrap());
    }
    assert!(page.find("<script id=\"items-data\"").unwrap() < page.find(&tags[0]).unwrap());
}
// OriginalID: test_panel_assets.ThePanelScriptsShip.test_the_version_is_the_scripts_content
#[test]
fn assets_the_version_is_the_scripts_content() {
    use sha1::Digest;
    let root = support::project();
    let script = support::scripts(&root);
    let expected = sha1::Sha1::digest(script.as_bytes())
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect::<String>();
    assert_eq!(
        runtime::asset_version::versions(&root).unwrap().0,
        expected[..12]
    );
    for name in runtime::asset_version::PANEL_SCRIPTS {
        assert!(
            script.contains(
                &std::fs::read_to_string(root.join("assets").join(name))
                    .unwrap()
                    .replace("\r\n", "\n")
                    .replace('\r', "\n")
            )
        );
    }
}
// OriginalID: test_panel_assets.ThePanelScriptsShip.test_the_icon_url_is_versioned_by_the_icon_itself
#[test]
fn assets_the_icon_url_is_versioned_by_the_icon_itself() {
    use sha1::Digest;
    let root = support::project();
    let expected = sha1::Sha1::digest(std::fs::read(root.join("assets/logo-128.png")).unwrap())
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect::<String>();
    assert_eq!(
        runtime::asset_version::versions(&root).unwrap().1,
        expected[..12]
    );
    let page = std::fs::read_to_string(root.join("assets/panel.html")).unwrap();
    assert!(!page.contains("\"/static/logo-128.png\""));
    assert_eq!(
        page.matches("/static/logo-128.png?v=__ICON_VER__").count(),
        2
    );
}
// OriginalID: test_panel_assets.ThePanelScriptsShip.test_the_inline_block_carries_values_not_behaviour
#[test]
fn assets_the_inline_block_carries_values_not_behaviour() {
    let page = std::fs::read_to_string(support::project().join("assets/panel.html")).unwrap();
    let inline = &page[page.find("<script id=\"items-data\"").unwrap()..];
    assert!(!inline.contains("addEventListener"));
    assert!(!inline.contains("function "));
    for token in [
        "__TOKEN__",
        "__PREVIEW_TIERS__",
        "__STALE__",
        "__PER_SET__",
        "__HIDDEN__",
    ] {
        assert!(inline.contains(token));
    }
}
// OriginalID: test_panel_assets.TheStaticRouteServesVersionedScripts.test_the_served_page_names_the_current_version
#[test]
fn assets_the_served_page_names_the_current_version() {
    let s = Server::new(1);
    let html = s.page();
    let (scripts, icon) = runtime::asset_version::versions(&s.data.root).unwrap();
    assert!(!html.contains("__ASSET_VER__"));
    assert!(html.contains(&format!(
        "/static/{}?v={scripts}",
        runtime::asset_version::PANEL_SCRIPTS[0]
    )));
    assert!(!html.contains("__ICON_VER__"));
    assert!(html.contains(&format!("/static/logo-128.png?v={icon}")));
}
// OriginalID: test_panel_assets.TheStaticRouteServesVersionedScripts.test_the_version_query_reaches_the_file_on_disk
#[test]
fn assets_the_version_query_reaches_the_file_on_disk() {
    let s = Server::new(1);
    let version = runtime::asset_version::versions(&s.data.root).unwrap().0;
    let name = runtime::asset_version::PANEL_SCRIPTS[0];
    let response = s.get(&format!("/static/{name}?v={version}"));
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["Content-Type"], "application/javascript");
    assert!(
        response.headers()["Cache-Control"]
            .to_str()
            .unwrap()
            .contains("immutable")
    );
    assert_eq!(
        response.bytes().unwrap().as_ref(),
        std::fs::read(s.data.root.join("assets").join(name)).unwrap()
    );
}
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
const NETSTAT_SAMPLE: &str = "Active Connections\n\n  Proto  Local Address      Foreign Address    State       PID\n  TCP    127.0.0.1:9450     0.0.0.0:0          LISTENING   4321\n  TCP    127.0.0.1:9451     0.0.0.0:0          LISTENING   9999\n  TCP    10.0.0.5:59450     1.2.3.4:443        ESTABLISHED 1111";
// OriginalID: test_panel_server.TheBusyPortMessageNamesTheProcess.test_it_finds_the_listener
#[test]
fn server_it_finds_the_listener() {
    assert_eq!(
        runtime::panel_session::listener_pid(NETSTAT_SAMPLE, 9450).as_deref(),
        Some("4321")
    );
    assert_eq!(
        runtime::panel_session::listener_pid(NETSTAT_SAMPLE, 9451).as_deref(),
        Some("9999")
    );
}
// OriginalID: test_panel_server.TheBusyPortMessageNamesTheProcess.test_an_established_connection_is_not_a_listener
#[test]
fn server_an_established_connection_is_not_a_listener() {
    assert_eq!(
        runtime::panel_session::listener_pid(NETSTAT_SAMPLE, 443),
        None
    );
}
// OriginalID: test_panel_server.TheBusyPortMessageNamesTheProcess.test_a_port_nobody_listens_on_is_none
#[test]
fn server_a_port_nobody_listens_on_is_none() {
    assert_eq!(
        runtime::panel_session::listener_pid(NETSTAT_SAMPLE, 1234),
        None
    );
}
// OriginalID: test_panel_server.TheBusyPortMessageNamesTheProcess.test_it_never_raises
#[test]
fn server_it_never_raises() {
    if std::env::var("NUMERA_NO_NETSTAT_CASE").as_deref() == Ok("1") {
        let d = Data::new(0);
        assert!(runtime::codec_process::executable("netstat").is_err());
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert_eq!(
            rt.block_on(runtime::panel_session::port_holder(&d.root, 9450))
                .unwrap(),
            None
        );
        assert_eq!(
            runtime::panel_session::listener_pid("garbage\n", 9450),
            None
        );
        return;
    }
    let mut c = support::command(&std::env::current_exe().unwrap());
    c.args(["--exact", "server_it_never_raises", "--nocapture"])
        .env("NUMERA_NO_NETSTAT_CASE", "1")
        .env("PATH", "");
    let answer = support::output(&mut c, Duration::from_secs(15));
    assert!(
        answer.status.success(),
        "{}",
        String::from_utf8_lossy(&answer.stderr)
    );
}
// OriginalID: test_panel_server.TheSandboxCannotTakeTheRealPanelsPort.test_the_sandbox_derives_its_port_from_the_panel
#[test]
fn server_the_sandbox_derives_its_port_from_the_panel() {
    let real = runtime::panel_session::DEFAULT_PORT;
    let (_, options) = runtime::tooling_launch::sandbox_options(&[], real).unwrap();
    assert_eq!(options.port, i64::from(real) + 1);
    assert_ne!(options.port, i64::from(real));
}
// OriginalID: test_panel_server.TheSandboxCannotTakeTheRealPanelsPort.test_it_refuses_to_be_told_to_use_it
#[test]
fn server_it_refuses_to_be_told_to_use_it() {
    let real = runtime::panel_session::DEFAULT_PORT;
    let err = runtime::tooling_launch::sandbox_options(&["--port".into(), real.to_string()], real)
        .err()
        .unwrap();
    assert!(err.contains(&real.to_string()));
}
// OriginalID: test_panel_server.OnlyOnePanelPerPort.test_the_second_instance_reuses_the_live_panel
#[test]
fn server_the_second_instance_reuses_the_live_panel() {
    use std::process::Stdio;
    let d = Data::new(1);
    {
        let db = rusqlite::Connection::open(&d.db).unwrap();
        db.execute(
            "UPDATE items SET content_key=?,keywords=?",
            rusqlite::params![format!("s:{}", "a".repeat(30)), json!(["one"]).to_string()],
        )
        .unwrap();
    }
    let exe = d.root.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(support::binary(), &exe).unwrap();
    let reserve = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reserve.local_addr().unwrap().port();
    drop(reserve);
    let mut first_command = support::command(&exe);
    first_command
        .args(["panel", "--data-dir"])
        .arg(&d.root)
        .args([
            "--port",
            &port.to_string(),
            "--no-open",
            "--bot-username",
            "FixtureBot",
        ]);
    let capture = std::fs::File::create(d.root.join("first-panel-output.txt")).unwrap();
    first_command
        .stdin(Stdio::null())
        .stdout(capture.try_clone().unwrap())
        .stderr(capture);
    let mut first = support::Child(first_command.spawn().unwrap());
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut ready = false;
    while Instant::now() < deadline {
        assert!(
            first.0.try_wait().unwrap().is_none(),
            "first panel exited early"
        );
        if let Ok(r) = client
            .get(format!("http://127.0.0.1:{port}/api/ping"))
            .send()
            && r.status() == 200
        {
            ready = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ready);
    let mut second = support::command(&exe);
    second.args(["panel", "--data-dir"]).arg(&d.root).args([
        "--port",
        &port.to_string(),
        "--no-open",
        "--bot-username",
        "FixtureBot",
    ]);
    let answer = support::output(&mut second, Duration::from_secs(15));
    assert!(
        answer.status.success(),
        "{}{}",
        String::from_utf8_lossy(&answer.stdout),
        String::from_utf8_lossy(&answer.stderr)
    );
    assert!(String::from_utf8_lossy(&answer.stdout).contains("already running"));
    assert!(first.0.try_wait().unwrap().is_none());
    assert_eq!(
        client
            .get(format!("http://127.0.0.1:{port}/api/ping"))
            .send()
            .unwrap()
            .status(),
        200
    );
}
// OriginalID: test_panel_server.ConnectionsAreReused.test_two_requests_share_one_connection
#[test]
fn server_two_requests_share_one_connection() {
    let s = Server::with_token(0, "keep-alive-token");
    let mut conn = support::Connection::new(s.port, "keep-alive-token");
    for _ in 0..2 {
        assert_eq!(conn.request("GET", "/api/ping", &[], None).status, 200);
    }
    assert_eq!(s.accepted.load(Ordering::SeqCst), 1);
}
// OriginalID: test_panel_server.ConnectionsAreReused.test_an_oversized_body_closes_the_connection
#[test]
fn server_an_oversized_body_closes_the_connection() {
    let s = Server::with_token(0, "keep-alive-token");
    let mut conn = support::Connection::new(s.port, "keep-alive-token");
    let answer = conn.request("POST", "/api/save", &[], Some("99999999"));
    assert_eq!(answer.status, 413);
    assert_eq!(
        answer.headers.get("connection").map(String::as_str),
        Some("close")
    );
}
// OriginalID: test_panel_server.ConnectionsAreReused.test_the_page_refuses_framing_and_sniffing
#[test]
fn server_the_page_refuses_framing_and_sniffing() {
    let s = Server::with_token(0, "keep-alive-token");
    let mut conn = support::Connection::new(s.port, "keep-alive-token");
    let page = conn.request("GET", "/", &[], None);
    assert_eq!(page.status, 200);
    assert_eq!(page.headers["x-content-type-options"], "nosniff");
    assert_eq!(page.headers["referrer-policy"], "no-referrer");
    assert_eq!(page.headers["x-frame-options"], "DENY");
    assert_eq!(
        page.headers["content-security-policy"],
        "frame-ancestors 'none'"
    );
    let ver = runtime::asset_version::versions(&s.data.root).unwrap().0;
    let script = conn.request("GET", &format!("/static/panel-grid.js?v={ver}"), &[], None);
    assert_eq!(script.status, 200);
    assert_eq!(script.headers["x-content-type-options"], "nosniff");
    assert!(!script.headers.contains_key("x-frame-options"));
}
// OriginalID: test_panel_server.ConnectionsAreReused.test_an_empty_answer_keeps_the_connection_usable
#[test]
fn server_an_empty_answer_keeps_the_connection_usable() {
    let s = Server::with_token(0, "keep-alive-token");
    let mut conn = support::Connection::new(s.port, "keep-alive-token");
    let answer = conn.request(
        "POST",
        "/api/client-log",
        br#"{"events": [{"event": "ready"}]}"#,
        None,
    );
    assert!(answer.body.is_empty());
    assert_eq!(answer.status, 204);
    assert!(!answer.headers.contains_key("content-length"));
    assert_eq!(conn.request("GET", "/api/ping", &[], None).status, 200);
    assert_eq!(s.accepted.load(Ordering::SeqCst), 1);
}
// OriginalID: test_panel.CatalogUnavailable.test_save_answers_503
#[test]
fn panel_save_answers_503() {
    let s = Server::new(1);
    support::obstruct_catalog(&s.data);
    let (code, body) = s.post("/api/save", json!({"excluded":[key(0)],"known":[key(0)]}));
    assert_eq!(code, 503);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("catalog unavailable")
    );
}
// OriginalID: test_panel.CatalogUnavailable.test_order_answers_503
#[test]
fn panel_order_answers_503() {
    let s = Server::new(1);
    support::obstruct_catalog(&s.data);
    let (code, body) = s.post("/api/order", json!({"order":[key(0)]}));
    assert_eq!(code, 503);
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("catalog unavailable")
    );
}
// OriginalID: test_panel.CatalogUnavailable.test_a_page_served_from_a_busy_catalog_says_so
#[test]
fn panel_a_page_served_from_a_busy_catalog_says_so() {
    let s = Server::new(1);
    let cached_at = s
        .panel
        .last_refreshed
        .lock()
        .unwrap()
        .as_ref()
        .cloned()
        .unwrap();
    support::obstruct_catalog(&s.data);
    let page = s.page();
    let raw = page
        .split("const STALE = ")
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let stale = serde_json::from_str::<String>(raw).unwrap();
    assert_eq!(stale, cached_at.format("%H:%M").to_string());
    assert_eq!(
        s.panel.last_refreshed.lock().unwrap().as_ref(),
        Some(&cached_at)
    );
    assert_eq!(
        stale.len(),
        5,
        "stale view must retain the last-refresh HH:MM timestamp"
    );
    assert!(stale.bytes().enumerate().all(|(i, b)| if i == 2 {
        b == b':'
    } else {
        b.is_ascii_digit()
    }));
    for key in &s.data.keys {
        assert!(page.contains(key));
    }
    // Also cover a cache populated by the production startup task, before the
    // first HTTP request has had an opportunity to refresh it.
    support::startup_warm_cache_survives_catalog_failure();
}
// OriginalID: test_panel.SavingFromAFilteredGridKeepsHiddenChoices.test_a_hidden_exclusion_survives_a_save_from_the_filtered_grid
#[test]
fn panel_a_hidden_exclusion_survives_a_save_from_the_filtered_grid() {
    let d = Data::new(4);
    for i in [0, 1] {
        d.publish(i, Some("pk1_by_bot"));
    }
    d.cat().inclusion(&[key(0)]).unwrap();
    std::fs::write(
        d.root.join("publish_pk.json"),
        json!({"sets":[{"name":"pk1_by_bot","index":1,"live":200,"logo":true}]}).to_string(),
    )
    .unwrap();
    let v = d.view(false, json!({}), false, None);
    assert_eq!(v["hidden"], 2);
    let visible = support::keys(&v);
    assert!(!visible.contains(&key(0)));
    let mut cat = d.cat();
    let hidden = cat
        .all(None)
        .unwrap()
        .iter()
        .filter(|x| x["included"] == false && !visible.contains(x["content_key"].as_str().unwrap()))
        .map(|x| x["content_key"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    cat.inclusion(&hidden).unwrap();
    assert_eq!(d.excluded(), BTreeSet::from([key(0)]));
}
// OriginalID: test_panel.TheMediaMapIsNeverEmpty.test_a_reader_never_misses_a_key_both_maps_hold
#[test]
fn panel_a_reader_never_misses_a_key_both_maps_hold() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    let s = Server::new(0);
    let art = s.data.root.join("shared.png");
    support::png(&art, [10, 20, 30, 255], 40);
    let map = |start: usize, end: usize| {
        let mut out = serde_json::Map::new();
        for i in start..end {
            out.insert(format!("k{i}"), json!(art));
        }
        json!(out)
    };
    let a = map(0, 200);
    let b = map(100, 300);
    s.panel.view.lock().unwrap()["by_key"] = a.clone();
    let done = Arc::new(AtomicBool::new(false));
    let (checked, ready) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let done_read = done.clone();
        let client = s.client.clone();
        let url = s.url("");
        let reader = scope.spawn(move || {
            let mut missed = Vec::new();
            let mut announced = false;
            while !done_read.load(Ordering::SeqCst) {
                for key in ["k100", "k150", "k199"] {
                    if client
                        .get(format!("{url}/img/{key}"))
                        .send()
                        .unwrap()
                        .status()
                        != 200
                    {
                        missed.push(key);
                    }
                }
                if !announced {
                    checked.send(()).unwrap();
                    announced = true;
                }
            }
            missed
        });
        ready.recv_timeout(Duration::from_secs(10)).unwrap();
        for i in 0..1000 {
            let mut view = s.panel.view.lock().unwrap();
            view["by_key"] = b.clone();
            view["by_key"] = a.clone();
            drop(view);
            if i % 20 == 0 {
                std::thread::yield_now();
            }
        }
        done.store(true, Ordering::SeqCst);
        assert!(reader.join().unwrap().is_empty());
    });
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
