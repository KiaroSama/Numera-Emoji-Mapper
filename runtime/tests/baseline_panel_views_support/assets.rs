use super::*;

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
