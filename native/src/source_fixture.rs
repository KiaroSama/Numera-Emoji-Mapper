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
