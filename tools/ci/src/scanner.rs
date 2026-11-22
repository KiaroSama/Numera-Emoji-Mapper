use super::{
    ARCHIVE_LIMIT, ARCHIVE_SHA256, CAPTURE_LIMIT, DOWNLOAD_URL, FULL_SCOPE, Finding, Identity,
    LOG_OPTIONS, PR_SCOPE, REPORT_LIMIT, Result, VERSION, partition_findings, summary, valid_sha,
};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub(super) struct Captured {
    pub(super) status: ExitStatus,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
}

pub(super) fn bounded(
    command: &mut Command,
    timeout: Duration,
    temporary: &Path,
) -> Result<Captured> {
    let stdout = tempfile::NamedTempFile::new_in(temporary)
        .map_err(|_| "Could not create private child output capture.")?;
    let stderr = tempfile::NamedTempFile::new_in(temporary)
        .map_err(|_| "Could not create private child error capture.")?;
    command
        .stdin(Stdio::null())
        .stdout(
            stdout
                .reopen()
                .map_err(|_| "Could not capture child output.")?,
        )
        .stderr(
            stderr
                .reopen()
                .map_err(|_| "Could not capture child errors.")?,
        );
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "A verified child could not start.")?;
    let started = Instant::now();
    let status = loop {
        let state = child.try_wait();
        let oversized = [stdout.path(), stderr.path()].iter().any(|path| {
            fs::metadata(path)
                .map(|meta| meta.len() > CAPTURE_LIMIT)
                .unwrap_or(true)
        });
        if oversized || started.elapsed() > timeout || state.is_err() {
            #[cfg(unix)]
            // SAFETY: spawn created this child's new process group; never target ours.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(
                "A verified child exceeded its bounds or could not complete; no clean result was recorded.",
            );
        }
        if let Some(status) = state.map_err(|_| "A verified child could not complete.")? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    #[cfg(unix)]
    // Reclaim descendants even if their launcher exited without waiting for them.
    // SAFETY: only this child's independently created process group is addressed.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    Ok(Captured {
        status,
        stdout: read_limited(stdout.path(), CAPTURE_LIMIT)?,
        stderr: read_limited(stderr.path(), CAPTURE_LIMIT)?,
    })
}

/// Provenance is optional only for exclusion: any error keeps the match reportable.
pub fn reviewed_merge_origin(
    finding: &Finding,
    git_directory: &Path,
    git: &Path,
    temporary: &Path,
) -> Option<Identity> {
    if !finding.may_have_reviewed_origin() {
        return None;
    }
    let parents = bounded(
        Command::new(git).arg("-C").arg(git_directory).args([
            "rev-list",
            "--parents",
            "-n",
            "1",
            &finding.commit,
        ]),
        Duration::from_secs(15),
        temporary,
    )
    .ok()?;
    if !parents.status.success()
        || std::str::from_utf8(&parents.stdout)
            .ok()?
            .split_whitespace()
            .count()
            < 3
    {
        return None;
    }
    let line = finding.line?;
    let origin = bounded(
        Command::new(git).arg("-C").arg(git_directory).args([
            "blame",
            "--line-porcelain",
            "-L",
            &format!("{line},{line}"),
            &finding.commit,
            "--",
            &finding.file,
        ]),
        Duration::from_secs(15),
        temporary,
    )
    .ok()?;
    if !origin.status.success() {
        return None;
    }
    let text = std::str::from_utf8(&origin.stdout).ok()?;
    let header = text.lines().next()?.split_whitespace().collect::<Vec<_>>();
    let commit = *header.first()?;
    if !valid_sha(commit, false) {
        return None;
    }
    Some(Identity {
        rule: finding.rule.clone(),
        file: finding.file.clone(),
        line: header.get(1)?.parse().ok()?,
        commit: commit.into(),
    })
}

pub(super) fn find_git() -> Result<PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join("git"))
        .find(|path| path.is_file())
        .and_then(|path| path.canonicalize().ok())
        .ok_or("The Git executable is unavailable.")
}

fn environment(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| "Required workflow metadata is unavailable.")
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| "A required private input is unavailable.")?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "A required private input could not be read.")?;
    if bytes.len() as u64 > limit {
        return Err("A private input exceeds its size bound.");
    }
    Ok(bytes)
}

