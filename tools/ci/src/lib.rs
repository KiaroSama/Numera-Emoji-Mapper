use serde::Deserialize;

mod scanner;
#[cfg(test)]
mod tests;

pub use scanner::{git_proxy, reviewed_merge_origin, scan};

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
    // Exact original redaction-preservation fixture, not an issued credential.
    (
        "generic-api-key",
        "runtime/tests/baseline_observability.rs",
        121,
        "5d9d4bd09ac2080a119fe960cc1d923342d383fe",
    ),
    // SHA-256 integrity digests verified against each referenced committed file.
    (
        "generic-api-key",
        "tools/test-inventory/language-policy.json",
        18,
        "5d9d4bd09ac2080a119fe960cc1d923342d383fe",
    ),
    (
        "generic-api-key",
        "tools/test-inventory/language-policy.json",
        32,
        "5d9d4bd09ac2080a119fe960cc1d923342d383fe",
    ),
    (
        "generic-api-key",
        "tools/test-inventory/language-policy.json",
        38,
        "5d9d4bd09ac2080a119fe960cc1d923342d383fe",
    ),
    (
        "generic-api-key",
        "tools/test-inventory/language-policy.json",
        39,
        "5d9d4bd09ac2080a119fe960cc1d923342d383fe",
    ),
    (
        "generic-api-key",
        "tools/test-inventory/language-policy.json",
        40,
        "5d9d4bd09ac2080a119fe960cc1d923342d383fe",
    ),
    (
        "generic-api-key",
        "tools/test-inventory/language-policy.json",
        238,
        "5d9d4bd09ac2080a119fe960cc1d923342d383fe",
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
        if (finding.line == finding.end_line && approved(&finding.identity()))
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
        output.push_str(&format!("Excluded **{excluded}** reviewed historical non-secret matches (source SHA-256 digests and synthetic test data), pinned to exact introduction commit/file/line/rule identities, including Git-proven unchanged merge repetitions. New matches remain reportable.\n\n"));
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

#[cfg(test)]
#[path = "../../../runtime/tests/support/test_admission.rs"]
mod test_admission;
