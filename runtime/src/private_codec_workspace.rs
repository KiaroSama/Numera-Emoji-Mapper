//! Private codec storage for external catalogs, worker envelopes and retained diagnostics.
use crate::tooling_environment::confined_directory;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(windows)]
#[path = "private_codec_workspace_windows.rs"]
mod windows;

pub(crate) type EvidenceAllocator = fn(&Path) -> io::Result<()>;
const EXTERNAL_DATA: &str = "external-data";

/// The worker has already physically confined scratch and rejected links and reparse points.
/// Select once, before creating an envelope, and keep that allocator when moving outside scratch.
pub(crate) fn evidence_allocator(root: &Path, scratch: &Path) -> Result<EvidenceAllocator, String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let scratch = scratch.canonicalize().map_err(|error| error.to_string())?;
    let relative = scratch
        .strip_prefix(root)
        .map_err(|_| "codec scratch escaped project")?;
    Ok(select_allocator(relative, create_private_evidence))
}

pub(crate) fn create_private_evidence(path: &Path) -> io::Result<()> {
    private_directory(path, false)
}

fn select_allocator(relative: &Path, private: EvidenceAllocator) -> EvidenceAllocator {
    let mut parts = relative.iter();
    let external = ["logs", "tmp", EXTERNAL_DATA].into_iter().all(|name| {
        parts.next().is_some_and(|part| {
            if cfg!(windows) {
                part.eq_ignore_ascii_case(std::ffi::OsStr::new(name))
            } else {
                part == name
            }
        })
    });
    if external {
        private
    } else {
        |path| std::fs::create_dir(path)
    }
}

#[cfg(windows)]
use windows::private_directory;

pub(crate) fn external_base(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let parent = confined_directory(root, "logs/tmp", true)?;
    let base = confined_directory(&parent, EXTERNAL_DATA, false)?;
    prepare(&base, relative)
        .map_err(|error| format!("private external codec workspace unavailable: {error}"))?;
    confined_directory(&parent, EXTERNAL_DATA, false)
}

#[cfg(windows)]
fn prepare(base: &Path, relative: &str) -> Result<(), String> {
    // Parent traversal can be bypassed on Windows. Protect every shared directory
    // which will host new work so files and per-invocation directories inherit its DACL.
    private_directory(base, true).map_err(|e| e.to_string())?;
    let target = confined_directory(base, relative, false)?;
    let mut current = base.to_owned();
    for part in target
        .strip_prefix(base)
        .map_err(|e| e.to_string())?
        .components()
    {
        current.push(part);
        private_directory(&current, true).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(unix)]
fn prepare(path: &Path, _: &str) -> Result<(), String> {
    private_directory(path, true).map_err(|error| error.to_string())
}

#[cfg(unix)]
fn private_directory(path: &Path, reuse: bool) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    match std::fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if reuse && error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = std::fs::symlink_metadata(path)?;
    // geteuid takes no arguments or pointers and cannot fail.
    #[allow(unsafe_code)]
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o777 != 0o700 {
        return Err(io::Error::other(
            "expected a directory owned by this account with mode 0700",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_external_evidence_requires_private_directory_support() {
        // Model a volume with no persistent ACLs, including the existing internal routes.
        let unavailable: EvidenceAllocator = |_| Err(io::ErrorKind::Unsupported.into());
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent = confined_directory(parent, "logs/tmp", true).unwrap();
        let directory = parent.join(format!("codec allocation policy {}", std::process::id()));
        for relative in [
            "logs/tmp/external-data",
            "logs/tmp/external-data/tmp/publication/invocation",
        ] {
            let create = select_allocator(Path::new(relative), unavailable);
            assert_eq!(
                create(&directory).unwrap_err().kind(),
                io::ErrorKind::Unsupported
            );
            assert!(!directory.exists());
        }
        for relative in [
            "data/tmp/media",
            "logs/tmp/native-codec",
            "logs/tmp/external-data-backup",
            "other/logs/tmp/external-data",
        ] {
            let create = select_allocator(Path::new(relative), unavailable);
            create(&directory).unwrap();
            assert_eq!(
                create(&directory).unwrap_err().kind(),
                io::ErrorKind::AlreadyExists
            );
            std::fs::remove_dir(&directory).unwrap();
        }
    }
}
