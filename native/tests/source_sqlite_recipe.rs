//! Pure recipe admission keeps exact SQLite integer/blob inputs separate from source observations.
use _native::source_sqlite_oracle;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn fixture(bytes: &[u8], expected_hash: &str) -> Value {
    let hash = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(hash, expected_hash, "historical fixture bytes drift");
    serde_json::from_slice(bytes).unwrap()
}

#[test]
fn signature_recipe_pins_exact_signed_rows_but_source_must_replay_digest_and_repr() {
    let original = fixture(
        include_bytes!("../../tests/fixtures/backend/signature.json"),
        "9dfb74734872d03eac101cf41b9a177021315b90e077d07e7cc1c1052faddf0d",
    );
    assert_eq!(original["cases"][0]["rows"][0][1].as_i64(), Some(i64::MIN));
    assert_eq!(original["cases"][0]["rows"][1][1].as_i64(), Some(i64::MAX));
    source_sqlite_oracle::fixture_signature_identity(&original).unwrap();
    let mut sorted = original.clone();
    sorted.sort_all_objects();
    source_sqlite_oracle::fixture_signature_identity(&sorted).unwrap();
    for field in ["expected", "repr_order"] {
        let mut changed = original.clone();
        changed["cases"][0][field] = json!(["source must reject this observation"]);
        source_sqlite_oracle::fixture_signature_identity(&changed).unwrap();
    }
    for mutation in ["integer", "blob", "path", "name", "order", "extra"] {
        let mut changed = original.clone();
        match mutation {
            "integer" => changed["cases"][0]["rows"][0][1] = json!(i64::MIN + 1),
            "blob" => changed["cases"][0]["rows"][0][3]["blob"] = json!("ff"),
            "path" => changed["cases"][1]["files"][0]["source"] = json!("./other.png"),
            "name" => changed["cases"][0]["name"] = json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "extra" => changed["unchecked-metadata"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            source_sqlite_oracle::fixture_signature_identity(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
}

#[test]
fn rewrite_recipe_pins_unsigned_high_bit_null_mapping_and_case_order() {
    let original = fixture(
        include_bytes!("../../tests/fixtures/backend/migration-database.json"),
        "ebfd0bbbcc48f5eb6b0f36f403c140669d82e08f30207f7608efdc84ca6737a9",
    );
    assert_eq!(
        original["cases"][0]["hashes"]["v:a"].as_u64(),
        Some(1_u64 << 63)
    );
    assert!(original["cases"][1]["hashes"]["v:c"].is_null());
    source_sqlite_oracle::fixture_database_identity(&original).unwrap();
    let mut observation = original.clone();
    observation["cases"][0]["expected"] = json!({"target-derived":"source must reject"});
    source_sqlite_oracle::fixture_database_identity(&observation).unwrap();
    for mutation in ["hash", "mapping", "name", "order", "extra"] {
        let mut changed = original.clone();
        match mutation {
            "hash" => changed["cases"][0]["hashes"]["v:a"] = json!((1_u64 << 63) + 1),
            "mapping" => changed["cases"][0]["map"]["v:a"] = json!("v:c"),
            "name" => changed["cases"][0]["name"] = json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "extra" => changed["unchecked-metadata"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            source_sqlite_oracle::fixture_database_identity(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
}
