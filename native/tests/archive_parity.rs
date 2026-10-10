//! Exact source archive row/naming/metadata expectations, without network or media mutation.
use serde_json::Value;
use std::path::PathBuf;
#[test]
fn archive_metadata_matches_original_source() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let value: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/fixtures/backend/archive.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(value["source"], "d173116:pack_archive.names+rows+reports");
    assert_eq!(value["cases"].as_array().unwrap().len(), 1);
    for case in value["cases"].as_array().unwrap() {
        let rows = _native::archive_rows::rows(
            &case["record"],
            case["live"].as_array().unwrap(),
            &case["items"],
            &case["keys"],
        )
        .unwrap();
        assert_eq!(serde_json::json!(rows), case["expected"]);
        assert_eq!(
            _native::archive_rows::history(&case["record"], &rows).unwrap(),
            case["history_markdown"].as_str().unwrap()
        );
        assert_eq!(
            _native::archive_rows::manifest(&case["record"], &rows, &case["items"]).unwrap(),
            case["manifest_markdown"].as_str().unwrap()
        );
    }
    for case in value["folders"].as_array().unwrap() {
        assert_eq!(
            _native::archive_rows::folder(case["title"].as_str().unwrap()),
            case["folder"].as_str().unwrap()
        );
    }
    assert_eq!(
        _native::archive_rows::name(7, "animated", &format!("a:{}", "b".repeat(32)), ".tgs")
            .unwrap(),
        value["archive_name"].as_str().unwrap()
    );
}
