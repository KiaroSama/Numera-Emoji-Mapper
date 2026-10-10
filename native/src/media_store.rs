//! Non-clobbering media storage and durable provenance for refused destinations.
use crate::{atomic, media_adapter::MediaAdapter};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

pub async fn store(
    adapter: &MediaAdapter,
    source: &Path,
    destination: &Path,
    fmt: &str,
    key: &str,
    provenance: &Value,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(destination.parent().ok_or("invalid media destination")?)
        .map_err(|e| e.to_string())?;
    let collision = adapter
        .call(json!({"operation":"collision","source":source,"format":fmt,"key":key}))
        .await?;
    let alternative = destination.with_file_name(format!(
        "{}{}",
        collision
            .as_str()
            .ok_or("invalid collision key")?
            .replace(':', "_"),
        destination
            .extension()
            .and_then(|v| v.to_str())
            .map(|s| format!(".{s}"))
            .unwrap_or_default()
    ));
    for target in [destination.to_path_buf(), alternative] {
        match std::fs::symlink_metadata(&target) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                crate::archive_move::transfer(source, &target, true)?;
                return Ok(target);
            }
            Ok(_) => {
                if !target.is_symlink()
                    && adapter.compare(source, &target, fmt).await == Ok(Some(true))
                {
                    if source.canonicalize().map_err(|e| e.to_string())?
                        != target.canonicalize().map_err(|e| e.to_string())?
                    {
                        std::fs::remove_file(source).map_err(|e| e.to_string())?;
                    }
                    return Ok(target);
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    let retained = retain(source, destination, fmt, key, provenance)?;
    Err(format!(
        "media destination collision could not be resolved; incoming media retained at {}",
        retained.display()
    ))
}

fn retain(
    source: &Path,
    destination: &Path,
    fmt: &str,
    key: &str,
    provenance: &Value,
) -> Result<PathBuf, String> {
    let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let length = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
        size += length as u64;
    }
    let digest = digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let directory = destination
        .parent()
        .ok_or("invalid destination")?
        .join("quarantine");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let ext = destination
        .extension()
        .and_then(|v| v.to_str())
        .ok_or("missing media extension")?;
    let stable = directory.join(format!("{digest}.{ext}"));
    let retained = match std::fs::hard_link(source, &stable) {
        Ok(()) => stable,
        Err(error)
            if error.kind() == std::io::ErrorKind::AlreadyExists
                && !stable.is_symlink()
                && identical(source, &stable)? =>
        {
            stable
        }
        Err(_) => {
            let (mut output, path) = loop {
                let path = directory.join(format!(
                    "{digest}-{}-{}.{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed),
                    ext
                ));
                match std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                {
                    Ok(file) => break (file, path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error.to_string()),
                }
            };
            let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
            let result = std::io::copy(&mut input, &mut output).and_then(|_| output.sync_all());
            drop(output);
            if let Err(error) = result {
                let _ = std::fs::remove_file(path);
                return Err(error.to_string());
            }
            path
        }
    };
    let record = retained.with_extension("json");
    if !record.exists() {
        atomic::write_json(&record,&json!({"reason":"media-destination-collision","format":fmt,"content_key":key,
            "sha256":digest,"size":size,"destination":destination.file_name().and_then(|s|s.to_str()),"provenance":provenance}))
            .map_err(|e|e.to_string())?;
    }
    std::fs::remove_file(source).map_err(|e| e.to_string())?;
    Ok(retained)
}
pub fn identical(a: &Path, b: &Path) -> Result<bool, String> {
    let mut a = std::fs::File::open(a).map_err(|e| e.to_string())?;
    let mut b = std::fs::File::open(b).map_err(|e| e.to_string())?;
    if a.metadata().map_err(|e| e.to_string())?.len()
        != b.metadata().map_err(|e| e.to_string())?.len()
    {
        return Ok(false);
    }
    let (mut left, mut right) = ([0u8; 65536], [0u8; 65536]);
    loop {
        let n = a.read(&mut left).map_err(|e| e.to_string())?;
        b.read_exact(&mut right[..n]).map_err(|e| e.to_string())?;
        if left[..n] != right[..n] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}
