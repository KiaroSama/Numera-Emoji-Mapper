//! Offline freshness distinguishes unfinished packs from full archived packs.
use crate::{archive_catalog, archive_rows, config::Config};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;
pub fn check(config: &Config) -> Result<(bool, Vec<String>), String> {
    let (items, by_set) = archive_catalog::read(config)?;
    let root = config.path(config.require("EMOJI_ARCHIVE_DIR")?);
    let mut why = Vec::new();
    for record in archive_catalog::sets(config)? {
        let title = record["title"]
            .as_str()
            .ok_or("archive pack has no title")?;
        let name = record["name"].as_str().ok_or("archive pack has no name")?;
        let folder = root.join(archive_rows::folder(title));
        let count = record["live"].as_u64().unwrap_or(0);
        let keys = by_set.get(name).cloned().unwrap_or_default();
        if count < 200 {
            if folder.is_dir() {
                why.push(format!(
                    "{title}: not full ({count}/200) but already has an archive folder"
                ));
            }
            continue;
        }
        if !folder.is_dir() {
            why.push(format!("{title}: full but never archived"));
            continue;
        }
        let outside = keys
            .iter()
            .filter(|key| {
                items[*key]["path"]
                    .as_str()
                    .is_some_and(|p| !Path::new(p).starts_with(&root))
            })
            .count();
        if outside > 0 {
            why.push(format!(
                "{title}: {outside} published file(s) still in the project"
            ));
        }
        for file in [
            archive_rows::LOGO_NAME,
            "_history.json",
            "_history.md",
            "_manifest.md",
        ] {
            if !folder.join(file).is_file() {
                why.push(format!("{title}: missing {file}"));
            }
        }
        let path = folder.join("_history.json");
        if path.is_file() {
            let history = std::fs::read(&path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string())
                });
            let Ok(history) = history else {
                why.push(format!("{title}: _history.json is unreadable"));
                continue;
            };
            let recorded = history["emoji"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e["premium_id"].as_str())
                .filter(|s| !s.is_empty())
                .collect::<BTreeSet<_>>();
            let current = keys
                .iter()
                .filter_map(|k| items[k]["cid"].as_str())
                .filter(|s| !s.is_empty())
                .collect::<BTreeSet<_>>();
            if recorded != current {
                why.push(format!(
                    "{title}: {} id(s) added/changed, {} retired since the archive was written",
                    current.difference(&recorded).count(),
                    recorded.difference(&current).count()
                ));
            }
        }
    }
    Ok((!why.is_empty(), why))
}
