//! Production archives run from extracted copies with no Python, wheel or source tree.
use numera_runtime::package;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let path = parent.join(format!(
            "numera-standalone-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture(path: &Path) {
    std::fs::create_dir_all(path.join("assets")).unwrap();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for member in ["LICENSE", "README.md", ".env.example", "run.sh", "run.ps1"] {
        std::fs::copy(project.join(member), path.join(member)).unwrap();
    }
    fn assets(source: &Path, destination: &Path) {
        std::fs::create_dir_all(destination).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                assets(&entry.path(), &destination.join(entry.file_name()));
            } else {
                std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
            }
        }
    }
    assets(&project.join("assets"), &path.join("assets"));
}

fn extract(archive: &Path, destination: &Path) -> PathBuf {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(archive).unwrap()).unwrap();
    let prefix = archive
        .by_index(0)
        .unwrap()
        .name()
        .split('/')
        .next()
        .unwrap()
        .to_owned();
    archive.extract(destination).unwrap();
    destination.join(prefix)
}

fn archive(root: &Path) -> PathBuf {
    package::package(
        root,
        Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
        None,
        &root.join("dist"),
        "integration-fixture",
        &package::platform().unwrap(),
    )
    .unwrap()
}

fn offline(
    binary: &Path,
    directory: &Path,
    empty_path: &Path,
    args: &[&str],
) -> std::process::Output {
    let mut command = std::process::Command::new(binary);
    command
        .env_clear()
        .env("PATH", empty_path)
        .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
        .env("PYO3_PYTHON", empty_path.join("python-must-not-run"))
        .env("PYTHONHOME", empty_path.join("python-home-must-not-exist"))
        .env(
            "PYTHONPATH",
            empty_path.join("python-modules-must-not-exist"),
        )
        .current_dir(directory)
        .args(args);
    // The Windows loader needs its system directory; none of these selects Python.
    for name in ["SystemRoot", "WINDIR", "SYSTEMDRIVE"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command.output().unwrap()
}

#[test]
fn extracted_installed_runtime_converts_offline_without_python_or_build_sources() {
    let scratch = Scratch::new();
    let source = scratch.0.join("build source");
    fixture(&source);
    let zip = archive(&source);
    let external = scratch.0.join("downloads");
    std::fs::create_dir(&external).unwrap();
    let package_root = extract(&zip, &external);
    let metadata = package::verify(&package_root).unwrap();
    assert_eq!(metadata["schema"], 2);
    assert_eq!(metadata["runtime"], "rust");
    assert!(metadata.get("python").is_none());
    assert!(metadata.get("wheel").is_none());
    assert!(metadata["files"].get("THIRD_PARTY_NOTICES.md").is_some());
    for member in [
        "NUMERA-SOURCE.md",
        "COPYING",
        "licenses/COPYING.FTL",
        "licenses/COPYING.MIT",
        "licenses/COPYING.MPL",
        "licenses/COPYING.PIX",
        "licenses/COPYING.RPD",
        "licenses/COPYING.SKIA",
        "licenses/COPYING.STB",
    ] {
        let packaged = package_root.join("licenses/rlottie").join(member);
        let original = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("vendor/rlottie")
            .join(member);
        assert_eq!(
            std::fs::read(&packaged).unwrap(),
            std::fs::read(original).unwrap()
        );
        assert_eq!(
            metadata["files"][format!("licenses/rlottie/{member}")],
            package::digest(&packaged).unwrap()
        );
    }
    let notices = std::fs::read_to_string(package_root.join("THIRD_PARTY_NOTICES.md")).unwrap();
    assert!(
        notices
            .contains("This software is based in part on the work of the Independent JPEG Group.")
    );
    for component in [
        "image 0.25.10",
        "resvg 0.50.0",
        "libwebp-sys 0.14.4",
        "mozjpeg 0.10.13",
        "mozjpeg-sys 2.2.3",
        "Google hereby grants",
        "LEGAL ISSUES",
    ] {
        assert!(
            notices.contains(component),
            "Missing native media notice: {component}"
        );
    }
    let destination = scratch.0.join("installed application");
    let executable = package::install(&package_root, &destination).unwrap();
    std::fs::remove_dir_all(source).unwrap();
    std::fs::remove_dir_all(external).unwrap();
    let empty_path = scratch.0.join("no interpreters or compilers");
    std::fs::create_dir(&empty_path).unwrap();
    assert!(!destination.join("emojikit").exists());
    assert!(!destination.join(".venv").exists());
    assert!(!destination.join("runtime").exists());
    let help = offline(&executable, &scratch.0, &empty_path, &["--help"]);
    assert!(
        help.status.success(),
        "{}",
        String::from_utf8_lossy(&help.stderr)
    );
    assert!(String::from_utf8_lossy(&help.stdout).contains("numera-emoji"));
    let doctor = offline(&executable, &scratch.0, &empty_path, &["doctor"]);
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    assert!(String::from_utf8_lossy(&doctor.stdout).contains("Environment check complete"));
    #[cfg(unix)]
    {
        let doctor = offline(
            &destination.join("run.sh"),
            &scratch.0,
            &empty_path,
            &["--check"],
        );
        assert!(
            doctor.status.success(),
            "{}",
            String::from_utf8_lossy(&doctor.stderr)
        );
        let status = offline(
            &destination.join("run.sh"),
            &scratch.0,
            &empty_path,
            &["status"],
        );
        assert_eq!(status.status.code(), Some(3));
    }
    #[cfg(windows)]
    {
        // Preserve the original actual PowerShell launcher/doctor acceptance
        // type on schema2, independently of the schema1 Python compatibility lane.
        let powershell =
            PathBuf::from(std::env::var_os("SystemRoot").expect("Windows system root"))
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let launcher =
            numera_runtime::migration_bundle::resolved(&destination.join("run.ps1")).unwrap();
        let checked = offline(
            &powershell,
            &scratch.0,
            &empty_path,
            &[
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-File",
                launcher.to_str().unwrap(),
                "-NoRelaunch",
                "-Check",
            ],
        );
        assert_eq!(
            checked.status.code(),
            Some(0),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&checked.stdout),
            String::from_utf8_lossy(&checked.stderr)
        );
        assert!(
            String::from_utf8_lossy(&checked.stdout).contains("Environment check complete"),
            "PowerShell doctor omitted its completion message: stdout_len={} stdout_prefix={:02x?} stdout={:?} stderr_len={} stderr_prefix={:02x?} stderr={:?}",
            checked.stdout.len(),
            &checked.stdout[..checked.stdout.len().min(128)],
            String::from_utf8_lossy(&checked.stdout),
            checked.stderr.len(),
            &checked.stderr[..checked.stderr.len().min(128)],
            String::from_utf8_lossy(&checked.stderr)
        );
        assert!(!destination.join(".venv").exists());
        assert_eq!(std::fs::read_dir(&empty_path).unwrap().count(), 0);
    }
    let input = scratch.0.join("input");
    let output = scratch.0.join("output");
    std::fs::create_dir(&input).unwrap();
    std::fs::copy(
        destination.join("assets/numera-emoji-mapper-logo.png"),
        input.join("art.png"),
    )
    .unwrap();
    let converted = offline(
        &executable,
        &scratch.0,
        &empty_path,
        &[
            "make-emoji-pngs",
            "--in",
            input.to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
        ],
    );
    assert!(
        converted.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&converted.stdout),
        String::from_utf8_lossy(&converted.stderr)
    );
    let images = std::fs::read_dir(output)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .collect::<Vec<_>>();
    assert_eq!(images.len(), 1);
    let image = image::open(&images[0]).unwrap();
    assert_eq!((image.width(), image.height()), (100, 100));
}