fn download_archive() -> Result<Vec<u8>> {
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|_| "Could not initialize the HTTPS scanner download.")?;
    let mut archive = Vec::new();
    client
        .get(DOWNLOAD_URL)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|_| "The pinned scanner download failed.")?
        .take(ARCHIVE_LIMIT + 1)
        .read_to_end(&mut archive)
        .map_err(|_| "The pinned scanner download could not complete.")?;
    if archive.len() as u64 > ARCHIVE_LIMIT {
        return Err("The pinned scanner archive exceeds its size bound.");
    }
    verify_archive(&archive)?;
    Ok(archive)
}

pub(super) fn verify_archive(archive: &[u8]) -> Result<()> {
    let digest = Sha256::digest(archive)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if digest != ARCHIVE_SHA256 {
        return Err("The Gitleaks download checksum does not match the pinned release.");
    }
    Ok(())
}

pub(super) fn extract_binary(archive: &[u8], binary: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    let mut executable = None;
    for entry in archive
        .entries()
        .map_err(|_| "The verified scanner archive is invalid.")?
    {
        let mut entry = entry.map_err(|_| "The verified scanner archive entry is invalid.")?;
        if entry
            .path()
            .map_err(|_| "The verified scanner archive path is invalid.")?
            .as_ref()
            != Path::new("gitleaks")
        {
            continue;
        }
        if executable.is_some()
            || !entry.header().entry_type().is_file()
            || entry.size() > ARCHIVE_LIMIT
        {
            return Err("The verified archive does not contain one regular expected executable.");
        }
        let mut bytes = Vec::new();
        entry
            .by_ref()
            .take(ARCHIVE_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "The verified scanner executable could not be read.")?;
        if bytes.len() as u64 > ARCHIVE_LIMIT {
            return Err("The verified scanner executable exceeds its size bound.");
        }
        executable = Some(bytes);
    }
    let bytes =
        executable.ok_or("The verified archive does not contain the expected executable.")?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(binary)
        .map_err(|_| "The private scanner executable could not be created.")?;
    file.write_all(&bytes)
        .map_err(|_| "The private scanner executable could not be written.")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(binary, fs::Permissions::from_mode(0o700))
            .map_err(|_| "The private scanner executable permissions could not be set.")?;
    }
    Ok(())
}

pub fn git_proxy() -> Result<u8> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let history = arguments.get(2).is_some_and(|argument| argument == "log");
    let status = Command::new(environment("GITLEAKS_REAL_GIT")?)
        .args(&arguments)
        .status()
        .map_err(|_| "The verified Git child could not complete.")?;
    if history {
        fs::write(
            environment("GITLEAKS_GIT_STATUS")?,
            status.code().unwrap_or(1).to_string(),
        )
        .map_err(|_| "The Git-history status could not be recorded.")?;
        if !status.success() {
            return Err("The Git-history child did not complete successfully.");
        }
    }
    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1))
}

pub(super) fn parse_scanner_result(
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    git_status: &[u8],
    report: &[u8],
) -> Result<Vec<Finding>> {
    if !matches!(code, Some(0 | 2)) {
        return Err("Gitleaks failed; no clean result was recorded.");
    }
    if !stdout.trim_ascii().is_empty() || !stderr.trim_ascii().is_empty() {
        return Err("Gitleaks reported scanner errors; no clean result was recorded.");
    }
    if git_status != b"0" {
        return Err(
            "The Git-history child did not confirm successful completion; no clean result was recorded.",
        );
    }
    let findings: Vec<Finding> =
        serde_json::from_slice(report).map_err(|_| "The scanner report format is invalid.")?;
    if (code == Some(2)) == findings.is_empty() {
        return Err("The scanner result and report disagree.");
    }
    Ok(findings)
}

