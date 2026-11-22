//! Every supplementary Rust test declaration must belong to a real Cargo test context.
use crate::ast::{
    Inventory, Platform, TestContext, admitted, digest, test_attribute, test_enabled,
};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retirement {
    pub file: String,
    pub function: String,
    pub body_sha256: String,
    pub platform: String,
    pub reason: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retirements {
    pub schema: u32,
    pub cases: Vec<Retirement>,
}
#[derive(Debug, Serialize)]
pub struct Declaration {
    pub file: String,
    pub function: String,
    pub source_line: usize,
    pub body_sha256: String,
    pub source_linux: bool,
    pub source_windows: bool,
    pub source_macos: bool,
    pub reachable_linux: bool,
    pub reachable_windows: bool,
    pub target_contexts: Vec<String>,
    pub retired: bool,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub declarations: Vec<Declaration>,
    pub errors: Vec<String>,
    pub counts_as_original_ids: bool,
    pub proves_execution: bool,
}
fn scan_items(
    items: &[syn::Item],
    file: &str,
    namespace: &[String],
    mask: (bool, bool, bool),
    out: &mut Vec<Declaration>,
    context: &TestContext,
) {
    for item in items {
        match item {
            syn::Item::Fn(function) if function.attrs.iter().any(test_attribute) => {
                let mut name = namespace.to_vec();
                name.push(function.sig.ident.to_string());
                out.push(Declaration {
                    file: file.into(),
                    function: name.join("::"),
                    source_line: function.span().start().line,
                    body_sha256: digest(function.block.to_token_stream().to_string().as_bytes()),
                    source_linux: mask.0
                        && admitted(&function.attrs, Platform::LinuxX64, context)
                        && test_enabled(&function.attrs, Platform::LinuxX64, context)
                        && admitted(&function.attrs, Platform::LinuxArm64, context)
                        && test_enabled(&function.attrs, Platform::LinuxArm64, context),
                    source_windows: mask.1
                        && admitted(&function.attrs, Platform::WindowsX64, context)
                        && test_enabled(&function.attrs, Platform::WindowsX64, context)
                        && admitted(&function.attrs, Platform::WindowsArm64, context)
                        && test_enabled(&function.attrs, Platform::WindowsArm64, context),
                    source_macos: mask.2
                        && admitted(&function.attrs, Platform::Macos, context)
                        && test_enabled(&function.attrs, Platform::Macos, context),
                    reachable_linux: false,
                    reachable_windows: false,
                    target_contexts: Vec::new(),
                    retired: false,
                });
            }
            syn::Item::Mod(module) => {
                if let Some((_, items)) = &module.content {
                    let mut name = namespace.to_vec();
                    name.push(module.ident.to_string());
                    scan_items(
                        items,
                        file,
                        &name,
                        (
                            mask.0
                                && admitted(&module.attrs, Platform::LinuxX64, context)
                                && admitted(&module.attrs, Platform::LinuxArm64, context),
                            mask.1
                                && admitted(&module.attrs, Platform::WindowsX64, context)
                                && admitted(&module.attrs, Platform::WindowsArm64, context),
                            mask.2 && admitted(&module.attrs, Platform::Macos, context),
                        ),
                        out,
                        context,
                    );
                }
            }
            _ => {}
        }
    }
}
fn source_files(root: &Path, path: &Path, out: &mut BTreeSet<PathBuf>, errors: &mut Vec<String>) {
    if path.canonicalize().is_ok_and(|p| !p.starts_with(root)) {
        errors.push(format!(
            "supplemental source path escapes project root: {}",
            path.display()
        ));
        return;
    }
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return;
    };
    if crate::language::linked(&metadata) {
        errors.push(format!(
            "supplemental source scan refuses linked path {}",
            path.display()
        ));
        return;
    }
    if metadata.is_dir() {
        match std::fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(entry) => source_files(root, &entry.path(), out, errors),
                        Err(error) => errors.push(error.to_string()),
                    }
                }
            }
            Err(error) => errors.push(error.to_string()),
        }
    } else if metadata.is_file()
        && path.file_name().is_some_and(|name| {
            let name = name.to_string_lossy();
            name.ends_with(".rs") || name.ends_with(".rs.in")
        })
        && path.starts_with(root)
    {
        out.insert(path.into());
    }
}
pub fn check(root: &Path, inventory: &Inventory, retirements: &Retirements) -> Report {
    let root = root.canonicalize().unwrap_or_else(|_| root.into());
    let mut report = Report {
        declarations: Vec::new(),
        errors: Vec::new(),
        counts_as_original_ids: false,
        proves_execution: false,
    };
    report.errors.extend(ordinary_run_configuration(&root));
    if retirements.schema != 1 {
        report
            .errors
            .push("unknown supplemental retirement schema".into());
    }
    let mut paths = BTreeSet::new();
    source_files(
        &root,
        &root.join("runtime/src"),
        &mut paths,
        &mut report.errors,
    );
    source_files(
        &root,
        &root.join("runtime/tests"),
        &mut paths,
        &mut report.errors,
    );
    // During consolidation, production/test roots can still reference shared first-party files.
    // They are scanned by actual graph membership, not by an obsolete native target whitelist.
    for path in inventory
        .source_sha256
        .keys()
        .filter(|p| p.starts_with("native/src/") || p.starts_with("native/tests/support/"))
    {
        paths.insert(root.join(path));
    }
    let mut observed: BTreeMap<(&str, usize), Vec<_>> = BTreeMap::new();
    for function in &inventory.functions {
        observed
            .entry((&function.file, function.source_line))
            .or_default()
            .push(function);
    }
    for path in paths {
        let relative = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = match std::fs::read_to_string(&path) {
            Ok(v) => v,
            Err(error) => {
                report.errors.push(format!("{relative}: {error}"));
                continue;
            }
        };
        match syn::parse_file(&text) {
            Ok(file) => scan_items(
                &file.items,
                &relative,
                &[],
                (
                    admitted(&file.attrs, Platform::LinuxX64, &inventory.ci_context)
                        && admitted(&file.attrs, Platform::LinuxArm64, &inventory.ci_context),
                    admitted(&file.attrs, Platform::WindowsX64, &inventory.ci_context)
                        && admitted(&file.attrs, Platform::WindowsArm64, &inventory.ci_context),
                    admitted(&file.attrs, Platform::Macos, &inventory.ci_context),
                ),
                &mut report.declarations,
                &inventory.ci_context,
            ),
            Err(error) => report.errors.push(format!(
                "cannot inspect supplementary declarations in{relative}: {error}"
            )),
        }
    }
    let mut used_retirements = BTreeSet::new();
    for declaration in &mut report.declarations {
        if let Some(contexts) = observed.get(&(declaration.file.as_str(), declaration.source_line))
        {
            declaration.reachable_linux = contexts.iter().any(|f| f.linux);
            declaration.reachable_windows = contexts.iter().any(|f| f.windows);
            declaration.target_contexts = contexts
                .iter()
                .map(|f| format!("{}::{}", f.target, f.function))
                .collect();
            declaration.target_contexts.sort();
            declaration.target_contexts.dedup();
        }
        if declaration.reachable_linux || declaration.reachable_windows {
            continue;
        }
        let candidates = retirements
            .cases
            .iter()
            .enumerate()
            .filter(|(_, r)| r.file == declaration.file && r.function == declaration.function)
            .collect::<Vec<_>>();
        if let [(index, retirement)] = candidates.as_slice()
            && retirement.platform == "macos"
            && !retirement.reason.trim().is_empty()
            && retirement.body_sha256 == declaration.body_sha256
            && declaration.source_macos
            && !declaration.source_linux
            && !declaration.source_windows
        {
            declaration.retired = true;
            used_retirements.insert(*index);
            continue;
        }
        report.errors.push(format!("unwired or disabled supplementary test declaration {}:{} {} has no actual Cargo test context on Linux/Windows",declaration.file,declaration.source_line,declaration.function));
    }
    for (index, retirement) in retirements.cases.iter().enumerate() {
        if !used_retirements.contains(&index) {
            report.errors.push(format!(
                "stale, unsupported or non-genuine supplemental retirement {}::{}",
                retirement.file, retirement.function
            ));
        }
    }
    report
        .declarations
        .sort_by(|a, b| (&a.file, a.source_line).cmp(&(&b.file, b.source_line)));
    report
}
pub fn default_retirements() -> Result<Retirements, String> {
    serde_json::from_str(include_str!("../supplemental-retirements.json"))
        .map_err(|e| e.to_string())
}

