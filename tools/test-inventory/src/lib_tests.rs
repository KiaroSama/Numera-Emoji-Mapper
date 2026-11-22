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
