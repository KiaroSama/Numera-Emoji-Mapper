//! Supplemental cross-family lookup/fingerprint preservation.
use crate::root::Root;
use _native::config::Config;
use numera_runtime as _native;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
fn config(root: &Path) -> Config {
    Config::load_with_environment(
        root.into(),
        &BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("COLLECTION_PACK_BASE".into(), "mypacks".into()),
        ]),
    )
    .unwrap()
}
// Native supplemental: native/tests/roster_state.rs::original_family_merge_preserves_other_lookups_and_old_input_fingerprints
#[test]
fn original_family_merge_preserves_other_lookups_and_old_input_fingerprints() {
    let old = json!({"packs":[{"set_name":"gen1_by_bot","family":"general","count":1},{"set_name":"coin1_by_bot","family":"coins","count":1}],
        "by_current_id":{"g1":{"set":"gen1_by_bot","slot":0,"index":0},"c-old":{"set":"coin1_by_bot","slot":0,"index":0}},
        "by_source_id":{"src-g":"g1","src-c":"c-old"},"by_previous_id":{"g0":"g1"},
        "inputs":{"catalog.db":[1,1],"publish_state.json":[2,2],"rebuild_dedup_state.json":[3,3],"ticker_to_id.json":[4,4]}});
    let inputs = json!({"catalog.db":[9,9],"publish_state.json":[9,9],"rebuild_dedup_state.json":[5,5],"ticker_to_id.json":[6,6]});
    let coin = json!({"set_name":"coin1_by_bot","family":"coins","count":1,"emoji":[{"custom_emoji_id":"c-new","slot":0,"index":0,"source_emoji_ids":["src-c"],"previous_custom_emoji_ids":[]}]});
    let index =
        _native::roster_index::build(&[coin], "coins", &old, &inputs, "fixture UTC").unwrap();
    let names = index["packs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["set_name"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(names, BTreeSet::from(["gen1_by_bot", "coin1_by_bot"]));
    assert!(
        index["by_current_id"].get("g1").is_some() && index["by_current_id"].get("c-new").is_some()
    );
    assert!(index["by_current_id"].get("c-old").is_none());
    assert_eq!(index["by_source_id"], json!({"src-g":"g1","src-c":"c-new"}));
    assert_eq!(index["by_previous_id"], json!({"g0":"g1"}));
    assert_eq!(index["pack_count"], 2);
    assert_eq!(index["inputs"]["catalog.db"], json!([1, 1]));
    assert_eq!(index["inputs"]["publish_state.json"], json!([2, 2]));
    assert_eq!(index["inputs"]["rebuild_dedup_state.json"], json!([5, 5]));
    assert_eq!(index["inputs"]["ticker_to_id.json"], json!([6, 6]));
    let root = Root::new("roster family refusal café");
    let log = _native::logging::RunLog::open(&root.0, "roster-test", vec![]);
    assert!(
        _native::roster_cli::run(
            &config(&root.0),
            &["--check".into(), "--family".into(), "coins".into()],
            &log
        )
        .unwrap_err()
        .contains("--check covers all families")
    );
    assert!(!root.0.join("packs").exists());
    drop(log);
}
