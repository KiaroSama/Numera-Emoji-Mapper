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

/// Finite commands only. Interactive panel/bot lifetime is not certified by this API.
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
        (OwnedProcess::spawn, Stdio::null(), None),
    )
    .await
}

/// Dedicated runner only; Linux adopts orphan descendants, Windows retains its existing Job.
#[cfg(any(windows, target_os = "linux"))]
pub(crate) async fn capture_descendants(
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
        (OwnedProcess::spawn_descendants, Stdio::null(), None),
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
        (spawn, Stdio::from(file), None),
    )
    .await
}

fn retained_paths(root: &Path, stdout: &Path, stderr: &Path) -> Result<(PathBuf, PathBuf), String> {
    let root = tooling_environment::physical_root(root)?;
    let base = tooling_environment::confined_directory(&root, "logs/tmp", false)?;
    let admit = |path: &Path| -> Result<PathBuf, String> {
        if !path.is_absolute()
            || path.components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return Err(
                "retained output requires an absolute logs/tmp/*.log path; choose two new files"
                    .into(),
            );
        }
        tooling_environment::reject_links(
            path,
            "retained output cannot follow links/reparse points; choose new physical files",
        )?;
        let parent = tooling_environment::physical_root(
            path.parent()
                .ok_or("retained output needs an existing parent")?,
        )?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| {
                !n.chars().any(char::is_control) && n.ends_with(".log") && !n.starts_with('.')
            })
            .ok_or("retained output requires a public .log filename")?;
        let target = parent.join(name);
        let relative = target
            .strip_prefix(&root)
            .map_err(|_| "retained output escapes project")?
            .to_str()
            .ok_or("retained output path must be UTF-8")?
            .replace('\\', "/");
        if !crate::tooling_test_observation::paths::public_path(&relative) {
            return Err(
                "retained output path is protected; choose a public logs/tmp directory".into(),
            );
        }
        let absent = matches!(std::fs::symlink_metadata(&target),Err(e) if e.kind()==std::io::ErrorKind::NotFound);
        if !parent.starts_with(&base) || !absent {
            return Err("retained output must be new and project logs/tmp confined; never overwrite an earlier log".into());
        }
        Ok(target)
    };
    let output = admit(stdout)?;
    let errors = admit(stderr)?;
    if output == errors || output.parent() != errors.parent() {
        return Err(
            "retained stdout/stderr need distinct new files in the same owned directory".into(),
        );
    }
    Ok((output, errors))
}

/// Retain direct regular stdout/stderr on every outcome; total capture budget remains8MiB each.
pub(crate) async fn capture_retained(
    root: &Path,
    argv: &[OsString],
    environment: &BTreeMap<String, String>,
    outputs: (&Path, &Path),
    wall: Duration,
    idle: Duration,
    log: &RunLog,
) -> Result<Completed, String> {
    let (stdout, stderr) = outputs;
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (root, argv, environment, stdout, stderr, wall, idle, log);
        Err("retained capture requires verified descendant custody; no child started".into())
    }
    #[cfg(any(windows, target_os = "linux"))]
    {
        let paths = retained_paths(root, stdout, stderr)?;
        capture_owned(
            root,
            argv,
            environment,
            wall,
            idle,
            log,
            (OwnedProcess::spawn_descendants, Stdio::null(), Some(paths)),
        )
        .await
    }
}

#[cfg(test)]
#[path = "tooling_process_retained_tests.rs"]
mod retained_tests;

type Spawn = fn(&mut Command, PathBuf) -> Result<OwnedProcess, String>;

