// Retained native scenario source: native/tests/coin_audit.rs; original SHA256 0e07603e6c5f1dac2c3f4ae0e86bb3dd6ef81058a5767d52c4ab53284d266f7d.
//! Source-observed resume/reconcile/report behavior uses virtual transport and time.
use _native::coin_audit::{AuditIo, run};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;

fn image_fixture() -> Value {
    let text =
        include_str!("../../../tests/fixtures/native/coin-audit-image.json").replace("\r\n", "\n");
    let hash = Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        hash,
        "6f1fed3b033a76dc7bcd5a6c01520347c917ca34953016764b33d8c5fa2351a4"
    );
    let fixture: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_hashes"]["coins/check_all_packs.py"],
        "f62da85bd939464a2673a758740cab9001ae0920a000df32f7827d42485b5272"
    );
    assert_eq!(
        fixture["source_hashes"]["emojikit/media.py"],
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53"
    );
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 7);
    fixture
}

fn image_path(input: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(input)
}

fn image_bytes(case: &Value) -> Vec<u8> {
    let data = if let Some(path) = case["input"].as_str() {
        std::fs::read(image_path(path)).unwrap()
    } else {
        case["data"].as_str().unwrap().as_bytes().to_vec()
    };
    let hash = Sha256::digest(&data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(hash, case["input_sha256"].as_str().unwrap());
    data
}

struct Fixture {
    input: Value,
    calls: Vec<String>,
    downloads: Vec<String>,
    writes: Vec<Value>,
    waits: Vec<f64>,
    reports: Vec<String>,
}
impl AuditIo for Fixture {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String> {
        self.calls.push(name.into());
        let count = self.calls.iter().filter(|called| *called == name).count();
        if count as u64 <= self.input["failures"][name].as_u64().unwrap_or(0) {
            return Err("listing refused".into());
        }
        Ok(self.input["sets"][name]
            .as_array()
            .unwrap()
            .iter()
            .map(|cid| json!({"custom_emoji_id":cid,"file_id":cid}))
            .collect())
    }
    fn analyze(&mut self, sticker: &Value) -> Result<(String, bool), String> {
        let cid = sticker["file_id"].as_str().unwrap();
        self.downloads.push(cid.into());
        if let Some(image) = self.input["images"].get(cid) {
            return _native::codec_image::audit_image(&image_bytes(image));
        }
        let value = &self.input["analysis"][cid];
        if let Some(error) = value.as_str() {
            return Err(error.into());
        }
        Ok((
            value[0].as_str().unwrap().into(),
            value[1].as_bool().unwrap(),
        ))
    }
    fn save(&mut self, value: &Map<String, Value>) -> Result<(), String> {
        self.writes.push(json!(value));
        Ok(())
    }
    fn report(&mut self, text: &str) -> Result<(), String> {
        self.reports.push(text.into());
        Ok(())
    }
    fn wait(&mut self, duration: Duration) {
        self.waits.push(duration.as_secs_f64());
    }
}

fn audit_fixture() -> Value {
    let text = include_str!("../../../tests/fixtures/native/coin-audit.json").replace("\r\n", "\n");
    let hash = Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        hash,
        "74d21e13f82d61d2cd3f4b137633f869195b09196808d61312b1a46b58e9e9ec"
    );
    let fixture: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"],
        "f62da85bd939464a2673a758740cab9001ae0920a000df32f7827d42485b5272"
    );
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 7);
    fixture
}

