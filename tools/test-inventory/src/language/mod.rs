//! Repository language boundaries; this is distinct from original-case wiring/execution.
use crate::ast::digest;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

mod artifacts;
mod rust_source;

pub(crate) use artifacts::linked;
use artifacts::{
    browser, ci_program, config, digest_receipt, forbidden_suffix, json_has_program,
    legacy_metadata, license, manifest_binding, markdown, media, package_scripts, physical_file,
    private, safe, script_line, script_text, worker,
};
use rust_source::rust_violations;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vendor {
    pub root: String,
    pub upstream: String,
    pub revision: String,
    pub provenance: String,
    pub notice: String,
    pub files: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub schema: u32,
    pub vendor: Vendor,
    pub frozen_data: BTreeMap<String, String>,
}
#[derive(Clone, Debug)]
pub struct Candidate {
    pub path: String,
    pub executable: bool,
    pub git_link: bool,
}
#[derive(Debug, Serialize)]
pub struct Finding {
    pub path: String,
    pub rule: &'static str,
    pub message: String,
}
#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub schema: u32,
    pub checked_files: usize,
    pub categories: BTreeMap<String, usize>,
    pub findings: Vec<Finding>,
    pub proves_behavioral_retirement: bool,
}
impl Report {
    fn reject(&mut self, path: &str, rule: &'static str, message: impl Into<String>) {
        self.findings.push(Finding {
            path: path.into(),
            rule,
            message: message.into(),
        });
    }
    fn accept(&mut self, category: &str) {
        *self.categories.entry(category.into()).or_default() += 1;
    }
}

pub fn git_candidates(root: &Path) -> Result<Vec<Candidate>, String> {
    fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
        let output = std::process::Command::new("git")
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .map_err(|_| "cannot invoke Git to enumerate candidate files")?;
        if !output.status.success() {
            return Err(
                "language policy requires a Git worktree; Git candidate enumeration failed".into(),
            );
        }
        Ok(output.stdout)
    }
    let mut paths = BTreeMap::new();
    for row in git(root, &["ls-files", "--cached", "--stage", "-z"])?
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
    {
        let text = std::str::from_utf8(row).map_err(|_| "candidate filename is not UTF-8")?;
        let (meta, path) = text.split_once('\t').ok_or("invalid Git index record")?;
        if root.join(path).symlink_metadata().is_ok() {
            paths.insert(
                path.to_owned(),
                (meta.starts_with("100755 "), meta.starts_with("120000 ")),
            );
        }
    }
    for row in git(root, &["ls-files", "--others", "--exclude-standard", "-z"])?
        .split(|b| *b == 0)
        .filter(|r| !r.is_empty())
    {
        let path = std::str::from_utf8(row).map_err(|_| "candidate filename is not UTF-8")?;
        paths.entry(path.into()).or_insert((false, false));
    }
    Ok(paths
        .into_iter()
        .map(|(path, (executable, git_link))| Candidate {
            path,
            executable,
            git_link,
        })
        .collect())
}
fn vendor_receipts(root: &Path, policy: &Policy, paths: &BTreeSet<&str>, report: &mut Report) {
    let vendor = &policy.vendor;
    if policy.schema != 1
        || vendor.root != "runtime/vendor/rlottie"
        || vendor.upstream != "https://github.com/laggykiller/rlottie"
        || vendor.revision != "47b71a7e9072ac67b2e18c408d3668fed278fb93"
    {
        report.reject(
            "tools/test-inventory/language-policy.json",
            "policy",
            "unknown policy schema or unreviewed native vendor identity",
        );
    }
    for path in vendor.files.keys() {
        if !safe(path)
            || !path.starts_with(&(vendor.root.clone() + "/"))
            || !paths.contains(path.as_str())
        {
            report.reject(
                path,
                "vendor-receipt",
                "pinned third-party member is absent or outside its declared subset",
            );
        }
    }
    for required in [
        "COPYING",
        "NUMERA-SOURCE.md",
        "licenses/COPYING.MIT",
        "licenses/COPYING.MPL",
        "licenses/COPYING.SKIA",
        "licenses/COPYING.FTL",
        "licenses/COPYING.PIX",
        "licenses/COPYING.RPD",
        "licenses/COPYING.STB",
    ] {
        let path = format!("{}/{required}", vendor.root);
        if !vendor.files.contains_key(&path) {
            report.reject(
                &path,
                "vendor-license",
                "third-party license/provenance is not pinned",
            );
        }
    }
    for (path, needles) in [
        (
            &vendor.provenance,
            vec![vendor.upstream.as_str(), vendor.revision.as_str()],
        ),
        (&vendor.notice, vec!["rlottie", vendor.revision.as_str()]),
    ] {
        match physical_file(root,path).and_then(|p|std::fs::read_to_string(p).map_err(|_|"cannot read third-party notice".into())) {
            Ok(text) if needles.iter().all(|needle|text.contains(needle))=>{},
            _=>report.reject(path,"vendor-notice","native dependency requires its real upstream provenance and redistribution notices"),
        }
    }
}

