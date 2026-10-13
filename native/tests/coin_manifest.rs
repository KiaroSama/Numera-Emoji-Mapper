//! Independent source rendering and physical live-report writes share the public seam.
use _native::coin_manifest::{render, write};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn source_rows_keep_live_order_aliases_unmapped_and_hostile_markdown() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-manifest.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"],
        "64d9de164ec0bb767c745e7191f408eaab860d7a00ceb33ba453d25541e12ffa"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let aliases = case["aliases"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, values)| {
                (
                    key.clone(),
                    values
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_owned())
                        .collect(),
                )
            })
            .collect::<BTreeMap<String, Vec<String>>>();
        assert_eq!(
            render(&case["set"], case["stickers"].as_array().unwrap(), &aliases).unwrap(),
            case["expected"].as_str().unwrap()
        );
    }
    assert!(
        render(
            &json!({"name":"s"}),
            &[json!({"custom_emoji_id":[]})],
            &BTreeMap::new()
        )
        .is_err()
    );
}

#[test]
fn all_live_sets_are_known_before_any_report_changes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin manifest {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let out = scratch.join("manifests");
    std::fs::create_dir(&out).unwrap();
    std::fs::write(out.join("INDEX.md"), b"prior index").unwrap();
    std::fs::write(out.join("s1.md"), b"prior pack").unwrap();
    let state = json!({"sets":[{"name":"s2","index":2,"title":"Two"},{"name":"s1","index":1,"title":"One"}]});
    let mapping = json!({"z":"111","aaa":"111","other":"222"});
    let mut calls = Vec::new();
    assert!(
        write(&scratch, &state, &mapping, |name| {
            calls.push(name.to_owned());
            if name == "s2" {
                Err("controlled unknown live state".into())
            } else {
                Ok(json!({"stickers":[{"custom_emoji_id":"111"}]}))
            }
        })
        .is_err()
    );
    assert_eq!(calls, ["s1", "s2"]);
    assert_eq!(std::fs::read(out.join("INDEX.md")).unwrap(), b"prior index");
    assert_eq!(std::fs::read(out.join("s1.md")).unwrap(), b"prior pack");
    assert!(!out.join("s2.md").exists());
    let summary = write(&scratch, &state, &mapping, |name| Ok(json!({"stickers":
        if name=="s1" {vec![json!({"custom_emoji_id":"111"}),json!({"custom_emoji_id":"unknown"})]} else {vec![]}}))).unwrap();
    assert_eq!(summary, (2, 2));
    let text = std::fs::read_to_string(out.join("s1.md")).unwrap();
    assert!(text.contains("| 1 | aaa, z | 111 |"));
    assert!(text.contains("| 2 | (unmapped) | unknown |"));
    assert_eq!(
        std::fs::read_to_string(out.join("INDEX.md")).unwrap(),
        "# Coin Emoji Packs - Manifest Index\n\n| Pack | Stickers | Link |\n|------|----------|------|\n| s1 | 2 | https://t.me/addemoji/s1 |\n| s2 | 0 | https://t.me/addemoji/s2 |\n\nTotal: 2 packs, 2 stickers.\n"
    );
    assert!(
        write(
            &scratch,
            &json!({"sets":[{"name":"../escape","index":1}]}),
            &mapping,
            |_| panic!("unsafe filename contacted transport")
        )
        .is_err()
    );
    assert!(write(&scratch, &state, &mapping, |_| Ok(json!({"stickers":null}))).is_err());
    std::fs::remove_dir_all(scratch).unwrap();
}
