//! Additional native product properties beyond the 1516 historical collected IDs.
//! Kept unwired until the owner completes the supplemental target.
use crate::root::Root;
use numera_runtime as rt;
use rusqlite::Connection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    let mut remaining = 64;
    while let Some(path) = pending.pop() {
        assert!(remaining > 0, "bounded native supplemental inventory");
        remaining -= 1;
        rt::tooling_environment::reject_links(&path, "fixture cannot follow links").unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        let key = path.strip_prefix(root).unwrap().to_owned();
        if metadata.is_dir() {
            found.insert(key, None);
            pending.extend(std::fs::read_dir(path).unwrap().map(|e| e.unwrap().path()));
        } else {
            assert!(metadata.is_file() && metadata.len() <= 65536);
            found.insert(key, Some(std::fs::read(path).unwrap()));
        }
    }
    found
}

// Native supplemental: native/tests/bot_roster_parity.rs::original_bot_extraction_keeps_entity_source_order_and_refuses_markup
#[test]
fn hostile_extracted_id_is_escaped_and_refused_at_the_payload_sink() {
    let bad = "1\"><b>x</b>";
    let escaped = rt::bot_payloads::escape(bad);
    assert!(!escaped.contains("<b>"));
    assert!(escaped.contains("&lt;b&gt;"));
    assert!(rt::bot_payloads::payloads(&[bad.into()], &json!({}), true).is_err());
}

// Native supplemental: native/tests/bot_roster_parity.rs::original_bot_typed_lists_preserve_whole_message_grammar_and_unicode_decimals
#[test]
fn typed_unicode_control_whitespace_and_numeric_boundaries() {
    let ascii = "5893098741073715471";
    let unicode = "۵۸۹۳۰۹۸۷۴۱۰۷۳۷۱۵۴۷۱";
    assert_eq!(
        rt::bot_payloads::typed(&format!("\u{1c}{unicode}\u{1f};, {ascii}\u{1e}")),
        vec![unicode, ascii]
    );
    assert!(rt::bot_payloads::typed(&"²".repeat(19)).is_empty());
    assert!(rt::bot_payloads::typed(&"9".repeat(26)).is_empty());
    for text in [
        format!(",{ascii}"),
        format!(";{ascii}"),
        format!("{ascii},"),
        format!("{ascii};"),
        format!("{ascii}, \n"),
    ] {
        assert!(rt::bot_payloads::typed(&text).is_empty(), "{text:?}");
    }
}

// Native supplemental: native/tests/bot_roster_parity.rs::shared_worker_emoji_messages_preserve_every_extracted_and_typed_id
#[test]
fn shared_worker_message_fixture_has_its_independent_hash_and_seven_cases() {
    let bytes = include_bytes!("../../../tests/fixtures/contracts/emoji_messages.json");
    assert_eq!(
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        "25e3883b746f98dd366361536b327a143583d8fedde2a7003cbb9d50a1e3d7db"
    );
    let cases: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(cases.as_array().unwrap().len(), 7);
    // The retained baseline_units contracts_shared test executes both business parsers for every case.
}

// Native supplemental: native/tests/plan_steps_parity.rs::original_four_hundred_key_plan_finds_all_398_removals_within_ten_seconds
#[test]
fn four_hundred_key_swap_counts_all_796_remove_and_add_changes() {
    let keys = (0..400).map(|i| format!("s:{i:04}")).collect::<Vec<_>>();
    let items = keys
        .iter()
        .enumerate()
        .map(|(i, k)| (k.clone(), json!({"included":true,"pos":i})))
        .collect::<serde_json::Map<_, _>>();
    let state = json!({"sets":[{"fmt":"mixed","index":1,"name":"a","logo":true,"keys":&keys[..199]},{"fmt":"mixed","index":2,"name":"b","logo":true,"keys":&keys[199..398]}]});
    let targets = keys[..199]
        .iter()
        .map(|k| json!([k, 2]))
        .chain(keys[199..398].iter().map(|k| json!([k, 1])))
        .collect::<Vec<_>>();
    let ids = keys
        .iter()
        .map(|k| (k.clone(), json!(format!("cid-{k}"))))
        .collect::<serde_json::Map<_, _>>();
    let started = std::time::Instant::now();
    let result = rt::plan_steps::compute(
        &json!({"plan":{"targets":targets},"state":state,"items":items,"ids":ids}),
    )
    .unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(result["removals"].as_array().unwrap().len(), 398);
    assert_eq!(result["counted"], 796);
}

// Native supplemental: native/tests/plan_steps_parity.rs::original_plan_report_counts_moves_holding_candidates_and_over_capacity
#[test]
fn retired_id_report_distinguishes_an_explicitly_empty_live_set() {
    let state = json!({"sets":[{"index":1,"name":"mine1_by_bot","live":4,"keys":["s:a","s:b","s:c"]},{"index":2,"name":"mine2_by_bot","live":2,"keys":["s:d"]}]});
    let plan = json!({"version":1,"per_set":200,"targets":[["s:a",2],["s:c",1],["s:d",2],["s:new",2]],"counts":{"1":2,"2":1},"logo_slots":{"1":1,"2":1},"over_capacity":{},"moves":[{"key":"s:a","label":"a","from_pack":1,"to_pack":2}],"held":[{"key":"s:b","label":"b","from_pack":1}],"excluded":["s:b"]});
    let summary = rt::plan_status::summarize(&plan, &state).unwrap();
    let ids = BTreeMap::from([("s:a".into(), "5000000000000000001".into())]);
    let report = rt::plan_status::render(&summary, &ids, None).unwrap();
    assert!(report.contains("5000000000000000001  s:a"));
    assert!(report.contains("RETIRE"));
    let absent = rt::plan_status::render(&summary, &ids, Some(&BTreeSet::new())).unwrap();
    assert!(absent.contains("5000000000000000001  s:a  (not live now)"));
}

// Native supplemental: native/tests/status.rs::original_pending_count_is_read_only_and_unreadable_is_not_zero
#[test]
fn pending_queries_refuse_absent_and_broken_catalogs_without_creating_or_writing() {
    let root = Root::new("status pending café");
    let data = root.0.join("collection");
    std::fs::create_dir(&data).unwrap();
    let absent = data.join("absent.db");
    assert!(rt::catalog_read::pending(&absent, "mypacks").is_err());
    assert!(!absent.exists());
    let db = data.join("catalog.db");
    let connection = Connection::open(&db).unwrap();
    connection.execute_batch("CREATE TABLE items(content_key TEXT,included INTEGER);CREATE TABLE publications(base TEXT,content_key TEXT);INSERT INTO items VALUES('s:a',1),('s:b',1),('s:c',0);INSERT INTO publications VALUES('mypacks','s:a');").unwrap();
    connection.close().unwrap();
    let before = snapshot(&data);
    assert_eq!(rt::catalog_read::pending(&db, "mypacks").unwrap(), 1);
    assert_eq!(rt::catalog_read::pending(&db, "another").unwrap(), 2);
    assert_eq!(snapshot(&data), before);
    let broken = data.join("broken.db");
    std::fs::write(&broken, b"not a database").unwrap();
    let before = snapshot(&data);
    assert!(rt::catalog_read::pending(&broken, "mypacks").is_err());
    assert_eq!(snapshot(&data), before);
}
