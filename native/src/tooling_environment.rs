//! Source-compatible physical temp/cache confinement; no process-global environment mutation.
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|e| e.to_string())
    }
}

fn linked(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Check original path ancestors before canonicalization can hide a junction or link.
pub fn reject_links(path: &Path, error: &str) -> Result<(), String> {
    let path = absolute(path)?;
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) if linked(&metadata) => return Err(error.into()),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

pub fn project_root() -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    for directory in executable.ancestors().skip(1) {
        if directory.join("assets/panel.html").is_file()
            && directory.join("pyproject.toml").is_file()
        {
            return Ok(directory.into());
        }
    }
    Err("cannot locate project assets beside native executable".into())
}

pub fn physical_root(root: &Path) -> Result<PathBuf, String> {
    reject_links(
        root,
        "Project-local storage cannot use a linked/reparse directory",
    )?;
    let root = crate::migration_bundle::resolved(root)?;
    if !root.is_dir() {
        return Err("Project-local storage root must be a directory".into());
    }
    Ok(root)
}

fn canonical_existing_prefix(path: &Path) -> Result<PathBuf, String> {
    let mut missing = Vec::new();
    let mut ancestor = path;
    loop {
        match std::fs::symlink_metadata(ancestor) {
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or("Project-local storage escaped its root")?,
                );
                ancestor = ancestor
                    .parent()
                    .ok_or("Project-local storage escaped its root")?;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let mut resolved = crate::migration_bundle::resolved(ancestor)?;
    for name in missing.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

pub fn confined_directory(root: &Path, relative: &str, create: bool) -> Result<PathBuf, String> {
    let root = absolute(root)?;
    let path = Path::new(relative);
    // Backslashes and drive prefixes must not become harmless filenames when a
    // Windows-shaped request is checked on a Unix package runner.
    if path.is_absolute()
        || relative.starts_with('/')
        || relative.contains(':')
        || relative.split(['/', '\\']).any(|part| part == "..")
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return Err("Project-local storage escaped its root".into());
    }
    let directory = root.join(path);
    reject_links(
        &directory,
        &format!("Linked project-local storage: {relative}"),
    )?;
    let physical = physical_root(&root)?;
    if !canonical_existing_prefix(&directory)?.starts_with(&physical) {
        return Err(format!(
            "Project-local storage escaped its root: {relative}"
        ));
    }
    if create {
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        reject_links(
            &directory,
            &format!("Linked project-local storage: {relative}"),
        )?;
        if !crate::migration_bundle::resolved(&directory)?.starts_with(&physical) {
            return Err(format!(
                "Project-local storage escaped its root: {relative}"
            ));
        }
    }
    Ok(directory)
}

pub fn environment(
    root: &Path,
    original: &BTreeMap<String, String>,
    build: bool,
) -> Result<BTreeMap<String, String>, String> {
    let root = physical_root(root)?;
    let mut paths = vec![
        ("TEMP", "logs/tmp"),
        ("TMP", "logs/tmp"),
        ("TMPDIR", "logs/tmp"),
        ("PIP_CACHE_DIR", ".cache/pip"),
        ("XDG_CACHE_HOME", ".cache"),
        ("PYTHONPYCACHEPREFIX", ".cache/pycache"),
    ];
    if build {
        paths.extend([
            ("CARGO_HOME", ".cache/cargo"),
            ("CARGO_TARGET_DIR", "native/target"),
            ("RUSTUP_HOME", ".cache/rustup"),
            ("CCACHE_DIR", ".cache/ccache"),
            ("SCCACHE_DIR", ".cache/sccache"),
            ("HOMEBREW_CACHE", ".cache/homebrew"),
            ("HOMEBREW_TEMP", "logs/tmp"),
            ("npm_config_cache", ".cache/npm"),
        ]);
    }
    // Validate every destination before creating even the first cache directory.
    for (_, relative) in &paths {
        confined_directory(&root, relative, false)?;
    }
    let mut result = original.clone();
    for (key, relative) in paths {
        let directory = confined_directory(&root, relative, true)?;
        result.insert(
            key.into(),
            directory
                .to_str()
                .ok_or("Project-local storage path is not UTF-8")?
                .into(),
        );
    }
    result.insert("PYTHONUTF8".into(), "1".into());
    result.insert("PIP_DISABLE_PIP_VERSION_CHECK".into(), "1".into());
    Ok(result)
}
