//! Per-run download ownership; an unresolved upload's exact source is retained for recovery.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) fn physical(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("provider path must be absolute without parent traversal".into());
    }
    for current in path.ancestors() {
        let metadata =
            std::fs::symlink_metadata(current).map_err(|_| "provider path is unavailable")?;
        if metadata.file_type().is_symlink() {
            return Err("provider paths must not use links".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("provider paths must not use reparse points".into());
            }
        }
    }
    Ok(())
}

fn ticker(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.trim_matches(['.', '_', '-']).is_empty()
        || !value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
        || value.contains("..")
    {
        return Err("provider ticker is not a safe filename".into());
    }
    Ok(value)
}

pub struct Stage {
    directory: PathBuf,
    published: PathBuf,
}
impl Stage {
    pub fn new(published: &Path) -> Result<Self, String> {
        let parent = published.parent().ok_or("provider media has no parent")?;
        // Inspect the existing ancestor before creating any descendants through it.
        let ancestor = parent
            .ancestors()
            .find(|p| p.exists())
            .ok_or("provider parent is unavailable")?;
        physical(ancestor)?;
        if !parent.is_absolute()
            || parent
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("provider staging requires an absolute physical parent".into());
        }
        std::fs::create_dir_all(parent).map_err(|_| "cannot create provider media parent")?;
        physical(parent)?;
        let incoming = parent.join(".incoming");
        if incoming.exists() {
            physical(&incoming)?;
        } else {
            std::fs::create_dir(&incoming).map_err(|_| "cannot create provider staging parent")?;
        }
        let directory = loop {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = incoming.join(format!(
                "{}-{}-{id}",
                std::process::id(),
                chrono::Utc::now().timestamp_micros()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => break path,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err("cannot create private provider staging".into()),
            }
        };
        Ok(Self {
            directory,
            published: published.into(),
        })
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn source(&self, value: &str) -> Result<PathBuf, String> {
        Ok(self.directory.join(format!("{}.png", ticker(value)?)))
    }
    pub fn for_intent(&self, intent: &Value) -> Result<PathBuf, String> {
        let tk = ticker(
            intent["key"]
                .as_str()
                .ok_or("provider intent has no ticker")?,
        )?;
        if let Some(recorded) = intent.get("source_path").and_then(Value::as_str) {
            let path = PathBuf::from(recorded);
            if path.is_file() {
                physical(&path)?;
                let incoming = self
                    .directory
                    .parent()
                    .ok_or("provider stage has no parent")?
                    .canonicalize()
                    .map_err(|_| "cannot resolve provider stage")?;
                let actual = path
                    .canonicalize()
                    .map_err(|_| "cannot resolve recorded provider source")?;
                if actual.starts_with(&incoming)
                    || actual
                        == self
                            .published
                            .join(format!("{tk}.png"))
                            .canonicalize()
                            .unwrap_or_default()
                {
                    return Ok(path);
                }
                return Err("recorded provider source is outside its media family".into());
            }
        }
        let staged = self.source(tk)?;
        Ok(if staged.is_file() {
            staged
        } else {
            self.published.join(format!("{tk}.png"))
        })
    }
    pub fn promote(&self, source: &Path, tk: &str, want: u64) -> Result<bool, String> {
        let tk = ticker(tk)?;
        if !source.is_file() {
            return Ok(false);
        }
        physical(source)?;
        let allowed = self.for_intent(&serde_json::json!({"key":tk,"source_path":source}))?;
        if allowed != source {
            return Err("provider promotion source is outside its media family".into());
        }
        let bytes = std::fs::read(source).map_err(|_| "cannot read provider promotion source")?;
        if crate::codec_image::provider_dhash(&bytes).ok() != Some(want) {
            return Ok(false);
        }
        if self.published.exists() {
            physical(&self.published)?;
        }
        std::fs::create_dir_all(&self.published)
            .map_err(|_| "cannot create published provider media")?;
        physical(&self.published)?;
        let destination = self.published.join(format!("{tk}.png"));
        if destination.exists() {
            physical(&destination)?;
        }
        crate::atomic::write_bytes(&destination, &bytes)
            .map_err(|_| "cannot promote proven provider source")?;
        if source != destination {
            std::fs::remove_file(source).map_err(|_| "cannot remove promoted provider staging")?;
        }
        Ok(true)
    }
    pub fn clean(&mut self, keep: Option<&Path>) -> Result<(), String> {
        if !self.directory.exists() {
            return Ok(());
        }
        physical(&self.directory)?;
        let keep =
            keep.filter(|path| path.parent() == Some(self.directory.as_path()) && path.is_file());
        for entry in
            std::fs::read_dir(&self.directory).map_err(|_| "cannot inspect provider staging")?
        {
            let path = entry
                .map_err(|_| "cannot inspect provider staged entry")?
                .path();
            if keep == Some(path.as_path()) {
                continue;
            }
            physical(&path)?;
            if path.is_dir() {
                return Err("unexpected directory in provider staging; retained".into());
            }
            std::fs::remove_file(path).map_err(|_| "cannot clean owned provider file")?;
        }
        if keep.is_none() {
            std::fs::remove_dir(&self.directory)
                .map_err(|_| "cannot clean provider staging directory")?;
        }
        Ok(())
    }
}
