//! Original candidate nomination/call order and pure card model use unchanged source corpora.
use _native::reconcile::NearMatch;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "support/bounded_media.rs"]
mod bounded_media;
#[path = "support/panel_asset_cases.rs"]
mod panel_asset_cases;
#[path = "support/panel_concurrency_cases.rs"]
mod panel_concurrency_cases;
#[path = "support/panel_connection_cases.rs"]
mod panel_connection_cases;
#[path = "support/panel_logging_cases.rs"]
mod panel_logging_cases;
#[path = "support/panel_migration_cases.rs"]
mod panel_migration_cases;
#[path = "support/panel_persistence_cases.rs"]
mod panel_persistence_cases;
#[path = "support/panel_preview_contracts.rs"]
mod panel_preview_contracts;
#[path = "support/panel_request_cases.rs"]
mod panel_request_cases;
#[path = "support/panel_session_cases.rs"]
mod panel_session_cases;
#[path = "support/panel_stale_cases.rs"]
mod panel_stale_cases;
#[path = "support/panel_view_cases.rs"]
mod panel_view_cases;

#[test]
fn original_served_text_actions_and_grid_keep_all_gesture_and_layout_contracts() {
    panel_asset_cases::check();
}

#[test]
fn original_panel_scoped_saves_and_intent_reloads_preserve_committed_decisions() {
    panel_persistence_cases::check();
}
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[test]
fn original_panel_membership_branding_and_copy_contracts_use_the_real_catalog() {
    panel_view_cases::check();
}

