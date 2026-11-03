//! Original shared table assertions through the public renderer and all four report writers.
use _native::{catalog::Catalog, pack_rows};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

const HOSTILE: &str = "left|right\nnext line";
const ESCAPED: &str = "left\\|right next line";

fn row(text: &str, header: &str) {
    assert!(
        text.contains(header),
        "original writer column headers changed"
    );
    assert!(text.contains(ESCAPED));
    assert!(
        !text.contains("left|right"),
        "a raw pipe breaks the original table"
    );
}

#[test]
fn original_shared_rows_keep_splitlines_alignment_empty_cells_and_every_keyword() {
    let table = pack_rows::markdown(&[json!({"a":HOSTILE,"b":""})], &[("a", "A"), ("b", "B")]);
    assert_eq!(table.lines().nth(2), Some("| left\\|right next line | — |"));
    // The historical Python wrapper stringifies before JSON; native public inputs do the same.
    let table = pack_rows::aligned(
        &[
            json!({"a":"one\r\ntwo\u{001c}next","b":"True"}),
            json!({"a":"","b":"123"}),
        ],
        &[("a", "Label", false), ("b", "Count", true)],
    );
    assert_eq!(
        table,
        "| Label | Count |\n|-------|-----:|\n| one two next | True |\n| — | 123 |"
    );
    assert_eq!(
        pack_rows::keywords(&json!({"keywords":["a","","b","c"]})),
        "a, b, c"
    );
    assert_eq!(pack_rows::keywords(&json!({"keywords":null})), "");
}

#[test]
fn original_all_report_writers_keep_hostile_labels_headers_and_complete_keywords() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let root = Root::new("shared reports café");
    let data = root.0.join("collection");
    std::fs::create_dir_all(data.join("media")).unwrap();
    let image = data.join("media/a.png");
    let source = project.join("tests/fixtures/native/static-codec/alpha-edge.png");
    assert_eq!(
        _native::tooling_manifest::digest(&source).unwrap(),
        "d359d28b44f1871b588f04dc95ca56f8716b18768b6b91ef3f9aad66ed01aa71"
    );
    let original = std::fs::read(&source).unwrap();
    std::fs::write(&image, &original).unwrap();
    let mut catalog =
        Catalog::open(&data.join("catalog.db"), "2026-10-07 00:00:00 UTC", &root.0).unwrap();
    assert!(
        catalog
            .insert(
                &json!({"content_key":"s:a","fmt":"static","file_path":image,
        "emojis":["😀"],"keywords":["one","two",HOSTILE]}),
                "2026-10-07 00:00:00 UTC"
            )
            .unwrap()
    );
    _native::collection_manifest::write(
        &data,
        &mut catalog,
        &json!({"name":"pk1_by_bot","title":"P","fmt":"static","keys":["s:a"]}),
        "pk",
    )
    .unwrap();
    let publisher = std::fs::read_to_string(data.join("manifests/pk1_by_bot.md")).unwrap();
    row(&publisher, "| # | Name | Emoji ID |");
    assert!(
        publisher.contains("one, two, left"),
        "all keywords must survive, not the first two"
    );
    let archive = _native::archive_rows::manifest(
        &json!({"name":"set1","title":"One","fmt":"mixed"}),
        &[json!({"content_key":"s:a","premium_id":"111"})],
        &json!({"s:a":{"keywords":[HOSTILE]}}),
    )
    .unwrap();
    row(&archive, "| # | Name | Emoji ID |");
    let mut document: Value = _native::roster::pack(
        &json!({"name":"s","index":1,"logo":false}),
        &json!({"stickers":[{"custom_emoji_id":"111","emoji":"✅"}]}),
        "general",
        &json!({}),
        &json!({}),
        "2026-10-07 00:00:00 UTC",
    )
    .unwrap();
    document["emoji"][0]["name"] = json!(HOSTILE);
    row(
        &_native::roster_markdown::pack(&document).unwrap(),
        "| # | slot | custom_emoji_id | ours before | glyph | format | name | was |",
    );
    let coin = _native::coin_manifest::render(
        &json!({"name":"coin1_by_bot","title":"C 1"}),
        &[json!({"custom_emoji_id":"111"})],
        &BTreeMap::from([("111".into(), vec![HOSTILE.into()])]),
    )
    .unwrap();
    row(&coin, "| # | Ticker(s) | Emoji ID |");
    drop(catalog);
    assert_eq!(std::fs::read(image).unwrap(), original);
    assert_eq!(std::fs::read(source).unwrap(), original);
    root.clean().unwrap();
}
