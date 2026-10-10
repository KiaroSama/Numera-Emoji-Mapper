//! Read-only identity survey keeps missing/undecodable/collision evidence separate from unchanged.
use crate::media_adapter::MediaAdapter;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
#[derive(Default)]
pub struct Survey {
    pub checked: u64,
    pub unchanged: u64,
    pub changed: Vec<(String, String, String)>,
    pub hashes: Vec<(String, Option<u64>, Option<u64>)>,
    pub missing: Vec<String>,
    pub undecodable: Vec<String>,
    pub collisions: BTreeMap<String, Vec<String>>,
    pub phashes: BTreeMap<String, Option<u64>>,
}
impl Survey {
    pub fn complete(&self) -> bool {
        self.missing.is_empty() && self.undecodable.is_empty()
    }
    pub fn pending(&self) -> bool {
        !self.changed.is_empty() || !self.hashes.is_empty() || !self.collisions.is_empty()
    }
    pub fn map(&self) -> BTreeMap<String, String> {
        self.changed
            .iter()
            .map(|(old, new, _)| (old.clone(), new.clone()))
            .collect()
    }
    pub fn json(&self) -> Value {
        json!({"checked":self.checked,"unchanged":self.unchanged,"changed":self.changed,"phash_only":self.hashes,"missing":self.missing,"undecodable":self.undecodable,"collisions":self.collisions,"phashes":self.phashes})
    }
}
pub fn read(data: &Path) -> Result<Connection, String> {
    let path = data.join("catalog.db");
    if !path.is_file() {
        return Err("no catalog in data directory".into());
    }
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())
}
pub fn all_keys(db: &Connection) -> Result<BTreeSet<String>, String> {
    let mut query = db
        .prepare("SELECT content_key FROM items")
        .map_err(|e| e.to_string())?;
    query
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .map(|r| r.map_err(|e| e.to_string()))
        .collect()
}
pub fn survey(
    data: &Path,
    root: &Path,
    media: &MediaAdapter,
    runtime: &tokio::runtime::Runtime,
) -> Result<Survey, String> {
    let db = read(data)?;
    let existing = all_keys(&db)?;
    let mut query = db
        .prepare("SELECT content_key,file_path,phash FROM items WHERE format='video'")
        .map_err(|e| e.to_string())?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .map(|r| r.map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    drop(query);
    drop(db);
    let mut out = Survey::default();
    for (old, path, hash) in rows {
        out.checked += 1;
        let source = crate::paths::resolve(data, &path, root);
        if !source.is_file() {
            out.missing.push(format!("{old}: {}", source.display()));
            continue;
        }
        let result = runtime.block_on(async {
            let fresh = media
                .call(json!({"operation":"fingerprint","source":source,"format":"video"}))
                .await?;
            let mut key = fresh["content_key"]
                .as_str()
                .ok_or("codec fingerprint has no key")?
                .to_owned();
            if old.matches(':').count() == 2 {
                key = media
                    .call(
                        json!({"operation":"collision","source":source,"format":"video","key":key}),
                    )
                    .await?
                    .as_str()
                    .ok_or("invalid collision key")?
                    .into();
            }
            Ok::<_, String>((key, fresh["phash"].as_u64()))
        });
        let (new, fresh_hash) = match result {
            Ok(value) => value,
            Err(error) => {
                out.undecodable.push(format!("{old}: {error}"));
                continue;
            }
        };
        out.phashes.insert(new.clone(), fresh_hash);
        if new != old {
            out.changed.push((
                old,
                new,
                source
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
            ));
        } else if fresh_hash != hash.map(|h| h as u64) {
            out.hashes.push((old, hash.map(|h| h as u64), fresh_hash));
        } else {
            out.unchanged += 1;
        }
    }
    let moving = out.map().keys().cloned().collect::<BTreeSet<_>>();
    let mut targets = BTreeMap::<String, Vec<String>>::new();
    for (old, new, _) in &out.changed {
        targets.entry(new.clone()).or_default().push(old.clone());
    }
    for (new, mut olds) in targets {
        if existing.contains(&new) && !moving.contains(&new) {
            olds.push("(already in the catalog)".into());
        }
        if olds.len() > 1 {
            out.collisions.insert(new, olds);
        }
    }
    Ok(out)
}
