//! Retirement fails closed on missing source cases, independent fixtures or verified target evidence.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("coverage entry requires {field}"))
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
fn target_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':', '\0'])
        && path
            .split('/')
            .all(|part| !part.is_empty() && !matches!(part, "." | ".."))
}

fn target(entry: &Value) -> Result<(), String> {
    let target = text(entry, "target_file")?;
    let integration = target.starts_with("native/tests/") && target.ends_with(".rs");
    let case = text(entry, "target_case")?;
    let unit = target.strip_prefix("native/src/").is_some_and(|name| {
        !name.contains('/')
            && (name.ends_with("_tests.rs")
                || name.strip_suffix(".rs").is_some_and(|module| {
                    case.strip_prefix(&format!("{module}::tests::"))
                        .is_some_and(|suffix| !suffix.is_empty())
                }))
    });
    if !(integration || unit) || !target_path(target) {
        return Err("coverage target must be a confined native test source".into());
    }
    let fixture = text(entry, "fixture")?;
    if !fixture.starts_with("tests/fixtures/")
        || !target_path(fixture)
        || !hash(text(entry, "fixture_hash")?)
    {
        return Err("coverage requires an independent fixture identity".into());
    }
    Ok(())
}

pub fn validate(inventory: &Value, coverage: &Value, release: bool) -> Result<Value, String> {
    let revision = text(inventory, "baseline_revision")?;
    if revision.len() != 40
        || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        || coverage["baseline_revision"].as_str() != Some(revision)
    {
        return Err("coverage must identify the exact pinned source revision".into());
    }
    let files = inventory["files"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or("coverage source inventory is empty")?;
    let mut source = BTreeMap::new();
    let mut expected = BTreeSet::new();
    for file in files {
        let path = text(file, "source")?;
        crate::source_test_inventory::source_path(path)?;
        let digest = text(file, "sha256")?;
        if !hash(digest) || source.insert(path, digest).is_some() {
            return Err("source inventory has duplicate paths or invalid hashes".into());
        }
        let cases = file["test_cases"]
            .as_array()
            .ok_or("source inventory requires test_cases")?;
        for case in cases {
            let name = case
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("source case name is empty")?;
            if !expected.insert((path, name)) {
                return Err("source inventory repeats a case".into());
            }
        }
    }
    if expected.is_empty() {
        return Err("coverage inventory has no source cases".into());
    }
    let discovery_complete = inventory["discovery_complete"] == true;
    if release && !discovery_complete {
        return Err(
            "source retirement blocked: inherited/dynamic source discovery is incomplete".into(),
        );
    }
    let entries = coverage["cases"]
        .as_array()
        .ok_or("coverage requires a cases list")?;
    let mut seen = BTreeSet::new();
    let mut pending = Vec::new();
    let mut verified = 0;
    let mut candidates = 0;
    for entry in entries {
        let path = text(entry, "source_file")?;
        let name = text(entry, "source_case")?;
        let key = (path, name);
        if !expected.contains(&key) || !seen.insert(key) {
            return Err("coverage has an unknown or duplicate source case".into());
        }
        if entry["source_hash"].as_str() != source.get(path).copied() {
            return Err("coverage source hash drift".into());
        }
        let file = files.iter().find(|file| file["source"] == path).unwrap();
        let matching = file["case_assertions"]["cases"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|case| case["source_case"] == name)
            .collect::<Vec<_>>();
        let discovered = file["discovery"]["case_origins"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|case| {
                name.split_once('.').is_some_and(|(binding, method)| {
                    case["binding"] == binding && case["method"] == method
                })
            })
            .collect::<Vec<_>>();
        // Missing joined assertions must not turn a discovered inherited case into a declaration.
        let inherited = discovered
            .iter()
            .chain(matching.iter())
            .any(|case| case["class_identity"] != case["defined_in_identity"]);
        if inherited || entry.get("origin").is_some() {
            let origin = entry["origin"]
                .as_object()
                .ok_or("inherited coverage requires original assertion origin")?;
            if matching.len() != 1
                || discovered.len() != 1
                || !inherited
                || origin
                    .get("method_line")
                    .and_then(Value::as_u64)
                    .is_none_or(|line| line == 0)
                || ["class_identity", "defined_in_identity", "method_line"]
                    .iter()
                    .any(|field| {
                        origin.get(*field) != matching[0].get(*field)
                            || origin.get(*field) != discovered[0].get(*field)
                    })
            {
                return Err("coverage origin differs from discovered original assertion".into());
            }
        }
        match text(entry, "status")? {
            "pending" => {
                if let Some(candidate) = entry.get("candidate") {
                    target(candidate)?;
                    text(candidate, "remaining")?;
                    candidates += 1;
                }
                pending.push(format!("{path}::{name}"));
            }
            "parity" | "native" | "redundant" => {
                target(entry)?;
                if text(entry, "status")? == "redundant" {
                    text(entry, "reason")?;
                }
                let evidence = &entry["evidence"];
                if evidence["result"] != "verified"
                    || evidence["baseline_revision"].as_str() != Some(revision)
                    || evidence["target_revision"].as_str().is_none_or(|sha| {
                        sha.len() != 40 || !sha.bytes().all(|c| c.is_ascii_hexdigit())
                    })
                {
                    return Err("coverage lacks exact source/target verification evidence".into());
                }
                for field in ["platform", "command", "run_identity"] {
                    text(evidence, field)?;
                }
                verified += 1;
            }
            _ => return Err("unsupported coverage disposition".into()),
        }
    }
    let missing = expected
        .difference(&seen)
        .map(|(path, name)| format!("{path}::{name}"))
        .collect::<Vec<_>>();
    if release && (!pending.is_empty() || !missing.is_empty() || seen.len() != expected.len()) {
        return Err(format!(
            "source retirement blocked: {} pending and {} unmapped cases",
            pending.len(),
            missing.len()
        ));
    }
    // This is only metadata completeness; filesystem/observed receipt checks are a separate gate.
    let metadata_complete =
        discovery_complete && pending.is_empty() && seen.len() == expected.len();
    let release_ready = false;
    Ok(
        json!({"declared":expected.len(),"mapped":seen.len(),"verified":verified,"candidates":candidates,"pending":pending,"unmapped":missing,"discovery_complete":discovery_complete,"metadata_complete":metadata_complete,"release_ready":release_ready}),
    )
}
