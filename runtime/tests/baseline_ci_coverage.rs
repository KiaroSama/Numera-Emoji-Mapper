//! The original nine CI ownership guards, adapted to automatic Rust/Playwright discovery.
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}
fn workflow() -> String {
    std::fs::read_to_string(root().join(".github/workflows/ci.yml")).unwrap()
}
fn job<'a>(text: &'a str, name: &str) -> &'a str {
    let heading = format!("\n  {name}:\n");
    let body = text
        .split_once(&heading)
        .unwrap_or_else(|| panic!("missing CI job {name}"))
        .1;
    let end = body
        .lines()
        .scan(0usize, |offset, line| {
            let start = *offset;
            *offset += line.len() + 1;
            Some((start, line))
        })
        .find(|(_, line)| {
            line.starts_with("  ") && !line.starts_with("   ") && !line.trim().is_empty()
        })
        .map_or(body.len(), |(offset, _)| offset);
    &body[..end]
}
fn step_run(text: &str, name: &str) -> String {
    let heading = format!("      - name: {name}\n");
    let body = text
        .split_once(&heading)
        .unwrap_or_else(|| panic!("missing CI step {name}"))
        .1;
    let end = body.find("\n      - ").unwrap_or(body.len());
    let step = &body[..end];
    let run = step
        .lines()
        .position(|line| line.trim_start().starts_with("run:"))
        .expect("step has no run command");
    let lines = step.lines().collect::<Vec<_>>();
    let first = lines[run].trim().strip_prefix("run:").unwrap().trim();
    if first == "|" {
        lines[run + 1..]
            .iter()
            .map(|line| line.trim_start())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        first.to_owned()
    }
}
fn browser_files() -> BTreeSet<String> {
    std::fs::read_dir(root().join("assets/tests"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(".spec.js"))
        })
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
        .collect()
}
fn browser_discovery(command: &str) -> bool {
    command == "npm test -- --reporter=line,json"
}
fn full_rust_discovery(command: &str) -> bool {
    command.trim()
        == "mkdir -p logs/checkpoint\ncargo test --config .cargo/runtime-tests.toml --manifest-path runtime/Cargo.toml --locked --release --no-fail-fast -- --test-threads=1 2>&1 | tee logs/checkpoint/runtime-tests.log"
}
fn rust_sources(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
struct WindowsCases(BTreeSet<String>);
impl<'ast> syn::visit::Visit<'ast> for WindowsCases {
    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        if function
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("test"))
        {
            struct Marked(bool);
            impl<'ast> syn::visit::Visit<'ast> for Marked {
                fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
                    if let syn::Meta::List(list) = &attr.meta
                        && matches!(
                            list.path.get_ident().map(ToString::to_string).as_deref(),
                            Some("cfg" | "cfg_attr")
                        )
                        && list
                            .tokens
                            .to_string()
                            .split(|c: char| !c.is_alphanumeric() && c != '_')
                            .any(|word| word == "windows")
                    {
                        self.0 = true;
                    }
                }
            }
            let mut marked = Marked(false);
            syn::visit::Visit::visit_item_fn(&mut marked, function);
            if marked.0 {
                self.0.insert(function.sig.ident.to_string());
            }
        }
        syn::visit::visit_item_fn(self, function);
    }
}
fn windows_cases() -> BTreeSet<String> {
    use syn::visit::Visit;
    let mut files = vec![];
    rust_sources(&root().join("runtime/tests"), &mut files);
    rust_sources(&root().join("runtime/src"), &mut files);
    let mut cases = WindowsCases(BTreeSet::new());
    for path in files {
        cases.visit_file(&syn::parse_file(&std::fs::read_to_string(path).unwrap()).unwrap());
    }
    cases.0
}
// tests.test_ci_coverage.EveryBrowserSuiteIsClaimedByCi.test_the_scan_finds_the_browser_suites
#[test]
fn browser_discovery_is_not_vacuous() {
    let files = browser_files();
    assert!(files.len() >= 2);
    let map: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("assets/tests/case-map.json")).unwrap(),
    )
    .unwrap();
    let entries = map.as_array().unwrap();
    assert_eq!(entries.len(), 156);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| matches!(
                entry["type"].as_str(),
                Some("browser" | "browser-performance")
            ))
            .count(),
        97
    );
    for entry in entries {
        assert!(files.contains(entry["test_file"].as_str().unwrap()));
    }
}
// tests.test_ci_coverage.EveryBrowserSuiteIsClaimedByCi.test_each_one_is_named_in_the_browser_job
#[test]
fn browser_job_claims_every_discovered_spec_automatically() {
    let text = workflow();
    assert!(browser_discovery(&step_run(
        job(&text, "panel-browser"),
        "Panel browser tests"
    )));
    let package: Value =
        serde_json::from_str(&std::fs::read_to_string(root().join("assets/package.json")).unwrap())
            .unwrap();
    assert_eq!(package["scripts"]["test"], "playwright test");
    let config = std::fs::read_to_string(root().join("assets/playwright.config.js")).unwrap();
    assert!(config.contains("testDir: './tests'"));
    assert!(!config.contains("testMatch:"));
    assert!(!config.contains("testIgnore:"));
    for filtered in [
        "npm test -- panel_browser",
        "npm test -- --grep happy",
        "npm run test:contracts",
    ] {
        assert!(!browser_discovery(filtered));
    }
}
// tests.test_ci_coverage.EveryBrowserSuiteIsClaimedByCi.test_the_matrix_opts_out_explicitly
#[test]
fn native_matrix_and_browser_job_have_explicit_separate_discovery() {
    let text = workflow();
    let native = job(&text, "native-build");
    let browser = job(&text, "panel-browser");
    assert!(full_rust_discovery(&step_run(
        native,
        "All Rust tests through the isolated runner"
    )));
    assert!(!native.contains("npm test"));
    assert!(browser.contains("needs: native-build"));
    assert!(browser.contains("name: rust-build-linux-x64"));
}
// tests.test_ci_coverage.EveryBrowserSuiteIsClaimedByCi.test_no_browser_suite_is_run_twice
#[test]
fn browser_specs_run_once_in_their_own_job() {
    let text = workflow();
    assert_eq!(
        text.matches("run: npm test -- --reporter=line,json")
            .count(),
        1
    );
    assert_eq!(
        text.matches("      - name: Panel browser tests\n").count(),
        1
    );
    assert!(browser_files().len() >= 2);
    assert!(!job(&text, "native-build").contains("playwright test"));
}
// tests.test_ci_coverage.EveryNativeWindowsSuiteIsClaimedByCi.test_the_job_exists
#[test]
fn the_native_windows_job_exists() {
    let text = workflow();
    let native = job(&text, "native-build");
    assert!(native.contains("os: windows-2025"));
    assert!(native.contains("platform: windows-x64"));
    assert!(native.contains("runs-on: ${{ matrix.os }}"));
    assert!(!native.contains("if: runner.os == 'Linux'\n    steps:"));
}
// tests.test_ci_coverage.EveryNativeWindowsSuiteIsClaimedByCi.test_the_scan_finds_the_marked_suites
#[test]
fn windows_case_scan_has_actual_platform_branches() {
    let cases = windows_cases();
    assert!(
        cases.len() >= 5,
        "Windows-case scan became vacuous: {cases:?}"
    );
    assert!(cases.contains("atomic_a_transient_windows_refusal_is_retried_to_completion"));
    assert!(cases.contains("atomic_a_persistent_refusal_still_fails_and_cleans_up"));
}
// tests.test_ci_coverage.EveryNativeWindowsSuiteIsClaimedByCi.test_each_marked_suite_is_named_in_the_job
#[test]
fn windows_automatically_runs_all_cargo_targets() {
    let text = workflow();
    let native = job(&text, "native-build");
    assert!(native.contains("platform: windows-x64"));
    assert!(full_rust_discovery(&step_run(
        native,
        "All Rust tests through the isolated runner"
    )));
    assert!(windows_cases().len() >= 5);
    for filtered in [
        "cargo test --manifest-path runtime/Cargo.toml --test locks",
        "cargo test --manifest-path runtime/Cargo.toml --no-run",
        "cargo test --manifest-path runtime/Cargo.toml --lib",
    ] {
        assert!(!full_rust_discovery(filtered));
    }
}
// tests.test_ci_coverage.EveryNativeWindowsSuiteIsClaimedByCi.test_a_windows_only_skip_needs_the_marker
#[test]
fn platform_gated_cases_have_a_real_windows_execution_bucket() {
    // Unfiltered Cargo discovery on both OSes replaces the handwritten module
    // ownership marker; a newly added Windows branch is covered automatically.
    let text = workflow();
    let native = job(&text, "native-build");
    let command = step_run(native, "All Rust tests through the isolated runner");
    assert!(full_rust_discovery(&command));
    assert!(native.contains("platform: linux-x64"));
    assert!(native.contains("platform: windows-x64"));
    let config = std::fs::read_to_string(root().join(".cargo/runtime-tests.toml")).unwrap();
    assert!(config.contains("[target.'cfg(windows)']"));
    assert!(config.contains("numera-runtime-tests.exe"));
    assert!(windows_cases().len() >= 5);
}
// tests.test_ci_coverage.EveryNativeWindowsSuiteIsClaimedByCi.test_every_name_in_the_job_still_claims_windows
#[test]
fn windows_job_has_no_stale_handwritten_suite_list() {
    let text = workflow();
    let native = job(&text, "native-build");
    let command = step_run(native, "All Rust tests through the isolated runner");
    assert!(full_rust_discovery(&command));
    let manifest = std::fs::read_to_string(root().join("runtime/Cargo.toml")).unwrap();
    for line in manifest
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
    {
        if let Some((key, value)) = line.split_once('=') {
            assert!(
                !(["autotests", "autolib", "autobins", "test", "doctest"].contains(&key.trim())
                    && value.split('#').next().unwrap().trim() == "false"),
                "Cargo manifest disables discovery: {line}"
            );
        }
    }
    for mutation in [
        format!("{command} --test obsolete"),
        format!("{command} --exclude new_suite"),
        format!("{command} --no-run"),
        format!("{command} no_such_case"),
        format!("{command} --list"),
        format!("{command} --exact no_such_case"),
        command.replace(" -- --test-threads=1", " -- no_such_case --test-threads=1"),
        command.replace(" -- --test-threads=1", " -- --list --test-threads=1"),
        command.replace(
            " -- --test-threads=1",
            " -- --exact no_such_case --test-threads=1",
        ),
    ] {
        assert!(!full_rust_discovery(&mutation));
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
