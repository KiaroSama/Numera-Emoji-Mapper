//! Real source image bytes and pinned cache/download/request traces, no live network or sleeps.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;
#[test]
fn valid_cache_skips_download_and_error_bodies_never_become_logo_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/gecko-fetch.json")).unwrap();
    for case in fixture["tickers"].as_array().unwrap() {
        assert_eq!(
            _native::coin_gecko_fetch::safe_ticker(case["input"].as_str().unwrap()),
            case["expected"]
        );
    }
    let bytes = std::fs::read(root.join(fixture["input"].as_str().unwrap())).unwrap();
    let digest = |b: &[u8]| {
        Sha256::digest(b)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    assert_eq!(digest(&bytes), fixture["input_sha256"]);
    let scratch = root
        .join("logs/tmp")
        .join(format!("gecko fetch {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let directory = scratch.join(index.to_string());
        std::fs::create_dir_all(directory.join("logos/png")).unwrap();
        let path = directory.join("logos/png/btc.png");
        match case["initial"].as_str().unwrap() {
            "valid" => std::fs::write(&path, &bytes).unwrap(),
            "corrupt" => std::fs::write(&path, b"error page").unwrap(),
            _ => {}
        }
        let mut calls = Vec::new();
        let mut waits = Vec::new();
        let result=_native::coin_gecko_fetch::fetch_with(&directory,1,Duration::from_secs(2),|url,binary,retries|{
            calls.push(serde_json::json!([url,binary,retries]));
            if !binary{
                if case["outcome"]=="failure"{return Err("controlled unavailable".into());}
                Ok(_native::coin_gecko_fetch::Payload::Json(serde_json::json!([{"symbol":"BTC","name":"Bitcoin","image":"http://example.invalid/btc.png"}])))
            }else{Ok(_native::coin_gecko_fetch::Payload::Bytes(if case["outcome"]=="valid"{bytes.clone()}else{b"error page".to_vec()}))}
        },|duration|{waits.push(duration.as_secs_f64());Ok(())}).unwrap();
        assert_eq!(serde_json::json!(result), case["expected"]["code"]);
        assert_eq!(serde_json::json!(calls), case["expected"]["calls"]);
        assert_eq!(serde_json::json!(waits), case["expected"]["waits"]);
        assert_eq!(
            std::fs::read(directory.join("keywords.csv")).unwrap(),
            case["expected"]["csv"].as_str().unwrap().as_bytes()
        );
        assert_eq!(
            serde_json::json!(std::fs::read(&path).ok().map(|b| digest(&b))),
            case["expected"]["destination_sha256"]
        );
    }
    std::fs::remove_dir_all(scratch).unwrap();
}