pub fn check(root: &Path, policy: &Policy, candidates: &[Candidate]) -> Report {
    let mut report = Report {
        schema: 1,
        ..Report::default()
    };
    let mut seen = BTreeSet::new();
    let paths = candidates
        .iter()
        .map(|c| c.path.as_str())
        .collect::<BTreeSet<_>>();
    vendor_receipts(root, policy, &paths, &mut report);
    for candidate in candidates {
        let path = &candidate.path;
        if !seen.insert(path) {
            report.reject(path, "candidate", "duplicate candidate path");
            continue;
        }
        report.checked_files += 1;
        if private(path) {
            report.reject(
                path,
                "private-candidate",
                "ignored/private/build material must not be published; content was not read",
            );
            continue;
        }
        if candidate.git_link {
            report.reject(path,"physical-file","Git symlink entries are not regular source/data, including on checkouts that materialize their text");
            continue;
        }
        let physical = match physical_file(root, path) {
            Ok(p) => p,
            Err(e) => {
                report.reject(path, "physical-file", e);
                continue;
            }
        };
        let bytes = match std::fs::read(&physical) {
            Ok(v) => v,
            Err(_) => {
                report.reject(path, "unreadable", "cannot read candidate source/data");
                continue;
            }
        };
        if path.starts_with(&(policy.vendor.root.clone() + "/")) {
            if policy
                .vendor
                .files
                .get(path)
                .is_some_and(|expected| digest(&bytes) == *expected)
            {
                report.accept("pinned-third-party-native");
            } else {
                report.reject(path,"vendor-receipt","unlisted or modified vendor member; a directory is not a first-party code exemption");
            }
            continue;
        }
        if forbidden_suffix(path) || legacy_metadata(path) {
            report.reject(path,"first-party-language","retired interpreter/native application source or launcher metadata is not permitted");
            continue;
        }
        let text = std::str::from_utf8(&bytes).ok();
        if text.is_some_and(|s| {
            s.lines()
                .next()
                .is_some_and(|l| l.starts_with("#!") && !l.starts_with("#![") && script_line(l))
        }) {
            report.reject(
                path,
                "interpreter-shebang",
                "retired interpreter shebang is not data or Rust source",
            );
            continue;
        }
        if path.ends_with(".rs") || path.ends_with(".rs.in") {
            match text
                .ok_or("Rust source is not UTF-8".into())
                .and_then(rust_violations)
            {
                Ok(violations) if violations.is_empty() => report.accept("first-party-rust"),
                Ok(violations) => {
                    for issue in violations {
                        report.reject(path, "rust-interpreter-handoff", issue);
                    }
                }
                Err(error) => report.reject(path, "rust-source", error),
            }
            continue;
        }
        if candidate.executable {
            report.reject(
                path,
                "executable-artifact",
                "first-party non-Rust source/data must not be a committed executable launcher",
            );
            continue;
        }
        if browser(path) || worker(path) {
            if let Some(text) = text.filter(|s| !script_text(s, false)) {
                if !text.is_empty() {
                    report.accept(if browser(path) {
                        "browser-source"
                    } else {
                        "worker-source"
                    });
                } else {
                    report.reject(path, "empty-source", "approved-language source is empty");
                }
            } else {
                report.reject(
                    path,
                    "disguised-source",
                    "approved browser/Worker path contains another language's script",
                );
            }
            continue;
        }
        if markdown(path) || license(path) {
            if text.is_some_and(|s| !script_text(s, true)) {
                report.accept("documentation-or-license-template");
            } else {
                report.reject(path,"disguised-document","document contains an unfenced executable script rather than declarative prose/examples");
            }
            continue;
        }
        if let Some(expected) = policy.frozen_data.get(path) {
            if digest(&bytes) == *expected {
                report.accept("frozen-data-fixture");
            } else {
                report.reject(
                    path,
                    "frozen-data",
                    "approved data exception changed; review its independent fixture provenance",
                );
            }
            continue;
        }
        if path.ends_with(".json") {
            match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(value) if !json_has_program(&value)=>match package_scripts(path,&value) {
                    Ok(())=>report.accept("declarative-json"),Err(e)=>report.reject(path,"json-launcher",e)
                },
                Ok(_)=>report.reject(path,"json-program","new JSON contains executable source; only exact reviewed frozen parser fixtures may retain source text"),
                Err(_)=>report.reject(path,"json-data","JSON is malformed and has no reviewed frozen-data receipt"),
            }
            continue;
        }
        if config(path) {
            if let Some(text) = text {
                let bad = if path.starts_with(".github/") {
                    ci_program(text)
                } else {
                    script_text(text, false)
                        || path.ends_with("Cargo.toml") && manifest_binding(text)
                };
                if !bad {
                    report.accept("declarative-config");
                } else {
                    report.reject(path,"configuration-launcher","declarative configuration contains a retired interpreter/binding or embedded project script");
                }
            } else {
                report.reject(path, "configuration-data", "configuration is not UTF-8");
            }
            continue;
        }
        if path.ends_with(".csv") && text.is_some_and(|s| !script_text(s, false)) {
            report.accept("csv-data");
            continue;
        }
        if path.ends_with(".sha256") && text.is_some_and(digest_receipt) {
            report.accept("source-digest-receipt");
            continue;
        }
        if media(path, &bytes) {
            report.accept("media-data");
            continue;
        }
        report.reject(path,"unclassified-artifact","unrecognized first-party source/data type; do not rename executable source into a data extension");
    }
    report
}

