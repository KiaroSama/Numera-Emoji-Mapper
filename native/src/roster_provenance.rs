//! Roster provenance is read-only and joined by custom identity, never sticker position.
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
pub fn general(
    path: &Path,
    live_ids: &std::collections::BTreeSet<String>,
) -> Result<BTreeMap<String, Value>, String> {
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let mut statement=db.prepare("SELECT p.custom_emoji_id,i.content_key,i.keywords FROM publications p JOIN items i ON i.content_key=p.content_key WHERE p.custom_emoji_id IS NOT NULL").map_err(|e|e.to_string())?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (id, key, keywords) = row.map_err(|e| e.to_string())?;
        let keywords: Vec<String> = serde_json::from_str(&keywords).map_err(|e| e.to_string())?;
        let mut sources = Vec::new();
        let mut labels = Vec::new();
        for keyword in keywords {
            if let Some(id) = keyword.strip_prefix("premium-id:") {
                if !live_ids.contains(id) {
                    sources.push(id.to_owned());
                }
            } else {
                labels.push(keyword);
            }
        }
        out.insert(
            id,
            json!({"name":if labels.is_empty(){Value::Null}else{json!(labels.join(", "))},"content_key":key,"source_emoji_ids":sources}),
        );
    }
    Ok(out)
}
pub fn art(
    path: &Path,
    data: &Path,
    root: &Path,
) -> Result<BTreeMap<String, std::path::PathBuf>, String> {
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let mut query = db.prepare("SELECT p.custom_emoji_id,i.file_path FROM publications p JOIN items i ON i.content_key=p.content_key WHERE p.custom_emoji_id IS NOT NULL").map_err(|e|e.to_string())?;
    let mut out = BTreeMap::new();
    for row in query
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
    {
        let (id, path) = row.map_err(|e| e.to_string())?;
        out.insert(id, crate::paths::resolve(data, &path, root));
    }
    Ok(out)
}

pub fn coins(map: &Value) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::<String, Vec<String>>::new();
    for (ticker, id) in map.as_object().into_iter().flat_map(|v| v.iter()) {
        let id = match id {
            Value::String(id) => Some(id.clone()),
            Value::Number(id) if id.is_u64() || id.is_i64() => Some(id.to_string()),
            _ => None,
        };
        if let Some(id) = id {
            out.entry(id).or_default().push(ticker.clone());
        }
    }
    out.into_iter()
        .map(|(id, mut tickers)| {
            tickers.sort();
            (id, json!(tickers))
        })
        .collect()
}
pub fn history(documents: &[Value]) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for document in documents {
        for entry in document["emoji"].as_array().into_iter().flatten() {
            if let Some(key) = entry["history_key"].as_str().filter(|s| !s.is_empty()) {
                out.insert(key.into(),json!({"id":crate::roster::identity(&entry["custom_emoji_id"]),"history":entry["previous_custom_emoji_ids"].as_array().into_iter().flatten().map(crate::roster::identity).collect::<Vec<_>>()}));
            }
        }
    }
    out
}
