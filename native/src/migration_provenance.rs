//! Legacy recovery requires shared unambiguous immutable identifiers, never paths or archive slots.
use crate::media_adapter::MediaAdapter;
use rusqlite::{Connection, OpenFlags, types::Value as SqlValue};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
type Ids = BTreeMap<(String, String), BTreeSet<String>>;
fn snapshot(
    path: &Path,
    data: &Path,
    root: &Path,
) -> Result<(BTreeMap<String, PathBuf>, Ids), String> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let mut query = db
        .prepare("SELECT content_key,file_path FROM items WHERE format='video'")
        .map_err(|e| e.to_string())?;
    let rows = query
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?
        .map(|r| r.map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let rows = rows
        .into_iter()
        .map(|(key, path)| (key, crate::paths::resolve(data, &path, root)))
        .collect();
    let mut identifiers = Ids::new();
    let mut collect = |kind: &str, sql: &str| -> Result<(), String> {
        let mut query = db.prepare(sql).map_err(|e| e.to_string())?;
        for row in query
            .query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, SqlValue>(1)?))
            })
            .map_err(|e| e.to_string())?
        {
            let (key, value) = row.map_err(|e| e.to_string())?;
            let value = match value {
                SqlValue::Text(text) => Some(text),
                SqlValue::Integer(n) => Some(n.to_string()),
                _ => None,
            };
            if let Some(value) = value.filter(|v| !v.trim().is_empty()) {
                identifiers
                    .entry((kind.into(), value))
                    .or_default()
                    .insert(key);
            }
        }
        Ok(())
    };
    collect("fuid", "SELECT content_key,file_unique_id FROM seen_files")?;
    collect(
        "cid",
        "SELECT content_key,custom_emoji_id FROM publications",
    )?;
    let mut query = db
        .prepare("PRAGMA table_info(items)")
        .map_err(|e| e.to_string())?;
    let columns = query
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .map(|r| r.map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if columns.iter().any(|c| c == "custom_emoji_id") {
        collect("cid", "SELECT content_key,custom_emoji_id FROM items")?;
    }
    Ok((rows, identifiers))
}
pub fn recover(
    data: &Path,
    root: &Path,
    backup: &Path,
    media: &MediaAdapter,
    runtime: &tokio::runtime::Runtime,
) -> Result<BTreeMap<String, String>, String> {
    let (previous, old_ids) = snapshot(backup, data, root)?;
    let (current, current_ids) = snapshot(&data.join("catalog.db"), data, root)?;
    let mut old_targets = BTreeMap::<String, BTreeSet<String>>::new();
    let mut current_sources = BTreeMap::<String, BTreeSet<String>>::new();
    let mut ambiguous_old = BTreeSet::new();
    let mut ambiguous_current = BTreeSet::new();
    for (identifier, sources) in &old_ids {
        let Some(targets) = current_ids.get(identifier) else {
            continue;
        };
        if sources.len() != 1 || targets.len() != 1 {
            ambiguous_old.extend(sources.iter().cloned());
            ambiguous_current.extend(targets.iter().cloned());
            continue;
        }
        let source = sources.first().ok_or("missing identifier source")?;
        let target = targets.first().ok_or("missing identifier target")?;
        old_targets
            .entry(source.clone())
            .or_default()
            .insert(target.clone());
        current_sources
            .entry(target.clone())
            .or_default()
            .insert(source.clone());
    }
    let mut map = BTreeMap::new();
    for key in previous.keys() {
        let Some(candidates) = old_targets.get(key).filter(|v| v.len() == 1) else {
            continue;
        };
        if ambiguous_old.contains(key) {
            continue;
        }
        let target = candidates.first().ok_or("missing target")?;
        if target == key
            || ambiguous_current.contains(target)
            || current_sources.get(target) != Some(&BTreeSet::from([key.clone()]))
        {
            continue;
        }
        let Some(path) = current.get(target) else {
            continue;
        };
        let fresh = runtime.block_on(async {
            let value = media
                .call(json!({"operation":"fingerprint","source":path,"format":"video"}))
                .await?;
            let mut fresh = value["content_key"]
                .as_str()
                .ok_or("missing identity")?
                .to_owned();
            if target.matches(':').count() == 2 {
                fresh = media
                    .call(
                        json!({"operation":"collision","source":path,"format":"video","key":fresh}),
                    )
                    .await?
                    .as_str()
                    .ok_or("invalid collision key")?
                    .into();
            }
            Ok::<_, String>(fresh)
        });
        if fresh.as_deref() == Ok(target.as_str()) {
            map.insert(key.clone(), target.clone());
        }
    }
    Ok(map)
}
