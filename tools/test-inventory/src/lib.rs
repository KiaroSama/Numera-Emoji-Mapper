pub mod ast;
pub mod language;
pub mod native;
pub mod supplemental;
use ast::{Function, Inventory, digest};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const REVISION: &str = "a32dbab6e3e694f1fac9656997fa2edb6e3b2ede";
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
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Project(PathBuf);
    impl Project {
        fn new(lib: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "numera-inventory-fixture-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("runtime/src")).unwrap();
            std::fs::create_dir(root.join("runtime/tests")).unwrap();
            std::fs::write(
                root.join("runtime/Cargo.toml"),
                "[package]\nname='fixture'\n",
            )
            .unwrap();
            std::fs::write(root.join("runtime/src/lib.rs"), lib).unwrap();
            Self(root)
        }
    }
    impl Drop for Project {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn baseline() -> Manifest {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/migration/reference-cases.json"
        ))
        .unwrap()
    }
    #[test]
    fn authoritative_collection_rejects_duplicate_replacement_ids_and_changed_source_receipts() {
        let original = baseline();
        assert!(validate_source(&original).is_empty());
        let mut duplicate = original.clone();
        duplicate.cases[1].id = duplicate.cases[0].id.clone();
        assert!(
            validate_source(&duplicate)
                .iter()
                .any(|e| e.contains("unique"))
        );
        let mut changed = original.clone();
        changed.cases[0].source.method_sha256 = "0".repeat(64);
        assert!(
            validate_source(&changed)
                .iter()
                .any(|e| e.contains("receipt"))
        );
        let mut substituted = original.clone();
        substituted.cases[0].kind = "browser".into();
        assert!(
            validate_source(&substituted)
                .iter()
                .any(|e| e.contains("type"))
        );
        let mut inherited = original;
        let row = inherited
            .cases
            .iter_mut()
            .find(|c| c.source.inherited)
            .unwrap();
        row.source.inherited = false;
        assert!(
            validate_source(&inherited)
                .iter()
                .any(|e| e.contains("inheritance"))
        );
    }
    #[test]
    fn raw_strings_comments_helpers_and_unwired_modules_cannot_supply_a_test() {
        let p = Project::new(
            r####"const FIXTURE:&str=r###"#[test] fn pretend(){assert!(true);} // tests.test_fake.X.test_fake"###;
fn helper(){assert!(true);}
// #[test] fn commented(){assert!(true);}
"####,
        );
        std::fs::write(
            p.0.join("runtime/src/unwired.rs"),
            "#[test] fn unwired(){assert!(true);}",
        )
        .unwrap();
        std::fs::write(
            p.0.join("runtime/tests/real.rs"),
            "#[test] fn actual(){assert_eq!(2+2,4);}",
        )
        .unwrap();
        let collected = ast::collect(&p.0);
        assert!(collected.errors.is_empty());
        assert_eq!(collected.functions.len(), 1);
        assert_eq!(collected.functions[0].function, "actual");
        assert!(collected.functions[0].original_claims.is_empty());
    }
    #[test]
    fn wired_path_modules_and_genuine_ignored_platforms_remain_discoverable() {
        let p = Project::new("#[cfg(test)] #[path=\"private.rs\"] mod private;");
        std::fs::write(p.0.join("runtime/src/private.rs"),"// OriginalID: tests.test_atomic_state_files.AtomicStateFiles.test_a_persistent_refusal_still_fails_and_cleans_up\n#[cfg_attr(not(windows),ignore=\"Windows' transient replace refusal\")]\n#[test] fn refused(){#[cfg(windows)] {assert!(true);}}\n#[cfg(windows)] #[test] fn disappears(){assert!(true);}").unwrap();
        let inventory = ast::collect(&p.0);
        assert!(inventory.errors.is_empty());
        let f = inventory
            .functions
            .iter()
            .find(|f| f.function == "private::refused")
            .unwrap();
        assert!(f.linux && f.windows);
        assert_eq!(
            f.ignore_linux.as_deref(),
            Some("Windows' transient replace refusal")
        );
        assert_eq!(f.ignore_windows, None);
        assert_eq!(f.original_claims.len(), 1);
        let f = inventory
            .functions
            .iter()
            .find(|f| f.function == "private::disappears")
            .unwrap();
        assert!(!f.linux && f.windows);
    }
    #[test]
    fn missing_wired_module_and_disabled_cargo_autotests_fail_closed() {
        let p = Project::new("#[cfg(test)] mod vanished;");
        std::fs::write(
            p.0.join("runtime/Cargo.toml"),
            "[package]\nautotests = false\n",
        )
        .unwrap();
        let inventory = ast::collect(&p.0);
        assert!(inventory.errors.iter().any(|e| e.contains("vanished")));
        assert!(inventory.errors.iter().any(|e| e.contains("automatic")));
    }

    #[test]
    fn module_directories_follow_explicit_path_and_integration_crate_root_rules() {
        let p = Project::new("#[path=\"support/unit.rs\"] mod unit;\nmod ordinary;");
        std::fs::create_dir_all(p.0.join("runtime/src/support")).unwrap();
        std::fs::create_dir_all(p.0.join("runtime/src/ordinary")).unwrap();
        std::fs::write(p.0.join("runtime/src/support/unit.rs"), "mod family;").unwrap();
        std::fs::write(
            p.0.join("runtime/src/support/family.rs"),
            "#[test] fn via_path(){assert!(true);}",
        )
        .unwrap();
        std::fs::write(p.0.join("runtime/src/ordinary.rs"), "mod nested;").unwrap();
        std::fs::write(
            p.0.join("runtime/src/ordinary/nested.rs"),
            "#[test] fn implicit(){assert!(true);}",
        )
        .unwrap();
        std::fs::write(p.0.join("runtime/tests/integration.rs"), "mod shared;").unwrap();
        std::fs::create_dir(p.0.join("runtime/tests/shared")).unwrap();
        std::fs::write(
            p.0.join("runtime/tests/shared/mod.rs"),
            "#[test] fn integration_child(){assert!(true);}",
        )
        .unwrap();
        let inventory = ast::collect(&p.0);
        assert!(inventory.errors.is_empty(), "{:?}", inventory.errors);
        let identities = inventory
            .functions
            .iter()
            .map(|f| f.function.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            identities,
            BTreeSet::from([
                "unit::family::via_path",
                "ordinary::nested::implicit",
                "shared::integration_child"
            ])
        );
    }
    #[test]
    fn duplicate_claims_cannot_refresh_two_originals_into_generic_name_wrappers() {
        let id = baseline().cases[0].id.clone();
        let p = Project::new("");
        std::fs::write(p.0.join("runtime/tests/duplicates.rs"),format!("// OriginalID: {id}\n#[test] fn one(){{assert!(true);}}\n// OriginalID: {id}\n#[test] fn two(){{assert!(true);}}")).unwrap();
        let inventory = ast::collect(&p.0);
        let mut manifest = baseline();
        assert!(
            refresh(&mut manifest, &inventory)
                .iter()
                .any(|e| e.contains("claimed by2"))
        );
    }
    #[test]
    fn a_mapping_needs_exact_live_function_identity_hash_and_nonempty_body() {
        let p = Project::new("#[test] fn empty(){}\n#[test] fn actual(){assert_eq!(3,3);}");
        let inventory = ast::collect(&p.0);
        let empty = inventory
            .functions
            .iter()
            .find(|f| f.function == "empty")
            .unwrap();
        let mut manifest = baseline();
        manifest.cases[0].target = Some(target(empty));
        let report = verify(&manifest, inventory, None, true);
        assert!(report.errors.iter().any(|e| e.contains("empty test body")));
        let inventory = ast::collect(&p.0);
        let actual = inventory
            .functions
            .iter()
            .find(|f| f.function == "actual")
            .unwrap();
        manifest.cases[0].target = Some(target(actual));
        manifest.cases[0].target.as_mut().unwrap().body_sha256 = Some("0".repeat(64));
        assert!(
            verify(&manifest, inventory, None, true)
                .errors
                .iter()
                .any(|e| e.contains("body changed"))
        );
    }
    #[test]
    fn playwright_collection_requires_unique_live_specs_without_skips_or_extra_projects() {
        let mut entries = Vec::new();
        let mut errors = Vec::new();
        playwright_cases(
            &json!({"suites":[{"specs":[{"title":"test_panel_browser.C.test_x","file":"panel_browser.spec.js","tests":[{"expectedStatus":"passed"}]}]}]}),
            &mut entries,
            &mut errors,
        );
        assert!(errors.is_empty());
        assert_eq!(
            entries,
            vec![(
                "tests.test_panel_browser.C.test_x".into(),
                "panel_browser.spec.js".into()
            )]
        );
        let mut entries = Vec::new();
        let mut errors = Vec::new();
        playwright_cases(
            &json!({"specs":[{"title":"test_panel_browser.C.test_x","file":"panel_browser.spec.js","tests":[{"expectedStatus":"skipped"},{"expectedStatus":"passed"}]}]}),
            &mut entries,
            &mut errors,
        );
        assert_eq!(errors.len(), 2);
        assert!(errors.iter().any(|e| e.contains("skipped")));
        assert!(errors.iter().any(|e| e.contains("one project")));
    }
    #[test]
    fn all_mode_requires_actual_browser_collection_and_never_claims_execution_or_parity() {
        let report = verify(&baseline(), Inventory::default(), None, false);
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("actual Playwright"))
        );
        assert!(
            !report.semantic_parity_proven_by_inventory
                && !report.test_execution_proven_by_inventory
                && !report.final_sha_ci_proven_by_inventory
        );
    }

    #[test]
    fn javascript_source_and_performance_boundaries_cannot_be_relabelled_as_browser_cases() {
        let manifest = baseline();
        let map: serde_json::Value =
            serde_json::from_str(include_str!("../../../assets/tests/case-map.json")).unwrap();
        assert!(verify_javascript_map(&manifest, &map).is_empty());
        let mut changed = map.clone();
        let row = changed
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row["type"] == "served-text-contract")
            .unwrap();
        row["type"] = json!("browser");
        assert!(
            verify_javascript_map(&manifest, &changed)
                .iter()
                .any(|e| e.contains("type differs"))
        );
        let mut duplicate = map.clone();
        let first = duplicate[0].clone();
        duplicate.as_array_mut().unwrap().push(first);
        assert!(
            verify_javascript_map(&manifest, &duplicate)
                .iter()
                .any(|e| e.contains("duplicate"))
        );
        let mut performance = map;
        let row = performance
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| {
                matches!(
                    row["type"].as_str(),
                    Some("browser-performance" | "performance")
                )
            })
            .unwrap();
        row["type"] = json!("browser");
        assert!(
            verify_javascript_map(&manifest, &performance)
                .iter()
                .any(|e| e.contains("type differs"))
        );
    }

    #[test]
    fn adjacent_id_claims_do_not_leak_from_a_previous_module_or_previous_test() {
        let p = Project::new(
            "mod first {\n// OriginalID: tests.test_alpha.A.test_first\n#[test] fn first(){assert!(true);}\n}\nmod second {\n// OriginalID: tests.test_beta.B.test_second\n#[test] fn second(){assert!(true);}\n}",
        );
        let inventory = ast::collect(&p.0);
        assert_eq!(inventory.functions.len(), 2);
        for f in inventory.functions {
            assert_eq!(f.original_claims.len(), 1);
        }
    }

    #[test]
    fn original_main_rust_helpers_and_retired_platform_never_inflate_python_originals() {
        let manifest: native::Manifest = serde_json::from_str(include_str!(
            "../../../tests/fixtures/migration/original-main-rust.json"
        ))
        .unwrap();
        assert_eq!(native::source_digest(&manifest), native::SOURCE_SHA256);
        let report = native::verify(&manifest, &Inventory::default());
        assert_eq!(
            (
                report.declarations,
                report.applicable,
                report.product_behaviors,
                report.technical_helpers,
                report.retired_macos
            ),
            (35, 34, 32, 2, 1)
        );
        assert!(!report.counts_toward_python_1516 && !report.proves_execution);
        let mut changed = manifest;
        changed.cases[0].technical_helper = !changed.cases[0].technical_helper;
        assert!(
            native::verify(&changed, &Inventory::default())
                .errors
                .iter()
                .any(|e| e.contains("source receipt changed"))
        );
    }

    #[test]
    fn frozen_original_main_fixture_bytes_cannot_be_changed_to_match_a_new_implementation() {
        let p = Project::new("");
        std::fs::create_dir_all(p.0.join("tests/fixtures")).unwrap();
        let path = p.0.join("tests/fixtures/golden.json");
        std::fs::write(&path, b"original independent oracle").unwrap();
        let mut manifest: native::Manifest = serde_json::from_str(include_str!(
            "../../../tests/fixtures/migration/original-main-rust.json"
        ))
        .unwrap();
        manifest.fixture_sha256 = BTreeMap::from([(
            "tests/fixtures/golden.json".into(),
            digest(b"original independent oracle"),
        )]);
        assert!(native::verify_fixtures(&manifest, &p.0).is_empty());
        std::fs::write(path, b"expectations derived from implementation").unwrap();
        assert!(
            native::verify_fixtures(&manifest, &p.0)
                .iter()
                .any(|e| e.contains("frozen fixture hash changed"))
        );
    }
}

#[cfg(test)]
#[path = "../../../runtime/tests/support/test_admission.rs"]
mod test_admission;
