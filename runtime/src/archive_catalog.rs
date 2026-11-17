//! Read-only archive projection; no schema migration or implicit media-path rewrite.
use crate::config::Config;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub fn read(config: &Config) -> Result<(Value, BTreeMap<String, Vec<String>>), String> {
    let data = config.root.join("collection");
    let path = data.join("catalog.db");
    if !path.is_file() {
        return Ok((json!({}), BTreeMap::new()));
    }
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    db.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let mut query = db.prepare("SELECT i.content_key,i.file_path,i.format,i.keywords,p.set_name,p.custom_emoji_id FROM items i JOIN publications p ON p.content_key=i.content_key WHERE p.base=?").map_err(|e|e.to_string())?;
    let rows = query
        .query_map([config.value("COLLECTION_PACK_BASE")], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut items = serde_json::Map::new();
    let mut sets = BTreeMap::<String, Vec<String>>::new();
    for row in rows {
        let (key, path, fmt, keywords, set, id) = row.map_err(|e| e.to_string())?;
        let keywords: Value = serde_json::from_str(
            keywords
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("[]"),
        )
        .map_err(|e| e.to_string())?;
        if !keywords.is_array() {
            return Err("catalog keywords must be a list".into());
        }
        if let Some(set) = &set {
            sets.entry(set.clone()).or_default().push(key.clone());
        }
        items.insert(key,json!({"path":crate::paths::resolve(&data,&path,&config.root),"fmt":fmt,"cid":id.unwrap_or_default(),"set":set,"keywords":keywords}));
    }
    Ok((json!(items), sets))
}
pub fn sets(config: &Config) -> Result<Vec<Value>, String> {
    let path = config.root.join("collection").join(format!(
        "publish_{}.json",
        config.value("COLLECTION_PACK_BASE")
    ));
    if !path.is_file() {
        return Ok(vec![]);
    }
    let value: Value = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(value["sets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| {
            r["name"].as_str().is_some_and(|s| !s.is_empty())
                && r["title"].as_str().is_some_and(|s| !s.is_empty())
        })
        .cloned()
        .collect())
}
