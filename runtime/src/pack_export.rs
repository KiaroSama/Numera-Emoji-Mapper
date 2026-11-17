//! A complete ordered ZIP is published only after every input and central directory is valid.
use crate::{archive_catalog, archive_rows, config::Config};
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub fn run(config: &Config, pack: &str, destination: &Path) -> Result<u8, String> {
    let (stale, why) = crate::roster_cli::stale(config)?;
    if stale {
        println!(
            "STALE roster ({why}); the order would be wrong. Refresh it first:\n    numera-emoji pack-manifest --refresh"
        );
        return Ok(3);
    }
    let out = config.root.join("packs");
    let mut paths = std::fs::read_dir(&out)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|s| s.to_str()) == Some("json")
                && p.file_name().and_then(|s| s.to_str()) != Some("index.json")
        })
        .collect::<Vec<_>>();
    paths.sort();
    let doc = paths
        .into_iter()
        .filter_map(|p| {
            std::fs::read(p)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        })
        .find(|doc| {
            doc["set_name"] == pack
                || pack.bytes().all(|b| b.is_ascii_digit())
                    && doc["family"] == "general"
                    && doc["pack_index"]
                        .as_u64()
                        .is_some_and(|n| n.to_string() == pack)
        });
    let Some(doc) = doc else {
        println!("ERROR: no general-family pack {pack:?} in packs/ (a pack number or a set name).");
        return Ok(1);
    };
    let (items, _) = archive_catalog::read(config)?;
    if !doc.is_object() {
        return Err("roster is not an object".into());
    }
    let mut entries = Vec::new();
    let mut missing = Vec::new();
    let mut manifest_rows = Vec::new();
    for row in doc["emoji"].as_array().ok_or("roster has no entries")? {
        let slot = row["slot"].as_u64().ok_or("invalid roster slot")?;
        if row["role"] == "brand-logo" {
            let Some(source) = config
                .logo_path(false)?
                .filter(|p| p.is_file() && !p.is_symlink())
            else {
                missing.push(format!("slot {slot}: the brand logo (BRAND_LOGO_PATH)"));
                continue;
            };
            let suffix = source.extension().and_then(|s| s.to_str()).unwrap_or("");
            let name = if suffix.eq_ignore_ascii_case("png") {
                archive_rows::LOGO_NAME.into()
            } else {
                format!("001_logo.{suffix}")
            };
            entries.push((source, name));
            continue;
        }
        let key = row["history_key"]
            .as_str()
            .and_then(|s| s.strip_prefix("ck:"))
            .unwrap_or("");
        let source = items[key]["path"].as_str().map(PathBuf::from);
        let Some(source) = source.filter(|p| p.is_file() && !p.is_symlink()) else {
            missing.push(format!(
                "slot {slot}: {}",
                if key.is_empty() {
                    row["custom_emoji_id"].as_str().unwrap_or("")
                } else {
                    key
                }
            ));
            continue;
        };
        let suffix = source
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| format!(".{s}"))
            .unwrap_or_default();
        let name = archive_rows::name(
            slot,
            items[key]["fmt"]
                .as_str()
                .ok_or("archive item has no format")?,
            key,
            &suffix,
        )?;
        entries.push((source, name));
        manifest_rows.push(json!({"content_key":key,"premium_id":row["custom_emoji_id"]}));
    }
    if !missing.is_empty() {
        println!(
            "ERROR: {} file(s) of {} were not found; nothing was written:\n  {}",
            missing.len(),
            doc["set_name"].as_str().unwrap_or(""),
            missing.join("\n  ")
        );
        return Ok(1);
    }
    let manifest = archive_rows::manifest(
        &json!({"name":doc["set_name"],"title":doc["title"].as_str().filter(|s|!s.is_empty()).unwrap_or(doc["set_name"].as_str().unwrap_or(""))}),
        &manifest_rows,
        &items,
    )?;
    if destination.is_symlink()
        || entries
            .iter()
            .any(|(p, _)| destination.canonicalize().ok() == p.canonicalize().ok())
    {
        return Err("ZIP output must not replace a source file or symlink".into());
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err("ZIP output parent directory does not exist".into());
    }
    let file_name = destination
        .file_name()
        .ok_or("ZIP output has no filename")?
        .to_string_lossy();
    let (stage, file) = loop {
        let path = parent.join(format!(
            ".{file_name}-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => break (Stage(path), file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (source, name) in &entries {
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        let mut source = std::fs::File::open(source).map_err(|e| e.to_string())?;
        std::io::copy(&mut source, &mut zip).map_err(|e| e.to_string())?;
    }
    zip.start_file("_manifest.md", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(manifest.as_bytes())
        .map_err(|e| e.to_string())?;
    let file = zip.finish().map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(&stage.0, destination).map_err(|e| e.to_string())?;
    println!(
        "exported {}: {} file(s) + _manifest.md -> {}",
        doc["set_name"].as_str().unwrap_or(""),
        entries.len(),
        destination.display()
    );
    Ok(0)
}
