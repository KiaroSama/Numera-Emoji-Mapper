//! Aggregate live/current/source/retired identities and untouched-family freshness separately.
use serde_json::{Value, json};

pub fn build(
    documents: &[Value],
    family: &str,
    old: &Value,
    inputs: &Value,
    captured: &str,
) -> Result<Value, String> {
    if !["all", "general", "coins"].contains(&family) {
        return Err("unknown roster family".into());
    }
    let mut packs = Vec::new();
    let mut current = serde_json::Map::new();
    let mut source = serde_json::Map::new();
    let mut previous = serde_json::Map::new();
    for doc in documents {
        let name = doc["set_name"].as_str().ok_or("roster has no name")?;
        let mut record = serde_json::Map::new();
        for key in ["set_name", "title", "family", "pack_index", "link", "count"] {
            record.insert(key.into(), doc[key].clone());
        }
        packs.push(json!(record));
        for entry in doc["emoji"].as_array().ok_or("roster has no entries")? {
            let id = entry["custom_emoji_id"]
                .as_str()
                .ok_or("entry has no custom emoji ID")?;
            current.insert(
                id.into(),
                json!({"set":name,"slot":entry["slot"],"index":entry["index"]}),
            );
            for (key, table) in [
                ("source_emoji_ids", &mut source),
                ("previous_custom_emoji_ids", &mut previous),
            ] {
                for id in entry[key].as_array().into_iter().flatten() {
                    table.insert(
                        id.as_str().ok_or("invalid source/history ID")?.into(),
                        entry["custom_emoji_id"].clone(),
                    );
                }
            }
        }
    }
    let mut inputs = inputs
        .as_object()
        .cloned()
        .ok_or("invalid input fingerprint")?;
    if family != "all" {
        let kept = old["packs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["family"] != family)
            .cloned()
            .collect::<Vec<_>>();
        let names = kept
            .iter()
            .filter_map(|p| p["set_name"].as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let kept_ids = old["by_current_id"]
            .as_object()
            .into_iter()
            .flat_map(|v| v.iter())
            .filter(|(_, location)| location["set"].as_str().is_some_and(|s| names.contains(s)))
            .collect::<Vec<_>>();
        for (id, location) in &kept_ids {
            current
                .entry((*id).clone())
                .or_insert_with(|| (*location).clone());
        }
        for (key, table) in [
            ("by_source_id", &mut source),
            ("by_previous_id", &mut previous),
        ] {
            for (id, value) in old[key].as_object().into_iter().flat_map(|v| v.iter()) {
                if value
                    .as_str()
                    .is_some_and(|v| kept_ids.iter().any(|(id, _)| id.as_str() == v))
                {
                    table.entry(id.clone()).or_insert_with(|| value.clone());
                }
            }
        }
        let mut merged = kept;
        merged.append(&mut packs);
        packs = merged;
        for (key, value) in &mut inputs {
            let general = key == "catalog.db" || key.starts_with("publish_");
            if general != (family == "general") {
                *value = old["inputs"].get(key).cloned().unwrap_or(Value::Null);
            }
        }
    }
    let count = packs
        .iter()
        .map(|p| p["count"].as_u64().ok_or("invalid pack count"))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum::<u64>();
    Ok(
        json!({"captured_utc":captured,"pack_count":packs.len(),"emoji_count":count,
        "packs":packs,"by_current_id":current,"by_source_id":source,"by_previous_id":previous,
        "id_changes":previous.len(),"inputs":inputs}),
    )
}