fn observe(case: &Value, cache: &mut Map<String, Value>) -> Fixture {
    let input = &case["input"];
    let sets = input["sets"]
        .as_object()
        .unwrap()
        .keys()
        .enumerate()
        .map(|(index, name)| json!({"name":name,"index":index+1}))
        .collect::<Vec<_>>();
    let mut io = Fixture {
        input: input.clone(),
        calls: vec![],
        downloads: vec![],
        writes: vec![],
        waits: vec![],
        reports: vec![],
    };
    let expected = &case["expected"];
    assert_eq!(
        u64::from(run(&sets, cache, &mut io).unwrap()),
        expected["code"].as_u64().unwrap(),
        "{input}"
    );
    assert_eq!(json!(io.calls), expected["calls"], "{input}");
    assert_eq!(json!(io.downloads), expected["downloads"], "{input}");
    assert_eq!(json!(io.writes), expected["writes"], "{input}");
    assert_eq!(
        &json!(cache),
        expected["writes"].as_array().unwrap().last().unwrap(),
        "{input}"
    );
    let expected_waits = expected["waits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_f64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(io.waits, expected_waits, "{input}");
    assert_eq!(json!(io.reports), expected["reports"], "{input}");
    io
}

#[test]
fn original_audit_recipe_refuses_input_drift_without_trusting_expected_outputs() {
    let fixture = audit_fixture();
    crate::support::frozen_audit::fixture_identity(&fixture).unwrap();
    let mut changed = fixture.clone();
    changed["cases"][0]["expected"]["code"] = json!(4);
    crate::support::frozen_audit::fixture_identity(&changed).unwrap();
    for (text, expected) in [
        (
            include_str!("../../../tests/fixtures/native/coin-audit.json"),
            "74d21e13f82d61d2cd3f4b137633f869195b09196808d61312b1a46b58e9e9ec",
        ),
        (
            include_str!("../../../tests/fixtures/native/coin-audit-image.json"),
            "6f1fed3b033a76dc7bcd5a6c01520347c917ca34953016764b33d8c5fa2351a4",
        ),
    ] {
        let lf = text.replace("\r\n", "\n");
        for checkout in [lf.clone(), lf.replace('\n', "\r\n")] {
            assert_eq!(
                Sha256::digest(checkout.replace("\r\n", "\n").as_bytes())
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>(),
                expected
            );
        }
        assert_ne!(
            Sha256::digest(format!("{lf} ").as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            expected
        );
    }
    let images = image_fixture();
    crate::support::frozen_audit::image_fixture_identity(&images).unwrap();
    let mut observations = images.clone();
    observations["cases"][0]["expected"] = json!({"hash":"not-an-oracle","blank":false});
    crate::support::frozen_audit::image_fixture_identity(&observations).unwrap();
    for mutation in ["input", "hash", "inline", "order", "extra"] {
        let mut changed = images.clone();
        match mutation {
            "input" => changed["cases"][0]["input"] = json!("tests/fixtures/other.png"),
            "hash" => changed["cases"][0]["input_sha256"] = json!("0".repeat(64)),
            "inline" => changed["cases"][6]["data"] = json!("different corrupt image"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "extra" => changed["unverified"] = json!(true),
            _ => unreachable!(),
        }
        assert!(crate::support::frozen_audit::image_fixture_identity(&changed).is_err());
    }
    for mutation in ["analysis", "cache", "set-order", "count", "extra"] {
        let mut changed = fixture.clone();
        match mutation {
            "analysis" => changed["cases"][1]["input"]["analysis"]["a"] = json!(["changed", false]),
            "cache" => changed["cases"][0]["input"]["cache"]["a"]["hash"] = json!("changed"),
            "set-order" => {
                let sets = changed["cases"][5]["input"]["sets"]
                    .as_object_mut()
                    .unwrap();
                let first = sets.shift_remove("s1").unwrap();
                sets.insert("s1".into(), first);
            }
            "count" => {
                changed["cases"].as_array_mut().unwrap().pop();
            }
            "extra" => changed["unverified"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            crate::support::frozen_audit::fixture_identity(&changed).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn source_blank_boundary_pixels_agree_across_audit_and_pipeline() {
    let fixture = image_fixture();
    for case in fixture["cases"].as_array().unwrap() {
        let result = _native::codec_image::audit_image(&image_bytes(case));
        if case["expected"].get("error_type").is_some() {
            assert!(result.is_err(), "{case}");
        } else {
            let (hash, blank) = result.unwrap();
            assert_eq!(hash, case["expected"]["hash"].as_str().unwrap(), "{case}");
            assert_eq!(
                blank,
                case["expected"]["blank"].as_bool().unwrap(),
                "{case}"
            );
            let metadata =
                _native::codec_image::metadata(&image_path(case["input"].as_str().unwrap()), true)
                    .unwrap();
            assert_eq!(metadata["blank"], case["expected"]["blank"], "{case}");
        }
    }
}

#[test]
fn source_audit_preserves_duplicate_positions_and_empty_sets() {
    let fixture = audit_fixture();
    for index in [3, 6] {
        let case = &fixture["cases"][index];
        let mut cache = case["input"]["cache"].as_object().unwrap().clone();
        observe(case, &mut cache);
    }
    let mut live = Map::new();
    live.insert("x".into(), json!({"set":1,"pos":0}));
    assert!(
        _native::coin_audit::report(
            &Map::from_iter([("x".into(), json!({"hash":"h","blank":false}))]),
            &live
        )
        .unwrap()
        .0
        .contains("live stickers: 1")
    );
}
