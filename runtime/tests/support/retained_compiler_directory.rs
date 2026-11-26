//! Exclusive compiler storage retained for cached fixtures and failure diagnostics.
use std::path::{Path, PathBuf};

pub fn allocate(project: &Path, namespace: &str) -> Result<PathBuf, String> {
    let parent =
        numera_runtime::tooling_environment::confined_directory(project, "logs/tmp", true)?;
    let prefix = format!("{namespace}-{}-", std::process::id());
    let directory = tempfile::Builder::new()
        .prefix(&prefix)
        .tempdir_in(parent)
        .map_err(|error| format!("Cannot allocate {namespace} compiler storage: {error}"))?;
    // A PID may be reused while an earlier compiler's evidence still exists.
    // Keep only our fresh, exclusively created directory before any compilation:
    // a failed compiler or OnceLock initializer must not erase its diagnostics.
    Ok(directory.keep())
}
