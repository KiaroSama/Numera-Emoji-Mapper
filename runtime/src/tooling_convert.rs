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

#[cfg(windows)]
fn ordinary_verbatim_component(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    // Verbatim paths preserve slash, trailing dot/space and DOS-special names;
    // bridging their prefix to DOS is faithful only for ordinary components.
    // See std::path::Prefix and Microsoft's Naming Files/Paths/Namespaces.
    if name.ends_with('.')
        || name.ends_with(' ')
        || name
            .chars()
            .any(|ch| ch <= '\u{1f}' || matches!(ch, '<' | '>' | ':' | '"' | '/' | '|' | '?' | '*'))
    {
        return false;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$"
    ) {
        return false;
    }
    if let Some(number) = stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"))
    {
        let mut chars = number.chars();
        if matches!(chars.next(), Some('1'..='9' | '¹' | '²' | '³')) && chars.next().is_none() {
            return false;
        }
    }
    true
}

fn owned_path(root: &Path, path: &Path, directory: bool) -> Result<PathBuf, String> {
    use std::path::Component;
    const OUTSIDE: &str = "Converter output/capture must stay inside its owning project";
    // Bind the original spelling before any stat/canonicalization. An outside
    // UNC/device path must never cause filesystem or credential access.
    if path.components().any(|part| part == Component::ParentDir) {
        return Err(OUTSIDE.into());
    }
    // Win32 may trim ASCII spaces and interpret this ordinary DOS component
    // as parent traversal. Refuse before even inspecting its filesystem path.
    #[cfg(windows)]
    if path.components().any(|part| {
        matches!(part, Component::Normal(name) if name.to_str().is_some_and(|text| text.trim_end_matches(' ') == ".."))
    }) {
        return Err(OUTSIDE.into());
    }
    #[cfg(windows)]
    if let Some(Component::Prefix(prefix)) = path.components().next() {
        use std::path::Prefix;
        if !path.is_absolute()
            || !matches!(
                prefix.kind(),
                Prefix::Disk(_)
                    | Prefix::VerbatimDisk(_)
                    | Prefix::UNC(..)
                    | Prefix::VerbatimUNC(..)
            )
        {
            return Err(OUTSIDE.into());
        }
        if matches!(
            prefix.kind(),
            Prefix::VerbatimDisk(_) | Prefix::VerbatimUNC(..)
        ) && path.components().any(|part| {
            matches!(part, Component::CurDir | Component::ParentDir)
                || matches!(part, Component::Normal(name) if !ordinary_verbatim_component(name))
        }) {
            return Err(OUTSIDE.into());
        }
    }
    let absolute = resolve_path(root, path);
    let mut parts = absolute.components();
    for expected in root.components() {
        let Some(actual) = parts.next() else {
            return Err(OUTSIDE.into());
        };
        #[cfg(windows)]
        let same = {
            use std::path::Prefix;
            match (expected, actual) {
                (Component::Prefix(left), Component::Prefix(right)) => {
                    match (left.kind(), right.kind()) {
                        (
                            Prefix::Disk(left) | Prefix::VerbatimDisk(left),
                            Prefix::Disk(right) | Prefix::VerbatimDisk(right),
                        ) => left == right,
                        (
                            Prefix::UNC(left_server, left_share)
                            | Prefix::VerbatimUNC(left_server, left_share),
                            Prefix::UNC(right_server, right_share)
                            | Prefix::VerbatimUNC(right_server, right_share),
                        ) => left_server == right_server && left_share == right_share,
                        _ => false,
                    }
                }
                (left, right) => left == right,
            }
        };
        #[cfg(not(windows))]
        let same = expected == actual;
        if !same {
            return Err(OUTSIDE.into());
        }
    }
    let relative = parts.collect::<PathBuf>();
    // Reuse the original directory-syntax and physical-confinement admission
    // before canonicalizing a candidate. Captures keep their prior leaf-name
    // policy; their containing directory is the confined directory.
    let containing = if directory {
        relative.as_path()
    } else {
        relative.parent().ok_or("Converter capture has no parent")?
    };
    tooling_environment::confined_directory(
        root,
        containing
            .to_str()
            .ok_or("Converter capture path is not UTF-8")?,
        false,
    )?;
    // Keep original ancestor admission before a resolver can hide a link.
    tooling_environment::reject_links(&absolute, "Linked converter output/capture")?;
    let normalized = tooling_environment::canonical_existing_prefix(&root.join(relative))?;
    if !normalized.starts_with(root) {
        return Err(OUTSIDE.into());
    }
    tooling_environment::reject_links(&normalized, "Linked converter output/capture")?;
    Ok(normalized)
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
    run_attempt_with_launch(
        root,
        source,
        output,
        stdout,
        stderr,
        log,
        (options, tooling_launch::plan),
    )
    .await
}

async fn run_attempt_with_launch<F>(
    root: &Path,
    source: &Path,
    output: &Path,
    stdout: &Path,
    stderr: &Path,
    log: &RunLog,
    controls: ((&std::collections::BTreeMap<String, String>, Watchdog), F),
) -> Result<i32, String>
where
    F: FnOnce(
        &Path,
        &[String],
        &std::collections::BTreeMap<String, String>,
    ) -> Result<tooling_launch::Launch, String>,
{
    let (options, launch_factory) = controls;
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
    let output = owned_path(&root, output, true)?;
    let stdout = owned_path(&root, stdout, false)?;
    let stderr = owned_path(&root, stderr, false)?;
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
    let launch = launch_factory(&root, &arguments, original)?;
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
    // native-convert-owner is a dedicated automation process; nested codec
    // sessions must remain in custody after the converter is stopped.
    let mut owner = match OwnedProcess::spawn_descendants(&mut command, directory.clone()) {
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

#[cfg(test)]
#[path = "baseline_cli_convert_tests.rs"]
mod baseline_cli_convert_tests;

#[cfg(test)]
#[path = "supplemental_tooling_convert_tests.rs"]
mod supplemental_tooling_convert_tests;
