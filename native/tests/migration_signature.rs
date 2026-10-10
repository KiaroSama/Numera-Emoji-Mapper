//! Version2 signature includes Python repr ordering, exact UTF8 payloads and path normalization.
use rusqlite::{Connection, params_from_iter, types::Value as SqlValue};
use serde_json::Value;
use std::path::PathBuf;
fn decode(value: &Value) -> SqlValue {
    match value {
        Value::Null => SqlValue::Null,
        Value::Number(n) => SqlValue::Integer(n.as_i64().unwrap()),
        Value::String(s) => SqlValue::Text(s.clone()),
        Value::Object(o) => {
            let hex = o["blob"].as_str().unwrap();
            SqlValue::Blob(
                (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect(),
            )
        }
        _ => panic!("unsupported fixture scalar"),
    }
}
#[test]
fn original_repr_sorted_sql_signature_matches_both_source_cases() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/fixtures/backend/signature.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        fixture["source"],
        "d173116:migration_bundle.database_signature"
    );
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 2);
    for case in fixture["cases"].as_array().unwrap() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE \"z weird\"(text TEXT,n INTEGER,nullable TEXT,b BLOB);CREATE TABLE items(content_key TEXT,file_path TEXT,phash INTEGER);CREATE TABLE aa(note TEXT);INSERT INTO aa VALUES(\"first table\");").unwrap();
        let mut repr = Vec::new();
        for row in case["rows"].as_array().unwrap() {
            let values = row
                .as_array()
                .unwrap()
                .iter()
                .map(decode)
                .collect::<Vec<_>>();
            repr.push(_native::python_repr::row(&values).unwrap());
            db.execute(
                "INSERT INTO \"z weird\" VALUES(?,?,?,?)",
                params_from_iter(&values),
            )
            .unwrap();
        }
        db.execute("INSERT INTO items VALUES('s:fixture','./after.png',-1)", [])
            .unwrap();
        repr.sort();
        assert_eq!(serde_json::json!(repr), case["repr_order"]);
        assert_eq!(
            _native::migration_signature::database(&db, case["files"].as_array().unwrap()).unwrap(),
            case["expected"].as_str().unwrap()
        );
        db.execute("INSERT INTO aa VALUES(1.25)", []).unwrap();
        // SQLite TEXT affinity converted this input to text; an actual REAL table must refuse.
        db.execute_batch(
            "CREATE TABLE real_values(value REAL);INSERT INTO real_values VALUES(1.25);",
        )
        .unwrap();
        assert!(
            _native::migration_signature::database(&db, &[])
                .unwrap_err()
                .contains("REAL formatting")
        );
    }
}
