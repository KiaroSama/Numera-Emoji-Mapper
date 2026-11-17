use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub const VERSION: &str = "8.30.1";
pub const ARCHIVE_SHA256: &str = "551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb";
const DOWNLOAD_URL: &str = "https://github.com/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_linux_x64.tar.gz";
const CAPTURE_LIMIT: u64 = 8 * 1024 * 1024;
const ARCHIVE_LIMIT: u64 = 64 * 1024 * 1024;
const REPORT_LIMIT: u64 = 32 * 1024 * 1024;
const LOG_OPTIONS: &str =
    "--full-history --diff-filter=uxdb --no-ext-diff --no-textconv --ignore-submodules=all -m";
const FULL_SCOPE: &str = "All fetched branches and tags, including full history and merge diffs";
const PR_SCOPE: &str = "Commits introduced by this pull request, including merge diffs";
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub rule: String,
    pub file: String,
    pub line: u64,
    pub commit: String,
}

const REVIEWED: &[(&str, &str, u64, &str)] = &[
    (
        "generic-api-key",
        "native/src/source_logging_oracle.rs",
        227,
        "fd9ce1dbc42e35a14879b9c3c2b56fa5b70d933d",
    ),
    (
        "generic-api-key",
        "native/tests/logging_parity.rs",
        26,
        "fd9ce1dbc42e35a14879b9c3c2b56fa5b70d933d",
    ),
    (
        "generic-api-key",
        "tests/fixtures/native/provider-image.json",
        5,
        "7cd8ebecb282361891f9d6aae8303da771c50b80",
    ),
    (
        "generic-api-key",
        "tests/fixtures/native/provider-policy.json",
        4,
        "e3cdd76b5830c1b30cfbc790b5d7e133b319114f",
    ),
];

/// Discard secret-bearing fields at deserialization. Invalid metadata fails closed.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Finding {
    #[serde(rename = "RuleID", default)]
    pub rule: String,
    #[serde(rename = "File", default)]
    pub file: String,
    #[serde(rename = "StartLine", default)]
    pub line: Option<u64>,
    #[serde(rename = "EndLine", default)]
    pub end_line: Option<u64>,
    #[serde(rename = "Commit", default)]
    pub commit: String,
}

impl Finding {
    fn identity(&self) -> Identity {
        Identity {
            rule: self.rule.clone(),
            file: self.file.clone(),
            line: self.line.unwrap_or(0),
            commit: self.commit.clone(),
        }
    }

    fn may_have_reviewed_origin(&self) -> bool {
        self.rule == "generic-api-key"
            && REVIEWED.iter().any(|(_, file, _, _)| *file == self.file)
            && self.line.is_some_and(|line| line > 0)
            && self.line == self.end_line
            && valid_sha(&self.commit, false)
    }
}

fn reviewed(identity: &Identity) -> bool {
    REVIEWED.iter().any(|(rule, file, line, commit)| {
        identity.rule == *rule
            && identity.file == *file
            && identity.line == *line
            && identity.commit == *commit
    })
}

fn valid_sha(value: &str, uppercase: bool) -> bool {
    value.len() == 40
        && value.bytes().all(|byte| {
            byte.is_ascii_digit()
                || (b'a'..=b'f').contains(&byte)
                || (uppercase && (b'A'..=b'F').contains(&byte))
        })
}

pub fn partition_findings(
    findings: Vec<Finding>,
    origin: impl FnMut(&Finding) -> Option<Identity>,
) -> (Vec<Finding>, usize) {
    partition_by(findings, origin, reviewed)
}

fn partition_by(
    findings: Vec<Finding>,
    mut origin: impl FnMut(&Finding) -> Option<Identity>,
    approved: impl Fn(&Identity) -> bool,
) -> (Vec<Finding>, usize) {
    let mut remaining = Vec::new();
    let mut excluded = 0;
    for finding in findings {
        if approved(&finding.identity())
            || (finding.may_have_reviewed_origin()
                && origin(&finding).as_ref().is_some_and(&approved))
        {
            excluded += 1;
        } else {
            remaining.push(finding);
        }
    }
    (remaining, excluded)
}

pub fn safe_cell(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        escaped.push_str(match character {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            '\'' => "&#x27;",
            '|' => "&#124;",
            '\n' | '\r' => " ",
            _ => {
                escaped.push(character);
                continue;
            }
        });
    }
    escaped
}

