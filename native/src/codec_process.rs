//! Captured codec commands remain owned, bounded and synchronously reaped.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Captured {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Present only after owner finalization, bounded capture reads and scratch cleanup succeed.
pub(crate) struct SettledCapture {
    pub(crate) outcome: crate::process_owner::WaitOutcome,
    pub(crate) captured: Captured,
}
impl SettledCapture {
    pub(crate) fn into_legacy(self) -> Result<Captured, String> {
        let status = self.outcome.status()?;
        if !status.success() {
            return Err(format!(
                "external video codec failed with exit {}",
                status
                    .code()
                    .map_or("unknown".into(), |value| value.to_string())
            ));
        }
        Ok(self.captured)
    }
}

pub struct Budget {
    pub wall: Duration,
    pub idle: Duration,
    pub warning: Option<String>,
}

pub fn codec_budget(raw: Option<&str>) -> Result<Budget, String> {
    let (value, warning) =
        crate::config_integer::resolve(raw, "EMOJI_FFMPEG_TIMEOUT", 300, Some(1), None);
    let seconds = value
        .parse::<u64>()
        .map_err(|_| "codec timeout exceeds native duration range; refusing")?;
    Ok(Budget {
        wall: Duration::from_secs(seconds),
        idle: Duration::from_secs(seconds),
        warning,
    })
}

struct Workspace(PathBuf);
impl Workspace {
    fn clean(&self) -> Result<(), String> {
        if crate::process_owner::cleanup_pending(&self.0) {
            return Err("codec ownership is unresolved; capture scratch retained".into());
        }
        match std::fs::remove_dir_all(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("cannot clean owned codec capture scratch".into()),
        }
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        if let Err(error) = self.clean() {
            eprintln!("ERROR: {error}");
        }
    }
}

pub fn executable(name: &str) -> Result<PathBuf, String> {
    let path = std::env::var_os("PATH").ok_or("codec PATH is missing")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        });
        if candidate.is_file() {
            return candidate
                .canonicalize()
                .map_err(|_| format!("cannot resolve {name}"));
        }
    }
    Err(format!(
        "{name} not found on PATH (required for video emoji)."
    ))
}