async fn capture_owned(
    root: &Path,
    argv: &[OsString],
    environment: &BTreeMap<String, String>,
    wall: Duration,
    idle: Duration,
    log: &RunLog,
    ownership: (Spawn, Stdio, Option<(PathBuf, PathBuf)>),
) -> Result<Completed, String> {
    let (spawn, input, retained) = ownership;
    let keep = retained.is_some();
    let executable = argv.first().ok_or("Tool command is empty")?;
    if wall.is_zero() || idle.is_zero() || idle > wall {
        return Err("Tool command needs positive idle <= wall bounds".into());
    }
    let root = tooling_environment::physical_root(root)?;
    let directory = workspace(&root)?;
    let owner_directory = directory.join("owner");
    if let Err(error) = std::fs::create_dir(&owner_directory) {
        clean_unspawned(&directory)?;
        return Err(error.to_string());
    }
    let (output, errors) =
        retained.unwrap_or_else(|| (directory.join("stdout"), directory.join("stderr")));
    let captures = (|| {
        let create = |path: &Path| -> Result<std::fs::File, String> {
            let mut options = std::fs::OpenOptions::new();
            options.read(keep).write(true).create_new(true);
            #[cfg(unix)]
            if keep {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options.open(path).map_err(|e| e.to_string())?;
            if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err("opened capture is not regular".into());
            }
            Ok(file)
        };
        if keep {
            retained_paths(&root, &output, &errors)?;
        }
        let files = (create(&output)?, create(&errors)?);
        if keep {
            tooling_environment::reject_links(
                &output,
                "retained stdout changed to a link before spawn",
            )?;
            tooling_environment::reject_links(
                &errors,
                "retained stderr changed to a link before spawn",
            )?;
        }
        Ok::<_, String>(files)
    })();
    let (out, err) = match captures {
        Ok(files) => files,
        Err(error) => {
            clean_unspawned(&directory)?;
            return Err(error);
        }
    };
    let held = if keep {
        match (out.try_clone(), err.try_clone()) {
            (Ok(out), Ok(err)) => Some((out, err)),
            _ => {
                clean_unspawned(&directory)?;
                return Err("cannot retain capture handles; inspect retained logs".into());
            }
        }
    } else {
        None
    };
    let mut command = Command::new(executable);
    command
        .args(&argv[1..])
        .current_dir(&root)
        .env_clear()
        .envs(environment)
        .stdin(input)
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));
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
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > CAPTURE_LIMIT {
            return Err("Tool child output exceeded8MiB capture budget".into());
        }
        Ok(())
    };
    let mut sizes = None;
    let progress = || -> Result<bool, String> {
        let Some((out, err)) = &held else {
            return Ok(false);
        };
        tooling_environment::reject_links(&output, "retained stdout changed to a link")?;
        tooling_environment::reject_links(&errors, "retained stderr changed to a link")?;
        let current = (
            out.metadata().map_err(|e| e.to_string())?.len(),
            err.metadata().map_err(|e| e.to_string())?.len(),
        );
        if current.0 > CAPTURE_LIMIT || current.1 > CAPTURE_LIMIT {
            return Err("Tool child output exceeded8MiB capture budget".into());
        }
        let changed = sizes != Some(current);
        sizes = Some(current);
        Ok(changed)
    };
    let status: Result<std::process::ExitStatus, String> = async {
        let wait = owner.wait_progress(wall, idle, progress);
        tokio::pin!(wait);
        let status = loop {
            bounded(&output)?;
            bounded(&errors)?;
            tokio::select! {
                result = &mut wait => break result?.status()?,
                _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            }
        };
        Ok(status)
    }
    .await;
    // Stop closed-stdio descendants and close the child's capture handles before
    // reading complete outputs. Only owner/ is removed by the process owner.
    if let Err(error) = owner.finish() {
        log.event("ERROR", "tooling", &error);
        return Err(error);
    }
    let result: Result<Completed, String> = (|| {
        if let Some((out, err)) = &held {
            out.sync_all()
                .and_then(|_| err.sync_all())
                .map_err(|_| "cannot sync retained capture files")?;
        }
        let status = status?;
        let decode = |path: &Path| -> Result<String, String> {
            use std::io::Read;
            bounded(path)?;
            let mut bytes = Vec::new();
            if let Some((out, err)) = &held {
                use std::io::{Seek, SeekFrom};
                let mut file = if path == output { out } else { err };
                file.seek(SeekFrom::Start(0))
                    .map_err(|_| "cannot rewind retained capture")?;
                file.take(CAPTURE_LIMIT + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
            } else {
                let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
                file.take(CAPTURE_LIMIT + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
            }
            if bytes.len() as u64 > CAPTURE_LIMIT {
                return Err("Tool child output exceeded8MiB capture budget".into());
            }
            String::from_utf8(bytes).map_err(|e| {
                format!(
                    "Non-UTF-8 tool output at byte{} (exit {:?}); cannot safely decode",
                    e.utf8_error().valid_up_to(),
                    status.code()
                )
            })
        };
        Ok(Completed {
            code: status.code().unwrap_or(1),
            stdout: decode(&output)?,
            stderr: decode(&errors)?,
        })
    })();
    // Log only bounded metadata; raw captured child bodies may contain private data.
    match &result {
        Ok(completed) => {
            log.event(
                "INFO",
                "tooling",
                &format!("owned child exited code={}", completed.code),
            );
            log.event(
                "DEBUG",
                "tooling",
                &format!(
                    "captured stdout_bytes={} stderr_bytes={}",
                    completed.stdout.len(),
                    completed.stderr.len()
                ),
            );
        }
        Err(error) => log.event("ERROR", "tooling", error),
    }
    clean_unspawned(&directory)?;
    result
}
