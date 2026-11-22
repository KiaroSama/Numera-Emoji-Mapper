use super::{
    BUNDLED_NOTICES, RUNTIME_ASSETS, binary, collect, copy, digest, member_path, normalized,
    physical, platform, reject_links, stage,
};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn verify(root: &Path) -> Result<Value, String> {
    let root = physical(root)?;
    let manifest_path = root.join("package.json");
    reject_links(&manifest_path)?;
    let file = std::fs::File::open(&manifest_path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    const MANIFEST_LIMIT: u64 = 4 * 1024 * 1024;
    if !metadata.is_file() || metadata.len() > MANIFEST_LIMIT {
        return Err("Standalone package manifest exceeds its size bound".into());
    }
    let mut bytes = Vec::new();
    file.take(MANIFEST_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MANIFEST_LIMIT {
        return Err("Standalone package manifest exceeds its size bound".into());
    }
    let manifest: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Unsupported standalone package manifest")?;
    if manifest["schema"].as_u64() != Some(2)
        || manifest["runtime"] != "rust"
        || manifest.get("python").is_some()
        || manifest.get("wheel").is_some()
    {
        return Err("Unsupported standalone package manifest".into());
    }
    let target = platform()?;
    if manifest["platform"] != target {
        return Err("Download the package matching this operating system and CPU".into());
    }
    if manifest["version"]
        .as_str()
        .is_none_or(|v| v.is_empty() || v.contains(['/', '\\', ':']))
        || manifest["executable"] != binary(&target)
    {
        return Err("Invalid standalone package version or executable".into());
    }
    let files = manifest["files"]
        .as_object()
        .filter(|files| !files.is_empty() && files.len() <= 4096)
        .ok_or("Package manifest has no file inventory")?;
    for required in [
        binary(&target),
        "assets/panel.html",
        "LICENSE",
        "README.md",
        ".env.example",
    ] {
        if !files.contains_key(required) {
            return Err(format!("Package is incomplete: {required}"));
        }
    }
    for (name, _) in BUNDLED_NOTICES {
        if !files.contains_key(*name) {
            return Err(format!("Package redistribution notice is missing: {name}"));
        }
    }
    for name in RUNTIME_ASSETS {
        if !files.contains_key(*name) {
            return Err(format!("Required panel asset is missing: {name}"));
        }
    }
    if target.starts_with("linux-") {
        for linux_asset in [
            "numera-emoji-mapper.desktop",
            "share/icons/hicolor/512x512/apps/numera-emoji-mapper.png",
        ] {
            if !files.contains_key(linux_asset) {
                return Err(format!(
                    "Required Linux desktop icon asset is missing: {linux_asset}"
                ));
            }
        }
    }
    for (name, hash) in files {
        let path = member_path(&root, name)?;
        if name == "package.json"
            || hash.as_str().is_none_or(|value| {
                value.len() != 64
                    || !value.bytes().all(|c| c.is_ascii_hexdigit())
                    || digest(&path).as_deref() != Ok(value)
            })
        {
            return Err(format!("Missing or changed package member: {name}"));
        }
    }
    if std::fs::metadata(root.join(binary(&target)))
        .map_err(|e| e.to_string())?
        .len()
        == 0
    {
        return Err("Standalone native executable must not be empty".into());
    }
    let mut actual = Vec::new();
    // These paths belong to the operator or to runtime scratch, not the archive.
    // In particular the doctor opens its log before checking immutable members.
    for entry in std::fs::read_dir(&root).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("Package member is not UTF-8")?;
        if matches!(
            name,
            "logs"
                | ".cache"
                | ".env"
                | "collection"
                | "coins"
                | "input"
                | "build"
                | "private"
                | "packs"
        ) {
            continue;
        }
        if path.is_dir() {
            collect(&root, &path, &mut actual)?;
        } else {
            member_path(&root, name)?;
            actual.push(name.into());
        }
    }
    if actual
        .iter()
        .any(|name| name != "package.json" && !files.contains_key(name))
    {
        return Err("Unlisted standalone package member".into());
    }
    Ok(manifest)
}

struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Installs a verified package as a whole directory, with an exclusive lock and rollback.
/// Existing installations must verify before replacement; user data is never swept.
pub fn install(package_root: &Path, destination: &Path) -> Result<PathBuf, String> {
    install_with(package_root, destination, |_| Ok(()))
}

fn pristine(root: &Path) -> Result<(), String> {
    let manifest = verify(root)?;
    let mut members = Vec::new();
    collect(root, root, &mut members)?;
    if members
        .iter()
        .any(|name| name != "package.json" && manifest["files"].get(name).is_none())
    {
        return Err(
            "Existing installation contains local data; install into a fresh directory".into(),
        );
    }
    Ok(())
}

fn restore(backup: &Path, destination: &Path, reason: &str) -> Result<(), String> {
    if std::fs::symlink_metadata(destination).is_ok() {
        return Err(format!(
            "{reason}; destination was recreated, previous installation and all local data retained at {}",
            backup.display()
        ));
    }
    std::fs::rename(backup, destination).map_err(|error| {
        format!(
            "{reason}; previous installation and all local data retained at {}: {error}",
            backup.display()
        )
    })
}

fn install_with(
    package_root: &Path,
    destination: &Path,
    after_stage: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let source = physical(package_root)?;
    let manifest = verify(&source)?;
    let destination = normalized(destination)?;
    if destination.starts_with(&source) || source.starts_with(&destination) {
        return Err("Installation and source package must be separate directories".into());
    }
    let parent = destination.parent().ok_or("Installation has no parent")?;
    let name = destination
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid installation name")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let lock_path = parent.join(format!(".{name}.install.lock"));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|e| format!("Installation is locked or unsupported: {e}"))?;
    let _lock = Lock(lock_path);
    if std::fs::symlink_metadata(&destination).is_ok() {
        pristine(&destination)?;
    }
    let stage = stage(parent, "native-install")?;
    for member in manifest["files"]
        .as_object()
        .ok_or("Missing inventory")?
        .keys()
        .map(String::as_str)
        .chain(std::iter::once("package.json"))
    {
        copy(&member_path(&source, member)?, &stage.0.join(member))?;
    }
    verify(&stage.0)?;
    after_stage(&destination)?;
    reject_links(&destination)?;
    let backup = stage.0.with_extension("previous");
    if std::fs::symlink_metadata(&backup).is_ok() {
        return Err("Installation recovery path is occupied".into());
    }
    let existed = destination.exists();
    if existed {
        std::fs::rename(&destination, &backup)
            .map_err(|e| format!("Cannot replace existing installation: {e}"))?;
        // Install locks coordinate installers, not a running application. Check
        // the renamed directory so writes during staging cannot be swept away.
        if let Err(error) = pristine(&backup) {
            restore(&backup, &destination, &error)?;
            return Err(format!(
                "Installation refused; previous installation restored: {error}"
            ));
        }
    }
    if let Err(error) = std::fs::rename(&stage.0, &destination) {
        if existed {
            restore(
                &backup,
                &destination,
                &format!("Installation publication failed: {error}"),
            )?;
        }
        return Err(format!(
            "Installation publication failed; previous installation preserved: {error}"
        ));
    }
    if existed {
        // A process holding a directory/file handle can still write after any
        // finite recheck. Keep the previous directory instead of deleting it.
        eprintln!(
            "Previous installation retained for recovery at {}",
            backup.display()
        );
    }
    Ok(destination.join(
        manifest["executable"]
            .as_str()
            .ok_or("Missing executable")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::NEXT;
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::atomic::Ordering;

    fn package_fixture(root: &Path) {
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let target = platform().unwrap();
        for member in [binary(&target), "LICENSE", "README.md", ".env.example"]
            .into_iter()
            .chain(RUNTIME_ASSETS.iter().copied())
        {
            std::fs::write(root.join(member), b"installation race fixture\n").unwrap();
        }
        if target.starts_with("linux-") {
            for linux_asset in [
                "numera-emoji-mapper.desktop",
                "share/icons/hicolor/512x512/apps/numera-emoji-mapper.png",
            ] {
                let path = root.join(linux_asset);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, b"installation race fixture\n").unwrap();
            }
        }
        for (name, bytes) in BUNDLED_NOTICES {
            let path = root.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        let mut members = Vec::new();
        collect(root, root, &mut members).unwrap();
        let files = members
            .iter()
            .map(|name| (name.clone(), digest(&root.join(name)).unwrap()))
            .collect::<BTreeMap<_, _>>();
        let manifest = json!({"schema":2,"runtime":"rust","version":"2.0.0","platform":target,"executable":binary(&target),"files":files});
        std::fs::write(
            root.join("package.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn operator_writes_during_staging_restore_the_previous_installation() {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let scratch = stage(&parent, "numera-install-race").unwrap();
        let source = scratch.0.join("package");
        package_fixture(&source);
        for member in [".env", "collection/catalog.db", "README.md"] {
            let destination = scratch.0.join(format!(
                "installed-{}",
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let installed = install(&source, &destination).unwrap();
            let executable_hash = digest(&installed).unwrap();
            let error = install_with(&source, &destination, |path| {
                let file = path.join(member);
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                std::fs::write(file, b"operator write while package copies\n")
                    .map_err(|e| e.to_string())
            })
            .unwrap_err();
            assert!(error.contains("previous installation restored"), "{error}");
            assert_eq!(
                std::fs::read(destination.join(member)).unwrap(),
                b"operator write while package copies\n"
            );
            assert_eq!(digest(&installed).unwrap(), executable_hash);
            assert!(
                !destination
                    .with_file_name(format!(
                        ".{}.install.lock",
                        destination.file_name().unwrap().to_string_lossy()
                    ))
                    .exists()
            );
        }
    }
}
