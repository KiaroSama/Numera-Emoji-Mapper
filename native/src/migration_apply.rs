//! Canonical maintenance owns re-survey through version2 journal retirement; every stage replays.
use crate::{config::Config, identity_survey, media_adapter::MediaAdapter, migration_bundle};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
pub fn locks(data: &Path, root: &Path) -> Result<Vec<crate::locks::Lease>, String> {
    let mut bases = BTreeSet::new();
    for path in crate::state_artifacts::files(data)? {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("state filename not UTF-8")?;
        let base = name
            .strip_prefix("publish_plan_")
            .or_else(|| name.strip_prefix("publish_"));
        if let Some(base) = base.filter(|s| !s.is_empty()) {
            bases.insert(base.to_owned());
        }
    }
    let mut leases = Vec::new();
    for base in bases {
        leases.push(
            crate::locks::Lease::acquire(
                &data.join(format!("publish_{base}.lock")),
                &crate::logging::iso_utc(),
            )
            .map_err(|e| e.to_string())?,
        );
        leases.push(
            crate::locks::Lease::acquire(
                &crate::locks::pack_family_path(root, &base),
                &crate::logging::iso_utc(),
            )
            .map_err(|e| e.to_string())?,
        );
    }
    Ok(leases)
}
pub fn journal(data: &Path) -> Result<Option<Value>, String> {
    let path = data.join("identity-migration.journal.json");
    match std::fs::read(path) {
        Ok(bytes) => {
            let doc: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if !doc.as_object().is_some_and(|object| !object.is_empty()) {
                return Err("invalid migration journal; inspect before continuing".into());
            }
            Ok(Some(doc))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
pub fn stage(data: &Path, doc: &mut Value, name: &str) -> Result<(), String> {
    doc["stage"] = json!(name);
    crate::atomic::write_json(&data.join("identity-migration.journal.json"), doc)
        .map_err(|e| e.to_string())
}
fn backup(data: &Path) -> Result<PathBuf, String> {
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let source = identity_survey::read(data)?;
    for attempt in 0..64 {
        let suffix = if attempt == 0 {
            String::new()
        } else {
            format!("-{attempt}")
        };
        let path = data.join(format!("catalog.before-video-identity-{stamp}{suffix}.db"));
        if path.exists() {
            continue;
        }
        crate::sqlite_snapshot::backup(&source, &path)?;
        return Ok(path);
    }
    Err("cannot reserve an unused migration backup filename".into())
}
pub fn apply(
    config: &Config,
    data: &Path,
    media: &MediaAdapter,
    runtime: &tokio::runtime::Runtime,
    recovery_backup: Option<&Path>,
) -> Result<Value, String> {
    let data = migration_bundle::canonical(data)?;
    let _ownership = crate::ownership::Ownership::acquire(
        &data,
        crate::ownership::Mode::Maintenance,
        &crate::logging::iso_utc(),
    )?;
    let _families = locks(&data, &config.root)?;
    let mut doc = if let Some(doc) = journal(&data)? {
        doc
    } else {
        let survey = identity_survey::survey(&data, &config.root, media, runtime)?;
        if !survey.complete() || !survey.collisions.is_empty() {
            return Err("migration survey incomplete or colliding; no writes".into());
        }
        let mut map = if let Some(backup) = recovery_backup {
            crate::migration_provenance::recover(&data, &config.root, backup, media, runtime)?
        } else {
            BTreeMap::new()
        };
        map.extend(survey.map());
        let required = crate::identity_references::stale(&data, true)?;
        if required
            .values()
            .flatten()
            .any(|key| !map.contains_key(key))
        {
            return Err("required state references have no trusted mapping".into());
        }
        if !survey.pending() && map.is_empty() && required.is_empty() {
            let issues = crate::identity_invariants::issues(&data, &config.root, media, runtime)?;
            if !issues.is_empty() {
                return Err(format!(
                    "migration invariants incomplete: {}",
                    issues.join("; ")
                ));
            }
            return Ok(json!({"noop":true,"moved":0,"state":[],"renamed":[]}));
        }
        let files = crate::migration_files::plan(&data, &config.root, &map)?;
        let mut states = serde_json::Map::new();
        for path in crate::state_artifacts::files(&data)? {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("state filename not UTF-8")?;
            let before: Value =
                serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let after = crate::state_artifacts::rewrite(name, &before, &map)?;
            states.insert(name.into(), json!({"before":before,"after":after}));
        }
        // Prove supported signature types before creating any recovery bundle.
        crate::migration_signature::signature(&data.join("catalog.db"), &[])?;
        let backup = backup(&data)?;
        let mut doc = migration_bundle::make(
            &data,
            &backup,
            &map,
            &survey.phashes,
            &files,
            &json!(states),
        )?;
        doc["started_utc"] = json!(crate::logging::iso_utc());
        doc["bundle"] = json!(backup.with_extension("rollback.json"));
        crate::atomic::write_json(
            Path::new(doc["bundle"].as_str().ok_or("missing bundle path")?),
            &doc,
        )
        .map_err(|e| e.to_string())?;
        stage(&data, &mut doc, "backup")?;
        doc
    };
    if doc["direction"] == "restore" {
        return Err("rollback interrupted; resume restore instead of apply".into());
    }
    let current = migration_bundle::current(&data, &config.root, &doc)?;
    let mut moved = 0;
    if current
        == doc["before_signature"]
            .as_str()
            .ok_or("missing original signature")?
    {
        let map: BTreeMap<String, String> =
            serde_json::from_value(doc["key_map"].clone()).map_err(|e| e.to_string())?;
        let signed: BTreeMap<String, Option<i64>> =
            serde_json::from_value(doc["phash"].clone()).map_err(|e| e.to_string())?;
        let hashes = signed
            .into_iter()
            .map(|(k, h)| (k, h.map(|h| h as u64)))
            .collect();
        let mut db = rusqlite::Connection::open_with_flags(
            data.join("catalog.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .map_err(|e| e.to_string())?;
        moved = crate::migration_database::rewrite(&mut db, &map, &hashes)?;
    }
    stage(&data, &mut doc, "database")?;
    let mut touched = Vec::new();
    for (name, intent) in doc["states"].as_object().ok_or("invalid states")? {
        let path = data.join(name);
        let actual: Value =
            serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if actual != intent["after"] {
            crate::atomic::write_json(&path, &intent["after"]).map_err(|e| e.to_string())?;
            touched.push(name.clone());
        }
    }
    stage(&data, &mut doc, "state")?;
    let renamed =
        crate::migration_files::apply(&data, doc["files"].as_array().ok_or("invalid files")?)?;
    stage(&data, &mut doc, "files")?;
    migration_bundle::applied(&data, &config.root, &doc)?;
    let survey = identity_survey::survey(&data, &config.root, media, runtime)?;
    let issues = crate::identity_invariants::issues(&data, &config.root, media, runtime)?;
    if !survey.complete() || survey.pending() || !issues.is_empty() {
        return Err(format!(
            "migration verification incomplete: {}",
            issues.join("; ")
        ));
    }
    stage(&data, &mut doc, "verified")?;
    crate::atomic::write_json(
        Path::new(doc["bundle"].as_str().ok_or("missing bundle path")?),
        &doc,
    )
    .map_err(|e| e.to_string())?;
    std::fs::remove_file(data.join("identity-migration.journal.json"))
        .map_err(|e| e.to_string())?;
    Ok(
        json!({"backup":doc["backup"],"bundle":doc["bundle"],"moved":moved,"state":touched,"renamed":renamed,"key_map":doc["key_map"]}),
    )
}
