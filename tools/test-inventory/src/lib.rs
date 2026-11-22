pub mod ast;
pub mod language;
pub mod native;
pub mod supplemental;
use ast::{Function, Inventory, digest};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const REVISION: &str = "45a96f9de70da8429b83c047e0626b6ed6694b20";
pub const ORIGINAL_COUNT: usize = 1516;
pub const ID_SHA256: &str = "29f47d2d5eddc9b0e7b3d952190440aa216879a4a45723c45cfac85341c1db63";
// Pins the immutable source receipts, inherited identities and original type boundaries.
// Updating target wiring cannot change these facts about the independently collected source suite.
pub const SOURCE_RECEIPTS_SHA256: &str =
    "3c33c3e0d36e84314596d635cb18501a601d7c5b9b869d5a415ffb141e3cc561";
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub file: String,
    pub file_sha256: String,
    pub method_sha256: String,
    pub defined_in: String,
    pub inherited: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub file: String,
    pub target: String,
    pub function: String,
    pub body_sha256: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformSkip {
    pub linux: Option<String>,
    pub windows: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub kind: String,
    pub source: Source,
    pub original_platform_skip: Option<PlatformSkip>,
    pub source_skip_reasons: Vec<String>,
    pub platform_adaptation: Option<String>,
    pub target: Option<Target>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub source_revision: String,
    pub collection: String,
    pub cases: Vec<Case>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub original_main_rust: Option<native::Report>,
    pub supplemental_wiring: Option<supplemental::Report>,
    pub schema: u32,
    pub source_revision: String,
    pub manifest_ids: usize,
    pub rust_mappings: usize,
    pub browser_mappings: usize,
    pub performance_mappings: usize,
    pub served_source_mappings: usize,
    pub rust_discovered_functions: usize,
    pub javascript_collection_checked: bool,
    pub javascript_source_sha256: BTreeMap<String, String>,
    pub errors: Vec<String>,
    pub inventory: Inventory,
    pub semantic_parity_proven_by_inventory: bool,
    pub test_execution_proven_by_inventory: bool,
    pub final_sha_ci_proven_by_inventory: bool,
}
fn hash_ok(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !Path::new(path).is_absolute()
        && !path.contains('\\')
        && !path.split('/').any(|part| part == ".." || part.is_empty())
}
pub fn source_receipt_digest(manifest: &Manifest) -> String {
    let mut rows = manifest
        .cases
        .iter()
        .map(|c| {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                c.id,
                c.source.file,
                c.source.file_sha256,
                c.source.method_sha256,
                c.source.defined_in,
                c.source.inherited,
                c.kind,
                c.original_platform_skip
                    .as_ref()
                    .and_then(|p| p.linux.as_deref())
                    .unwrap_or(""),
                c.original_platform_skip
                    .as_ref()
                    .and_then(|p| p.windows.as_deref())
                    .unwrap_or(""),
                c.source_skip_reasons.join("|"),
            )
        })
        .collect::<Vec<_>>();
    rows.sort();
    digest(rows.join("").as_bytes())
}
pub fn validate_source(manifest: &Manifest) -> Vec<String> {
    let mut errors = Vec::new();
    if manifest.schema != 1 || manifest.source_revision != REVISION {
        errors.push("manifest schema or authoritative reference revision changed".into());
    }
    let ids = manifest
        .cases
        .iter()
        .map(|c| c.id.as_str())
        .collect::<BTreeSet<_>>();
    if manifest.cases.len() != ORIGINAL_COUNT || ids.len() != ORIGINAL_COUNT {
        errors.push(format!(
            "expected1516 unique original IDs, found{} entries/{} unique",
            manifest.cases.len(),
            ids.len()
        ));
    }
    if digest((ids.iter().copied().collect::<Vec<_>>().join("\n") + "\n").as_bytes()) != ID_SHA256 {
        errors.push("authoritative original ID digest differs".into());
    }
    if source_receipt_digest(manifest) != SOURCE_RECEIPTS_SHA256 {
        errors.push("authoritative source receipt/type/inheritance digest differs".into());
    }
    for c in &manifest.cases {
        if !safe_path(&c.source.file)
            || !hash_ok(&c.source.file_sha256)
            || !hash_ok(&c.source.method_sha256)
        {
            errors.push(format!("{} has invalid source receipt", c.id));
        }
        if ![
            "backend",
            "browser",
            "served-source-contract",
            "browser-performance",
        ]
        .contains(&c.kind.as_str())
        {
            errors.push(format!("{} has unknown original test type", c.id));
        }
        if !c.id.starts_with("tests.test_") || c.id.split('.').count() != 4 {
            errors.push(format!("invalid original identity {}", c.id));
        }
    }
    errors
}
fn lookup<'a>(target: &Target, inventory: &'a Inventory) -> Vec<&'a Function> {
    inventory
        .functions
        .iter()
        .filter(|f| {
            f.file == target.file && f.target == target.target && f.function == target.function
        })
        .collect()
}
fn target(function: &Function) -> Target {
    Target {
        file: function.file.clone(),
        target: function.target.clone(),
        function: function.function.clone(),
        body_sha256: Some(function.body_sha256.clone()),
    }
}
/// Refresh only mutable Rust wiring from actual compiled-root source declarations.
/// A matching comment in a string, helper or unreferenced file cannot become a candidate.
pub fn refresh(manifest: &mut Manifest, inventory: &Inventory) -> Vec<String> {
    let originals = manifest
        .cases
        .iter()
        .map(|c| c.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut candidates: BTreeMap<String, Vec<&Function>> = BTreeMap::new();
    let mut errors = inventory.errors.clone();
    for f in &inventory.functions {
        for id in &f.original_claims {
            if originals.contains(id.as_str()) {
                candidates.entry(id.clone()).or_default().push(f);
            }
        }
    }
    for c in &mut manifest.cases {
        if c.kind != "backend" {
            continue;
        }
        match candidates.get(&c.id).map(Vec::as_slice).unwrap_or_default() {
            [function] => c.target = Some(target(function)),
            [] => {
                // Reviewed legacy mappings can predate the adjacent-comment convention.
                if let Some(existing) = &c.target
                    && let [function] = lookup(existing, inventory).as_slice()
                {
                    c.target = Some(target(function));
                }
            }
            found => errors.push(format!(
                "{} is claimed by{} actual test functions",
                c.id,
                found.len()
            )),
        }
    }
    errors
}
fn playwright_cases(
    value: &serde_json::Value,
    result: &mut Vec<(String, String)>,
    errors: &mut Vec<String>,
) {
    for suite in value
        .get("suites")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        playwright_cases(suite, result, errors);
    }
    for spec in value
        .get("specs")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        let title = spec.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let file = spec.get("file").and_then(|v| v.as_str()).unwrap_or("");
        let tests = spec.get("tests").and_then(|v| v.as_array());
        if tests.is_none_or(|v| v.len() != 1) {
            errors.push(format!(
                "browser spec{title} must be collected once in one project"
            ));
        }
        if tests
            .into_iter()
            .flatten()
            .any(|t| t.get("expectedStatus").and_then(|v| v.as_str()) != Some("passed"))
        {
            errors.push(format!(
                "browser spec{title} is skipped or expected to fail"
            ));
        }
        result.push((
            format!("tests.{}", title.strip_prefix("tests.").unwrap_or(title)),
            file.replace('\\', "/"),
        ));
    }
}