pub fn scan() -> Result<u8> {
    if std::env::var("RUNNER_DEBUG").as_deref() == Ok("1") {
        return Err(
            "Disable Actions step debug logging before scanning; checkout debug output can disclose commit contents.",
        );
    }
    let workspace = PathBuf::from(environment("GITHUB_WORKSPACE")?)
        .canonicalize()
        .map_err(|_| "The complete Git checkout is unavailable.")?;
    let git_directory = workspace.join(".git");
    if !git_directory.is_dir() {
        return Err("The complete Git checkout is unavailable.");
    }
    let event_name = environment("GITHUB_EVENT_NAME")?;
    if !["push", "pull_request", "schedule", "workflow_dispatch"].contains(&event_name.as_str()) {
        return Err("Unsupported workflow event.");
    }
    println!("[INFO] Starting the repository secret scan with checksum-pinned Gitleaks {VERSION}.");
    let temporary = tempfile::Builder::new()
        .prefix("gitleaks-")
        .tempdir_in(environment("RUNNER_TEMP")?)
        .map_err(|_| "Private scanner storage could not be created.")?;
    let git = find_git()?;
    let binary = temporary.path().join("gitleaks");
    extract_binary(&download_archive()?, &binary)?;
    let version = bounded(
        Command::new(&binary).arg("version"),
        Duration::from_secs(15),
        temporary.path(),
    )?;
    if !version.status.success() || version.stdout.trim_ascii() != VERSION.as_bytes() {
        return Err("The Gitleaks executable version could not be verified.");
    }
    println!("[INFO] Verified the scanner version and download checksum.");
    let config = temporary.path().join("scanner.toml");
    fs::write(&config, "[extend]\nuseDefault = true\n")
        .map_err(|_| "The pinned scanner configuration could not be written.")?;
    let mut log_options = LOG_OPTIONS.to_owned();
    let scope = if event_name == "pull_request" {
        let event: serde_json::Value = serde_json::from_slice(&read_limited(
            Path::new(&environment("GITHUB_EVENT_PATH")?),
            REPORT_LIMIT,
        )?)
        .map_err(|_| "The pull request event metadata is invalid.")?;
        let base = event
            .pointer("/pull_request/base/sha")
            .and_then(serde_json::Value::as_str)
            .ok_or("The pull request revision is invalid.")?;
        let head = event
            .pointer("/pull_request/head/sha")
            .and_then(serde_json::Value::as_str)
            .ok_or("The pull request revision is invalid.")?;
        for revision in [base, head] {
            if !valid_sha(revision, true) {
                return Err("The pull request revision is invalid.");
            }
            let check = bounded(
                Command::new(&git).arg("-C").arg(&git_directory).args([
                    "cat-file",
                    "-e",
                    &format!("{revision}^{{commit}}"),
                ]),
                Duration::from_secs(15),
                temporary.path(),
            )?;
            if !check.status.success() {
                return Err("A pull request revision is missing from the full checkout.");
            }
        }
        log_options.push_str(&format!(" {base}..{head}"));
        PR_SCOPE
    } else {
        log_options.push_str(" --all");
        FULL_SCOPE
    };
    // This same Rust executable becomes the Git observer, without generated scripts.
    fs::copy(
        std::env::current_exe().map_err(|_| "The Git observer executable is unavailable.")?,
        temporary.path().join("git"),
    )
    .map_err(|_| "The private Git observer could not be created.")?;
    let path = std::env::join_paths(std::iter::once(temporary.path().to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .map_err(|_| "The private scanner path is invalid.")?;
    let report = temporary.path().join("findings.json");
    let status_path = temporary.path().join("git-status");
    let result = bounded(
        Command::new(&binary)
            .arg("git")
            .arg(&git_directory)
            .arg("--config")
            .arg(&config)
            .arg("--gitleaks-ignore-path")
            .arg(temporary.path())
            .args([
                "--ignore-gitleaks-allow",
                "--redact=100",
                "--no-banner",
                "--no-color",
                "--log-level=error",
                "--exit-code=2",
                "--report-format=json",
                "--report-path",
            ])
            .arg(&report)
            .arg(format!("--log-opts={log_options}"))
            .env("PATH", path)
            .env("GITLEAKS_REAL_GIT", &git)
            .env("GITLEAKS_GIT_STATUS", &status_path),
        Duration::from_secs(930),
        temporary.path(),
    )?;
    let findings = parse_scanner_result(
        result.status.code(),
        &result.stdout,
        &result.stderr,
        &read_limited(&status_path, 32)?,
        &read_limited(&report, REPORT_LIMIT)?,
    )?;
    let (remaining, excluded) = partition_findings(findings, |finding| {
        reviewed_merge_origin(finding, &git_directory, &git, temporary.path())
    });
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| file.write_all(summary(&remaining, scope, excluded).as_bytes()))
            .map_err(|_| "The metadata-only scan summary could not be written.")?;
    }
    if remaining.is_empty() {
        println!("[INFO] The scan completed successfully; no unreviewed secret matches remain.");
        Ok(0)
    } else {
        println!(
            "[WARNING] Detected {} potential secrets. Review the redacted locations in the job summary.",
            remaining.len()
        );
        Ok(1)
    }
}
