//! Schema1 source allowlist packaging; no arbitrary checkout sweep or partial archive publication.
use crate::{atomic, tooling_environment, tooling_manifest};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Stage(PathBuf);
impl Stage {
    fn clean(&self) -> Result<(), String> {
        match std::fs::remove_dir_all(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => {
                let _ = atomic::write_json(
                    &self.0.join("package-cleanup-pending.json"),
                    &json!({"version":1,"reason":"owned package staging cleanup failed"}),
                );
                Err(format!(
                    "Owned package staging cleanup failed; recovery retained: {error}"
                ))
            }
        }
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if !self.0.join("package-cleanup-pending.json").exists()
            && let Err(error) = self.clean()
        {
            eprintln!("ERROR: {}", crate::logging::redact(&error, &[]));
        }
    }
}

fn regular(path: &Path, error: &str) -> Result<(), String> {
    tooling_environment::reject_links(path, error)?;
    if !std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return Err(error.into());
    }
    Ok(())
}

fn copy(source: &Path, destination: &Path) -> Result<(), String> {
    regular(source, "Missing or linked package input")?;
    let before = tooling_manifest::digest(source)?;
    std::fs::create_dir_all(destination.parent().ok_or("Package member has no parent")?)
        .map_err(|e| e.to_string())?;
    let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| e.to_string())?;
    std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
    output.sync_all().map_err(|e| e.to_string())?;
    drop(output);
    if tooling_manifest::digest(destination)? != before
        || tooling_manifest::digest(source)? != before
    {
        return Err("Package input changed or failed byte verification".into());
    }
    Ok(())
}

/// Retains the accepted013 source selection until primary approves final014 retirement.
pub fn source_members(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut members = [
        "LICENSE",
        "README.md",
        "run.ps1",
        "requirements.txt",
        "requirements-coins.txt",
        "pyproject.toml",
        ".env.example",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect::<Vec<_>>();
    for folder in ["emojikit", "assets", "coins"] {
        let directory = root.join(folder);
        if !directory.exists() {
            continue;
        }
        tooling_environment::reject_links(&directory, "Linked package input directory")?;
        let mut files = std::fs::read_dir(&directory)
            .map_err(|e| e.to_string())?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        files.sort();
        for file in files {
            if file.is_file()
                && (folder == "assets" || file.extension().and_then(|v| v.to_str()) == Some("py"))
            {
                regular(&file, "Missing or linked package input")?;
                members.push(
                    file.strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .to_path_buf(),
                );
            }
        }
    }
    members.extend(
        ["keywords.csv", "run_convert.ps1", "run_rebuild_loop.ps1"]
            .into_iter()
            .map(|name| Path::new("coins").join(name)),
    );
    members.extend(
        [
            "install_prebuilt.py",
            "prebuilt_manifest.py",
            "launch_prebuilt.py",
            "local_environment.py",
            "build_native.py",
            "build_job.py",
            "run-actions.ps1",
            "run-native.ps1",
            "native_convert_owner.py",
        ]
        .into_iter()
        .map(|name| Path::new("scripts").join(name)),
    );
    members.sort();
    Ok(members)
}

fn archive_members(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(current).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        tooling_environment::reject_links(&path, "Linked package staging member")?;
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        if metadata.is_dir() {
            archive_members(root, &path, files)?;
        } else if metadata.is_file() {
            files.push(
                path.strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_path_buf(),
            );
        } else {
            return Err("Unsupported package staging member".into());
        }
    }
    Ok(())
}

