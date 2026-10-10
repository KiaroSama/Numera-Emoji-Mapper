//! Version2 rollback evidence binds one canonical catalog, complete media intents and original state.
use crate::{migration_files, migration_signature, state_artifacts};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
pub const VERSION: u64 = 2;
pub fn resolved(path: &Path) -> Result<PathBuf, String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        let text = path.to_str().ok_or("canonical path is not UTF-8")?;
        Ok(PathBuf::from(
            if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
                format!("\\\\{unc}")
            } else {
                text.strip_prefix("\\\\?\\").unwrap_or(text).into()
            },
        ))
    }
    #[cfg(not(windows))]
    {
        Ok(path)
    }
}
pub fn canonical(data: &Path) -> Result<PathBuf, String> {
    let path = resolved(data)?;
    #[cfg(windows)]
    {
        Ok(PathBuf::from(
            path.to_str().ok_or("data path not UTF-8")?.to_lowercase(),
        ))
    }
    #[cfg(not(windows))]
    {
        Ok(path)
    }
}
fn text<'a>(doc: &'a Value, key: &str) -> Result<&'a str, String> {
    doc[key]
        .as_str()
        .ok_or_else(|| format!("migration bundle has no string {key}"))
}
pub fn make(
    data: &Path,
    backup: &Path,
    map: &BTreeMap<String, String>,
    hashes: &BTreeMap<String, Option<u64>>,
    files: &[Value],
    states: &Value,
) -> Result<Value, String> {
    let data = canonical(data)?;
    let catalog = resolved(&data.join("catalog.db"))?;
    let source =
        rusqlite::Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let mut memory = rusqlite::Connection::open_in_memory().map_err(|e| e.to_string())?;
    crate::sqlite_snapshot::copy(&source, &mut memory, std::time::Duration::from_secs(30))?;
    let before = migration_signature::database(&memory, &[])?;
    crate::migration_database::rewrite(&mut memory, map, hashes)?;
    let after = migration_signature::database(&memory, &[])?;
    let signed = hashes
        .iter()
        .map(|(key, value)| (key.clone(), value.map(|n| n as i64)))
        .collect::<BTreeMap<_, _>>();
    Ok(
        json!({"version":VERSION,"data_dir":data,"catalog":catalog,"backup":resolved(backup)?,"backup_sha256":migration_files::digest(backup)?,
        "before_signature":before,"database_signature":after,"key_map":map,"phash":signed,"files":files,"states":states,
        "stage":"backup","direction":"forward"}),
    )
}
pub fn validate(data: &Path, root: &Path, doc: &Value) -> Result<(), String> {
    let directory = canonical(data)?;
    if doc["version"].as_u64() != Some(VERSION)
        || doc["data_dir"] != json!(directory)
        || doc["catalog"] != json!(resolved(&directory.join("catalog.db"))?)
    {
        return Err("migration journal version or catalog binding does not match".into());
    }
    let backup = PathBuf::from(text(doc, "backup")?);
    if canonical(backup.parent().ok_or("backup has no parent")?)? != directory
        || migration_files::digest(&backup)? != text(doc, "backup_sha256")?
    {
        return Err("migration backup missing, modified or belongs elsewhere".into());
    }
    let manifest = PathBuf::from(text(doc, "bundle")?);
    if manifest.is_symlink()
        || resolved(&manifest)? != resolved(&backup.with_extension("rollback.json"))?
    {
        return Err("migration bundle output is not bound to its backup sibling; no writes".into());
    }
    if migration_signature::signature(&backup, &[])? != text(doc, "before_signature")? {
        return Err("migration backup does not match recorded snapshot".into());
    }
    let files = doc["files"]
        .as_array()
        .ok_or("migration has no complete file intents")?;
    let states = doc["states"]
        .as_object()
        .ok_or("migration has no state intents")?;
    for name in states.keys() {
        if !state_artifacts::state_name(name) {
            return Err("migration contains invalid state path".into());
        }
    }
    let map: BTreeMap<String, String> =
        serde_json::from_value(doc["key_map"].clone()).map_err(|e| e.to_string())?;
    let db =
        rusqlite::Connection::open_with_flags(&backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let signed: BTreeMap<String, Option<i64>> =
        serde_json::from_value(doc["phash"].clone()).map_err(|e| e.to_string())?;
    let hashes = signed
        .into_iter()
        .map(|(key, value)| (key, value.map(|value| value as u64)))
        .collect();
    let mut expected = rusqlite::Connection::open_in_memory().map_err(|e| e.to_string())?;
    crate::sqlite_snapshot::copy(&db, &mut expected, std::time::Duration::from_secs(30))?;
    crate::migration_database::rewrite(&mut expected, &map, &hashes)?;
    if migration_signature::database(&expected, &[])? != text(doc, "database_signature")? {
        return Err("migration SQL intents disagree with recorded result; no writes".into());
    }
    for (name, intent) in states {
        let before = intent
            .get("before")
            .ok_or("migration state has no original document")?;
        let after = intent
            .get("after")
            .ok_or("migration state has no result document")?;
        if state_artifacts::rewrite(name, before, &map)? != *after {
            return Err("migration state intents disagree with key map; no writes".into());
        }
    }
    let mut query = db
        .prepare("SELECT content_key,file_path,format FROM items")
        .map_err(|e| e.to_string())?;
    let mut original = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (r.get::<_, String>(1)?, r.get::<_, String>(2)?),
            ))
        })
        .map_err(|e| e.to_string())?
        .map(|r| r.map_err(|e| e.to_string()))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    if original.len() != files.len() {
        return Err("migration does not cover complete snapshot".into());
    }
    for file in files {
        let old = text(file, "old_key")?;
        let row = original
            .remove(old)
            .ok_or("duplicate/unknown migration source key")?;
        let key = map.get(old).map(String::as_str).unwrap_or(old);
        let origin = text(file, "filename_key")?;
        let source = PathBuf::from(text(file, "source")?);
        let dest = PathBuf::from(text(file, "destination")?);
        let stored = text(file, "stored_source")?;
        let origin_digest = origin
            .split_once(':')
            .ok_or("invalid filename identity")?
            .1
            .chars()
            .take(12)
            .collect::<String>();
        let digest = key
            .split_once(':')
            .ok_or("invalid new identity")?
            .1
            .chars()
            .take(12)
            .collect::<String>();
        let expected = source
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid source filename")?
            .replace(&origin_digest, &digest);
        let resolved_source = crate::paths::resolve(&directory, stored, root);
        // Source may already have moved; canonicalize the existing parent without requiring the old file.
        let intended = resolved(resolved_source.parent().ok_or("source has no parent")?)?.join(
            resolved_source
                .file_name()
                .ok_or("source has no filename")?,
        );
        if row != (stored.into(), text(file, "format")?.into())
            || file["key"] != key
            || origin != old && map.get(origin).map(String::as_str) != Some(key)
            || source != intended
            || !source.is_absolute()
            || source.parent() != dest.parent()
            || dest.file_name().and_then(|s| s.to_str()) != Some(&expected)
        {
            return Err("migration file intent disagrees with backup".into());
        }
    }
    Ok(())
}
pub fn current(data: &Path, root: &Path, doc: &Value) -> Result<String, String> {
    validate(data, root, doc)?;
    let files = doc["files"].as_array().ok_or("invalid file intents")?;
    migration_files::verify(files, false)?;
    let states = doc["states"].as_object().ok_or("invalid state intents")?;
    let names = state_artifacts::files(data)?
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>();
    if names != states.keys().cloned().collect() {
        return Err("publisher state files changed since migration snapshot".into());
    }
    for (name, intent) in states {
        let now: Value =
            serde_json::from_slice(&std::fs::read(data.join(name)).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if now != intent["before"] && now != intent["after"] {
            return Err("publisher state changed after migration snapshot".into());
        }
    }
    let mut normalized = Vec::new();
    for file in files {
        normalized.push(json!({"destination":file["destination"],"source":file["stored_source"]}));
    }
    for file in files {
        normalized.push(json!({"destination":crate::paths::store(data,Path::new(text(file,"destination")?)).map_err(|e|e.to_string())?,"source":file["stored_source"]}));
    }
    let now = migration_signature::signature(&data.join("catalog.db"), &normalized)?;
    if now != text(doc, "before_signature")? && now != text(doc, "database_signature")? {
        return Err("catalog changed outside migration; refusing to overwrite later data".into());
    }
    Ok(now)
}
pub fn applied(data: &Path, root: &Path, doc: &Value) -> Result<(), String> {
    if current(data, root, doc)? != text(doc, "database_signature")? {
        return Err("migration database edits have not committed".into());
    }
    let files = doc["files"].as_array().ok_or("invalid file intents")?;
    migration_files::verify(files, true)?;
    let db = crate::identity_survey::read(data)?;
    for file in files {
        let stored: String = db
            .query_row(
                "SELECT file_path FROM items WHERE content_key=?",
                [text(file, "key")?],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if resolved(&crate::paths::resolve(data, &stored, root))?
            != resolved(Path::new(text(file, "destination")?))?
        {
            return Err("migration destination not recorded in catalog".into());
        }
    }
    for (name, intent) in doc["states"].as_object().ok_or("invalid states")? {
        let now: Value =
            serde_json::from_slice(&std::fs::read(data.join(name)).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if now != intent["after"] {
            return Err("migration state edits incomplete".into());
        }
    }
    Ok(())
}
