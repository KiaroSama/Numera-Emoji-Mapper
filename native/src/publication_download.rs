//! Invocation-owned private bytes and codec custody shared by upload verification and live attribution.
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
pub(crate) struct Download {
    pub(crate) path: PathBuf,
    workspace: PathBuf,
    created: bool,
    active: bool,
}
impl Download {
    pub(crate) fn new(data: &Path, prefix: &str, bytes: &[u8]) -> Result<Self, String> {
        let parent = crate::tooling_environment::confined_directory(data, "tmp", true)?;
        let workspace = (0..128)
            .find_map(|_| {
                let path = parent.join(format!(
                    "{prefix}-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                #[cfg(unix)]
                let mut builder = std::fs::DirBuilder::new();
                #[cfg(not(unix))]
                let builder = std::fs::DirBuilder::new();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                match builder.create(&path) {
                    Ok(()) => Some(Ok(path)),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(error) => Some(Err(error.to_string())),
                }
            })
            .ok_or(
                "private download namespace exhausted; refusing to overwrite existing files",
            )??;
        let mut owned = Self {
            path: workspace.join("download.dl"),
            workspace,
            created: false,
            active: true,
        };
        let written = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&owned.path)?;
            owned.created = true;
            file.write_all(bytes)?;
            file.sync_all()
        })();
        if let Err(error) = written {
            owned.clean()?;
            return Err(error.to_string());
        }
        Ok(owned)
    }
    pub(crate) fn codec(&self) -> PathBuf {
        self.workspace.join("codec")
    }
    fn clean(&mut self) -> Result<(), String> {
        if !std::mem::replace(&mut self.active, false) {
            return Ok(());
        }
        crate::tooling_environment::reject_links(
            &self.workspace,
            "private download path became linked; evidence retained",
        )?;
        let codec = self.codec();
        if crate::process_owner::cleanup_pending(&codec)
            || std::fs::read_dir(&codec)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or_else(|error| error.kind() != std::io::ErrorKind::NotFound)
        {
            return Err("private download codec custody unresolved; evidence retained".into());
        }
        for entry in std::fs::read_dir(&self.workspace).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path != self.path && path != codec {
                return Err("unexpected private download evidence; retained".into());
            }
        }
        crate::tooling_environment::reject_links(
            &self.path,
            "private download path became linked; evidence retained",
        )?;
        if self.created {
            std::fs::remove_file(&self.path).map_err(|error| {
                format!("private download cleanup failed; evidence retained: {error}")
            })?;
        } else {
            match std::fs::symlink_metadata(&self.path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                _ => {
                    return Err(
                        "private download creation did not own this file; evidence retained".into(),
                    );
                }
            }
        }
        match std::fs::remove_dir(&codec) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "private download codec evidence remains; retained: {error}"
                ));
            }
        }
        std::fs::remove_dir(&self.workspace)
            .map_err(|error| format!("private download cleanup failed; evidence retained: {error}"))
    }
    pub(crate) fn finish<T>(mut self, result: Result<T, String>) -> Result<T, String> {
        self.clean()?;
        result
    }
}
impl Drop for Download {
    fn drop(&mut self) {
        if let Err(error) = self.clean() {
            eprintln!("ERROR: {}", crate::logging::redact(&error, &[]));
        }
    }
}
