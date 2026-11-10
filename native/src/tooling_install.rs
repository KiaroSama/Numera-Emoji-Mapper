//! Staged native executable/extension copies; an occupied or linked installation stays intact.
use crate::{tooling_environment, tooling_manifest};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Stage(PathBuf);
impl Stage {
    fn clean(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "Native installation staging cleanup failed; staged file retained: {error}"
            )),
        }
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if let Err(error) = self.clean() {
            eprintln!("ERROR: {}", crate::logging::redact(&error, &[]));
        }
    }
}

fn replace(source: &Path, destination: &Path, nonempty: bool) -> Result<PathBuf, String> {
    tooling_environment::reject_links(source, "Unsupported native input path")?;
    let metadata = std::fs::metadata(source).map_err(|e| e.to_string())?;
    if !metadata.is_file() || nonempty && metadata.len() == 0 {
        return Err("Native executable is missing or unsupported.".into());
    }
    tooling_environment::reject_links(
        destination,
        "Refusing to replace a linked native installation.",
    )?;
    let before = tooling_manifest::digest(source)?;
    let parent = destination
        .parent()
        .ok_or("Native installation has no parent")?;
    let (staged, mut output) = loop {
        let path = parent.join(format!(
            ".native-install-{}-{}.tmp",
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
    let result = (|| {
        let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
        std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        if tooling_manifest::digest(source)? != before
            || tooling_manifest::digest(&staged.0)? != before
        {
            return Err("Native executable copy failed byte verification.".into());
        }
        std::fs::set_permissions(&staged.0, metadata.permissions()).map_err(|e| e.to_string())?;
        tooling_environment::reject_links(
            destination,
            "Refusing to replace a linked native installation.",
        )?;
        std::fs::rename(&staged.0, destination).map_err(|e| e.to_string())?;
        Ok(destination.to_path_buf())
    })();
    staged.clean()?;
    result
}

/// Source-compatible physical installed path, separate from disposable Cargo target.
pub fn install_executable(root: &Path, source: &Path) -> Result<PathBuf, String> {
    let root = tooling_environment::physical_root(root)?;
    // Validate before creating an installation directory on a missing-input failure.
    tooling_environment::reject_links(source, "Native executable is missing or unsupported.")?;
    if !std::fs::metadata(source).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0) {
        return Err("Native executable is missing or unsupported.".into());
    }
    let directory = tooling_environment::confined_directory(&root, "native/runtime", true)?;
    replace(
        source,
        &directory.join(if cfg!(windows) {
            "numera-emoji.exe"
        } else {
            "numera-emoji"
        }),
        true,
    )
}

/// Keep auditwheel's relative shared-library directory when checkout shadows site-packages.
pub fn install_extension(root: &Path, source: &Path) -> Result<PathBuf, String> {
    let root = tooling_environment::physical_root(root)?;
    tooling_environment::reject_links(source, "Unsupported native extension path.")?;
    if !source.is_file() {
        return Err("Unsupported native extension path.".into());
    }
    let package = tooling_environment::confined_directory(&root, "emojikit", true)?;
    let libraries = source
        .parent()
        .and_then(Path::parent)
        .ok_or("Unsupported native extension path.")?
        .join("numera_emoji_core.libs");
    if libraries.exists() {
        tooling_environment::reject_links(
            &libraries,
            "Unsupported native shared-library directory.",
        )?;
        if !libraries.is_dir() {
            return Err("Unsupported native shared-library directory.".into());
        }
        let target =
            tooling_environment::confined_directory(&root, "numera_emoji_core.libs", true)?;
        let mut files = std::fs::read_dir(&libraries)
            .map_err(|e| e.to_string())?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        files.sort();
        // Prevalidate the entire dependency set before replacing its first member.
        for dependency in &files {
            tooling_environment::reject_links(
                dependency,
                "Unsupported native shared-library member.",
            )?;
            if !dependency.is_file() {
                return Err("Unsupported native shared-library member.".into());
            }
            tooling_environment::reject_links(
                &target.join(
                    dependency
                        .file_name()
                        .ok_or("Unsupported native shared-library member.")?,
                ),
                "Refusing to replace a linked native shared library.",
            )?;
        }
        for dependency in files {
            replace(
                &dependency,
                &target.join(
                    dependency
                        .file_name()
                        .ok_or("Unsupported native shared-library member.")?,
                ),
                false,
            )?;
        }
    }
    replace(
        source,
        &package.join(
            source
                .file_name()
                .ok_or("Unsupported native extension path.")?,
        ),
        false,
    )
}

/// Resolve the selected wheel's module once for both contributor and offline installation.
pub fn installed_extension(root: &Path, report: &serde_json::Value) -> Result<PathBuf, String> {
    let root = tooling_environment::physical_root(root)?;
    let platlib = report["platlib"]
        .as_str()
        .ok_or("runtime report has no platlib")?;
    let package = Path::new(platlib).join("emojikit");
    tooling_environment::reject_links(
        &package,
        "installed extension directory cannot follow links",
    )?;
    let package = crate::migration_bundle::resolved(&package)?;
    if !package.starts_with(root.join(".venv")) {
        return Err("installed wheel escaped the selected package environment".into());
    }
    let suffixes = report["extension_suffixes"]
        .as_array()
        .filter(|suffixes| !suffixes.is_empty())
        .ok_or("runtime report has no extension suffixes")?;
    if suffixes.iter().any(|suffix| {
        suffix
            .as_str()
            .is_none_or(|suffix| !suffix.starts_with('.') || suffix.contains(['/', '\\', ':']))
    }) {
        return Err("runtime report has invalid extension suffixes".into());
    }
    let mut matches = Vec::new();
    for entry in std::fs::read_dir(package).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("extension filename must be UTF-8")?;
        if suffixes
            .iter()
            .filter_map(|value| value.as_str())
            .any(|suffix| name == format!("_native{suffix}"))
        {
            tooling_environment::reject_links(&path, "installed extension cannot follow links")?;
            if !path.is_file() {
                return Err("installed native extension is not a regular file".into());
            }
            matches.push(path);
        }
    }
    if matches.len() != 1 {
        return Err("installed wheel must contain exactly one native extension".into());
    }
    matches
        .pop()
        .ok_or("installed wheel has no native module".into())
}

/// Refuse before even environment/log directory creation; no live installation overwritten.
pub fn require_fresh_environment(root: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(root.join(".venv")).is_ok() {
        return Err(
            "Existing installation preserved; extract the new package to a separate directory"
                .into(),
        );
    }
    tooling_environment::confined_directory(root, ".venv", false)?;
    Ok(())
}

pub fn offline_install_arguments(root: &Path, wheel: &str) -> Result<Vec<String>, String> {
    if wheel.is_empty()
        || wheel.starts_with('-')
        || wheel.contains(['/', '\\', ':'])
        || !wheel.ends_with(".whl")
    {
        return Err("Invalid application wheel filename".into());
    }
    let text = |path: PathBuf| {
        path.to_str()
            .map(str::to_owned)
            .ok_or("Package path is not UTF-8".to_owned())
    };
    Ok(vec![
        "-m".into(),
        "pip".into(),
        "install".into(),
        "--no-index".into(),
        "--only-binary=:all:".into(),
        "--find-links".into(),
        text(root.join("wheels"))?,
        "-r".into(),
        text(root.join("requirements.txt"))?,
        "-r".into(),
        text(root.join("requirements-coins.txt"))?,
        text(root.join("wheels").join(wheel))?,
    ])
}