#[test]
fn hashes_inventory_paths_and_exclusive_install_preserve_existing_bytes() {
    let scratch = Scratch::new();
    let root = scratch.0.join("source");
    fixture(&root);
    let zip = archive(&root);
    assert!(
        package::package(
            &root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            None,
            &root.join("dist"),
            "duplicate",
            &package::platform().unwrap()
        )
        .is_err()
    );
    let extracted = extract(&zip, &scratch.0.join("downloads"));
    let destination = scratch.0.join("installed");
    let executable = package::install(&extracted, &destination).unwrap();
    let original = package::digest(&executable).unwrap();
    assert_eq!(
        package::install(&extracted, &destination).unwrap(),
        executable
    );
    let backups = std::fs::read_dir(&scratch.0)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "previous")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        backups.len(),
        1,
        "replacement must retain previous bytes for late application writes"
    );
    std::fs::write(backups[0].join(".env"), b"late operator write\n").unwrap();
    assert_eq!(
        std::fs::read(backups[0].join(".env")).unwrap(),
        b"late operator write\n"
    );
    let lock = scratch.0.join(".installed.install.lock");
    std::fs::write(&lock, b"other installer").unwrap();
    assert!(
        package::install(&extracted, &destination)
            .unwrap_err()
            .contains("locked")
    );
    std::fs::remove_file(lock).unwrap();
    std::fs::write(destination.join(".env"), b"operator-owned configuration\n").unwrap();
    assert!(package::verify(&destination).is_ok());
    assert!(
        package::install(&extracted, &destination)
            .unwrap_err()
            .contains("local data")
    );
    assert_eq!(package::digest(&executable).unwrap(), original);
    assert_eq!(
        std::fs::read(destination.join(".env")).unwrap(),
        b"operator-owned configuration\n"
    );
    std::fs::remove_file(destination.join(".env")).unwrap();
    std::fs::write(extracted.join("README.md"), b"changed").unwrap();
    assert!(package::verify(&extracted).unwrap_err().contains("changed"));
    assert!(package::install(&extracted, &destination).is_err());
    assert_eq!(package::digest(&executable).unwrap(), original);
    let manifest_path = extracted.join("package.json");
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["files"]
        .as_object_mut()
        .unwrap()
        .insert("../escape".into(), Value::String("0".repeat(64)));
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(package::verify(&extracted).is_err());
    assert_eq!(package::digest(&executable).unwrap(), original);
}