fn read_capture(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "cannot open codec capture")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("codec capture must be a regular non-symlink file".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("codec capture must not be a reparse point".into());
        }
    }
    // Check the opened object's type too; this does not prove path/object identity across races.
    let file = std::fs::File::open(path).map_err(|_| "cannot open codec capture")?;
    let opened = file
        .metadata()
        .map_err(|_| "cannot inspect codec capture")?;
    if !opened.is_file() {
        return Err("opened codec capture must be a regular file".into());
    }
    if opened.len() > limit {
        return Err("codec capture exceeds its output limit".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read codec capture")?;
    if bytes.len() as u64 > limit {
        return Err("codec capture exceeds its output limit".into());
    }
    Ok(bytes)
}

pub async fn capture(
    root: &Path,
    tool: &Path,
    args: &[String],
    limit: u64,
) -> Result<Captured, String> {
    capture_with_input(root, tool, args, limit, None).await
}

pub async fn capture_with_input(
    root: &Path,
    tool: &Path,
    args: &[String],
    limit: u64,
    input: Option<&Path>,
) -> Result<Captured, String> {
    capture_settled_input(root, tool, args, limit, input).await?
}

pub(crate) async fn capture_settled_input(
    root: &Path,
    tool: &Path,
    args: &[String],
    limit: u64,
    input: Option<&Path>,
) -> Result<Result<Captured, String>, String> {
    Ok(capture_outcome_input(root, tool, args, limit, input)
        .await?
        .into_legacy())
}

pub(crate) async fn capture_outcome_input(
    root: &Path,
    tool: &Path,
    args: &[String],
    limit: u64,
    input: Option<&Path>,
) -> Result<SettledCapture, String> {
    let environment = std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>();
    let raw = environment.get(std::ffi::OsStr::new("EMOJI_FFMPEG_TIMEOUT"));
    let raw = raw
        .map(|value| value.to_str().ok_or("codec timeout is not UTF-8"))
        .transpose()?;
    let budget = codec_budget(raw)?;
    capture_outcome(root, tool, args, limit, input, &environment, budget).await
}

pub async fn capture_with_environment(
    root: &Path,
    tool: &Path,
    args: &[String],
    limit: u64,
    input: Option<&Path>,
    environment: &std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
) -> Result<Captured, String> {
    capture_with_budget(
        root,
        tool,
        args,
        limit,
        input,
        environment,
        Budget {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(15),
            warning: None,
        },
    )
    .await
}

pub async fn capture_with_budget(
    root: &Path,
    tool: &Path,
    args: &[String],
    limit: u64,
    input: Option<&Path>,
    environment: &std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    budget: Budget,
) -> Result<Captured, String> {
    capture_settled(root, tool, args, limit, input, environment, budget).await?
}

/// Only a fully settled command failure is recoverable; custody and I/O errors stay outer errors.
pub(crate) async fn capture_settled(
    root: &Path,
    tool: &Path,
    args: &[String],
    limit: u64,
    input: Option<&Path>,
    environment: &std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    budget: Budget,
) -> Result<Result<Captured, String>, String> {
    Ok(
        capture_outcome(root, tool, args, limit, input, environment, budget)
            .await?
            .into_legacy(),
    )
}

pub(crate) async fn capture_outcome(
    root: &Path,
    tool: &Path,
    args: &[String],
    limit: u64,
    input: Option<&Path>,
    environment: &std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    budget: Budget,
) -> Result<SettledCapture, String> {
    if budget.wall.is_zero() || budget.idle.is_zero() || budget.idle > budget.wall {
        return Err("capture needs positive idle <= wall bounds".into());
    }
    if let Some(warning) = &budget.warning {
        eprint!("{warning}");
    }
    crate::tooling_environment::reject_links(root, "codec root must not be linked/reparse")?;
    let physical_root = root
        .canonicalize()
        .map_err(|_| "cannot resolve codec root")?;
    let stdin = if let Some(input) = input {
        crate::tooling_environment::reject_links(
            input,
            "capture input must not use linked/reparse ancestors",
        )?;
        let metadata =
            std::fs::symlink_metadata(input).map_err(|_| "cannot inspect capture input")?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 256 * 1024 {
            return Err("capture input must be a regular project-local file under256KiB".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("capture input must not be a reparse point".into());
            }
        }
        if !input
            .canonicalize()
            .map_err(|_| "cannot resolve capture input")?
            .starts_with(&physical_root)
        {
            return Err("capture input must remain physically project-local".into());
        }
        Stdio::from(std::fs::File::open(input).map_err(|_| "cannot open capture input")?)
    } else {
        Stdio::null()
    };
    let parent = root.join("logs/tmp");
    for path in [root.join("logs"), parent.clone()] {
        if let Err(error) = std::fs::symlink_metadata(&path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err("cannot inspect codec scratch parent".into());
            }
            std::fs::create_dir(&path).map_err(|_| "cannot create codec scratch parent")?;
        }
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|_| "cannot inspect codec scratch parent")?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || !path
                .canonicalize()
                .map_err(|_| "cannot resolve codec scratch parent")?
                .starts_with(&physical_root)
        {
            return Err("codec scratch must remain in physical project directories".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("codec scratch must not use reparse points".into());
            }
        }
    }
    let directory = loop {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!("video-codec-{}-{id}", std::process::id()));
        match std::fs::create_dir(&path) {
            Ok(()) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("cannot create owned codec scratch".into()),
        }
    };
    let workspace = Workspace(directory.clone());
    let owner_directory = directory.join("owner");
    std::fs::create_dir(&owner_directory)
        .map_err(|_| "cannot create capture ownership directory")?;
    let stdout = directory.join("stdout.bin");
    let stderr = directory.join("stderr.bin");
    let spawn = (|| {
        let out = std::fs::File::create(&stdout).map_err(|_| "cannot create stdout capture")?;
        let err = std::fs::File::create(&stderr).map_err(|_| "cannot create stderr capture")?;
        let mut command = Command::new(tool);
        command
            .args(args)
            .current_dir(root)
            .env_clear()
            .envs(environment)
            .stdin(stdin)
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err))
            .env("TEMP", &directory)
            .env("TMP", &directory)
            .env("TMPDIR", &directory);
        for name in environment.keys() {
            let upper = name.to_string_lossy().to_uppercase();
            if ["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
                .iter()
                .any(|key| upper.contains(key))
            {
                command.env_remove(name);
            }
        }
        crate::process_owner::OwnedProcess::spawn(&mut command, owner_directory)
    })();
    let mut owner = match spawn {
        Ok(owner) => owner,
        Err(error) => {
            workspace.clean()?;
            return Err(error);
        }
    };
    let status = async {
        let status = {
            use std::future::{Future, poll_fn};
            use std::task::Poll;
            let mut waiting = std::pin::pin!(owner.wait_outcome(budget.wall, budget.idle));
            // The owner's 100 ms progress timer wakes this same future; no detached monitor.
            poll_fn(|context| {
                for (path, ceiling) in [(&stdout, limit), (&stderr, 1024 * 1024)] {
                    match std::fs::metadata(path) {
                        Ok(metadata) if metadata.len() <= ceiling => {}
                        Ok(_) => {
                            let error = "codec capture exceeds its output limit".into();
                            return Poll::Ready(Err(error));
                        }
                        Err(_) => return Poll::Ready(Err("cannot inspect codec capture".into())),
                    }
                }
                waiting.as_mut().poll(context)
            })
            .await?
        };
        Ok::<_, String>(status)
    }
    .await;
    owner.finish()?;
    let result = (|| {
        let status = status?;
        // A failed/timed-out probe must not bypass final output admission after its pipes close.
        let stdout = read_capture(&stdout, limit)?;
        let stderr = read_capture(&stderr, 1024 * 1024)?;
        Ok(SettledCapture {
            outcome: status,
            captured: Captured { stdout, stderr },
        })
    })();
    workspace.clean()?;
    result
}