pub fn summary(findings: &[Finding], scope: &str, excluded: usize) -> String {
    let mut output = format!(
        "## Gitleaks\n\nScanner: **{VERSION}**\n\nScope: **{scope}**\n\nRules: Pinned built-in Gitleaks rules with standard exclusions. Full history describes revision coverage, not universal byte or credential coverage.\n\n"
    );
    if excluded > 0 {
        output.push_str(&format!("Excluded **{excluded}** reviewed historical non-secret matches (source SHA-256 digests and synthetic test identifiers), pinned to exact introduction commit/file/line/rule identities, including Git-proven unchanged merge repetitions. New matches remain reportable.\n\n"));
    }
    if findings.is_empty() {
        output.push_str("No unreviewed secret matches remain in the scanned Git history.\n");
        return output;
    }
    output.push_str(&format!("**{} potential secrets detected.** Review the locations below; rotate any real exposed credentials.\n\nSecret values, source snippets, and commit messages are never included in this summary.\n\n| Rule | File | Line | Commit |\n| --- | --- | --- | --- |\n",findings.len()));
    for finding in findings.iter().take(200) {
        let line = finding
            .line
            .map(|line| line.to_string())
            .unwrap_or_default();
        let cells = [&finding.rule, &finding.file, &line, &finding.commit]
            .map(|value| format!("<code>{}</code>", safe_cell(value)));
        output.push_str(&format!("| {} |\n", cells.join(" | ")));
    }
    if findings.len() > 200 {
        output.push_str(&format!("\nShowing 200 of {} findings. Repeat the same version locally for the complete report.\n",findings.len()));
    }
    output
}