#[test]
fn original_similarity_groups_and_first_minimum_ties_preserve_source_orders() {
    let text = include_str!("../../tests/fixtures/similarity.json").replace("\r\n", "\n");
    let hash = Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        hash,
        "34df25f072dc51a8812006223465259493c8499815e8e91d9df52c3f0a680dcb"
    );
    let fixture: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(fixture["greedy"].as_array().unwrap().len(), 6);
    for case in fixture["greedy"].as_array().unwrap() {
        let hashes = case["hashes"].as_array().unwrap();
        let mut items = vec![json!({"fmt":"animated","phash":null,"identity":"animated-plain"})];
        for format in ["video", "static"] {
            items.push(json!({"fmt":format,"phash":null,"identity":format!("{format}-plain")}));
            items.extend(hashes.iter().enumerate().map(
                |(i, hash)| json!({"fmt":format,"phash":hash,"identity":format!("{format}-{i}")}),
            ));
        }
        let before = items.clone();
        let sorted = _native::panel_view::similarity(&items).unwrap();
        let mut expected = Vec::new();
        for format in ["static", "video"] {
            expected.extend(
                case["expected"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|index| format!("{format}-{}", index.as_u64().unwrap())),
            );
            expected.push(format!("{format}-plain"));
        }
        expected.push("animated-plain".into());
        assert_eq!(
            sorted
                .iter()
                .map(|item| item["identity"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(items, before);
    }
    let items = json!([{"fmt":"animated","phash":null},{"fmt":"static","phash":1},{"fmt":"static","phash":2}]);
    let sorted = _native::panel_view::similarity(items.as_array().unwrap()).unwrap();
    assert_eq!(
        sorted
            .iter()
            .map(|item| item["fmt"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["static", "static", "animated"]
    );
}

#[test]
fn original_served_scripts_copy_before_toggle_and_play_videos_without_hover() {
    let actions = include_str!("../../assets/panel-actions.js");
    let grid = include_str!("../../assets/panel-grid.js");
    let copy = actions.find("closest('.copyable')").unwrap();
    let toggle = actions[copy.saturating_sub(400)..]
        .find("closest('.card')")
        .unwrap()
        + copy.saturating_sub(400);
    assert!(copy < toggle);
    assert!(actions[copy..copy + 200].contains("e.stopPropagation()"));
    assert!(
        grid.replace("dataset.play", "data-play")
            .contains("data-play")
    );
    assert!(grid.contains("video[data-play]"));
    assert!(actions.contains("if (RM) {"));
    for junk in [
        "xpremium-id:12",
        "premium-id:12x",
        "premium-id:",
        "premium-id:12 34",
        "",
        "coin logo",
        "premium-id:abc",
    ] {
        assert_eq!(_native::panel_view::copy_id(junk), "", "{junk}");
    }
    assert_eq!(
        _native::panel_view::copy_id("premium-id:5406926593698312391"),
        "5406926593698312391"
    );
    for (label, expected) in [
        ("premium-id:۱۲۳۴۵", "۱۲۳۴۵"),
        ("premium-id:１２３４５", "１２３４５"),
        ("premium-id:123\n", "123"),
        ("premium-id:123\r\n", ""),
        ("premium-id:²", ""),
        ("premium-id:123\n\n", ""),
    ] {
        assert_eq!(_native::panel_view::copy_id(label), expected);
    }
}

#[test]
fn original_panel_assets_keep_versions_load_order_and_data_only_inline_block() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let page = include_str!("../../assets/panel.html");
    let files = [
        "panel-grid.js",
        "panel-motion.js",
        "panel-save.js",
        "panel-drag.js",
        "panel-actions.js",
        "panel-holding.js",
        "panel-draft.js",
    ];
    let mut previous = page.find("<script id=\"items-data\"").unwrap();
    for name in files {
        let path = root.join("assets").join(name);
        assert!(path.is_file());
        assert!(
            path.canonicalize()
                .unwrap()
                .starts_with(root.canonicalize().unwrap())
        );
        let tag = format!("<script src=\"/static/{name}?v=__ASSET_VER__\"></script>");
        let position = page.find(&tag).unwrap();
        assert!(previous < position);
        previous = position;
    }
    assert_eq!(
        _native::asset_version::versions(root).unwrap(),
        ("23a5e175b383".into(), "4636a2cbdf7a".into())
    );
    assert!(!page.contains("\"/static/logo-128.png\""));
    assert_eq!(
        page.matches("/static/logo-128.png?v=__ICON_VER__").count(),
        2
    );
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
        assert!(inline.contains(token), "{token}");
    }
}

fn fixture(bytes: &[u8], digest: &str, source: &str, count: usize) -> Value {
    let actual = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(actual, digest, "independent source fixture drift");
    let document: Value = serde_json::from_slice(bytes).unwrap();
    _native::source_fixture::cases(&document, source, count).unwrap();
    document
}

#[test]
fn reconciliation_oracle_pins_inputs_but_replays_results_and_call_order() {
    let original: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/backend/reconcile.json")).unwrap();
    _native::source_reconcile_oracle::fixture_identity(&original).unwrap();
    let mut trace_only = original.clone();
    trace_only["cases"][4]["calls"] = json!(["s:blue", "s:red"]);
    _native::source_reconcile_oracle::fixture_identity(&trace_only).unwrap();
    let mut output_only = original.clone();
    output_only["cases"][0]["expected"] = json!("unverified");
    _native::source_reconcile_oracle::fixture_identity(&output_only).unwrap();
    for mutation in ["input", "name", "order", "comparison"] {
        let mut changed = original.clone();
        match mutation {
            "input" => changed["cases"][4]["probe"] = json!(255),
            "name" => changed["cases"][0]["name"] = json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "comparison" => changed["cases"][4]["comparison"]["s:red"] = json!(false),
            _ => unreachable!(),
        }
        assert!(
            _native::source_reconcile_oracle::fixture_identity(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
}

#[test]
fn view_oracle_rejects_coordinated_input_result_and_case_identity_drift() {
    let original: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/backend/view.json")).unwrap();
    _native::source_view_oracle::fixture_identity(&original).unwrap();
    for mutation in ["input-result", "name", "order", "extra"] {
        let mut changed = original.clone();
        match mutation {
            "input-result" => {
                changed["cases"][0]["request"]["items"][0]["keywords"] = json!(["changed"]);
                changed["cases"][0]["expected"]["view"][0]["label"] = json!("changed");
                changed["cases"][0]["expected"]["view"][0]["copyId"] = json!("");
            }
            "name" => changed["cases"][0]["name"] = json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "extra" => changed["unused"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            _native::source_view_oracle::fixture_identity(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
    let mut observed_only = original.clone();
    observed_only["cases"][0]["expected"]["hidden"] = json!(0);
    _native::source_view_oracle::fixture_identity(&observed_only).unwrap();
    let mut reordered_objects = original;
    reordered_objects.sort_all_objects();
    _native::source_view_oracle::fixture_identity(&reordered_objects).unwrap();
}

#[test]
fn original_near_candidates_keep_all_eight_results_and_comparison_call_order() {
    let document = fixture(
        include_bytes!("../../tests/fixtures/backend/reconcile.json"),
        "bc430cc2de230c5e5791601be1e9297e0d6f4c9702134e355f3d56a5465a27a7",
        "d173116:collection_reconcile._near_catalog_match",
        8,
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    for case in document["cases"].as_array().unwrap() {
        let mut calls = Vec::new();
        let result = runtime
            .block_on(_native::reconcile::near_catalog(
                case["items"].as_array().unwrap(),
                case["fmt"].as_str().unwrap(),
                case["probe"].as_u64(),
                case["lookup"].as_str().unwrap(),
                async |item: &Value| {
                    let key = item["content_key"]
                        .as_str()
                        .ok_or("fixture item lacks key")?;
                    calls.push(key.to_owned());
                    let evidence = case["comparison"]
                        .get(key)
                        .ok_or("unexpected comparison call")?;
                    match evidence {
                        Value::Bool(value) => Ok(Some(*value)),
                        Value::Null => Ok(None),
                        _ => Err("invalid fixture comparison".into()),
                    }
                },
            ))
            .unwrap();
        let expected = match &case["expected"] {
            Value::Null => NearMatch::Missing,
            Value::String(value) if value == "undecidable" => NearMatch::Undecidable,
            Value::String(value) if value == "ambiguous" => NearMatch::Ambiguous,
            Value::String(value) => NearMatch::Unique(value.clone()),
            _ => panic!("invalid source result"),
        };
        assert_eq!(result, expected, "{}", case["name"]);
        assert_eq!(json!(calls), case["calls"], "{}", case["name"]);
    }
}

#[test]
fn original_view_visibility_membership_and_labels_match_three_source_outcomes() {
    let document = fixture(
        include_bytes!("../../tests/fixtures/backend/view.json"),
        "4c54a770153bc4c3887512215f689f571e37ae2311a22dd5aa6bdcba93359122",
        "d173116:panel_view.build_view",
        3,
    );
    for case in document["cases"].as_array().unwrap() {
        assert_eq!(
            _native::panel_view::build(case["request"].clone()).unwrap(),
            case["expected"],
            "{}",
            case["name"],
        );
    }
}
