//! Build/bootstrap children reuse the accepted native containment and cleanup owner.
use crate::{logging::RunLog, process_owner::OwnedProcess, tooling_environment};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
static NEXT: AtomicU64 = AtomicU64::new(0);
const CAPTURE_LIMIT: u64 = 8 * 1024 * 1024;

pub(crate) fn clean_unspawned(path: &Path) -> Result<(), String> {
    // Spawn writes this marker before containment; never override uncertain
    // ownership by removing its workspace after a failed spawn.
    if crate::process_owner::cleanup_pending(path) {
        return Ok(());
    }
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "Unspawned tooling workspace cleanup failed; scratch retained: {error}"
        )),
    }
}

pub struct Completed {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn workspace(root: &Path) -> Result<PathBuf, String> {
    let directory = tooling_environment::confined_directory(root, "logs/tmp/tooling", true)?;
    loop {
        let path = directory.join(format!(
            "child-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Execute a finite command with bounded output and the established descendant owner.
pub async fn capture(
    root: &Path,
    argv: &[OsString],
    environment: &BTreeMap<String, String>,
    wall: Duration,
    idle: Duration,
    log: &RunLog,
) -> Result<Completed, String> {
    capture_owned(
        root,
        argv,
        environment,
        wall,
        idle,
        log,
        (OwnedProcess::spawn, Stdio::null()),
    )
    .await
}

/// Dedicated automation process only: scripted input plus the existing descendant owner.
pub async fn capture_with_input(
    root: &Path,
    argv: &[OsString],
    environment: &BTreeMap<String, String>,
    input: &Path,
    wall: Duration,
    idle: Duration,
    log: &RunLog,
) -> Result<Completed, String> {
    if !cfg!(any(windows, target_os = "linux")) {
        return Err(
            "scripted tooling capture requires verified descendant custody; no child started"
                .into(),
        );
    }
    let root = tooling_environment::physical_root(root)?;
    tooling_environment::reject_links(input, "tooling input cannot follow links")?;
    let path =
        crate::migration_bundle::resolved(input).map_err(|_| "cannot resolve tooling input")?;
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| "cannot inspect tooling input")?;
    let invalid = "tooling input must be a regular project-contained file of at most256KiB";
    if !path.starts_with(&root) || !metadata.is_file() || metadata.len() > 256 * 1024 {
        return Err(invalid.into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Refuse writers/replacement while the inherited input handle is live.
        options.share_mode(1).custom_flags(0x0020_0000);
    }
    let file = options
        .open(&path)
        .map_err(|_| "cannot open tooling input")?;
    // A path inspection is not admission of the object actually opened.
    let opened = file
        .metadata()
        .map_err(|_| "cannot inspect opened tooling input")?;
    if !opened.is_file() || opened.len() > 256 * 1024 {
        return Err(invalid.into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if opened.file_attributes() & 0x400 != 0 {
            return Err("tooling input cannot follow links".into());
        }
    }
    #[cfg(any(windows, target_os = "linux"))]
    let spawn: Spawn = OwnedProcess::spawn_descendants;
    #[cfg(not(any(windows, target_os = "linux")))]
    let spawn: Spawn = OwnedProcess::spawn_session;
    capture_owned(
        &root,
        argv,
        environment,
        wall,
        idle,
        log,
        (spawn, Stdio::from(file)),
    )
    .await
}

type Spawn = fn(&mut Command, PathBuf) -> Result<OwnedProcess, String>;

async fn capture_owned(
    root: &Path,
    argv: &[OsString],
    environment: &BTreeMap<String, String>,
    wall: Duration,
    idle: Duration,
    log: &RunLog,
    ownership: (Spawn, Stdio),
) -> Result<Completed, String> {
    let (spawn, input) = ownership;
    let executable = argv.first().ok_or("Tool command is empty")?;
    if wall.is_zero() || idle.is_zero() || idle > wall {
        return Err("Tool command needs positive idle <= wall bounds".into());
    }
    let root = tooling_environment::physical_root(root)?;
    let directory = workspace(&root)?;
    let owner_directory = directory.join("owner");
    let output = directory.join("stdout");
    let errors = directory.join("stderr");
    let captures = (|| -> Result<_, String> {
        std::fs::create_dir(&owner_directory).map_err(|error| error.to_string())?;
        let create = |path: &Path| {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| error.to_string())
        };
        Ok((create(&output)?, create(&errors)?))
    })();
    let (stdout, stderr) = match captures {
        Ok(files) => files,
        Err(error) => {
            clean_unspawned(&directory)?;
            return Err(error);
        }
    };
    let mut command = Command::new(executable);
    command
        .args(&argv[1..])
        .current_dir(&root)
        .env_clear()
        .envs(environment)
        .stdin(input)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let mut owner = match spawn(&mut command, owner_directory) {
        Ok(owner) => owner,
        Err(error) => {
            drop(command);
            clean_unspawned(&directory)?;
            return Err(error);
        }
    };
    drop(command);
    log.event("INFO", "tooling", "owned child started");
    let bounded = |path: &Path| -> Result<(), String> {
        if std::fs::metadata(path)
            .map_err(|error| error.to_string())?
            .len()
            > CAPTURE_LIMIT
        {
            Err("Tool child output exceeded8MiB capture budget".into())
        } else {
            Ok(())
        }
    };
    let result: Result<std::process::ExitStatus, String> = async {
        let wait = owner.wait(wall, idle);
        tokio::pin!(wait);
        loop {
            bounded(&output)?;
            bounded(&errors)?;
            tokio::select! {
                result = &mut wait => break result,
                _ = tokio::time::sleep(Duration::from_millis(50)) => {},
            }
        }
    }
    .await;
    // No capture can be read or removed until process custody has settled.
    owner.finish()?;
    let result = result.and_then(|status| {
        use std::io::Read;
        let decode = |path: &Path| -> Result<String, String> {
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .map_err(|error| error.to_string())?
                .take(CAPTURE_LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() as u64 > CAPTURE_LIMIT {
                return Err("Tool child output exceeded8MiB capture budget".into());
            }
            String::from_utf8(bytes).map_err(|error| {
                format!(
                    "Non-UTF-8 tool output at byte{} (exit {:?}); cannot safely decode",
                    error.utf8_error().valid_up_to(),
                    status.code()
                )
            })
        };
        Ok(Completed {
            code: status.code().unwrap_or(1),
            stdout: decode(&output)?,
            stderr: decode(&errors)?,
        })
    });
    match &result {
        Ok(completed) => log.event(
            "INFO",
            "tooling",
            &format!("owned child exited code={}", completed.code),
        ),
        Err(error) => log.event("ERROR", "tooling", error),
    }
    clean_unspawned(&directory)?;
    result
}
