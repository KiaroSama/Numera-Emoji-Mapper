//! Publish only complete archive bytes; an interrupted copy never becomes a final slot name.
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
pub fn transfer(source: &Path, destination: &Path, move_source: bool) -> Result<(), String> {
    if source.is_symlink() || !source.is_file() || destination.is_symlink() {
        return Err("archive transfer requires regular non-symlink files".into());
    }
    if source.canonicalize().map_err(|e| e.to_string())?
        == destination
            .canonicalize()
            .unwrap_or_else(|_| destination.to_path_buf())
    {
        return Ok(());
    }
    if destination.exists() {
        if !crate::media_store::identical(source, destination)? {
            return Err("archive destination holds different bytes; refusing overwrite".into());
        }
        if move_source {
            std::fs::remove_file(source).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let parent = destination
        .parent()
        .ok_or("archive destination has no parent")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let (stage, mut out) = loop {
        let path = parent.join(format!(
            ".archive-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => break (Stage(path), file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
    std::io::copy(&mut input, &mut out).map_err(|e| e.to_string())?;
    out.flush()
        .and_then(|_| out.sync_all())
        .map_err(|e| e.to_string())?;
    drop(out);
    drop(input);
    match crate::atomic::publish_new(&stage.0, destination) {
        Ok(()) => {}
        Err(e)
            if e.kind() == std::io::ErrorKind::AlreadyExists
                && !destination.is_symlink()
                && crate::media_store::identical(source, destination)? => {}
        Err(e) => return Err(e.to_string()),
    }
    if move_source {
        std::fs::remove_file(source).map_err(|e| e.to_string())?;
    }
    Ok(())
}
