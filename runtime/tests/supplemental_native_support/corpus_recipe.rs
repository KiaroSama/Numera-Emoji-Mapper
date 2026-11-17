// Retained pure frozen input admission from native/src/source_fixture.rs; SHA256 635f668090697f26ee2af5e549d6c9e6cf9b7f9e911bb493adc58e57ee4a2c75.
//! Shared corpus admission preserves the original source-oracle validation contract.
use serde_json::Value;
use std::collections::BTreeSet;

pub fn cases<'a>(document: &'a Value, source: &str, count: usize) -> Result<&'a [Value], String> {
    if document.get("source").and_then(Value::as_str) != Some(source) {
        return Err("Fixture source does not match its pinned seam".into());
    }
    let cases = document
        .get("cases")
        .and_then(Value::as_array)
        .filter(|cases| !cases.is_empty() && cases.len() == count)
        .ok_or_else(|| format!("Expected {count} non-empty oracle cases"))?;
    let mut names = BTreeSet::new();
    for case in cases {
        let name = case
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| !name.is_empty())
            .ok_or("Oracle case names must be non-empty and unique")?;
        if !names.insert(name) {
            return Err("Oracle case names must be non-empty and unique".into());
        }
        if case.get("expected").is_some() == case.get("error").is_some() {
            return Err(format!(
                "Oracle case {name} needs exactly one expected result or error"
            ));
        }
        if let Some(error) = case.get("error")
            && error.as_str().is_none_or(str::is_empty)
        {
            return Err(format!("Oracle case {name} has no meaningful source error"));
        }
    }
    Ok(cases)
}

/// Pin the historical input recipe separately from the outputs source replay must observe.
pub(crate) fn recipe(
    document: &Value,
    source: &str,
    count: usize,
    hash: &str,
) -> Result<(), String> {
    recipe_with_observations(document, source, count, hash, &[])
}

pub(crate) fn recipe_with_observations(
    document: &Value,
    source: &str,
    count: usize,
    hash: &str,
    observations: &[&str],
) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    cases(document, source, count)?;
    let mut recipe = document.clone();
    if let Some(cases) = recipe["cases"].as_array_mut() {
        for case in cases {
            if let Some(case) = case.as_object_mut() {
                case.remove("expected");
                case.remove("error");
                for &field in observations {
                    case.remove(field);
                }
            }
        }
    }
    recipe.sort_all_objects();
    let bytes = serde_json::to_vec(&recipe).map_err(|_| "cannot encode source oracle recipe")?;
    let actual = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if actual != hash {
        return Err("source oracle recipe drift; refusing execution".into());
    }
    Ok(())
}

pub fn bot(fixture: &Value) -> Result<(), String> {
    recipe(
        fixture,
        "d173116:emoji_bot.pure_payloads",
        6,
        "fa6b256a78abe5255867d5960d6c9233f7265c99426edca96c988c355d3241d9",
    )
}

pub fn roster(fixture: &Value) -> Result<(), String> {
    recipe(
        fixture,
        "d173116:pack_manifest._row",
        4,
        "4cb0be295bc5ae632482ed9dfb41172498aee0afeeb6deda52fa48815c0d2a51",
    )
}

pub fn intent(fixture: &Value) -> Result<(), String> {
    recipe(
        fixture,
        "d173116:emojikit/panel_plan.py",
        8,
        "01939b9f49d2bda93bf39b38ad2fd897973674fd712fa2cb9d8d2c732df15882",
    )
}

pub fn resume(fixture: &Value) -> Result<(), String> {
    recipe(
        fixture,
        "d173116:collection_state._validate_state",
        8,
        "eef2e51afff050f934ae7fd244a5ee2c89ac482166d30723f133c185efcd15d4",
    )
}

pub fn reconcile(fixture: &Value) -> Result<(), String> {
    recipe_with_observations(
        fixture,
        "d173116:collection_reconcile._near_catalog_match",
        8,
        "eba14460b2fe430e7b4df37cebdd80a9c71a3a1970cdc073b6f9e8a8b9323266",
        &["calls"],
    )
}

pub fn view(fixture: &Value) -> Result<(), String> {
    recipe(
        fixture,
        "d173116:panel_view.build_view",
        3,
        "bfb1d18cfc11a82ec8b35dc454c318b84a14517871e7a8e56fb920e60609d3c2",
    )
}
