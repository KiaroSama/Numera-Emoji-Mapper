//! Standalone schema2 archives. Checksums detect changes; they do not authenticate a publisher.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

// Embed redistribution notices so even an installed binary can package itself
// without a Cargo registry, vendor checkout, or access to the build machine.
const BUNDLED_NOTICES: &[(&str, &[u8])] = &[
    (
        "THIRD_PARTY_NOTICES.md",
        include_bytes!("../../THIRD_PARTY_NOTICES.md"),
    ),
    (
        "licenses/rlottie/NUMERA-SOURCE.md",
        include_bytes!("../vendor/rlottie/NUMERA-SOURCE.md"),
    ),
    (
        "licenses/rlottie/COPYING",
        include_bytes!("../vendor/rlottie/COPYING"),
    ),
    (
        "licenses/rlottie/licenses/COPYING.FTL",
        include_bytes!("../vendor/rlottie/licenses/COPYING.FTL"),
    ),
    (
        "licenses/rlottie/licenses/COPYING.MIT",
        include_bytes!("../vendor/rlottie/licenses/COPYING.MIT"),
    ),
    (
        "licenses/rlottie/licenses/COPYING.MPL",
        include_bytes!("../vendor/rlottie/licenses/COPYING.MPL"),
    ),
    (
        "licenses/rlottie/licenses/COPYING.PIX",
        include_bytes!("../vendor/rlottie/licenses/COPYING.PIX"),
    ),
    (
        "licenses/rlottie/licenses/COPYING.RPD",
        include_bytes!("../vendor/rlottie/licenses/COPYING.RPD"),
    ),
    (
        "licenses/rlottie/licenses/COPYING.SKIA",
        include_bytes!("../vendor/rlottie/licenses/COPYING.SKIA"),
    ),
    (
        "licenses/rlottie/licenses/COPYING.STB",
        include_bytes!("../vendor/rlottie/licenses/COPYING.STB"),
    ),
];

const PANEL_ASSETS: &[&str] = &[
    "assets/panel.html",
    "assets/logo-128.png",
    "assets/panel-grid.js",
    "assets/panel-motion.js",
    "assets/panel-save.js",
    "assets/panel-drag.js",
    "assets/panel-actions.js",
    "assets/panel-holding.js",
    "assets/panel-draft.js",
];

pub fn platform() -> Result<String, String> {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "linux" => "linux",
        "macos" => "macos",
        _ => return Err("Unsupported package platform or architecture".into()),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return Err("Unsupported package platform or architecture".into()),
    };
    Ok(format!("{os}-{arch}"))
}

fn binary(target: &str) -> &'static str {
    if target.starts_with("windows-") {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    }
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("Parent traversal is not supported for package paths".into());
    }
    if path.is_absolute() {
        Ok(path.into())
    } else {
        Ok(std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path))
    }
}

fn reject_links(path: &Path) -> Result<(), String> {
    for ancestor in absolute(path)?.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                #[cfg(windows)]
                let reparse = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let reparse = false;
                if metadata.file_type().is_symlink() || reparse {
                    return Err("Linked/reparse package paths are not supported".into());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

fn physical(path: &Path) -> Result<PathBuf, String> {
    reject_links(path)?;
    let path = crate::migration_bundle::resolved(path)?;
    if !path.is_dir() {
        return Err("Package root must be a directory".into());
    }
    Ok(path)
}

/// Resolve the existing prefix before comparing paths. On Windows the shared
/// resolver also removes canonicalize's verbatim prefix, so DOS/verbatim aliases
/// cannot disagree about confinement or source/destination separation.
fn normalized(path: &Path) -> Result<PathBuf, String> {
    let path = absolute(path)?;
    reject_links(&path)?;
    let mut prefix = path.as_path();
    let mut missing = Vec::new();
    loop {
        match std::fs::symlink_metadata(prefix) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    prefix
                        .file_name()
                        .ok_or("Package path has no existing prefix")?
                        .to_os_string(),
                );
                prefix = prefix
                    .parent()
                    .ok_or("Package path has no existing prefix")?;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    let mut path = crate::migration_bundle::resolved(prefix)?;
    for name in missing.into_iter().rev() {
        path.push(name);
    }
    reject_links(&path)?;
    Ok(path)
}

pub fn digest(path: &Path) -> Result<String, String> {
    reject_links(path)?;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Package members must be regular files".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn copy(source: &Path, destination: &Path) -> Result<(), String> {
    let before = digest(source)?;
    reject_links(destination)?;
    std::fs::create_dir_all(destination.parent().ok_or("Missing package parent")?)
        .map_err(|e| e.to_string())?;
    let mut input = std::fs::File::open(source).map_err(|e| e.to_string())?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|e| e.to_string())?;
    std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
    output.sync_all().map_err(|e| e.to_string())?;
    std::fs::set_permissions(
        destination,
        input.metadata().map_err(|e| e.to_string())?.permissions(),
    )
    .map_err(|e| e.to_string())?;
    if digest(source)? != before || digest(destination)? != before {
        return Err("Package input changed or copy failed byte verification".into());
    }
    Ok(())
}

fn collect(root: &Path, directory: &Path, files: &mut Vec<String>) -> Result<(), String> {
    reject_links(directory)?;
    for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        reject_links(&path)?;
        let metadata = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if metadata.is_dir() {
            collect(root, &path, files)?;
        } else if metadata.is_file() {
            let name = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("Package member is not UTF-8")?
                .replace('\\', "/");
            member_path(root, &name)?;
            files.push(name);
        } else {
            return Err("Package members must be regular files".into());
        }
    }
    files.sort();
    Ok(())
}