struct Captured {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn bounded(command: &mut Command, timeout: Duration, temporary: &Path) -> Result<Captured> {
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

fn find_git() -> Result<PathBuf> {
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

fn verify_archive(archive: &[u8]) -> Result<()> {
    let digest = Sha256::digest(archive)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if digest != ARCHIVE_SHA256 {
        return Err("The Gitleaks download checksum does not match the pinned release.");
    }
    Ok(())
}

fn extract_binary(archive: &[u8], binary: &Path) -> Result<()> {
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

fn parse_scanner_result(
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn finding(identity: &(&str, &str, u64, &str)) -> Finding {
        Finding {
            rule: identity.0.into(),
            file: identity.1.into(),
            line: Some(identity.2),
            end_line: Some(identity.2),
            commit: identity.3.into(),
        }
    }

    #[test]
    fn exact_reviewed_identities_only() {
        let all = REVIEWED.iter().map(finding).collect::<Vec<_>>();
        assert_eq!(partition_findings(all.clone(), |_| None), (vec![], 4));
        for original in &all {
            for field in 0..4 {
                let mut changed = original.clone();
                match field {
                    0 => changed.rule.push_str("-unknown"),
                    1 => changed.file.push_str(".other"),
                    2 => changed.line = Some(changed.line.unwrap() + 1),
                    _ => changed.commit = "f".repeat(40),
                }
                let mut input = all.clone();
                input.push(changed.clone());
                assert_eq!(partition_findings(input, |_| None), (vec![changed], 4));
            }
        }
        assert_eq!(partition_findings(vec![], |_| None), (vec![], 0));
    }

    #[test]
    fn malformed_or_incomplete_metadata_cannot_claim_reviewed_identity() {
        let original = REVIEWED[0];
        let spoofed: Finding = serde_json::from_value(json!({"RuleID":original.0,
            "File":original.1,"StartLine":original.2,"Commit":"f".repeat(40),
            "Fingerprint":format!("{}:{}:generic-api-key:{}",original.3,original.1,original.2)}))
        .unwrap();
        assert_eq!(
            partition_findings(vec![spoofed.clone()], |_| None),
            (vec![spoofed], 0)
        );
        for line in [json!("227"), json!(true), json!(-1), json!(227.0)] {
            let spoofed =
                json!({"RuleID":original.0,"File":original.1,"StartLine":line,"Commit":original.3});
            assert!(serde_json::from_value::<Finding>(spoofed).is_err());
        }
        for missing in ["RuleID", "File", "StartLine", "Commit"] {
            let mut value = json!({"RuleID":original.0,"File":original.1,"StartLine":original.2,"Commit":original.3});
            value.as_object_mut().unwrap().remove(missing);
            let incomplete: Finding = serde_json::from_value(value).unwrap();
            assert_eq!(
                partition_findings(vec![incomplete.clone()], |_| None),
                (vec![incomplete], 0)
            );
        }
    }

    #[test]
    fn merge_origin_requires_valid_single_line_metadata_and_exact_reviewed_origin() {
        let original = finding(&REVIEWED[0]);
        let mut repeated = original.clone();
        repeated.commit = "a".repeat(40);
        assert_eq!(
            partition_findings(vec![repeated.clone()], |_| Some(original.identity())),
            (vec![], 1)
        );
        assert_eq!(
            partition_findings(vec![repeated.clone()], |_| None),
            (vec![repeated.clone()], 0)
        );
        let mut wrong = original.identity();
        wrong.line += 1;
        assert_eq!(
            partition_findings(vec![repeated.clone()], |_| Some(wrong.clone())),
            (vec![repeated.clone()], 0)
        );
        for invalid in 0..7 {
            let mut changed = repeated.clone();
            match invalid {
                0 => changed.end_line = Some(changed.line.unwrap() + 1),
                1 => {
                    changed.line = Some(0);
                    changed.end_line = Some(0);
                }
                2 => changed.commit = "A".repeat(40),
                3 => changed.commit = "a".repeat(39),
                4 => changed.commit = "g".repeat(40),
                5 => changed.file.push_str(".other"),
                _ => changed.rule.push_str(".other"),
            }
            let mut called = false;
            assert_eq!(
                partition_findings(vec![changed.clone()], |_| {
                    called = true;
                    Some(original.identity())
                }),
                (vec![changed], 0)
            );
            assert!(
                !called,
                "invalid metadata must never ask Git for an exclusion"
            );
        }
    }

    #[test]
    fn summary_escapes_metadata_and_discards_secret_bearing_fields() {
        let value = json!({"RuleID":"rule|<script>&\"'\r\n", "File":"path|<img>","StartLine":7,
            "Commit":"commit|<svg>","Secret":"SYNTHETIC-DO-NOT-DISCLOSE", "Match":"SYNTHETIC-SNIPPET",
            "Message":"SYNTHETIC-COMMIT-MESSAGE", "Author":"SYNTHETIC-AUTHOR"});
        let parsed: Finding = serde_json::from_value(value).unwrap();
        let output = summary(&[parsed], PR_SCOPE, 2);
        assert!(output.contains("rule&#124;&lt;script&gt;&amp;&quot;&#x27;  "));
        assert!(output.contains("path&#124;&lt;img&gt;"));
        assert!(output.contains("Excluded **2**"));
        assert!(!output.contains("SYNTHETIC"));
        assert!(!output.contains("<script>"));
        assert_eq!(safe_cell("plain"), "plain");
        let long = vec![finding(&REVIEWED[0]); 201];
        let long_summary = summary(&long, FULL_SCOPE, 0);
        assert_eq!(
            long_summary.matches("<code>generic-api-key</code>").count(),
            200
        );
        assert!(long_summary.contains("Showing 200 of 201 findings"));
        assert!(summary(&[], FULL_SCOPE, 0).contains("No unreviewed secret matches remain"));
    }

    #[test]
    fn scanner_status_output_git_status_and_report_must_agree() {
        assert!(
            parse_scanner_result(Some(0), b" \n", b"", b"0", b"[]")
                .unwrap()
                .is_empty()
        );
        let report = br#"[{"RuleID":"unknown","Secret":"SYNTHETIC-PRIVATE"}]"#;
        assert_eq!(
            parse_scanner_result(Some(2), b"", b"", b"0", report)
                .unwrap()
                .len(),
            1
        );
        for (code, stdout, stderr, git_status, report) in [
            (
                Some(1),
                b"".as_slice(),
                b"".as_slice(),
                b"0".as_slice(),
                b"[]".as_slice(),
            ),
            (None, b"", b"", b"0", b"[]"),
            (Some(0), b"SYNTHETIC-PRIVATE", b"", b"0", b"[]"),
            (Some(0), b"", b"SYNTHETIC-PRIVATE", b"0", b"[]"),
            (Some(0), b"", b"", b"1", b"[]"),
            (Some(0), b"", b"", b"", b"[]"),
            (Some(0), b"", b"", b"0", b"null"),
            (Some(0), b"", b"", b"0", b"[1]"),
            (Some(2), b"", b"", b"0", b"[]"),
            (Some(0), b"", b"", b"0", report),
        ] {
            let error = parse_scanner_result(code, stdout, stderr, git_status, report).unwrap_err();
            assert!(!error.contains("SYNTHETIC"));
        }
    }

    fn archive(entries: &[(&str, tar::EntryType)]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (name, kind) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(*kind);
            header.set_mode(0o700);
            header.set_size(4);
            if kind.is_symlink() {
                header.set_link_name("/outside").unwrap();
            }
            header.set_cksum();
            builder
                .append_data(&mut header, name, b"test".as_slice())
                .unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn archive_extraction_selects_only_one_regular_expected_member() {
        let temporary = tempfile::tempdir().unwrap();
        let binary = temporary.path().join("scanner");
        let bytes = archive(&[
            ("ignored", tar::EntryType::Regular),
            ("gitleaks", tar::EntryType::Regular),
        ]);
        extract_binary(&bytes, &binary).unwrap();
        assert_eq!(fs::read(&binary).unwrap(), b"test");
        assert!(!temporary.path().join("ignored").exists());
        assert!(
            extract_binary(&bytes, &binary).is_err(),
            "must not overwrite"
        );
        for entries in [
            vec![("other", tar::EntryType::Regular)],
            vec![("gitleaks", tar::EntryType::Symlink)],
            vec![
                ("gitleaks", tar::EntryType::Regular),
                ("gitleaks", tar::EntryType::Regular),
            ],
        ] {
            assert!(extract_binary(&archive(&entries), &temporary.path().join("refused")).is_err());
            assert!(!temporary.path().join("refused").exists());
        }
        assert!(verify_archive(b"untrusted-download").is_err());
        assert!(extract_binary(b"invalid gzip", &temporary.path().join("invalid")).is_err());
    }

    fn fixture_git(root: &Path, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(arguments)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Synthetic Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Synthetic Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "synthetic Git fixture command failed"
        );
        String::from_utf8(output.stdout).unwrap().trim().into()
    }

    #[test]
    fn real_git_proves_only_unchanged_merge_line_origins() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("repository");
        fs::create_dir(&root).unwrap();
        fixture_git(&root, &["init", "-b", "main"]);
        let file = REVIEWED[2].1;
        fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
        fs::write(
            root.join(file),
            "one\ntwo\nthree\nfour\nsynthetic reviewed content\n",
        )
        .unwrap();
        fixture_git(&root, &["add", "."]);
        fixture_git(&root, &["commit", "-m", "synthetic original"]);
        let original = fixture_git(&root, &["rev-parse", "HEAD"]);
        fixture_git(&root, &["checkout", "-b", "side"]);
        fs::write(root.join("side"), "synthetic side").unwrap();
        fixture_git(&root, &["add", "."]);
        fixture_git(&root, &["commit", "-m", "synthetic side"]);
        fixture_git(&root, &["checkout", "main"]);
        fs::write(root.join("main"), "synthetic main").unwrap();
        fixture_git(&root, &["add", "."]);
        fixture_git(&root, &["commit", "-m", "synthetic main"]);
        fixture_git(
            &root,
            &["merge", "--no-ff", "side", "-m", "synthetic merge"],
        );
        let merge = fixture_git(&root, &["rev-parse", "HEAD"]);
        let mut repeated = finding(&REVIEWED[2]);
        repeated.commit = merge;
        let git = find_git().unwrap();
        let origin =
            reviewed_merge_origin(&repeated, &root.join(".git"), &git, temporary.path()).unwrap();
        assert_eq!(
            origin,
            Identity {
                rule: repeated.rule.clone(),
                file: repeated.file.clone(),
                line: 5,
                commit: original.clone()
            }
        );
        let accepted = |identity: &Identity| {
            identity.rule == repeated.rule
                && identity.file == repeated.file
                && identity.line == 5
                && identity.commit == original
        };
        let probe = |finding: &Finding| {
            reviewed_merge_origin(finding, &root.join(".git"), &git, temporary.path())
        };
        assert_eq!(
            partition_by(vec![repeated.clone()], probe, accepted),
            (vec![], 1)
        );
        for invalid in 0..5 {
            let mut unknown = repeated.clone();
            match invalid {
                0 => unknown.commit = "f".repeat(40),
                1 => unknown.file = "side".into(),
                2 => {
                    unknown.line = Some(6);
                    unknown.end_line = Some(6);
                }
                3 => unknown.end_line = Some(6),
                _ => unknown.rule = "another-rule".into(),
            }
            assert_eq!(
                partition_by(vec![unknown.clone()], probe, accepted),
                (vec![unknown], 0)
            );
        }
        // Replayed new content under a new merge SHA must remain reportable.
        fixture_git(&root, &["checkout", "side"]);
        fs::write(
            root.join(file),
            "one\ntwo\nthree\nfour\nchanged synthetic content\nnew synthetic content\n",
        )
        .unwrap();
        fixture_git(&root, &["add", "."]);
        fixture_git(&root, &["commit", "-m", "changed synthetic content"]);
        let changed = fixture_git(&root, &["rev-parse", "HEAD"]);
        fixture_git(&root, &["checkout", "main"]);
        fs::write(root.join("main"), "main advanced").unwrap();
        fixture_git(&root, &["add", "."]);
        fixture_git(&root, &["commit", "-m", "main advanced"]);
        fixture_git(&root, &["checkout", "side"]);
        fixture_git(
            &root,
            &["merge", "--no-ff", "main", "-m", "changed content merge"],
        );
        let changed_merge = fixture_git(&root, &["rev-parse", "HEAD"]);
        for commit in [changed, changed_merge] {
            for line in [5, 6] {
                let mut unknown = repeated.clone();
                unknown.commit = commit.clone();
                unknown.line = Some(line);
                unknown.end_line = Some(line);
                assert_eq!(
                    partition_by(vec![unknown.clone()], probe, accepted),
                    (vec![unknown], 0)
                );
            }
        }
        repeated.commit = original;
        assert!(
            reviewed_merge_origin(&repeated, &root.join(".git"), &git, temporary.path()).is_none(),
            "ordinary commit is not a merge repetition"
        );
        repeated.commit = "f".repeat(40);
        assert!(
            reviewed_merge_origin(&repeated, &root.join(".git"), &git, temporary.path()).is_none(),
            "missing commit must not exclude"
        );
    }

    #[cfg(unix)]
    fn child_fixture(mode: &str, pid_file: &Path) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .env_clear()
            .env(
                "NUMERA_RUNTIME_TEST_ADMISSION",
                std::env::var_os("NUMERA_RUNTIME_TEST_ADMISSION").expect("actual test admission"),
            )
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("NUMERA_CI_BOUNDED_FIXTURE", mode)
            .env("NUMERA_CI_BOUNDED_PID", pid_file)
            .args([
                "--exact",
                "tests::bounded_child_fixture",
                "--nocapture",
                "--quiet",
            ]);
        command
    }

    // Technical self-exec entrypoint; never a production binary mode or an original behavior ID.
    #[cfg(unix)]
    #[test]
    fn bounded_child_fixture() {
        let Some(mode) = std::env::var_os("NUMERA_CI_BOUNDED_FIXTURE") else {
            return;
        };
        match mode.to_str().unwrap() {
            "output" => {
                std::io::stdout().write_all(b"synthetic").unwrap();
                std::io::stdout().flush().unwrap();
                std::io::stderr().write_all(b"private").unwrap();
                std::io::stderr().flush().unwrap();
                std::process::exit(0);
            }
            "tree" => {
                let pid_file = std::env::var_os("NUMERA_CI_BOUNDED_PID").unwrap();
                let mut leaf = child_fixture("leaf", Path::new(&pid_file))
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap();
                let _ = leaf.wait();
                std::process::exit(0);
            }
            "leaf" => {
                fs::write(
                    std::env::var_os("NUMERA_CI_BOUNDED_PID").unwrap(),
                    std::process::id().to_string(),
                )
                .unwrap();
                std::thread::sleep(Duration::from_secs(10));
                std::process::exit(0);
            }
            _ => panic!("unknown bounded fixture role"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn private_child_output_and_process_tree_are_bounded() {
        let temporary = tempfile::tempdir().unwrap();
        let pid_file = temporary.path().join("descendant.pid");
        let output = bounded(
            &mut child_fixture("output", &pid_file),
            Duration::from_secs(2),
            temporary.path(),
        )
        .unwrap();
        assert!(output.status.success());
        // The exact libtest prelude is incidental to the Rust technical entrypoint;
        // no additional payload bytes or private stderr may enter stdout.
        assert_eq!(output.stdout, b"\nrunning 1 test\nsynthetic");
        assert_eq!(output.stderr, b"private");
        let started = Instant::now();
        assert!(
            bounded(
                &mut child_fixture("tree", &pid_file),
                Duration::from_millis(200),
                temporary.path()
            )
            .is_err()
        );
        assert!(started.elapsed() < Duration::from_secs(3));
        let pid: i32 = fs::read_to_string(pid_file)
            .expect("real descendant started before timeout")
            .parse()
            .unwrap();
        #[cfg(target_os = "linux")]
        {
            // SAFETY: this only opens/observes the recorded fixture PID, never signals another owner.
            let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
            if fd < 0 {
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ESRCH)
                );
            } else {
                // SAFETY: fd is an owned pidfd; poll observes kernel process termination.
                let mut ready = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                let observed = unsafe { libc::poll(&mut ready, 1, 1000) };
                unsafe { libc::close(fd) };
                assert_eq!(
                    observed, 1,
                    "owned descendant survived the bounded process-group timeout"
                );
                assert_ne!(ready.revents & libc::POLLIN, 0);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../runtime/tests/support/test_admission.rs"]
mod test_admission;
