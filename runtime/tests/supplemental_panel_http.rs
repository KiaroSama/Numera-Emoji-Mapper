//! Complete-page regressions for catalog text crossing the HTML template boundary.
use numera_runtime as runtime;
use serde_json::{Value, json};

#[allow(dead_code)]
#[path = "baseline_panel_views_support/mod.rs"]
mod support;
#[path = "support/test_admission.rs"]
mod test_admission;

fn page_with_labels(labels: &[&str]) -> (support::Server, String) {
    let data = support::Data::new(labels.len());
    {
        let db = rusqlite::Connection::open(&data.db).unwrap();
        for (key, label) in data.keys.iter().zip(labels) {
            db.execute(
                "UPDATE items SET keywords=?,emojis=? WHERE content_key=?",
                rusqlite::params![
                    json!([label]).to_string(),
                    json!([format!("glyph {label}")]).to_string(),
                    key
                ],
            )
            .unwrap();
        }
    }
    let panel = data.panel(support::options());
    let server = support::Server::start(data, panel);
    let page = server.page();
    (server, page)
}

fn items_json(page: &str) -> &str {
    page.split_once("<script id=\"items-data\" type=\"application/json\">")
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0
}

#[test]
fn placeholder_looking_catalog_text_roundtrips_through_the_served_page() {
    let labels = [
        "ordinary label",
        "__ITEMS__",
        "__TOKEN__",
        "__PER_SET__",
        "__HIDDEN__",
        "__ASSET_VER__",
        "__ICON_VER__",
        "prefix __TOKEN____PER_SET__ suffix",
    ];
    let (_server, page) = page_with_labels(&labels);
    let cards: Vec<Value> = serde_json::from_str(items_json(&page)).unwrap();
    for (card, expected) in cards.iter().zip(labels) {
        assert_eq!(card["label"], expected, "catalog labels are literal data");
        assert_eq!(card["emoji"], format!("glyph {expected}"));
    }
    assert_eq!(cards.len(), labels.len());
    assert!(page.contains("const TOKEN = \"test-token-value\";"));
    assert!(page.contains("const PER_SET = 200;"));
    assert!(page.contains("const HIDDEN = 0;"));
    assert!(page.contains("const STALE = \"\";"));
}

#[test]
fn json_valued_placeholders_in_labels_cannot_break_panel_boot_data() {
    let labels = [
        "__STALE__",
        "__PREVIEW_TIERS__",
        "before __STALE__ after __PREVIEW_TIERS__",
        "</script><script>throw Error('inert')</script> __STALE__ <>& \u{2028}\u{2029}",
    ];
    let (_server, page) = page_with_labels(&labels);
    let raw = items_json(&page);
    let cards: Vec<Value> = serde_json::from_str(raw)
        .expect("valid catalog text must remain valid JSON after the full template is rendered");
    assert_eq!(cards.len(), labels.len());
    for (card, expected) in cards.iter().zip(labels) {
        assert_eq!(card["label"], expected);
        assert_eq!(card["emoji"], format!("glyph {expected}"));
    }
    assert!(!raw.contains('<'));
    assert!(!raw.contains('>'));
    assert!(!raw.contains('&'));
    assert!(!raw.contains('\u{2028}'));
    assert!(!raw.contains('\u{2029}'));
    assert_eq!(page.matches("<script>").count(), 1);
}
