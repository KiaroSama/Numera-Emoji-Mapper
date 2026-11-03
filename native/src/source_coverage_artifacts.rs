//! Bind coverage rows to actual source/target/fixture bytes and an independently supplied run receipt.
use crate::source_git_inventory::Source;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

fn artifact_hash(root: &Path, name: &str) -> Result<String, String> {
    if name.contains(['\\', ':', '\0'])
        || name
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err("coverage artifact requires a confined relative path".into());
    }
    let mut path = root.to_path_buf();
    for part in name.split('/') {
        path.push(part);
        let metadata =
            std::fs::symlink_metadata(&path).map_err(|_| "coverage artifact is missing")?;
        if metadata.file_type().is_symlink() {
            return Err("coverage artifact must not follow links".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("coverage artifact must not follow reparse points".into());
            }
        }
    }
    let file = std::fs::File::open(path).map_err(|_| "cannot open coverage artifact")?;
    let metadata = file
        .metadata()
        .map_err(|_| "cannot inspect coverage artifact")?;
    if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
        return Err("coverage artifact must be a regular file under8MiB".into());
    }
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read coverage artifact")?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("coverage artifact grew beyond8MiB".into());
    }
    Ok(Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// The caller must authenticate the receipt's CI/run provenance and exact HEAD independently.
/// This seam validates identity/content; it does not turn caller-authored JSON into CI proof.
pub fn verify(
    root: &Path,
    sources: &[Source],
    coverage: &Value,
    receipt: &Value,
    target_revision: &str,
    release: bool,
) -> Result<Value, String> {
    if target_revision.len() != 40
        || !target_revision
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("coverage requires exact target revision".into());
    }
    let root = crate::tooling_environment::physical_root(root)?;
    let inventory = crate::source_git_inventory::inventory(sources)?;
    let report = crate::source_migration_coverage::validate(&inventory, coverage, release)?;
    if receipt["result"] != "verified"
        || receipt["target_revision"] != target_revision
        || receipt["baseline_revision"] != inventory["baseline_revision"]
    {
        return Err("coverage receipt does not verify exact source/target revisions".into());
    }
    let passed = receipt["passed_cases"]
        .as_array()
        .ok_or("coverage receipt requires observed passed cases")?;
    for entry in coverage["cases"]
        .as_array()
        .ok_or("coverage requires cases")?
    {
        if entry["status"] == "pending" {
            continue;
        }
        let target = entry["target_file"]
            .as_str()
            .ok_or("coverage requires target file")?;
        let fixture = entry["fixture"]
            .as_str()
            .ok_or("coverage requires fixture")?;
        if entry["target_hash"].as_str() != Some(artifact_hash(&root, target)?.as_str()) {
            return Err("coverage target artifact hash drift".into());
        }
        if entry["fixture_hash"].as_str() != Some(artifact_hash(&root, fixture)?.as_str()) {
            return Err("coverage fixture artifact hash drift".into());
        }
        for field in [
            "baseline_revision",
            "target_revision",
            "result",
            "platform",
            "command",
            "run_identity",
        ] {
            if entry["evidence"][field] != receipt[field] {
                return Err(format!("coverage evidence differs from receipt: {field}"));
            }
        }
        let matches = passed
            .iter()
            .filter(|case| {
                [
                    "target_file",
                    "target_case",
                    "target_hash",
                    "fixture",
                    "fixture_hash",
                ]
                .iter()
                .all(|field| case[*field] == entry[*field])
            })
            .count();
        if matches != 1 {
            return Err("coverage target case lacks unique observed receipt identity".into());
        }
    }
    Ok(
        json!({"coverage":report,"artifacts_verified":true,"receipt_provenance":"caller must verify trusted CI source"}),
    )
}