pub fn verify_javascript_map(manifest: &Manifest, map: &serde_json::Value) -> Vec<String> {
    let mut errors = Vec::new();
    let Some(entries) = map.as_array() else {
        return vec!["JavaScript case map must be an array".into()];
    };
    let expected = manifest
        .cases
        .iter()
        .filter(|c| c.kind != "backend")
        .map(|c| (c.id.as_str(), c))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    for entry in entries {
        let original = entry["original"].as_str().unwrap_or("");
        let id = format!(
            "tests.{}",
            original.strip_prefix("tests.").unwrap_or(original)
        );
        if !seen.insert(id.clone()) {
            errors.push(format!("duplicate JavaScript mapping {id}"));
        }
        let Some(case) = expected.get(id.as_str()) else {
            errors.push(format!("unknown JavaScript original {id}"));
            continue;
        };
        let kind = match entry["type"].as_str() {
            Some("browser") => "browser",
            Some("browser-performance" | "performance") => "browser-performance",
            Some("served-text-contract") => "served-source-contract",
            _ => "unknown",
        };
        if kind != case.kind {
            errors.push(format!(
                "JavaScript {id} type differs from the immutable original boundary"
            ));
        }
        let file = format!("assets/tests/{}", entry["test_file"].as_str().unwrap_or(""));
        if case.target.as_ref().is_none_or(|t| {
            t.file != file
                || t.target != "playwright"
                || t.function != original.strip_prefix("tests.").unwrap_or(original)
        }) {
            errors.push(format!(
                "JavaScript {id} target differs from its actual case map"
            ));
        }
    }
    if seen.len() != 156 || entries.len() != 156 {
        errors.push(format!(
            "JavaScript case map requires156 unique originals; found{}/{}",
            entries.len(),
            seen.len()
        ));
    }
    errors
}
pub fn verify(
    manifest: &Manifest,
    inventory: Inventory,
    playwright: Option<&serde_json::Value>,
    rust_only: bool,
) -> Report {
    let mut errors = validate_source(manifest);
    errors.extend(inventory.errors.clone());
    let mut occupied = BTreeSet::new();
    let mut rust_mappings = 0;
    let mut browser_mappings = 0;
    let mut performance_mappings = 0;
    let mut served_source_mappings = 0;
    for c in &manifest.cases {
        let Some(t) = &c.target else {
            errors.push(format!("{} has no authored target mapping", c.id));
            continue;
        };
        if !safe_path(&t.file) {
            errors.push(format!("{} has an unsafe target path", c.id));
            continue;
        }
        if !occupied.insert((t.file.clone(), t.target.clone(), t.function.clone())) {
            errors.push(format!(
                "multiple originals map to one test: {}::{}",
                t.target, t.function
            ));
        }
        match c.kind.as_str() {
            "backend" => {
                rust_mappings += 1;
                let functions = lookup(t, &inventory);
                let [f] = functions.as_slice() else {
                    errors.push(format!(
                        "{} maps to{} wired Rust functions: {}::{}",
                        c.id,
                        functions.len(),
                        t.target,
                        t.function
                    ));
                    continue;
                };
                if !f.linux || !f.windows {
                    errors.push(format!("{} disappears from Rust discovery on Linux or Windows; use a genuine source-aware ignore",c.id));
                }
                if !f.nonempty || (f.assertions == 0 && f.calls.is_empty()) {
                    errors.push(format!("{} maps to an empty test body", c.id));
                }
                if t.body_sha256.as_deref() != Some(f.body_sha256.as_str()) {
                    errors.push(format!("{} target body changed since the reviewed mapping; refresh and review its disposition",c.id));
                }
                let expected = c.original_platform_skip.clone().unwrap_or(PlatformSkip {
                    linux: None,
                    windows: None,
                });
                let valid_ignore = |actual: &Option<String>, expected: &Option<String>| {
                    if let Some(expected) = expected {
                        actual.as_ref() == Some(expected)
                    } else {
                        actual.is_none()
                            || actual
                                .as_ref()
                                .is_some_and(|r| c.source_skip_reasons.contains(r))
                    }
                };
                if c.platform_adaptation.is_none()
                    && (!valid_ignore(&f.ignore_linux, &expected.linux)
                        || !valid_ignore(&f.ignore_windows, &expected.windows))
                {
                    errors.push(format!("{} ignore reasons/platforms differ from original source; preserve them or document an explicit reviewed native adaptation",c.id));
                }
            }
            "browser" => browser_mappings += 1,
            "browser-performance" => {
                browser_mappings += 1;
                performance_mappings += 1;
            }
            "served-source-contract" => served_source_mappings += 1,
            _ => {}
        }
    }
    if rust_mappings != 1360
        || browser_mappings != 97
        || served_source_mappings != 59
        || performance_mappings != 13
    {
        errors.push(format!("type boundaries require1360Rust+97browser+59served-source, found{rust_mappings}/{browser_mappings}/{served_source_mappings}"));
    }
    let mut checked = false;
    if let Some(list) = playwright {
        checked = true;
        if list
            .get("errors")
            .and_then(|v| v.as_array())
            .is_some_and(|v| !v.is_empty())
        {
            errors.push("Playwright discovery reports errors".into());
        }
        let mut collected = Vec::new();
        playwright_cases(list, &mut collected, &mut errors);
        let set = collected.iter().cloned().collect::<BTreeSet<_>>();
        if collected.len() != 156 || set.len() != 156 {
            errors.push(format!(
                "expected156 unique actual Playwright specs, found{}/{}",
                collected.len(),
                set.len()
            ));
        }
        let expected = manifest
            .cases
            .iter()
            .filter(|c| c.kind != "backend")
            .filter_map(|c| {
                c.target.as_ref().map(|t| {
                    (
                        c.id.clone(),
                        t.file
                            .strip_prefix("assets/tests/")
                            .unwrap_or(&t.file)
                            .to_owned(),
                    )
                })
            })
            .collect::<BTreeSet<_>>();
        for missing in expected.difference(&set) {
            errors.push(format!(
                "missing actual browser/source spec: {} in{}",
                missing.0, missing.1
            ));
        }
        for extra in set.difference(&expected) {
            errors.push(format!(
                "unexpected actual browser/source spec: {} in{}",
                extra.0, extra.1
            ));
        }
    } else if !rust_only {
        errors.push(
            "all mode requires actual Playwright --list --reporter=json discovery output".into(),
        );
    }
    Report {
        original_main_rust: None,
        supplemental_wiring: None,
        schema: 1,
        source_revision: REVISION.into(),
        manifest_ids: manifest.cases.len(),
        rust_mappings,
        browser_mappings,
        performance_mappings,
        served_source_mappings,
        rust_discovered_functions: inventory.functions.len(),
        javascript_collection_checked: checked,
        javascript_source_sha256: BTreeMap::new(),
        errors,
        inventory,
        semantic_parity_proven_by_inventory: false,
        test_execution_proven_by_inventory: false,
        final_sha_ci_proven_by_inventory: false,
    }
}

#[cfg(test)]
mod lib_tests;
#[cfg(test)]
#[path = "../../../runtime/tests/support/test_admission.rs"]
mod test_admission;