fn ordinary_run_configuration(root: &Path) -> Vec<String> {
    let mut errors = Vec::new();
    for name in [".cargo/config.toml", ".cargo/config"] {
        let path = root.join(name);
        if !path.exists() {
            continue;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(v) => v,
            Err(_) => {
                errors.push(format!(
                    "cannot inspect ordinary Cargo configuration {name}"
                ));
                continue;
            }
        };
        let value = match toml_span::parse(&text) {
            Ok(value) => value,
            Err(_) => {
                errors.push(format!("cannot parse ordinary Cargo configuration {name}"));
                continue;
            }
        };
        let has_runner = value
            .as_table()
            .and_then(|table| table.get("target"))
            .and_then(|targets| targets.as_table())
            .is_some_and(|targets| targets.values().any(|target| target.has_key("runner")));
        if has_runner {
            errors.push(format!("{name} installs a global Cargo target runner; ordinary Cargo run must remain direct; select isolated testing with an explicit test config"));
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Tree(PathBuf);
    impl Tree {
        fn new(lib: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "numera-supplemental-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join("runtime/src")).unwrap();
            std::fs::create_dir(root.join("runtime/tests")).unwrap();
            let tree = Self(root);
            tree.put(
                "runtime/Cargo.toml",
                "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n",
            );
            tree.put(
                "runtime/Cargo.lock",
                "version = 4\n[[package]]\nname = \"fixture\"\nversion = \"0.1.0\"\n",
            );
            tree.put("runtime/src/lib.rs", lib);
            tree
        }
        fn put(&self, path: &str, text: &str) {
            let path = self.0.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        fn scan(&self) -> Report {
            let roots = crate::ast::cargo_targets(&self.0).unwrap();
            check(
                &self.0,
                &crate::ast::collect_cargo_targets(&self.0, &roots),
                &Retirements {
                    schema: 1,
                    cases: Vec::new(),
                },
            )
        }
        fn release_list(&self) -> String {
            let cargo = std::env::var_os("CARGO")
                .or_else(|| option_env!("CARGO").map(Into::into))
                .unwrap();
            let output = std::process::Command::new(cargo)
                .args([
                    "test",
                    "--locked",
                    "--offline",
                    "--release",
                    "--manifest-path",
                ])
                .arg(self.0.join("runtime/Cargo.toml"))
                .args(["--", "--list"])
                .env("CARGO_TARGET_DIR", self.0.join("target"))
                .env("RUSTC", env!("NUMERA_INVENTORY_RUSTC"))
                .env("RUSTDOC", env!("NUMERA_INVENTORY_RUSTDOC"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        }
    }
    impl Drop for Tree {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn an_actual_unwired_private_file_is_rejected_then_real_path_wiring_is_accepted() {
        let tree = Tree::new("");
        tree.put(
            "runtime/src/extra_tests.rs",
            "#[test] fn preserves_actual_state(){assert_eq!(2+2,4);}",
        );
        let red = tree.scan();
        assert_eq!(red.declarations.len(), 1);
        assert!(red.errors.iter().any(|e| e.contains("unwired")));
        tree.put(
            "runtime/src/lib.rs",
            "#[cfg(test)] #[path=\"extra_tests.rs\"] mod extra;",
        );
        let green = tree.scan();
        assert!(green.errors.is_empty(), "{:?}", green.errors);
        assert_eq!(
            green.declarations[0].target_contexts,
            vec!["lib::extra::preserves_actual_state"]
        );
    }
    #[test]
    fn a_module_disabled_on_both_supported_platforms_cannot_satisfy_private_wiring() {
        let tree = Tree::new("#[cfg(all(test,any()))] mod old_tests;");
        tree.put(
            "runtime/src/old_tests.rs",
            "#[test] fn real_regression(){assert!(true);}",
        );
        assert!(tree.scan().errors.iter().any(|e| e.contains("disabled")));
        tree.put("runtime/src/lib.rs", "#[cfg(test)] mod old_tests;");
        assert!(tree.scan().errors.is_empty());
    }
    #[test]
    fn retired_native_runtime_flag_cannot_supply_a_test_that_actual_cargo_does_not_compile() {
        let tree = Tree::new("#[cfg(all(test,numera_native_runtime))] mod stale;");
        tree.put(
            "runtime/src/stale.rs",
            "#[test] fn actual_contract(){assert_eq!(9,9);}",
        );
        assert!(!tree.release_list().contains("stale::actual_contract: test"));
        assert!(
            tree.scan()
                .errors
                .iter()
                .any(|e| e.contains("actual_contract"))
        );
        tree.put("runtime/src/lib.rs", "#[cfg(test)] mod stale;");
        assert!(tree.release_list().contains("stale::actual_contract: test"));
        assert!(tree.scan().errors.is_empty());
    }
    #[test]
    fn actual_x64_gnu_msvc_context_is_known_and_unsupported_predicates_fail_closed_through_negation()
     {
        let tree = Tree::new(
            "#[cfg(all(test,target_env=\"gnu\",target_pointer_width=\"64\"))] mod linux; #[cfg(all(test,target_env=\"msvc\",target_pointer_width=\"64\"))] mod windows;",
        );
        tree.put(
            "runtime/src/linux.rs",
            "#[test] fn linux_context(){assert!(true);}",
        );
        tree.put(
            "runtime/src/windows.rs",
            "#[test] fn windows_context(){assert!(true);}",
        );
        assert!(tree.release_list().contains(if cfg!(windows) {
            "windows::windows_context: test"
        } else {
            "linux::linux_context: test"
        }));
        let green = tree.scan();
        assert!(green.errors.is_empty(), "{:?}", green.errors);
        assert!(
            green
                .declarations
                .iter()
                .find(|d| d.function == "linux_context")
                .is_some_and(|d| d.reachable_linux && !d.reachable_windows)
        );
        assert!(
            green
                .declarations
                .iter()
                .find(|d| d.function == "windows_context")
                .is_some_and(|d| !d.reachable_linux && d.reachable_windows)
        );
        tree.put(
            "runtime/src/lib.rs",
            "#[cfg(all(test,not(unmodeled_build_flag)))] mod unknown;",
        );
        tree.put(
            "runtime/src/unknown.rs",
            "#[test] fn uncertain_context(){assert!(true);}",
        );
        assert!(
            tree.scan()
                .errors
                .iter()
                .any(|e| e.contains("uncertain_context"))
        );
    }
    #[test]
    fn actual_release_discovery_rejects_debug_only_and_reaches_release_only_modules() {
        let tree = Tree::new(
            "#[cfg(all(test,debug_assertions))] mod debug_only; #[cfg(all(test,not(debug_assertions)))] mod release_only;",
        );
        tree.put(
            "runtime/src/debug_only.rs",
            "#[test] fn debug_behavior(){assert_eq!(1,1);}",
        );
        tree.put(
            "runtime/src/release_only.rs",
            "#[test] fn release_behavior(){assert_eq!(2,2);}",
        );
        let actual = tree.release_list();
        assert!(actual.contains("release_only::release_behavior: test"));
        assert!(!actual.contains("debug_only::debug_behavior: test"));
        let report = tree.scan();
        assert!(
            report.errors.iter().any(|e| e.contains("debug_behavior")),
            "{:?}",
            report.errors
        );
        assert!(
            !report.errors.iter().any(|e| e.contains("release_behavior")),
            "{:?}",
            report.errors
        );
        tree.put("runtime/src/lib.rs","#[cfg(test)] mod debug_only; #[cfg(all(test,not(debug_assertions)))] mod release_only;");
        assert!(tree.scan().errors.is_empty());
        let manifest = std::fs::read_to_string(tree.0.join("runtime/Cargo.toml")).unwrap();
        tree.put(
            "runtime/Cargo.toml",
            &format!("{manifest}\n[profile.release]\ndebug-assertions=true\n"),
        );
        tree.put("runtime/src/lib.rs","#[cfg(all(test,debug_assertions))] mod debug_only; #[cfg(all(test,not(debug_assertions)))] mod release_only;");
        let actual = tree.release_list();
        assert!(actual.contains("debug_only::debug_behavior: test"));
        assert!(!actual.contains("release_only::release_behavior: test"));
        let report = tree.scan();
        assert!(report.errors.iter().any(|e| e.contains("release_behavior")));
        assert!(!report.errors.iter().any(|e| e.contains("debug_behavior")));
    }
    #[test]
    fn effective_cfg_attr_tests_are_discovered_and_unwired_conditional_tests_are_rejected() {
        let tree = Tree::new("#[cfg(test)] mod wired;");
        tree.put(
            "runtime/src/wired.rs",
            "#[cfg_attr(test,test)] fn conditional(){assert_eq!(5,5);}",
        );
        assert!(tree.release_list().contains("wired::conditional: test"));
        let wired = tree.scan();
        assert_eq!(wired.declarations.len(), 1);
        assert!(wired.declarations[0].reachable_linux);
        tree.put(
            "runtime/src/unwired.rs",
            "#[cfg_attr(test,test)] fn missing(){assert_eq!(6,6);}",
        );
        assert!(tree.scan().errors.iter().any(|e| e.contains("missing")));
        tree.put(
            "runtime/src/lib.rs",
            "#[cfg(test)] mod wired; #[cfg(test)] mod unwired;",
        );
        assert!(tree.scan().errors.is_empty());
    }
    #[test]
    fn actual_default_feature_discovery_excludes_gated_targets_until_the_default_closure_enables_them()
     {
        let tree = Tree::new("");
        let manifest = "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n[features]\ndefault=[]\nbridge=['hidden']\nhidden=[]\n[[test]]\nname='gated-contract'\npath='tests/gated/case.rs'\nrequired-features=['hidden']\n";
        tree.put("runtime/Cargo.toml", manifest);
        tree.put(
            "runtime/tests/gated/case.rs",
            "#[cfg(feature=\"hidden\")] #[test] fn gated_behavior(){assert_eq!(7,7);}",
        );
        assert!(!tree.release_list().contains("gated_behavior: test"));
        assert!(
            tree.scan()
                .errors
                .iter()
                .any(|e| e.contains("gated_behavior"))
        );
        tree.put(
            "runtime/Cargo.toml",
            &manifest.replace("default=[]", "default=['bridge']"),
        );
        assert!(tree.release_list().contains("gated_behavior: test"));
        let green = tree.scan();
        assert!(green.errors.is_empty(), "{:?}", green.errors);
        assert!(
            green.declarations[0]
                .target_contexts
                .iter()
                .any(|t| t.starts_with("gated-contract::"))
        );
    }
    #[test]
    fn dropping_only_a_supplementary_module_is_detected_without_any_original_id_list() {
        let tree = Tree::new("#[cfg(test)] mod current;\n#[cfg(test)] mod supplementary;");
        for name in ["current", "supplementary"] {
            tree.put(
                &format!("runtime/src/{name}.rs"),
                "#[test] fn contract(){assert_eq!(1,1);}",
            );
        }
        assert!(tree.scan().errors.is_empty());
        tree.put("runtime/src/lib.rs", "#[cfg(test)] mod current;");
        let report = tree.scan();
        assert_eq!(report.declarations.len(), 2);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("supplementary.rs"));
        assert!(!report.counts_as_original_ids && !report.proves_execution);
    }
    #[test]
    fn private_tokio_test_is_an_actual_declaration_and_raw_fixture_text_is_not() {
        let tree = Tree::new("#[cfg(test)] mod helpers;");
        tree.put("runtime/src/helpers.rs",r##"const QUOTED:&str=r#"#[test] fn fake(){assert!(true);}"#; #[tokio::test] async fn actual_http_contract(){assert_eq!(1,1);}"##);
        let report = tree.scan();
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.declarations.len(), 1);
        assert_eq!(report.declarations[0].function, "actual_http_contract");
    }
    #[test]
    fn genuine_windows_private_context_is_recorded_without_inventing_linux_execution() {
        let tree = Tree::new("#[cfg(all(test,windows))] mod windows_tests;");
        tree.put(
            "runtime/src/windows_tests.rs",
            "#[test] fn kernel_contract(){assert!(true);}",
        );
        let report = tree.scan();
        assert!(report.errors.is_empty());
        assert!(!report.declarations[0].reachable_linux);
        assert!(report.declarations[0].reachable_windows);
    }
    #[test]
    fn test_support_files_with_real_tests_must_be_included_by_an_actual_target() {
        let tree = Tree::new("");
        tree.put(
            "runtime/tests/private/support.rs",
            "#[test] fn worker_entry(){assert_eq!(3,3);}",
        );
        assert!(tree.scan().errors.iter().any(|e| e.contains("support.rs")));
        tree.put(
            "runtime/tests/integration.rs",
            "#[path=\"private/support.rs\"] mod support;",
        );
        assert!(tree.scan().errors.is_empty());
        tree.put(
            "runtime/tests/private/template.rs.in",
            "#[test] fn another_entry(){assert_eq!(4,4);}",
        );
        assert!(
            tree.scan()
                .errors
                .iter()
                .any(|e| e.contains("template.rs.in"))
        );
        tree.put("runtime/tests/integration.rs", "#[path=\"private/support.rs\"] mod support; #[path=\"private/template.rs.in\"] mod template;");
        assert!(tree.scan().errors.is_empty());
    }
    #[test]
    fn platform_retirement_requires_exact_declaration_hash_and_a_real_unsupported_predicate() {
        let tree = Tree::new("");
        tree.put(
            "runtime/src/mac_tests.rs",
            "#[cfg(target_os=\"macos\")] #[test] fn retired(){assert_eq!(1,1);}",
        );
        let inventory = crate::ast::collect(&tree.0);
        let initial = tree.scan();
        assert_eq!(initial.errors.len(), 1);
        let declaration = &initial.declarations[0];
        let mut retirements = Retirements {
            schema: 1,
            cases: vec![Retirement {
                file: declaration.file.clone(),
                function: declaration.function.clone(),
                body_sha256: declaration.body_sha256.clone(),
                platform: "macos".into(),
                reason: "Approved Linux/Windows scope excludes macOS".into(),
            }],
        };
        assert!(check(&tree.0, &inventory, &retirements).errors.is_empty());
        retirements.cases[0].body_sha256 = "0".repeat(64);
        assert!(!check(&tree.0, &inventory, &retirements).errors.is_empty());
        tree.put(
            "runtime/src/mac_tests.rs",
            "#[test] fn retired(){assert_eq!(1,1);}",
        );
        let current = tree.scan();
        retirements.cases[0].body_sha256 = current.declarations[0].body_sha256.clone();
        assert!(
            !check(&tree.0, &crate::ast::collect(&tree.0), &retirements)
                .errors
                .is_empty()
        );
    }
    #[test]
    fn ordinary_cargo_run_has_no_global_runner_but_explicit_test_configuration_is_allowed() {
        let tree = Tree::new("");
        tree.put(
            ".cargo/runtime-tests.toml",
            "[target.'cfg(unix)']\nrunner=\"runtime/target/release/numera-runtime-tests\"\n",
        );
        assert!(tree.scan().errors.is_empty());
        tree.put(
            ".cargo/config.toml",
            "[target.'cfg(unix)']\nrunner = \"runtime/target/release/numera-runtime-tests\"\n",
        );
        assert!(
            tree.scan()
                .errors
                .iter()
                .any(|e| e.contains("ordinary Cargo run"))
        );
        tree.put(
            ".cargo/config.toml",
            "target.x86_64-unknown-linux-gnu.runner=[\"runner\"]\n",
        );
        assert!(
            tree.scan()
                .errors
                .iter()
                .any(|e| e.contains("ordinary Cargo run"))
        );
        for config in [
            "[target]\n'cfg(unix)' = { 'runner' = ['runner'] }\n",
            "\"target\".\"cfg(windows)\".\"runner\" = \"runner\"\n",
            "target = { x86_64-unknown-linux-gnu = { runner = 'runner' } }\n",
            "[target.'cfg(unix)']\nrunner = [\n 'runner',\n '--isolate'\n]\n",
        ] {
            tree.put(".cargo/config.toml", config);
            assert!(
                tree.scan()
                    .errors
                    .iter()
                    .any(|e| e.contains("ordinary Cargo run")),
                "{config}"
            );
        }
        tree.put(
            ".cargo/config.toml",
            "# runner = 'historical example'\n[alias]\nchecked=\"test --config .cargo/runtime-tests.toml\"\nnote=\"runner = inert example text\"\n",
        );
        assert!(tree.scan().errors.is_empty());
    }
}