pub fn package(
    root: &Path,
    executable: &Path,
    wheel: &Path,
    dependencies: &Path,
    output: &Path,
    revision: &str,
    target: &str,
) -> Result<PathBuf, String> {
    if ![
        "windows-x64",
        "windows-arm64",
        "linux-x64",
        "linux-arm64",
        "macos-x64",
        "macos-arm64",
    ]
    .contains(&target)
    {
        return Err("Unsupported package platform or architecture".into());
    }
    let root = tooling_environment::physical_root(root)?;
    let version = tooling_manifest::project_version(&root)?;
    if version.contains(['/', '\\', ':']) || version.is_empty() {
        return Err("Invalid application version for package filename".into());
    }
    regular(
        executable,
        "A regular native executable and wheel are required",
    )?;
    regular(wheel, "A regular native executable and wheel are required")?;
    let wheel_name = wheel
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid wheel filename")?;
    if !wheel_name.ends_with(".whl")
        || !wheel_name.starts_with(&format!("numera_emoji_core-{version}-"))
    {
        return Err("Wheel version does not match this application".into());
    }
    let raw_output = if output.is_absolute() {
        output.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(output)
    };
    let relative = raw_output
        .strip_prefix(&root)
        .map_err(|_| "Package output must stay inside its owning project")?;
    let output = tooling_environment::confined_directory(
        &root,
        relative.to_str().ok_or("Package output is not UTF-8")?,
        true,
    )?;
    let stem = format!("numera-emoji-mapper-{version}-{target}");
    let destination = output.join(format!("{stem}.zip"));
    if std::fs::symlink_metadata(&destination).is_ok() {
        return Err(format!(
            "Archive already exists: {}",
            destination
                .file_name()
                .ok_or("Invalid archive name")?
                .to_string_lossy()
        ));
    }
    let stage = loop {
        let directory = output.join(format!(
            "package-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&directory) {
            Ok(()) => break Stage(directory),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    let result = (|| {
        let content = stage.0.join(&stem);
        std::fs::create_dir(&content).map_err(|e| e.to_string())?;
        for member in source_members(&root)? {
            copy(&root.join(&member), &content.join(member))?;
        }
        let binary = if target.starts_with("windows-") {
            "native/runtime/numera-emoji.exe"
        } else {
            "native/runtime/numera-emoji"
        };
        copy(executable, &content.join(binary))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(content.join(binary), std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        copy(wheel, &content.join("wheels").join(wheel_name))?;
        tooling_environment::reject_links(
            dependencies,
            "Dependency payload must contain regular wheels only",
        )?;
        for entry in std::fs::read_dir(dependencies).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let file = entry.path();
            regular(&file, "Dependency payload must contain regular wheels only")?;
            if file.extension().and_then(|v| v.to_str()) != Some("whl") {
                return Err("Dependency payload must contain regular wheels only".into());
            }
            if entry.file_name() == wheel.file_name().ok_or("Invalid wheel filename")? {
                return Err("Duplicate application wheel".into());
            }
            copy(&file, &content.join("wheels").join(entry.file_name()))?;
        }
        let mut members = Vec::new();
        archive_members(&content, &content, &mut members)?;
        members.sort();
        let mut hashes = BTreeMap::new();
        for member in &members {
            hashes.insert(
                member
                    .to_str()
                    .ok_or("Package member is not UTF-8")?
                    .replace('\\', "/"),
                tooling_manifest::digest(&content.join(member))?,
            );
        }
        let minimum = if target.starts_with("linux-") {
            "Ubuntu 24.04 / glibc 2.39"
        } else if target.starts_with("macos-") {
            "macOS 15"
        } else {
            "Windows 10"
        };
        let manifest = json!({"schema":1,"version":version,"revision":revision,"platform":target,"python":"3.11","wheel":wheel_name,"files":hashes,"minimum_os":minimum});
        let mut bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        atomic::write_bytes(&content.join("package.json"), &bytes).map_err(|e| e.to_string())?;
        members.push(PathBuf::from("package.json"));
        members.sort();
        let temporary = stage.0.join("archive.zip");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        let mut archive = zip::ZipWriter::new(file);
        for member in members {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(if member == Path::new(binary) {
                    0o755
                } else {
                    0o644
                });
            archive
                .start_file(
                    format!(
                        "{stem}/{}",
                        member
                            .to_str()
                            .ok_or("Package member is not UTF-8")?
                            .replace('\\', "/")
                    ),
                    options,
                )
                .map_err(|e| e.to_string())?;
            let mut source =
                std::fs::File::open(content.join(member)).map_err(|e| e.to_string())?;
            std::io::copy(&mut source, &mut archive).map_err(|e| e.to_string())?;
        }
        let file = archive.finish().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        let checksum = tooling_manifest::digest(&temporary)?;
        atomic::publish_new(&temporary, &destination).map_err(|e| e.to_string())?;
        if let Err(error) = atomic::write_bytes(
            &destination.with_extension("zip.sha256"),
            format!("{checksum}  {}.zip\n", stem).as_bytes(),
        ) {
            let _ = atomic::write_json(
                &stage.0.join("package-cleanup-pending.json"),
                &json!({"version":1,"reason":"archive published but checksum publication failed"}),
            );
            return Err(format!(
                "Archive published but checksum publication failed; no complete package accepted, recovery retained: {error}"
            ));
        }
        Ok(destination)
    })();
    // Publication errors already leave an explicit recovery marker. All other
    // exits clean this invocation's stage and propagate cleanup failure.
    if !stage.0.join("package-cleanup-pending.json").exists() {
        stage.clean()?;
    }
    result
}