fn member_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty()
        || name.contains(['\\', ':', '\0'])
        || name
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err("Invalid package member path".into());
    }
    if name.split('/').any(|part| {
        let part = part.to_ascii_lowercase();
        matches!(part.as_str(), ".venv" | "__pycache__")
            || part.ends_with(".py")
            || part.ends_with(".pyc")
            || part.ends_with(".whl")
    }) {
        return Err("Standalone packages cannot contain Python runtime payloads".into());
    }
    let path = root.join(name);
    reject_links(&path)?;
    Ok(path)
}

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!(
                "Owned package staging cleanup failed; retained {}: {error}",
                self.0.display()
            );
        }
    }
}

fn stage(parent: &Path, label: &str) -> Result<Stage, String> {
    loop {
        let path = parent.join(format!(
            ".{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(Stage(path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Builds only the production crate; compatibility/PyO3 is outside this graph.
pub fn build(root: &Path, release: bool) -> Result<PathBuf, String> {
    let root = physical(root)?;
    let cargo = crate::codec_process::executable("cargo")
        .map_err(|error| format!("Rust/Cargo is required to build the runtime: {error}"))?;
    let mut argv = vec![cargo.into_os_string()];
    argv.extend(
        [
            "build",
            "--manifest-path",
            "runtime/Cargo.toml",
            "--locked",
            "--bin",
            "numera-emoji",
        ]
        .into_iter()
        .map(std::ffi::OsString::from),
    );
    if release {
        argv.push("--release".into());
    }
    let mut inherited = BTreeMap::new();
    for name in [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "PATHEXT",
        "HOME",
        "USERPROFILE",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "LANG",
        "LC_ALL",
        "CARGO_HOME",
        "CARGO_TARGET_DIR",
        "RUSTUP_HOME",
        "RUSTUP_TOOLCHAIN",
        "CARGO_BUILD_JOBS",
        "CC",
        "CXX",
        "AR",
        "CMAKE",
        "INCLUDE",
        "LIB",
        "LIBPATH",
        "SDKROOT",
        "MACOSX_DEPLOYMENT_TARGET",
    ] {
        if let Ok(value) = std::env::var(name) {
            inherited.insert(name.into(), value);
        }
    }
    inherited.insert("RUSTUP_AUTO_INSTALL".into(), "0".into());
    let environment = crate::tooling_environment::environment(&root, &inherited, false)?;
    let log = crate::logging::RunLog::open(&root, "build-runtime", vec![]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let completed = runtime.block_on(crate::tooling_process::capture(
        &root,
        &argv,
        &environment,
        std::time::Duration::from_secs(1800),
        std::time::Duration::from_secs(120),
        &log,
    ))?;
    print!("{}", completed.stdout);
    eprint!("{}", completed.stderr);
    log.finish(completed.code);
    if completed.code != 0 {
        return Err("Standalone Rust build failed".into());
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("runtime/target"));
    let target = if target.is_absolute() {
        target
    } else {
        root.join(target)
    };
    let executable = target
        .join(if release { "release" } else { "debug" })
        .join(binary(&platform()?));
    digest(&executable)?;
    Ok(executable)
}

pub fn package(
    root: &Path,
    executable: &Path,
    libraries: Option<&Path>,
    output: &Path,
    revision: &str,
    target: &str,
) -> Result<PathBuf, String> {
    if ![
        "linux-x64",
        "linux-arm64",
        "windows-x64",
        "windows-arm64",
        "macos-x64",
        "macos-arm64",
    ]
    .contains(&target)
    {
        return Err("Unsupported package platform or architecture".into());
    }
    let root = physical(root)?;
    let version = env!("CARGO_PKG_VERSION");
    digest(executable)?;
    if std::fs::metadata(executable)
        .map_err(|e| e.to_string())?
        .len()
        == 0
    {
        return Err("Standalone native executable must not be empty".into());
    }
    let output = normalized(output)?;
    if !output.starts_with(&root) {
        return Err("Package output must stay inside its owning project".into());
    }
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let output = physical(&output)?;
    if !output.starts_with(&root) {
        return Err("Package output must stay inside its owning project".into());
    }
    let stem = format!("numera-emoji-mapper-{version}-{target}");
    let destination = output.join(format!("{stem}.zip"));
    let checksum_path = destination.with_extension("zip.sha256");
    if std::fs::symlink_metadata(&destination).is_ok()
        || std::fs::symlink_metadata(&checksum_path).is_ok()
    {
        return Err("Archive or checksum already exists".into());
    }
    let stage = stage(&output, "package")?;
    let content = stage.0.join(&stem);
    std::fs::create_dir(&content).map_err(|e| e.to_string())?;
    for member in ["LICENSE", "README.md", ".env.example", "run.sh", "run.ps1"] {
        copy(&root.join(member), &content.join(member))?;
    }
    for (name, bytes) in BUNDLED_NOTICES {
        let destination = member_path(&content, name)?;
        std::fs::create_dir_all(destination.parent().ok_or("Notice has no parent")?)
            .map_err(|e| e.to_string())?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    let mut assets = Vec::new();
    collect(&root, &root.join("assets"), &mut assets)?;
    for member in assets {
        copy(&root.join(&member), &content.join(&member))?;
    }
    for member in PANEL_ASSETS {
        digest(&content.join(member))
            .map_err(|_| format!("Required panel asset is missing: {member}"))?;
    }
    copy(executable, &content.join(binary(target)))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            content.join(binary(target)),
            std::fs::Permissions::from_mode(0o755),
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(libraries) = libraries {
        let libraries = physical(libraries)?;
        let mut names = Vec::new();
        collect(&libraries, &libraries, &mut names)?;
        for name in names {
            if name.contains('/')
                || !(name.ends_with(".dll")
                    || name.ends_with(".dylib")
                    || name.ends_with(".so")
                    || name.contains(".so."))
            {
                return Err(
                    "Native library input must contain only regular shared libraries".into(),
                );
            }
            copy(&libraries.join(&name), &content.join("lib").join(name))?;
        }
    }
    let mut members = Vec::new();
    collect(&content, &content, &mut members)?;
    let files = members
        .iter()
        .map(|name| Ok((name.clone(), digest(&content.join(name))?)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let minimum = if target.starts_with("linux-") {
        "Ubuntu 24.04 / glibc 2.39"
    } else if target.starts_with("macos-") {
        "macOS 15"
    } else {
        "Windows 10"
    };
    let notices = BUNDLED_NOTICES
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    let manifest = json!({"schema":2,"version":version,"revision":revision,"platform":target,"runtime":"rust","executable":binary(target),"minimum_os":minimum,"external_tools":["ffmpeg","ffprobe"],"integrity":"sha256 (not publisher authentication)","notices":notices,"files":files});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(content.join("package.json"))
        .map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut file, &manifest).map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    members.push("package.json".into());
    members.sort();
    let archive_path = stage.0.join("archive.zip");
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&archive_path)
        .map_err(|e| e.to_string())?;
    let mut archive = zip::ZipWriter::new(file);
    for member in members {
        let permissions = if member == binary(target) || member == "run.sh" {
            0o755
        } else {
            0o644
        };
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(permissions);
        archive
            .start_file(format!("{stem}/{member}"), options)
            .map_err(|e| e.to_string())?;
        std::io::copy(
            &mut std::fs::File::open(content.join(member)).map_err(|e| e.to_string())?,
            &mut archive,
        )
        .map_err(|e| e.to_string())?;
    }
    archive
        .finish()
        .map_err(|e| e.to_string())?
        .sync_all()
        .map_err(|e| e.to_string())?;
    let checksum = digest(&archive_path)?;
    let staged_checksum = stage.0.join("archive.sha256");
    std::fs::write(&staged_checksum, format!("{checksum}  {stem}.zip\n"))
        .map_err(|e| e.to_string())?;
    crate::atomic::publish_new(&archive_path, &destination).map_err(|e| e.to_string())?;
    if let Err(error) = crate::atomic::publish_new(&staged_checksum, &checksum_path) {
        // A package is accepted only when both exclusive publications succeed.
        std::fs::remove_file(&destination).map_err(|e| {
            format!("Checksum publication failed ({error}); archive rollback failed: {e}")
        })?;
        return Err(format!(
            "Checksum publication failed; archive rolled back: {error}"
        ));
    }
    Ok(destination)
}

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
        "run.sh",
        "run.ps1",
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
    for name in PANEL_ASSETS {
        if !files.contains_key(*name) {
            return Err(format!("Required panel asset is missing: {name}"));
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
    use super::*;

    fn package_fixture(root: &Path) {
        std::fs::create_dir_all(root.join("assets")).unwrap();
        let target = platform().unwrap();
        for member in [
            binary(&target),
            "LICENSE",
            "README.md",
            ".env.example",
            "run.sh",
            "run.ps1",
        ]
        .into_iter()
        .chain(PANEL_ASSETS.iter().copied())
        {
            std::fs::write(root.join(member), b"installation race fixture\n").unwrap();
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