#[test]
fn standalone_inventory_refuses_unlisted_python_and_linked_inputs() {
    let scratch = Scratch::new();
    let root = scratch.0.join("source");
    fixture(&root);
    let zip = archive(&root);
    let extracted = extract(&zip, &scratch.0.join("downloads"));
    std::fs::write(extracted.join("payload.py"), b"must not execute").unwrap();
    assert!(package::verify(&extracted).unwrap_err().contains("Python"));
    std::fs::remove_file(extracted.join("payload.py")).unwrap();
    let notice = extracted.join("licenses/rlottie/licenses/COPYING.MIT");
    std::fs::remove_file(&notice).unwrap();
    assert!(
        package::verify(&extracted)
            .unwrap_err()
            .contains("COPYING.MIT")
    );
    let original =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/rlottie/licenses/COPYING.MIT");
    std::fs::copy(original, notice).unwrap();
    let incomplete = scratch.0.join("incomplete-source");
    fixture(&incomplete);
    std::fs::remove_file(incomplete.join("assets/panel-draft.js")).unwrap();
    assert!(
        package::package(
            &incomplete,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            None,
            &incomplete.join("dist"),
            "missing-panel-asset",
            &package::platform().unwrap()
        )
        .unwrap_err()
        .contains("panel-draft.js")
    );
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(extracted.join("package.json")).unwrap()).unwrap();
    let original_manifest = manifest.clone();
    manifest["files"]
        .as_object_mut()
        .unwrap()
        .remove("assets/panel-grid.js");
    std::fs::write(
        extracted.join("package.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(
        package::verify(&extracted)
            .unwrap_err()
            .contains("Required panel asset")
    );
    manifest = original_manifest.clone();
    manifest["files"]
        .as_object_mut()
        .unwrap()
        .remove("licenses/rlottie/licenses/COPYING.MIT");
    std::fs::write(
        extracted.join("package.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(
        package::verify(&extracted)
            .unwrap_err()
            .contains("redistribution notice")
    );
    manifest = original_manifest;
    manifest["files"]
        .as_object_mut()
        .unwrap()
        .insert("/absolute".into(), Value::String("0".repeat(64)));
    std::fs::write(
        extracted.join("package.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(package::verify(&extracted).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(root.join("LICENSE"), root.join("assets/linked")).unwrap();
        assert!(
            package::package(
                &root,
                Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
                None,
                &root.join("another-dist"),
                "linked",
                &package::platform().unwrap()
            )
            .unwrap_err()
            .contains("Linked")
        );
    }
}

#[test]
fn existing_and_missing_descendants_and_source_ancestors_refuse_before_writes() {
    let scratch = Scratch::new();
    let source = scratch.0.join("source");
    fixture(&source);
    let zip = archive(&source);
    let extracted = extract(&zip, &scratch.0.join("downloads"));
    let existing_child = extracted.join("existing-child");
    std::fs::create_dir(&existing_child).unwrap();
    let missing_child = extracted.join("missing-parent/install");
    let executable = extracted.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    let before = package::digest(&executable).unwrap();
    for destination in [
        &extracted,
        &existing_child,
        &missing_child,
        extracted.parent().unwrap(),
    ] {
        let error = package::install(&extracted.join("."), destination).unwrap_err();
        assert!(error.contains("separate directories"), "{error}");
    }
    assert!(!missing_child.parent().unwrap().exists());
    assert_eq!(package::digest(&executable).unwrap(), before);
    assert!(package::verify(&extracted).is_ok());
}

#[cfg(windows)]
#[test]
fn windows_dos_cli_paths_and_verbatim_aliases_share_confinement_rules() {
    let scratch = Scratch::new();
    let source = scratch.0.join("DOS source with spaces");
    fixture(&source);
    let normal = numera_runtime::migration_bundle::resolved(&source).unwrap();
    assert!(!normal.to_string_lossy().starts_with(r"\\?\"));
    let executable = normal.join("numera-emoji.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let api_output = normal.join("API output");
    let verbatim = source.canonicalize().unwrap();
    assert!(verbatim.to_string_lossy().starts_with(r"\\?\"));
    let zip = package::package(
        &verbatim,
        &executable,
        None,
        &api_output,
        "windows-alias",
        &package::platform().unwrap(),
    )
    .unwrap();
    assert!(zip.is_file());
    let empty_path = scratch.0.join("empty PATH");
    std::fs::create_dir(&empty_path).unwrap();
    let cli_output = normal.join("CLI output");
    let result = offline(
        &executable,
        &scratch.0,
        &empty_path,
        &[
            "package-native",
            "--executable",
            executable.to_str().unwrap(),
            "--out",
            cli_output.to_str().unwrap(),
            "--revision",
            "windows-dos-cli",
        ],
    );
    assert!(
        result.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let cli_archive = PathBuf::from(String::from_utf8(result.stdout).unwrap().trim());
    assert!(cli_archive.is_file());
    let extracted = extract(&zip, &scratch.0.join("downloads"));
    let normal_package = numera_runtime::migration_bundle::resolved(&extracted).unwrap();
    let verbatim_package = extracted.canonicalize().unwrap();
    for (source, destination) in [
        (&normal_package, verbatim_package.join("missing/install")),
        (&verbatim_package, normal_package.clone()),
        (
            &verbatim_package,
            normal_package.parent().unwrap().to_path_buf(),
        ),
    ] {
        assert!(
            package::install(source, &destination)
                .unwrap_err()
                .contains("separate directories")
        );
    }
    assert!(!normal_package.join("missing").exists());
    let destination = normal.join("separate installation");
    let installed = package::install(&verbatim_package, &destination).unwrap();
    assert_eq!(installed, destination.join("numera-emoji.exe"));
    assert!(package::verify(&destination).is_ok());
}
