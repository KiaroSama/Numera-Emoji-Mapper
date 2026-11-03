//! One bounded process/envelope owner shared by the closed historical source-oracle dispatch.
use crate::source_git_inventory::Source;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);
const LIMIT: u64 = 512 * 1024;
struct Workspace(PathBuf);
impl Workspace {
    fn clean(&self) -> Result<(), String> {
        if crate::process_owner::cleanup_pending(&self.0) {
            return Err("source oracle ownership unresolved; scratch retained".into());
        }
        match std::fs::remove_dir_all(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("cannot clean source oracle scratch".into()),
        }
    }
}
pub(crate) fn read(root: &Path, path: &Path) -> Result<Value, String> {
    if !path.is_absolute() {
        return Err("source oracle envelope must be absolute".into());
    }
    crate::tooling_environment::reject_links(path, "source oracle envelope cannot follow links")?;
    if !crate::migration_bundle::resolved(path)
        .map_err(|_| "cannot resolve source oracle envelope")?
        .starts_with(root)
    {
        return Err("source oracle envelope must stay project-contained".into());
    }
    let file = std::fs::File::open(path).map_err(|_| "cannot open source oracle envelope")?;
    let metadata = file
        .metadata()
        .map_err(|_| "cannot inspect source oracle envelope")?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err("source oracle envelope must be a regular file under512KiB".into());
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read source oracle envelope")?;
    if bytes.len() as u64 > LIMIT {
        return Err("source oracle envelope grew beyond512KiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid source oracle JSON".into())
}
pub(crate) fn write_new(path: &Path, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "cannot encode source oracle envelope")?;
    if bytes.len() as u64 > LIMIT {
        return Err("source oracle envelope exceeds512KiB".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "source oracle output must be new")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "cannot persist source oracle envelope".into())
}

pub(crate) async fn check(
    root: &Path,
    executable: &Path,
    oracle: &str,
    source: &Source,
    dependencies: &[&Source],
    fixture: &Value,
) -> Result<Value, String> {
    let root = crate::tooling_environment::physical_root(root)?;
    let parent = crate::tooling_environment::confined_directory(&root, "logs/tmp", true)?;
    let directory = loop {
        let directory = parent.join(format!(
            "source-oracle-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&directory) {
            Ok(()) => break directory,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("cannot create source oracle workspace".into()),
        }
    };
    let workspace = Workspace(directory.clone());
    let result = async {
        let owner_path = directory.join("owner");
        std::fs::create_dir(&owner_path).map_err(|_| "cannot create source oracle owner")?;
        let input = directory.join("request.json");
        let output = directory.join("response.json");
        let dependencies = dependencies.iter().map(|source| {
            json!({"baseline_revision":source.baseline_revision,"path":source.path,"sha256":source.sha256,"source":source.text})
        }).collect::<Vec<_>>();
        write_new(
            &input,
            &json!({"oracle":oracle,"baseline_revision":source.baseline_revision,"path":source.path,"sha256":source.sha256,"source":source.text,"dependencies":dependencies,"fixture":fixture}),
        )?;
        let mut command = Command::new(executable);
        command
            .arg("source-oracle-worker")
            .arg(&input)
            .arg(&output)
            .current_dir(&root)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for name in [
            "PATH",
            "SystemRoot",
            "SYSTEMROOT",
            "WINDIR",
            "LD_LIBRARY_PATH",
            "DYLD_LIBRARY_PATH",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "PYTHONHOME",
            "LANG",
            "LC_ALL",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        if oracle == "catalog-tests"
            && let Some(path) = std::env::var_os("NUMERA_REAL_CATALOG_FIXTURE")
        {
            // Only this explicit local fixture is forwarded; worker admission precedes Python.
            command.env("NUMERA_REAL_CATALOG_FIXTURE", path);
        }
        command
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("PYTHONUTF8", "1")
            .env("PYTHONNOUSERSITE", "1")
            .env("PYTHONSAFEPATH", "1")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("TEMP", &directory)
            .env("TMP", &directory)
            .env("TMPDIR", &directory);
        // Original media generators own inner FFmpeg process groups.
        let mut owner = if matches!(oracle, "roster-tests" | "reencode-tests") {
            crate::process_owner::OwnedProcess::spawn_session(&mut command, owner_path)?
        } else {
            crate::process_owner::OwnedProcess::spawn(&mut command, owner_path)?
        };
        drop(command);
        let (wall, idle) = if oracle == "benchmark" {
            (120, 15)
        } else {
            (15, 8)
        };
        let status = owner
            .wait(Duration::from_secs(wall), Duration::from_secs(idle))
            .await;
        owner.finish()?;
        let status = status?;
        let response = read(&root, &output)?;
        if !status.success() || response["ok"] != true {
            return Err(crate::logging::redact(
                response["error"].as_str().unwrap_or("source oracle failed"),
                &[],
            ));
        }
        let value = response
            .get("result")
            .cloned()
            .ok_or("source oracle result is missing")?;
        workspace.clean()?;
        Ok::<_, String>(value)
    }
    .await;
    result.map_err(|error| {
        format!(
            "{error}; source oracle evidence retained at {}",
            directory.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_or_unwinding_oracle_workspaces_retain_evidence_until_explicit_cleanup() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let root = parent.join(format!(
            "oracle evidence {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        for name in ["returned-error", "unwind", "successful", "unresolved"] {
            std::fs::create_dir(root.join(name)).unwrap();
            std::fs::write(
                root.join(name).join("response.json"),
                b"source failure evidence",
            )
            .unwrap();
        }
        let failed: Result<(), &str> = {
            let _workspace = Workspace(root.join("returned-error"));
            Err("controlled source refusal")
        };
        assert_eq!(failed, Err("controlled source refusal"));
        let unwind = std::panic::catch_unwind(|| {
            let _workspace = Workspace(root.join("unwind"));
            panic!("controlled source observation panic");
        });
        assert!(unwind.is_err());
        for name in ["returned-error", "unwind"] {
            assert_eq!(
                std::fs::read(root.join(name).join("response.json")).unwrap(),
                b"source failure evidence"
            );
        }
        Workspace(root.join("successful")).clean().unwrap();
        assert!(!root.join("successful").exists());
        let pending = root.join("unresolved/codec-cleanup-pending.json");
        std::fs::write(&pending, b"pending owner evidence").unwrap();
        assert!(
            Workspace(root.join("unresolved"))
                .clean()
                .unwrap_err()
                .contains("ownership unresolved")
        );
        assert_eq!(std::fs::read(&pending).unwrap(), b"pending owner evidence");
        assert_eq!(
            std::fs::read(root.join("unresolved/response.json")).unwrap(),
            b"source failure evidence"
        );
        // Only this fixture owns the marker; no child was started or left unresolved.
        std::fs::remove_file(pending).unwrap();
        for name in ["returned-error", "unwind", "unresolved"] {
            Workspace(root.join(name)).clean().unwrap();
        }
        std::fs::remove_dir(root).expect("clean verified oracle ownership fixture");
    }
}
