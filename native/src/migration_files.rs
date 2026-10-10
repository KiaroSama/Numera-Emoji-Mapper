//! Complete byte-digested media intents precede identity changes and replay without overwrites.
use crate::identity_survey;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
pub fn digest(path: &Path) -> Result<String, String> {
    if path.is_symlink() || !path.is_file() {
        return Err("migration media must be a regular non-symlink file".into());
    }
    let mut input = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let count = input.read(&mut bytes).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&bytes[..count]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
fn prefix(key: &str) -> Result<String, String> {
    Ok(key
        .split_once(':')
        .ok_or("invalid content key for migration filename")?
        .1
        .chars()
        .take(12)
        .collect())
}
pub fn plan(
    data: &Path,
    root: &Path,
    map: &BTreeMap<String, String>,
) -> Result<Vec<Value>, String> {
    let db = identity_survey::read(data)?;
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
    let mut files = Vec::new();
    let mut destinations = BTreeMap::new();
    for (old, stored, fmt) in rows {
        let source = crate::paths::resolve(data, &stored, root);
        if source.is_symlink() || !source.is_file() {
            return Err("missing or unsupported migration media".into());
        }
        let source = crate::migration_bundle::resolved(&source)?;
        let new = map.get(&old).unwrap_or(&old);
        let mut origin = old.clone();
        let filename = source
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("media filename not UTF-8")?;
        if &old == new {
            let predecessors = map
                .iter()
                .filter(|(_, v)| *v == &old)
                .filter_map(|(k, _)| prefix(k).ok().filter(|p| filename.contains(p)).map(|_| k))
                .collect::<Vec<_>>();
            if predecessors.len() > 1 {
                return Err("ambiguous recovered filename mapping".into());
            }
            if let Some(key) = predecessors.first() {
                origin = (*key).clone();
            }
        }
        let destination = source.with_file_name(filename.replace(&prefix(&origin)?, &prefix(new)?));
        let hash = digest(&source)?;
        if destination.exists() && digest(&destination)? != hash {
            return Err("migration destination contains unrelated bytes".into());
        }
        if destinations
            .insert(destination.clone(), old.clone())
            .is_some()
        {
            return Err("two catalog rows would share a migration destination".into());
        }
        files.push(json!({"old_key":old,"key":new,"format":fmt,"filename_key":origin,"source":source,"stored_source":stored,"destination":destination,"sha256":hash,"destination_existed":destination.exists()}));
    }
    Ok(files)
}
pub fn verify(files: &[Value], final_state: bool) -> Result<(), String> {
    for file in files {
        let source = PathBuf::from(file["source"].as_str().ok_or("missing source intent")?);
        let destination = PathBuf::from(
            file["destination"]
                .as_str()
                .ok_or("missing destination intent")?,
        );
        let expected = file["sha256"].as_str().ok_or("missing media digest")?;
        let mut present = Vec::new();
        for path in [&source, &destination] {
            if path.exists() || path.is_symlink() {
                present.push(path);
            }
        }
        if present.is_empty() || final_state && !destination.is_file() {
            return Err("migration media is missing".into());
        }
        for path in present {
            if digest(path)? != expected {
                return Err("migration media changed or destination collided".into());
            }
        }
    }
    Ok(())
}
pub fn apply(data: &Path, files: &[Value]) -> Result<Vec<String>, String> {
    verify(files, false)?;
    let db = rusqlite::Connection::open_with_flags(
        data.join("catalog.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .map_err(|e| e.to_string())?;
    let mut renamed = Vec::new();
    for file in files {
        let source = PathBuf::from(file["source"].as_str().ok_or("invalid source")?);
        let destination = PathBuf::from(file["destination"].as_str().ok_or("invalid destination")?);
        if source != destination && source.exists() {
            crate::archive_move::transfer(&source, &destination, true)?;
        }
        if digest(&destination)? != file["sha256"].as_str().ok_or("missing digest")? {
            return Err("migration output digest differs".into());
        }
        if source != destination {
            renamed.push(
                destination
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("migration destination filename is not UTF-8")?
                    .to_owned(),
            );
        }
        let stored = crate::paths::store(data, &destination).map_err(|e| e.to_string())?;
        if file["old_key"] != file["key"] || file["stored_source"] != stored {
            db.execute(
                "UPDATE items SET file_path=? WHERE content_key=?",
                rusqlite::params![stored, file["key"].as_str().ok_or("missing migrated key")?],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(renamed)
}
