//! One native coin-watchdog attempt; marker progress governs idle, containment governs cleanup.
use crate::{
    logging::RunLog, process_owner::OwnedProcess, tooling_environment, tooling_launch,
    tooling_process,
};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, PartialEq, Eq)]
pub struct Marker {
    modified: SystemTime,
    bytes: Vec<u8>,
}

pub fn marker_state(path: &Path) -> Result<Option<Marker>, String> {
    tooling_environment::reject_links(path, "Linked converter progress marker")?;
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    // Source marker is one filename;64KiB exceeds supported pathname sizes but
    // prevents a damaged marker becoming an unbounded arbitrary-file read.
    if !metadata.is_file() || metadata.len() > 65536 {
        return Err("Converter progress marker must be a regular file <=64KiB".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65536 {
        return Err("Converter progress marker exceeds64KiB".into());
    }
    Ok(Some(Marker {
        modified: metadata.modified().map_err(|e| e.to_string())?,
        bytes,
    }))
}

pub fn resolve_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Watchdog {
    pub wall: Duration,
    pub idle: Duration,
    pub poll: Duration,
}
impl Watchdog {
    fn source() -> Self {
        Self {
            wall: Duration::from_secs(3600),
            idle: Duration::from_secs(120),
            poll: Duration::from_secs(2),
        }
    }
    fn code(self, elapsed: Duration, idle: Duration) -> Option<i32> {
        if elapsed >= self.wall {
            Some(125)
        } else if idle >= self.idle {
            Some(124)
        } else {
            None
        }
    }
}

/// Source order is significant: at both limits wall125 wins over idle124.
pub fn deadline_code(elapsed: Duration, idle: Duration) -> Option<i32> {
    Watchdog::source().code(elapsed, idle)
}

pub async fn run(
    root: &Path,
    source: &Path,
    output: &Path,
    stdout: &Path,
    stderr: &Path,
    log: &RunLog,
) -> Result<i32, String> {
    let environment = std::env::vars().collect();
    run_attempt(
        root,
        source,
        output,
        stdout,
        stderr,
        log,
        (&environment, Watchdog::source()),
    )
    .await
}

pub(crate) async fn run_attempt(
    root: &Path,
    source: &Path,
    output: &Path,
    stdout: &Path,
    stderr: &Path,
    log: &RunLog,
    options: (&std::collections::BTreeMap<String, String>, Watchdog),
) -> Result<i32, String> {
    let (original, limits) = options;
    if limits.wall.is_zero()
        || limits.idle.is_zero()
        || limits.poll.is_zero()
        || limits.poll > limits.wall.min(limits.idle)
    {
        return Err("Converter watchdog requires positive wall/idle/poll bounds".into());
    }
    let wait_limit = limits
        .wall
        .checked_add(limits.poll.max(Duration::from_secs(2)))
        .ok_or("Converter watchdog bound overflow")?;
    let root = tooling_environment::physical_root(root)?;
    let source = resolve_path(&root, source);
    let output = resolve_path(&root, output);
    let stdout = resolve_path(&root, stdout);
    let stderr = resolve_path(&root, stderr);
    let arguments = vec![
        "make-emoji-pngs".into(),
        "--in".into(),
        source
            .to_str()
            .ok_or("Converter input path is not UTF-8")?
            .into(),
        "--out".into(),
        output
            .to_str()
            .ok_or("Converter output path is not UTF-8")?
            .into(),
    ];
    let launch = tooling_launch::plan(&root, &arguments, original)?;
    for path in [&output, &stdout, &stderr] {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            launch.root.join(path)
        };
        let parent = if path == &output {
            &absolute
        } else {
            absolute.parent().ok_or("Converter capture has no parent")?
        };
        let relative = parent
            .strip_prefix(&launch.root)
            .map_err(|_| "Converter output/capture must stay inside its owning project")?;
        tooling_environment::confined_directory(
            &launch.root,
            relative
                .to_str()
                .ok_or("Converter capture path is not UTF-8")?,
            false,
        )?;
        tooling_environment::reject_links(&absolute, "Linked converter output/capture")?;
    }
    if stdout == stderr {
        return Err("Converter stdout/stderr captures must be distinct".into());
    }
    let mut command = Command::new(&launch.argv[0]);
    command
        .args(&launch.argv[1..])
        .current_dir(&launch.root)
        .env_clear()
        .envs(&launch.environment)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            std::fs::File::create(&stdout).map_err(|e| e.to_string())?,
        ))
        .stderr(Stdio::from(
            std::fs::File::create(&stderr).map_err(|e| e.to_string())?,
        ));
    let marker = output.join(".svg_cur");
    let mut state = marker_state(&marker)?;
    let started = Instant::now();
    let mut advanced = started;
    let directory = tooling_process::workspace(&launch.root)?;
    let mut owner = match OwnedProcess::spawn(&mut command, directory.clone()) {
        Ok(owner) => owner,
        Err(error) => {
            tooling_process::clean_unspawned(&directory)?;
            return Err(error);
        }
    };
    let result: Result<i32, String> = async {
        // CPU activity alone never resets the conversion marker's idle budget.
        let wait = owner.wait(wait_limit, wait_limit);
        tokio::pin!(wait);
        loop {
            tokio::select! {
                result = &mut wait => {
                    return Ok(result?.code().unwrap_or(1));
                }
                _ = tokio::time::sleep(limits.poll) => {
                    let current = marker_state(&marker)?;
                    if current != state { state = current; advanced = Instant::now(); }
                    if let Some(code) = limits.code(started.elapsed(), advanced.elapsed()) {
                        return Ok(code);
                    }
                }
            }
        }
    }
    .await;
    owner.finish()?;
    match &result {
        Ok(code) => log.event(
            if *code == 0 { "INFO" } else { "ERROR" },
            "converter",
            &format!("native conversion attempt exited code={code}"),
        ),
        Err(error) => log.event("ERROR", "converter", error),
    }
    result
}
