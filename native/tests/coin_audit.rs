//! Source-observed resume/reconcile/report behavior uses virtual transport and time.
use _native::coin_audit::{AuditIo, run};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;

fn image_fixture() -> Value {
    let text =
        include_str!("../../tests/fixtures/native/coin-audit-image.json").replace("\r\n", "\n");
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

#[test]
fn original_audit_recipe_refuses_input_drift_without_trusting_expected_outputs() {
    let fixture = audit_fixture();
    _native::source_coin_audit_oracle::fixture_identity(&fixture).unwrap();
    let mut changed = fixture.clone();
    changed["cases"][0]["expected"]["code"] = json!(4);
    _native::source_coin_audit_oracle::fixture_identity(&changed).unwrap();
    for (text, expected) in [
        (
            include_str!("../../tests/fixtures/native/coin-audit.json"),
            "74d21e13f82d61d2cd3f4b137633f869195b09196808d61312b1a46b58e9e9ec",
        ),
        (
            include_str!("../../tests/fixtures/native/coin-audit-image.json"),
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
    _native::source_coin_audit_oracle::image_fixture_identity(&images).unwrap();
    let mut observations = images.clone();
    observations["cases"][0]["expected"] = json!({"hash":"not-an-oracle","blank":false});
    _native::source_coin_audit_oracle::image_fixture_identity(&observations).unwrap();
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
        assert!(_native::source_coin_audit_oracle::image_fixture_identity(&changed).is_err());
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
            _native::source_coin_audit_oracle::fixture_identity(&changed).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn original_audit_classes_refuse_unadmitted_sources_before_workspace_or_runtime() {
    use _native::source_git_inventory::Source;
    let sources = [
        (
            "coins/check_all_packs.py",
            "f62da85bd939464a2673a758740cab9001ae0920a000df32f7827d42485b5272",
        ),
        (
            "emojikit/media.py",
            "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
        ),
        (
            "emojikit/cli_env.py",
            "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
        ),
        (
            "tests/test_check_all_packs.py",
            "33e02cce8a29db515d9babd96bc35de9452b687b7715e2c6cc59bdbc9f0214e0",
        ),
        (
            "tests/_coin_fixtures.py",
            "eb95df6b040a2b57ba521ec7f84405dad33cf3be7c7de61d8ccffb3d50475389",
        ),
        (
            "tests/reference/packstate.py",
            "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
        ),
    ]
    .map(|(path, sha256)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: sha256.into(),
        text: "must not execute".into(),
    });
    let admitted = sources.iter().collect::<Vec<_>>();
    assert!(
        _native::source_coin_audit_tests::source_identity(&admitted)
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(
        _native::source_coin_audit_tests::source_identity(&admitted[..5])
            .unwrap_err()
            .contains("exactly six")
    );
    let missing = std::path::Path::new("unavailable-audit-test-workspace");
    assert!(
        _native::source_coin_audit_tests::observe(missing, missing, &admitted)
            .unwrap_err()
            .contains("source identity drift")
    );
}

// Behavioral coverage, not the original Python monkeypatch/sentinel assertion.
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
    let text = include_str!("../../tests/fixtures/native/coin-audit.json").replace("\r\n", "\n");
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

// Original CheckAllPacksTest cache-reconciliation assertions at the public audit seam.
#[test]
fn test_deleted_sticker_leaves_totals_and_duplicate_groups() {
    let fixture = audit_fixture();
    let case = &fixture["cases"][0];
    assert_eq!(
        case["input"]["source_test"],
        "tests.test_check_all_packs.CheckAllPacksTest.test_deleted_sticker_leaves_totals_and_duplicate_groups"
    );
    let mut cache = case["input"]["cache"].as_object().unwrap().clone();
    let io = observe(case, &mut cache);
    let report = io.reports.first().unwrap();
    assert!(report.contains("live stickers: 2"));
    assert!(report.contains("DUPLICATE image groups: 0"));
    assert!(!serde_json::to_string(&cache).unwrap().contains("gone"));
}

// The second invocation consumes the first run's cache, never a fresh golden reset.
#[test]
fn test_analyse_failure_is_an_error_that_is_retried_not_a_blank() {
    let fixture = audit_fixture();
    let failed = &fixture["cases"][1];
    assert_eq!(
        failed["input"]["source_test"],
        "tests.test_check_all_packs.CheckAllPacksTest.test_analyse_failure_is_an_error_that_is_retried_not_a_blank"
    );
    let mut cache = failed["input"]["cache"].as_object().unwrap().clone();
    let io = observe(failed, &mut cache);
    let report = io.reports.first().unwrap();
    assert!(report.contains("FAILED (retried on next run): 1"));
    assert!(report.contains("BLANK stickers: 0"));
    assert!(cache["b"].as_object().unwrap().contains_key("error"));
    assert!(!serde_json::to_string(&cache).unwrap().contains("TOKEN123"));
    let resumed = &fixture["cases"][2];
    assert_eq!(json!(cache), resumed["input"]["cache"]);
    assert_eq!(failed["input"]["sets"], resumed["input"]["sets"]);
    let io = observe(resumed, &mut cache);
    assert_eq!(io.downloads, ["b"]);
    assert!(
        io.reports
            .first()
            .unwrap()
            .contains("FAILED (retried on next run): 0")
    );
}

#[test]
fn test_blank_sticker_is_still_reported_with_live_position() {
    let images = image_fixture();
    let image_cases = images["cases"].as_array().unwrap();
    let visible = image_cases
        .iter()
        .find(|case| {
            case["input"] == "tests/fixtures/native/provider-image/visible-16-alpha-128.png"
        })
        .unwrap();
    let blank = image_cases
        .iter()
        .find(|case| {
            case["input"] == "tests/fixtures/native/provider-image/visible-0-alpha-255.png"
        })
        .unwrap();
    assert_eq!(visible["expected"]["blank"], false);
    assert_eq!(blank["expected"]["blank"], true);
    let mut io = Fixture {
        input: json!({"sets":{"s1":["a","b"]},"images":{"a":visible,"b":blank}}),
        calls: vec![],
        downloads: vec![],
        writes: vec![],
        waits: vec![],
        reports: vec![],
    };
    let mut cache = Map::new();
    let fixture = audit_fixture();
    assert_eq!(
        fixture["cases"][3]["input"]["source_test"],
        "tests.test_check_all_packs.CheckAllPacksTest.test_blank_sticker_is_still_reported_with_live_position"
    );
    assert_eq!(
        u64::from(run(&[json!({"index":1,"name":"s1"})], &mut cache, &mut io).unwrap()),
        fixture["cases"][3]["expected"]["code"].as_u64().unwrap()
    );
    assert_eq!(io.downloads, ["a", "b"]);
    assert!(
        io.reports
            .first()
            .unwrap()
            .contains("blank cid=b set=1 pos=1")
    );
    for (cid, expected) in [("a", visible), ("b", blank)] {
        assert_eq!(cache[cid], expected["expected"], "{cid}");
    }
}

#[test]
fn test_a_throttled_listing_is_retried_instead_of_ending_the_run() {
    let fixture = audit_fixture();
    let case = &fixture["cases"][4];
    assert_eq!(
        case["input"]["source_test"],
        "tests.test_check_all_packs.CheckAllPacksListingRetry.test_a_throttled_listing_is_retried_instead_of_ending_the_run"
    );
    let mut cache = case["input"]["cache"].as_object().unwrap().clone();
    let io = observe(case, &mut cache);
    assert_eq!(
        io.calls.len(),
        3,
        "the first two failures must not be fatal"
    );
    assert!(io.reports.first().unwrap().contains("live stickers: 1"));
    // Retry stdout/redaction belongs to the CLI boundary, not this virtual-I/O seam.
}

#[test]
fn test_work_already_analysed_survives_a_set_that_will_not_list() {
    let fixture = audit_fixture();
    let case = &fixture["cases"][5];
    assert_eq!(
        case["input"]["source_test"],
        "tests.test_check_all_packs.CheckAllPacksListingRetry.test_work_already_analysed_survives_a_set_that_will_not_list"
    );
    // The source oracle also protects a stale cached ID from an incomplete live read.
    let mut cache = case["input"]["cache"].as_object().unwrap().clone();
    let io = observe(case, &mut cache);
    assert!(
        io.reports.is_empty(),
        "a partial live read cannot produce a report"
    );
    assert_eq!(cache["gone"], case["input"]["cache"]["gone"]);

    // Preserve the original test's empty-cache variant, not only the stronger oracle variant.
    let mut cache = Map::new();
    let mut io = Fixture {
        input: case["input"].clone(),
        calls: vec![],
        downloads: vec![],
        writes: vec![],
        waits: vec![],
        reports: vec![],
    };
    assert_eq!(
        u64::from(
            run(
                &[
                    json!({"index":1,"name":"s1"}),
                    json!({"index":2,"name":"s2"})
                ],
                &mut cache,
                &mut io,
            )
            .unwrap()
        ),
        case["expected"]["code"].as_u64().unwrap()
    );
    assert_eq!(cache.keys().map(String::as_str).collect::<Vec<_>>(), ["a"]);
    assert_eq!(cache["a"], case["expected"]["writes"][0]["a"]);
    assert_eq!(json!(io.calls), case["expected"]["calls"]);
    assert_eq!(json!(io.downloads), case["expected"]["downloads"]);
    assert!(
        io.reports.is_empty(),
        "a partial live read cannot produce a report"
    );
    assert_eq!(
        io.writes.len(),
        case["expected"]["writes"].as_array().unwrap().len()
    );
    for written in &io.writes {
        assert_eq!(
            written
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["a"]
        );
        assert_eq!(written["a"], case["expected"]["writes"][0]["a"]);
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
