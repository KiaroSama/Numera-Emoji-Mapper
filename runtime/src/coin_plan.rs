//! Frozen coin groups and alias replay preserve source order; no positional identity.
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn group(
    files: &[Value],
    inventory: &[String],
    keywords: &Map<String, Value>,
) -> Result<Value, String> {
    let mut files = files.iter().collect::<Vec<_>>();
    for file in &files {
        for field in ["filename", "ticker", "hash"] {
            file[field]
                .as_str()
                .ok_or_else(|| format!("coin input {field} must be a string"))?;
        }
    }
    files.sort_by_key(|file| file["filename"].as_str());
    let mut hashes: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for file in files {
        hashes
            .entry(file["hash"].as_str().ok_or("coin hash is missing")?)
            .or_default()
            .insert(file["ticker"].as_str().ok_or("coin ticker is missing")?);
    }
    let mut groups = Vec::new();
    for (hash, tickers) in hashes {
        let rep = tickers
            .iter()
            .find(|ticker| inventory.iter().any(|own| own == **ticker))
            .or_else(|| tickers.first())
            .ok_or("coin group is empty")?;
        let mut ordered = vec![*rep];
        ordered.extend(tickers.iter().copied().filter(|ticker| ticker != rep));
        let labels = ordered
            .iter()
            .map(|ticker| {
                keywords
                    .get(*ticker)
                    .map(|label| label.as_str().ok_or("coin keyword must be a string"))
                    .unwrap_or(Ok(*ticker))
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        groups.push(json!({"rep":rep,"tickers":ordered,"kw":labels,"hash":hash}));
    }
    groups.sort_by(|left, right| left["rep"].as_str().cmp(&right["rep"].as_str()));
    Ok(Value::Array(groups))
}

pub fn replay(
    new: &Map<String, Value>,
    current: &Map<String, Value>,
    approved: &[String],
) -> Result<Value, String> {
    let mut mapping = new.clone();
    let mut old_groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (ticker, id) in current {
        old_groups
            .entry(id.as_str().ok_or("coin map ID must be a string")?)
            .or_default()
            .push(ticker);
    }
    for (ticker, id) in current {
        if mapping.contains_key(ticker) {
            continue;
        }
        let id = id.as_str().ok_or("coin map ID must be a string")?;
        if let Some(siblings) = old_groups.get(id) {
            for sibling in siblings {
                if let Some(new_id) = mapping.get(*sibling).cloned() {
                    mapping.insert(ticker.clone(), new_id);
                    break;
                }
            }
        }
    }
    let mut missing = Vec::new();
    for (ticker, id) in current {
        if !mapping.contains_key(ticker)
            && old_groups
                .get(id.as_str().ok_or("coin ID must be a string")?)
                .is_some_and(|group| group.len() > 1)
        {
            missing.push(ticker);
        }
    }
    missing.sort();
    let mut groups: Map<String, Value> = Map::new();
    for (ticker, id) in &mapping {
        let id = id.as_str().ok_or("coin map ID must be a string")?;
        groups
            .entry(id)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("invalid shared group")?
            .push(json!(ticker));
    }
    groups.retain(|_, tickers| {
        tickers.as_array().is_some_and(|group| {
            group.len() > 1
                && !group
                    .iter()
                    .all(|ticker| approved.iter().any(|value| ticker.as_str() == Some(value)))
        })
    });
    Ok(json!({"mapping":mapping,"missing":missing,"unapproved":groups}))
}
