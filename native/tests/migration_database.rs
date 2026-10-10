//! Swaps preserve CID/FUID/labels rather than merging catalog rows by destination position.
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
#[test]
fn source_transaction_swaps_every_reference_and_preserves_signed_hashes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/fixtures/backend/migration-database.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fixture["source"],
        "d173116:migration_bundle.rewrite_database"
    );
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    for case in cases {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,phash INTEGER,label TEXT);CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);CREATE TABLE seen_files(content_key TEXT,file_unique_id TEXT);INSERT INTO items VALUES('v:a',123,'alpha'),('v:b',456,'beta');INSERT INTO publications VALUES('v:a','111111111'),('v:b','222222222');INSERT INTO seen_files VALUES('v:a','unique-a'),('v:b','unique-b');").unwrap();
        let map: BTreeMap<String, String> = serde_json::from_value(case["map"].clone()).unwrap();
        let hashes: BTreeMap<String, Option<u64>> =
            serde_json::from_value(case["hashes"].clone()).unwrap();
        let moved = _native::migration_database::rewrite(&mut db, &map, &hashes).unwrap();
        let mut query = db
            .prepare("SELECT * FROM items ORDER BY content_key")
            .unwrap();
        let items = query
            .query_map([], |r| {
                Ok(json!([
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, String>(2)?
                ]))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        let rows = |table: &str| {
            let mut query = db
                .prepare(&format!("SELECT * FROM {table} ORDER BY content_key"))
                .unwrap();
            query
                .query_map([], |r| {
                    Ok(json!([r.get::<_, String>(0)?, r.get::<_, String>(1)?]))
                })
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            json!({"moved":moved,"items":items,"publications":rows("publications"),"seen_files":rows("seen_files")}),
            case["expected"]
        );
    }
}