pub fn default_policy() -> Result<Policy, String> {
    serde_json::from_str(include_str!("../../language-policy.json")).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod index_controls {
    use super::*;
    #[test]
    fn git_link_and_executable_modes_cannot_disappear_on_a_regular_file_checkout() {
        let tree = super::tests::Tree::new();
        let root = &tree.root;
        assert!(tree.run().findings.is_empty());
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        };
        git(&["init", "--quiet"]);
        std::fs::write(root.join("README.md"), b"outside target text").unwrap();
        let hash = git(&["hash-object", "-w", "--no-filters", "README.md"]);
        git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("120000,{},README.md", hash.trim()),
        ]);
        let rows = git_candidates(root).unwrap();
        assert!(rows.iter().any(|c| c.path == "README.md" && c.git_link));
        let policy = &tree.policy;
        assert!(
            check(root, policy, &rows)
                .findings
                .iter()
                .any(|f| f.path == "README.md" && f.rule == "physical-file")
        );
        std::fs::write(root.join("payload.json"), b"{}").unwrap();
        let hash = git(&["hash-object", "-w", "--no-filters", "payload.json"]);
        git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100755,{},payload.json", hash.trim()),
        ]);
        let rows = git_candidates(root).unwrap();
        assert!(
            rows.iter()
                .any(|c| c.path == "payload.json" && c.executable)
        );
        assert!(
            check(root, policy, &rows)
                .findings
                .iter()
                .any(|f| f.path == "payload.json" && f.rule == "executable-artifact")
        );
    }
}