#[cfg(all(test, any(unix, windows)))]
mod settled_tests {
    use super::*;
    use crate::process_owner::WaitOutcome;

    fn finished(code: u32) -> WaitOutcome {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            WaitOutcome::Finished(std::process::ExitStatus::from_raw((code as i32) << 8))
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            WaitOutcome::Finished(std::process::ExitStatus::from_raw(code))
        }
    }

    fn captured(outcome: WaitOutcome) -> SettledCapture {
        SettledCapture {
            outcome,
            captured: Captured {
                stdout: b"exact stdout\0\xff".to_vec(),
                stderr: b"exact stderr\n".to_vec(),
            },
        }
    }

    #[test]
    fn actual_outcomes_keep_bytes_and_preserve_legacy_success_exit_and_timeout_messages() {
        for (outcome, expected) in [
            (finished(0), None),
            (
                finished(37),
                Some("external video codec failed with exit 37"),
            ),
            (
                WaitOutcome::Wall(Duration::from_secs(7)),
                Some("media adapter exceeded 7s wall budget"),
            ),
            (
                WaitOutcome::Idle(Duration::from_secs(11)),
                Some("media adapter made no CPU/IO progress for 11s"),
            ),
        ] {
            let observed = captured(outcome);
            assert_eq!(observed.captured.stdout, b"exact stdout\0\xff");
            assert_eq!(observed.captured.stderr, b"exact stderr\n");
            match &observed.outcome {
                WaitOutcome::Finished(status) => {
                    assert_eq!(status.code(), Some(if expected.is_none() { 0 } else { 37 }))
                }
                WaitOutcome::Wall(limit) => assert_eq!(*limit, Duration::from_secs(7)),
                WaitOutcome::Idle(limit) => assert_eq!(*limit, Duration::from_secs(11)),
            }
            let legacy = observed.into_legacy();
            if let Some(message) = expected {
                assert_eq!(legacy.err().unwrap(), message);
            } else {
                let legacy = legacy.unwrap();
                assert_eq!(legacy.stdout, b"exact stdout\0\xff");
                assert_eq!(legacy.stderr, b"exact stderr\n");
            }
        }
    }

    #[test]
    fn invalid_budgets_remain_outer_errors_before_capture_can_start() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let environment = std::collections::BTreeMap::new();
        for (wall, idle) in [(0, 1), (1, 0), (1, 2)] {
            let budget = || Budget {
                wall: Duration::from_secs(wall),
                idle: Duration::from_secs(idle),
                warning: None,
            };
            let structured = runtime.block_on(capture_outcome(
                Path::new("unopened-root"),
                Path::new("unstarted-tool"),
                &[],
                1,
                None,
                &environment,
                budget(),
            ));
            assert_eq!(
                structured.err().unwrap(),
                "capture needs positive idle <= wall bounds"
            );
            let legacy = runtime.block_on(capture_settled(
                Path::new("unopened-root"),
                Path::new("unstarted-tool"),
                &[],
                1,
                None,
                &environment,
                budget(),
            ));
            assert_eq!(
                legacy.err().unwrap(),
                "capture needs positive idle <= wall bounds"
            );
        }
    }

    #[test]
    fn capture_reads_require_regular_nonlinked_files_with_exact_bounds() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let root = parent.join(format!(
            "capture read admission {}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let source = root.join("capture.bin");
        let bytes = b"capture\0\xff";
        std::fs::write(&source, bytes).unwrap();
        assert_eq!(read_capture(&source, bytes.len() as u64).unwrap(), bytes);
        assert_eq!(
            read_capture(&source, bytes.len() as u64 - 1).unwrap_err(),
            "codec capture exceeds its output limit"
        );
        assert_eq!(
            read_capture(&root, 4096).unwrap_err(),
            "codec capture must be a regular non-symlink file"
        );
        assert_eq!(
            read_capture(&root.join("missing"), 4096).unwrap_err(),
            "cannot open codec capture"
        );
        #[cfg(unix)]
        {
            let link = root.join("linked-capture");
            std::os::unix::fs::symlink(&source, &link).unwrap();
            assert_eq!(
                read_capture(&link, 4096).unwrap_err(),
                "codec capture must be a regular non-symlink file"
            );
            assert_eq!(std::fs::read_link(&link).unwrap(), source);
            std::fs::remove_file(&link).unwrap();
            std::os::unix::fs::symlink(root.join("missing"), &link).unwrap();
            assert_eq!(
                read_capture(&link, 4096).unwrap_err(),
                "codec capture must be a regular non-symlink file"
            );
            std::fs::remove_file(link).unwrap();
        }
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
        std::fs::remove_file(source).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn signal_status_remains_observable_without_parsing_unknown_legacy_exit_text() {
        use std::os::unix::process::ExitStatusExt;
        let observed = captured(WaitOutcome::Finished(std::process::ExitStatus::from_raw(9)));
        let WaitOutcome::Finished(status) = &observed.outcome else {
            panic!("signal was not a finished status")
        };
        assert_eq!(status.code(), None);
        assert_eq!(status.signal(), Some(9));
        assert_eq!(observed.captured.stdout, b"exact stdout\0\xff");
        assert_eq!(observed.captured.stderr, b"exact stderr\n");
        assert_eq!(
            observed.into_legacy().err().unwrap(),
            "external video codec failed with exit unknown"
        );
    }
}
