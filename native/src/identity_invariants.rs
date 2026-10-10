//! Completion evidence checks SQL references and actual media instead of trusting a migration label.
use crate::{identity_survey, media_adapter::MediaAdapter};
use serde_json::json;
use std::path::Path;
pub fn issues(
    data: &Path,
    root: &Path,
    media: &MediaAdapter,
    runtime: &tokio::runtime::Runtime,
) -> Result<Vec<String>, String> {
    let mut issues = Vec::new();
    for (name, keys) in crate::identity_references::stale(data, true)? {
        issues.push(format!(
            "missing required keys in {name}: {}",
            keys.join(", ")
        ));
    }
    for (name, keys) in crate::identity_references::stale(data, false)? {
        if keys
            .iter()
            .any(|key| key == "<unreadable>" || key == "<invalid state>")
        {
            issues.push(format!("unreadable migration references in {name}"));
        }
    }
    let db = identity_survey::read(data)?;
    for table in ["publications", "seen_files"] {
        let count:i64=db.query_row(&format!("SELECT COUNT(*) FROM {table} WHERE content_key NOT IN (SELECT content_key FROM items)"),[],|r|r.get(0)).map_err(|e|e.to_string())?;
        if count > 0 {
            issues.push(format!("{table} has {count} orphaned reference(s)"));
        }
    }
    let integrity: String = db
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if integrity != "ok" {
        issues.push(format!("database integrity: {integrity}"));
    }
    let mut query = db
        .prepare("SELECT content_key,file_path,format FROM items")
        .map_err(|e| e.to_string())?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .map(|r| r.map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    drop(query);
    drop(db);
    for (key, path, fmt) in rows {
        let source = crate::paths::resolve(data, &path, root);
        if !source.is_file() {
            issues.push(format!("missing catalog media: {key}"));
            continue;
        }
        if fmt == "video" {
            continue;
        }
        let result = runtime.block_on(async {
            let value = media
                .call(json!({"operation":"fingerprint","source":source,"format":fmt}))
                .await?;
            let mut fresh = value["content_key"]
                .as_str()
                .ok_or("codec returned no identity")?
                .to_owned();
            if key.matches(':').count() == 2 {
                fresh = media
                    .call(json!({"operation":"collision","source":source,"format":fmt,"key":fresh}))
                    .await?
                    .as_str()
                    .ok_or("invalid collision identity")?
                    .into();
            }
            Ok::<_, String>(fresh)
        });
        match result {
            Ok(fresh) if fresh != key => issues.push(format!("non-video identity mismatch: {key}")),
            Ok(_) => {}
            Err(error) => issues.push(format!("non-video identity undecodable: {key}: {error}")),
        }
    }
    Ok(issues)
}
