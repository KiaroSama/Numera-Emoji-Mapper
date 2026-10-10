//! Full-pack archive sync follows live identities and commits each moved media path.
use crate::{archive_catalog, archive_rows, config::Config, logging::RunLog, telegram::Telegram};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn run(config: &Config, tg: &Telegram, log: &RunLog) -> Result<u8, String> {
    let data = config.root.join("collection");
    let canonical = data.canonicalize().map_err(|e| e.to_string())?;
    let _writer = crate::ownership::Ownership::acquire(
        &canonical,
        crate::ownership::Mode::Writer,
        &crate::logging::iso_utc(),
    )?;
    let db = rusqlite::Connection::open_with_flags(
        data.join("catalog.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .map_err(|e| e.to_string())?;
    db.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let (items, _) = archive_catalog::read(config)?;
    let mut key_of = serde_json::Map::new();
    for (key, item) in items.as_object().ok_or("invalid archive catalog")? {
        if let Some(id) = item["cid"].as_str().filter(|s| !s.is_empty()) {
            key_of.insert(id.into(), json!(key));
        }
    }
    let key_of = json!(key_of);
    let root = config.path(config.require("EMOJI_ARCHIVE_DIR")?);
    if root.is_symlink() {
        return Err("archive root must not be a symlink".into());
    }
    let (mut touched, mut moved, mut renamed) = (0, 0, 0);
    for record in archive_catalog::sets(config)? {
        let title = record["title"]
            .as_str()
            .ok_or("archive pack has no title")?;
        let folder = root.join(archive_rows::folder(title));
        if record["live"].as_u64().unwrap_or(0) < 200 {
            if folder.is_dir() {
                log.event("WARNING", "archive", "unfinished pack folder is left alone");
            }
            continue;
        }
        if folder.is_symlink() {
            return Err("archive pack folder must not be a symlink".into());
        }
        let name = record["name"].as_str().ok_or("archive pack has no name")?;
        let live = tg
            .read_call(
                "getStickerSet",
                &BTreeMap::from([("name".into(), name.into())]),
            )
            .map_err(|e| e.to_string())?;
        let rows = archive_rows::rows(
            &record,
            live["stickers"]
                .as_array()
                .ok_or("live pack has no stickers")?,
            &items,
            &key_of,
        )?;
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let logo = folder.join(archive_rows::LOGO_NAME);
        let mut have = BTreeMap::<String, PathBuf>::new();
        let mut older = Vec::new();
        for entry in std::fs::read_dir(&folder).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if !path.is_file() || path.is_symlink() {
                continue;
            }
            let file = path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("archive filename not UTF-8")?;
            if file.starts_with("001_logo") && file.ends_with(".png") {
                older.push(path.clone());
            }
            let parts = file.split('_').collect::<Vec<_>>();
            if parts.len() == 3
                && parts[0].len() == 3
                && parts[0].bytes().all(|b| b.is_ascii_digit())
                && ["static", "video", "animated"].contains(&parts[1])
                && let Some((digest, _)) = parts[2].split_once('.')
                && digest.len() == 12
                && digest
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            {
                have.insert(digest.into(), path);
            }
        }
        older.sort();
        let logo_src = if !logo.is_file() && older.is_empty() {
            Some(
                config
                    .logo_path(true)?
                    .filter(|p| p.is_file())
                    .ok_or("brand logo is missing; nothing moved")?,
            )
        } else {
            None
        };
        let mut wanted = std::collections::BTreeSet::new();
        for row in &rows {
            let Some(key) = row["content_key"].as_str() else {
                continue;
            };
            let file = row["file"].as_str().ok_or("archive row has no filename")?;
            wanted.insert(file.to_owned());
            let dest = folder.join(file);
            let digest = key
                .split_once(':')
                .ok_or("invalid archive key")?
                .1
                .chars()
                .take(12)
                .collect::<String>();
            if let Some(current) = have.get(&digest) {
                let source = PathBuf::from(
                    items[key]["path"]
                        .as_str()
                        .ok_or("archive source path missing")?,
                );
                if source.is_file()
                    && source.canonicalize().map_err(|e| e.to_string())?
                        != current.canonicalize().map_err(|e| e.to_string())?
                    && !crate::media_store::identical(&source, current)?
                {
                    return Err("existing archive file differs from retained source; refusing corrupted adoption".into());
                }
                if current != &dest {
                    crate::archive_move::transfer(current, &dest, true)?;
                    renamed += 1;
                }
            } else {
                let source = PathBuf::from(
                    items[key]["path"]
                        .as_str()
                        .ok_or("archive source path missing")?,
                );
                if !source.is_file() {
                    log.event(
                        "ERROR",
                        "archive",
                        "catalog source missing; prior moved files remain committed",
                    );
                    return Ok(1);
                }
                crate::archive_move::transfer(&source, &dest, true)?;
                moved += 1;
            }
            let stored = crate::paths::store(&data, &dest).map_err(|e| e.to_string())?;
            let changed = db
                .execute(
                    "UPDATE items SET file_path=? WHERE content_key=?",
                    rusqlite::params![stored, key],
                )
                .map_err(|e| e.to_string())?;
            if changed != 1 {
                return Err("archived media no longer has exactly one catalog row".into());
            }
        }
        if !logo.is_file() {
            if let Some(old) = older.first() {
                crate::archive_move::transfer(old, &logo, true)?;
            } else {
                crate::archive_move::transfer(
                    logo_src.as_ref().ok_or("logo source missing")?,
                    &logo,
                    false,
                )?;
            }
        }
        crate::atomic::write_json(&folder.join("_history.json"),&json!({"set_name":name,"link":format!("https://t.me/addemoji/{name}"),"total":rows.len(),"emoji":rows})).map_err(|e|e.to_string())?;
        std::fs::write(
            folder.join("_history.md"),
            archive_rows::history(&record, &rows)?.as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(
            folder.join("_manifest.md"),
            archive_rows::manifest(&record, &rows, &items)?.as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        let stray = std::fs::read_dir(&folder)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter(|e| e.path().is_file())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                !wanted.contains(&name)
                    && name != archive_rows::LOGO_NAME
                    && !archive_rows::META.contains(&name.as_str())
            })
            .count();
        if stray > 0 {
            log.event(
                "WARNING",
                "archive",
                &format!("{stray} unreferenced files preserved"),
            );
        }
        touched += 1;
        log.event(
            "INFO",
            "archive",
            &format!("pack synced items={}", wanted.len()),
        );
    }
    println!(
        "archive: {touched} full pack(s) synced, {moved} file(s) moved, {renamed} renamed to a new slot"
    );
    Ok(0)
}
