//! Version2 restore refuses later edits and records reverse direction before any mutation.
use crate::{migration_apply, migration_bundle, migration_files, migration_signature};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
pub fn restore(data: &Path, root: &Path, manifest: &Path, apply: bool) -> Result<(), String> {
    let data = migration_bundle::canonical(data)?;
    let _owner = crate::ownership::Ownership::acquire(
        &data,
        crate::ownership::Mode::Maintenance,
        &crate::logging::iso_utc(),
    )?;
    let _families = migration_apply::locks(&data, root)?;
    let mut doc: Value =
        serde_json::from_slice(&std::fs::read(manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if let Some(journal) = migration_apply::journal(&data)? {
        let active = Path::new(
            journal["bundle"]
                .as_str()
                .ok_or("active journal has no bundle binding")?,
        );
        if migration_bundle::resolved(active)? != migration_bundle::resolved(manifest)? {
            return Err("another migration journal is active; requested bundle differs".into());
        }
        if journal["direction"] == "restore" {
            doc = journal;
        }
    }
    migration_bundle::current(&data, root, &doc)?;
    if !apply {
        return Ok(());
    }
    doc["direction"] = json!("restore");
    migration_apply::stage(&data, &mut doc, "restore")?;
    let files = doc["files"]
        .as_array()
        .ok_or("invalid restore media intents")?;
    for file in files.iter().rev() {
        let source = PathBuf::from(file["source"].as_str().ok_or("invalid restore source")?);
        let destination = PathBuf::from(
            file["destination"]
                .as_str()
                .ok_or("invalid restore destination")?,
        );
        if source != destination && destination.exists() {
            crate::archive_move::transfer(
                &destination,
                &source,
                file["destination_existed"] != true,
            )?;
        }
        if migration_files::digest(&source)?
            != file["sha256"].as_str().ok_or("restore digest missing")?
        {
            return Err("restored media failed byte verification".into());
        }
    }
    for (name, intent) in doc["states"].as_object().ok_or("invalid restore states")? {
        crate::atomic::write_json(&data.join(name), &intent["before"])
            .map_err(|e| e.to_string())?;
    }
    let source = rusqlite::Connection::open_with_flags(
        Path::new(doc["backup"].as_str().ok_or("restore backup missing")?),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| e.to_string())?;
    let mut destination = rusqlite::Connection::open_with_flags(
        data.join("catalog.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .map_err(|e| e.to_string())?;
    crate::sqlite_snapshot::copy(
        &source,
        &mut destination,
        std::time::Duration::from_secs(900),
    )?;
    drop(destination);
    drop(source);
    if migration_signature::signature(&data.join("catalog.db"), &[])?
        != doc["before_signature"]
            .as_str()
            .ok_or("original signature missing")?
    {
        return Err("restored database differs from snapshot".into());
    }
    for file in files {
        if migration_files::digest(Path::new(
            file["source"].as_str().ok_or("restore source missing")?,
        ))? != file["sha256"].as_str().ok_or("restore digest missing")?
        {
            return Err("restored media differs".into());
        }
    }
    for (name, intent) in doc["states"].as_object().ok_or("invalid states")? {
        let actual: Value =
            serde_json::from_slice(&std::fs::read(data.join(name)).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if actual != intent["before"] {
            return Err("restored state differs from snapshot".into());
        }
    }
    migration_apply::stage(&data, &mut doc, "restored")?;
    crate::atomic::write_json(manifest, &doc).map_err(|e| e.to_string())?;
    std::fs::remove_file(data.join("identity-migration.journal.json"))
        .map_err(|e| e.to_string())?;
    Ok(())
}
