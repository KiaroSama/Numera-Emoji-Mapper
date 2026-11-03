//! Faithful sandbox copies reuse native catalog ownership, WAL backup and lifetime leases.
use crate::{
    atomic,
    locks::Lease,
    ownership::{Mode, Ownership},
    paths, sqlite_snapshot, state_artifacts, tooling_environment, tooling_manifest,
};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PREFIX: &str = "panel-sandbox-";
const MARKER: &str = ".sandbox-owner.json";

pub fn safe_key(key: &str) -> String {
    key.as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn lifetime_lock_path(directory: &Path) -> Result<PathBuf, String> {
    let parent = directory
        .parent()
        .ok_or("Sandbox directory has no parent")?;
    let parent = tooling_environment::physical_root(parent)?;
    let directory = parent.join(
        directory
            .file_name()
            .ok_or("Sandbox directory has no name")?,
    );
    let text = directory.to_str().ok_or("Sandbox path is not UTF-8")?;
    #[cfg(windows)]
    let text = text.to_lowercase();
    let name = {
        use sha2::{Digest, Sha256};
        Sha256::digest(text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    let path = parent
        .join(".panel-sandbox-leases")
        .join(format!("{name}.sandbox-lifetime.lock"));
    tooling_environment::reject_links(&path, "Unsupported sandbox lease directory")?;
    Ok(path)
}

pub fn marker_is_self_describing(directory: &Path) -> bool {
    if tooling_environment::reject_links(directory, "Linked sandbox").is_err()
        || !directory.is_dir()
    {
        return false;
    }
    let marker = directory.join(MARKER);
    if tooling_environment::reject_links(&marker, "Linked sandbox marker").is_err()
        || !marker.is_file()
    {
        return false;
    }
    let doc: Option<Value> = std::fs::read(marker)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    doc.is_some_and(|doc| {
        doc["version"].as_i64() == Some(2)
            && crate::migration_bundle::resolved(directory)
                .ok()
                .and_then(|path| path.to_str().map(str::to_owned))
                == doc["directory"].as_str().map(str::to_owned)
    })
}

/// Sweep only named, self-describing v2 clones under a physical caller-owned root.
pub fn sweep_stale(base: &Path) -> Result<usize, String> {
    sweep_stale_with(base, |_| Ok(()))
}

pub(crate) fn sweep_stale_with(
    base: &Path,
    mut acquired: impl FnMut(&Path) -> Result<(), String>,
) -> Result<usize, String> {
    let base = tooling_environment::physical_root(base)?;
    let mut entries = std::fs::read_dir(&base)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    entries.sort();
    let mut removed = 0;
    for directory in entries {
        if !directory
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(PREFIX))
            || !marker_is_self_describing(&directory)
        {
            continue;
        }
        let Ok(path) = lifetime_lock_path(&directory) else {
            continue;
        };
        let Ok(_lease) = Lease::acquire(&path, &crate::logging::utc()) else {
            continue;
        };
        acquired(&directory)?;
        if marker_is_self_describing(&directory) && std::fs::remove_dir_all(&directory).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

fn destination(source: &Path, target: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(target).is_ok() {
        return Err("Sandbox destination already exists; refusing to erase it".into());
    }
    if source == target || source.starts_with(target) || target.starts_with(source) {
        return Err("Sandbox destination overlaps its source".into());
    }
    Ok(())
}

pub(crate) fn copy_media(source: &Path, output: &Path) -> Result<(), String> {
    copy_media_with(source, output, |reader, writer| {
        std::io::copy(reader, writer)
    })
}

pub(crate) fn copy_media_with(
    source: &Path,
    output: &Path,
    transfer: impl FnOnce(&mut std::fs::File, &mut std::fs::File) -> std::io::Result<u64>,
) -> Result<(), String> {
    tooling_environment::reject_links(source, "Unsupported sandbox media source")?;
    let before = tooling_manifest::digest(source)?;
    let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
    let mut copy = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    transfer(&mut input, &mut copy).map_err(|e| e.to_string())?;
    copy.sync_all().map_err(|e| e.to_string())?;
    drop(copy);
    if tooling_manifest::digest(output)? != before || tooling_manifest::digest(source)? != before {
        return Err("Sandbox media changed or was not copied faithfully".into());
    }
    Ok(())
}

#[cfg(unix)]
type Lifetime = std::fs::File;
#[cfg(not(unix))]
type Lifetime = Lease;

pub struct Session {
    pub directory: PathBuf,
    pub count: usize,
    lease: Option<Lifetime>,
    pub warnings: Vec<String>,
}
impl Session {
    /// Lifetime lease spans construction, caller serving and verified child stop.
    pub fn create(project: &Path, source: &Path, target: &Path) -> Result<Self, String> {
        Self::create_with(project, source, target, copy_media, |_| Ok(()))
    }

    pub(crate) fn create_with(
        project: &Path,
        source: &Path,
        target: &Path,
        mut copy: impl FnMut(&Path, &Path) -> Result<(), String>,
        mut published: impl FnMut(&Path) -> Result<(), String>,
    ) -> Result<Self, String> {
        let project = tooling_environment::physical_root(project)?;
        let source = tooling_environment::physical_root(source)?;
        let parent = target.parent().ok_or("Sandbox target has no parent")?;
        let parent = tooling_environment::physical_root(parent)?;
        if !parent.starts_with(&project) {
            return Err("Sandbox scratch must stay inside its owning project".into());
        }
        let target = parent.join(target.file_name().ok_or("Sandbox target has no name")?);
        destination(&source, &target)?;
        let database = source.join("catalog.db");
        tooling_environment::reject_links(&database, "No supported catalog; nothing to sandbox")?;
        if !database.is_file() {
            return Err("No supported catalog; nothing to sandbox".into());
        }
        let lease = Lease::acquire(&lifetime_lock_path(&target)?, &crate::logging::utc())
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        let lease = lease.into_inherited_file().map_err(|e| e.to_string())?;
        destination(&source, &target)?;
        std::fs::create_dir(&target).map_err(|e| e.to_string())?;
        let result = (|| {
            std::fs::create_dir(target.join("media")).map_err(|e| e.to_string())?;
            let count = {
                let _source_owner =
                    Ownership::acquire(&source, Mode::Writer, &crate::logging::utc())?;
                let input =
                    Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)
                        .map_err(|e| e.to_string())?;
                let mut output =
                    Connection::open(target.join("catalog.db")).map_err(|e| e.to_string())?;
                sqlite_snapshot::copy(&input, &mut output, Duration::from_secs(30))?;
                let integrity: String = output
                    .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                    .map_err(|e| e.to_string())?;
                if integrity != "ok" {
                    return Err("Sandbox database failed integrity_check".into());
                }
                for state in state_artifacts::files(&source)? {
                    copy(
                        &state,
                        &target.join(state.file_name().ok_or("Unsupported sandbox state")?),
                    )?;
                }
                let rows = {
                    let mut query = output
                        .prepare("SELECT content_key,file_path FROM items")
                        .map_err(|e| e.to_string())?;
                    query
                        .query_map([], |row| {
                            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                        })
                        .map_err(|e| e.to_string())?
                        .collect::<rusqlite::Result<Vec<_>>>()
                        .map_err(|e| e.to_string())?
                };
                let transaction = output.transaction().map_err(|e| e.to_string())?;
                for (key, stored) in &rows {
                    if key.is_empty() || stored.is_empty() {
                        return Err("Catalog key/media path must be a nonempty string".into());
                    }
                    let origin = paths::resolve(&source, stored, &project);
                    let suffix = origin
                        .extension()
                        .map(|suffix| format!(".{}", suffix.to_string_lossy()))
                        .unwrap_or_default();
                    let copied = target
                        .join("media")
                        .join(format!("{}{suffix}", safe_key(key)));
                    copy(&origin, &copied)?;
                    let stored = copied
                        .to_str()
                        .ok_or("Sandbox media path is not UTF-8")?
                        .replace('\\', "/");
                    transaction
                        .execute(
                            "UPDATE items SET file_path=? WHERE content_key=?",
                            params![stored, key],
                        )
                        .map_err(|e| e.to_string())?;
                }
                transaction.commit().map_err(|e| e.to_string())?;
                output.close().map_err(|(_, e)| e.to_string())?;
                input.close().map_err(|(_, e)| e.to_string())?;
                rows.len()
            };
            let mut warnings = Vec::new();
            let previews: Result<(), String> = (|| {
                let source_preview = source.join("preview");
                if !source_preview.exists() {
                    return Ok(());
                }
                for entry in std::fs::read_dir(source_preview).map_err(|e| e.to_string())? {
                    let path = entry.map_err(|e| e.to_string())?.path();
                    if path.extension().and_then(|v| v.to_str()) == Some("webp")
                        && path.is_file()
                        && !path.is_symlink()
                    {
                        std::fs::create_dir_all(target.join("preview"))
                            .map_err(|e| e.to_string())?;
                        copy(
                            &path,
                            &target
                                .join("preview")
                                .join(path.file_name().ok_or("Invalid preview name")?),
                        )?;
                    }
                }
                Ok(())
            })();
            if previews.is_err() {
                warnings
                    .push("Preview cache not copied completely; previews render on demand".into());
            }
            atomic::write_json(&target.join(MARKER), &json!({"version":2,"directory":target,"created_utc":crate::logging::iso_utc(),"pid":std::process::id()})).map_err(|e| e.to_string())?;
            published(&target)?;
            Ok((count, warnings))
        })();
        match result {
            Ok((count, warnings)) => Ok(Self {
                directory: target,
                count,
                lease: Some(lease),
                warnings,
            }),
            Err(error) => {
                std::fs::remove_dir_all(&target)
                    .map_err(|e| format!("{error}; incomplete sandbox cleanup failed: {e}"))?;
                Err(error)
            }
        }
    }

    pub fn lease_file(&self) -> Result<std::fs::File, String> {
        let lease = self
            .lease
            .as_ref()
            .ok_or("Sandbox session is already closed")?;
        #[cfg(unix)]
        {
            lease.try_clone().map_err(|e| e.to_string())
        }
        #[cfg(not(unix))]
        {
            lease.clone_file().map_err(|e| e.to_string())
        }
    }

    /// Invalidate the reclaimable marker while this lifetime lease is still held.
    /// An uncertain descendant must never race a later source/native sweeper.
    pub fn retain_for_recovery(&self) -> Result<(), String> {
        let marker = self.directory.join(MARKER);
        let recovery = self.directory.join(".sandbox-recovery.json");
        tooling_environment::reject_links(&marker, "Linked sandbox marker")?;
        tooling_environment::reject_links(&recovery, "Linked sandbox recovery marker")?;
        if std::fs::symlink_metadata(&marker).is_ok() {
            if std::fs::symlink_metadata(&recovery).is_ok()
                || !marker_is_self_describing(&self.directory)
            {
                return Err(
                    "Cannot retain an unsupported or ambiguous sandbox owner marker".into(),
                );
            }
            atomic::publish_new(&marker, &recovery).map_err(|e| {
                format!("Cannot retain sandbox recovery marker before child spawn: {e}")
            })?;
        }
        // Unix publish_new can create the recovery link before unlinking owner;
        // neither that partial move nor an old arbitrary recovery file is readiness.
        if std::fs::symlink_metadata(&marker).is_ok() || !recovery.is_file() {
            return Err("Sandbox must have only its recovery marker before child spawn".into());
        }
        crate::sandbox_lease::validate_marker(&self.directory)
    }

    /// Call only after every server/codec child has been stopped and reaped.
    pub fn finish(mut self) -> Result<(), String> {
        std::fs::remove_dir_all(&self.directory).map_err(|e| e.to_string())?;
        self.lease.take();
        Ok(())
    }
}

// No implicit directory deletion in Drop: a caller that fails to prove its child
// stopped must retain the copy, not race an abandoned server accessing its files.
