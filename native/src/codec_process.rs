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
    let file = std::fs::File::open(path).map_err(|_| "cannot open codec capture")?;
    if file
        .metadata()
        .map_err(|_| "cannot inspect codec capture")?
        .len()
        > limit
    {
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
    let physical_root = root
        .canonicalize()
        .map_err(|_| "cannot resolve codec root")?;
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
    let stdout = directory.join("stdout.bin");
    let stderr = directory.join("stderr.bin");
    let spawn = (|| {
        let out = std::fs::File::create(&stdout).map_err(|_| "cannot create stdout capture")?;
        let err = std::fs::File::create(&stderr).map_err(|_| "cannot create stderr capture")?;
        let mut command = Command::new(tool);
        command
            .args(args)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err))
            .env("TEMP", &directory)
            .env("TMP", &directory)
            .env("TMPDIR", &directory);
        for (name, _) in std::env::vars() {
            let upper = name.to_uppercase();
            if ["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
                .iter()
                .any(|key| upper.contains(key))
            {
                command.env_remove(name);
            }
        }
        crate::process_owner::OwnedProcess::spawn(&mut command, directory.clone())
    })();
    let mut owner = match spawn {
        Ok(owner) => owner,
        Err(error) => {
            // An owner marker may mean termination failed; never remove its recovery evidence.
            if !directory.join("codec-cleanup-pending.json").exists() {
                std::fs::remove_dir_all(&directory)
                    .map_err(|_| "cannot clean failed codec scratch")?;
            }
            return Err(error);
        }
    };
    let result = async {
        let status = {
            use std::future::{Future, poll_fn};
            use std::task::Poll;
            let mut waiting =
                std::pin::pin!(owner.wait(Duration::from_secs(30), Duration::from_secs(15)));
            // The owner's 100 ms progress timer wakes this same future; no detached monitor.
            poll_fn(|context| {
                for (path, ceiling) in [(&stdout, limit), (&stderr, 1024 * 1024)] {
                    match std::fs::metadata(path) {
                        Ok(metadata) if metadata.len() <= ceiling => {}
                        Ok(_) => {
                            return Poll::Ready(
                                Err("codec capture exceeds its output limit".into()),
                            );
                        }
                        Err(_) => return Poll::Ready(Err("cannot inspect codec capture".into())),
                    }
                }
                waiting.as_mut().poll(context)
            })
            .await?
        };
        if !status.success() {
            return Err(format!(
                "external video codec failed with exit {}",
                status.code().map_or("unknown".into(), |v| v.to_string())
            ));
        }
        Ok(Captured {
            stdout: read_capture(&stdout, limit)?,
            stderr: read_capture(&stderr, 1024 * 1024)?,
        })
    }
    .await;
    owner.finish()?;
    result
}
