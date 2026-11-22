//! Retained native CI policy tails; original application identities stay separate.
use std::path::{Path, PathBuf};
use syn::visit::Visit;

#[path = "support/test_admission.rs"]
mod test_admission;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

#[test]
fn private_namespace_profiles_follow_both_configured_rust_test_runners() {
    let action =
        std::fs::read_to_string(root().join(".github/actions/test-network/action.yml")).unwrap();
    assert!(
        !root().join(".cargo/config.toml").exists(),
        "test isolation must not change ordinary Cargo run"
    );
    let config = std::fs::read_to_string(root().join(".cargo/runtime-tests.toml")).unwrap();
    let mut runners = 0;
    for line in config
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("runner ="))
    {
        let value = line.strip_prefix("runner =").unwrap().trim();
        assert!(
            value.starts_with('"') && value.ends_with('"'),
            "runner must be scalar, avoiding Cargo array concatenation"
        );
        let runner = value.trim_matches('"');
        assert!(matches!(
            runner,
            "runtime/target/release/numera-runtime-tests"
                | "runtime/target/release/numera-runtime-tests.exe"
        ));
        let name = runner.rsplit('/').next().unwrap().trim_end_matches(".exe");
        assert!(action.contains(&format!("\"$GITHUB_WORKSPACE/**/{name}\"")));
        runners += 1;
    }
    assert_eq!(
        runners, 2,
        "both supported platform configurations need the guard"
    );
    assert!(config.contains("[target.'cfg(windows)']"));
    assert!(config.contains("[target.'cfg(unix)']"));
}

fn module_file(name: &str) -> PathBuf {
    let directory = root().join("runtime/src");
    let library =
        syn::parse_file(&std::fs::read_to_string(directory.join("lib.rs")).unwrap()).unwrap();
    let module = library
        .items
        .iter()
        .find_map(|item| {
            let syn::Item::Mod(module) = item else {
                return None;
            };
            (module.ident == name).then_some(module)
        })
        .unwrap_or_else(|| panic!("missing actual production module {name}"));
    for attr in &module.attrs {
        if attr.path().is_ident("path") {
            let syn::Meta::NameValue(value) = &attr.meta else {
                panic!("invalid module path");
            };
            let syn::Expr::Lit(value) = &value.value else {
                panic!("nonliteral module path");
            };
            let syn::Lit::Str(value) = &value.lit else {
                panic!("nonstring module path");
            };
            return directory.join(value.value());
        }
    }
    directory.join(format!("{name}.rs"))
}

#[derive(Default)]
struct LoggerCalls {
    opens: usize,
    hooks: usize,
}
impl<'ast> Visit<'ast> for LoggerCalls {
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = call.func.as_ref() {
            let names = path
                .path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect::<Vec<_>>();
            if names.len() >= 2 && names[names.len() - 2..] == ["RunLog", "open"] {
                self.opens += 1;
            }
        }
        syn::visit::visit_expr_call(self, call);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == "install_panic_hook" {
            self.hooks += 1;
        }
        syn::visit::visit_expr_method_call(self, call);
    }
}
fn logger_policy(source: &str) -> Result<(), &'static str> {
    let parsed = syn::parse_file(source).map_err(|_| "invalid production Rust")?;
    let functions = parsed
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Fn(function) if function.sig.ident == "run" => Some(function),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [function] = functions.as_slice() else {
        return Err("expected one actual command entry");
    };
    let mut calls = LoggerCalls::default();
    calls.visit_block(&function.block);
    if calls.opens == 0 || calls.opens != calls.hooks {
        return Err("every command log owner needs panic capture");
    }
    Ok(())
}

#[test]
fn executable_log_owners_install_panic_capture_without_changing_extension_imports() {
    for module in [
        "entry",
        "codec_worker",
        "developer",
        "tooling_doctor",
        "tooling_menu_cli",
    ] {
        let source = std::fs::read_to_string(module_file(module)).unwrap();
        logger_policy(&source).unwrap_or_else(|error| panic!("{module}: {error}"));
        assert!(
            logger_policy(&source.replace("log.install_panic_hook();", "")).is_err(),
            "missing-hook negative must affect the actual entry"
        );
    }
    let mut library = LoggerCalls::default();
    library.visit_file(
        &syn::parse_file(&std::fs::read_to_string(root().join("runtime/src/lib.rs")).unwrap())
            .unwrap(),
    );
    assert_eq!(
        library.hooks, 0,
        "loading the production library must not replace a caller's panic hook"
    );
    assert!(logger_policy("fn run(){ let log=RunLog::open(); log.install_panic_hook(); }").is_ok());
    assert!(
        logger_policy("fn run(){ let log=RunLog::open(); /* log.install_panic_hook(); */ }")
            .is_err()
    );
    assert!(logger_policy("fn run(){}").is_err());
}

// Migration worktree native CI input SHA256: 0b498289373e796ca3da5247a72082fee38d84c09f58fda818f514754e4db891.
// Its security workflow assertions had already been adapted to Rust tools/ci.
pub fn step_run(workflow: &str, name: &str) -> Result<String, &'static str> {
    read_step(workflow, name, false)
}

fn required_step_run(workflow: &str, name: &str) -> Result<String, &'static str> {
    read_step(workflow, name, true)
}

fn read_step(workflow: &str, name: &str, required: bool) -> Result<String, &'static str> {
    let heading = format!("- name: {name}");
    let lines = workflow.lines().collect::<Vec<_>>();
    let starts = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.trim() == heading)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if starts.len() != 1 {
        return Err("CI step must exist exactly once");
    }
    let start = starts[0];
    let indentation = lines[start].len() - lines[start].trim_start().len();
    let end = (start + 1..lines.len())
        .find(|index| {
            let line = lines[*index];
            !line.trim().is_empty()
                && !line.trim().starts_with('#')
                && line.len() - line.trim_start().len() <= indentation
        })
        .unwrap_or(lines.len());
    let block = &lines[start + 1..end];
    if required {
        for line in block.iter().filter(|line| {
            !line.trim().is_empty()
                && !line.trim_start().starts_with('#')
                && line.len() - line.trim_start().len() == indentation + 2
        }) {
            let (key, value) = line
                .trim()
                .split_once(':')
                .ok_or("unsupported required-step metadata")?;
            let key = key.trim().trim_matches(['\'', '"']);
            match key {
                "if" => return Err("required execution step must be unconditional"),
                "continue-on-error" if value.split('#').next().unwrap().trim() == "false" => {}
                "continue-on-error" => return Err("required execution failure must fail the job"),
                "name" | "id" | "shell" | "working-directory" | "env" | "with"
                | "timeout-minutes" | "run" | "uses" => {}
                _ => return Err("unsupported required-step metadata"),
            }
        }
    }
    let run = block
        .iter()
        .position(|line| line.trim_start().starts_with("run:"))
        .ok_or("CI step has no run command")?;
    let line = block[run];
    let value = line.trim_start().strip_prefix("run:").unwrap().trim();
    if !matches!(value, "|" | ">-" | ">") {
        return Ok(value.into());
    }
    let run_indent = line.len() - line.trim_start().len();
    Ok(block[run + 1..]
        .iter()
        .take_while(|line| {
            line.trim().is_empty() || line.len() - line.trim_start().len() > run_indent
        })
        .map(|line| line.trim())
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn initiating_events(workflow: &str, expected: &[&str]) -> Result<(), &'static str> {
    let lines = workflow.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| *line == "on:")
        .ok_or("missing events")?;
    let mut events = Vec::new();
    for line in &lines[start + 1..] {
        if !line.trim().is_empty() && !line.starts_with(' ') && !line.starts_with('#') {
            break;
        }
        if line.starts_with("  ") && !line.starts_with("   ") && !line.trim().starts_with('#') {
            events.push(line.trim());
        }
    }
    events.sort_unstable();
    if events.as_slice() != expected {
        return Err("workflow triggers must preserve their build, security or manual policy");
    }
    Ok(())
}

const BUILD_EVENTS: &[&str] = &["pull_request:", "push:", "workflow_dispatch:"];
const REUSABLE_BUILD_EVENTS: &[&str] = &[
    "pull_request:",
    "push:",
    "workflow_call:",
    "workflow_dispatch:",
];
const MANUAL_EVENTS: &[&str] = &["workflow_dispatch:"];
const SECURITY_EVENTS: &[&str] = &[
    "pull_request:",
    "push:",
    "schedule:",
    "workflow_call:",
    "workflow_dispatch:",
];

#[test]
fn secret_scan_triage_cannot_hide_future_matches_or_disclose_values() {
    // Actual triage, summary privacy, synthetic Git provenance and bounded-process
    // behaviors moved from embedded Python to the always-run Rust tools/ci tests.
    // This original CI guard retains ownership of their event and execution policy.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let workflow = include_str!("../../.github/workflows/gitleaks.yml");
    assert!(!workflow.contains("python3"));
    assert!(!workflow.contains("<<'PYTHON'"));
    let tests = workflow
        .split_once("\n  security-tool-tests:")
        .unwrap()
        .1
        .split_once("\n  gitleaks:")
        .unwrap()
        .0;
    let policy = tests.split_once("    steps:").unwrap().0;
    assert!(
        !policy
            .lines()
            .any(|line| line.trim_start().starts_with("if:")),
        "security regression behavior must run for every inherited workflow event"
    );
    assert_eq!(
        required_step_run(tests, "Rust security tooling behavior tests").unwrap(),
        "cargo test --config .cargo/runtime-tests.toml --manifest-path tools/ci/Cargo.toml --locked"
    );
    let scanner = workflow.split_once("\n  gitleaks:").unwrap().1;
    assert!(scanner.contains("needs: security-tool-tests"));
    assert_eq!(
        required_step_run(scanner, "Scan Git history with the verified Gitleaks CLI").unwrap(),
        "cargo run --manifest-path tools/ci/Cargo.toml --locked --bin gitleaks"
    );
    let implementation = format!(
        "{}\n{}",
        std::fs::read_to_string(root.join("tools/ci/src/lib.rs")).unwrap(),
        std::fs::read_to_string(root.join("tools/ci/src/tests.rs")).unwrap()
    );
    for case in [
        "exact_reviewed_identities_only",
        "malformed_or_incomplete_metadata_cannot_claim_reviewed_identity",
        "merge_origin_requires_valid_single_line_metadata_and_exact_reviewed_origin",
        "summary_escapes_metadata_and_discards_secret_bearing_fields",
        "scanner_status_output_git_status_and_report_must_agree",
        "archive_extraction_selects_only_one_regular_expected_member",
        "real_git_proves_only_unchanged_merge_line_origins",
        "private_child_output_and_process_tree_are_bounded",
    ] {
        assert!(
            implementation.contains(&format!("fn {case}()")),
            "relocated security regression is missing: {case}"
        );
    }
}

#[test]
fn every_ci_and_package_workflow_requires_push_pull_request_and_manual_dispatch() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut count = 0;
    for entry in std::fs::read_dir(root.join(".github/workflows")).unwrap() {
        let path = entry.unwrap().path();
        if matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("yml" | "yaml")
        ) {
            let text = std::fs::read_to_string(&path).unwrap();
            let name = path.file_name().and_then(|name| name.to_str());
            let automatic = matches!(name, Some("ci.yml" | "release.yml"));
            // The inherited security scan also runs on a schedule. Build and
            // checkpoint workflows retain their existing, stricter event sets.
            let expected = match name {
                Some("ci.yml") => REUSABLE_BUILD_EVENTS,
                Some("release.yml") => BUILD_EVENTS,
                Some("gitleaks.yml") => SECURITY_EVENTS,
                _ => MANUAL_EVENTS,
            };
            initiating_events(&text, expected)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            if automatic {
                assert!(
                    text.lines().any(|line| {
                        line.trim_start().starts_with("group:")
                            && line.contains("github.event_name")
                    }),
                    "{}: push and pull-request runs must not cancel each other",
                    path.display()
                );
            }
            if name == Some("gitleaks.yml") {
                assert!(
                    text.lines().any(|line| {
                        line.trim_start().starts_with("group:")
                            && line.contains("github.ref")
                            && line.contains("github.event_name")
                    }),
                    "security runs must retain event- and ref-scoped concurrency groups"
                );
            }
            count += 1;
        }
    }
    assert!(
        count >= 4,
        "workflow discovery must not silently match nothing"
    );
    let valid = "on:\n  push:\n  pull_request:\n  workflow_dispatch:\njobs:\n";
    assert!(initiating_events(valid, BUILD_EVENTS).is_ok());
    for event in ["push:", "pull_request:", "workflow_dispatch:"] {
        assert!(
            initiating_events(&valid.replace(&format!("  {event}\n"), ""), BUILD_EVENTS).is_err()
        );
    }
    for event in [
        "pull_request_target:",
        "schedule:",
        "workflow_run:",
        "push:",
    ] {
        assert!(
            initiating_events(
                &valid.replace("jobs:", &format!("  {event}\njobs:")),
                BUILD_EVENTS
            )
            .is_err()
        );
    }
    assert!(
        initiating_events(
            "on:\n  # workflow_dispatch:\n  push:\njobs:\n",
            BUILD_EVENTS
        )
        .is_err()
    );
    let reusable = valid.replace("jobs:", "  workflow_call:\njobs:");
    assert!(initiating_events(&reusable, REUSABLE_BUILD_EVENTS).is_ok());
    assert!(initiating_events(valid, REUSABLE_BUILD_EVENTS).is_err());
    assert!(initiating_events(&reusable, BUILD_EVENTS).is_err());
    for event in ["pull_request_target:", "workflow_run:", "workflow_call:"] {
        assert!(
            initiating_events(
                &reusable.replace("jobs:", &format!("  {event}\njobs:")),
                REUSABLE_BUILD_EVENTS
            )
            .is_err()
        );
    }
    let security = reusable.replace("jobs:", "  schedule:\n    - cron: '21 4 * * 4'\njobs:");
    assert!(initiating_events(&security, SECURITY_EVENTS).is_ok());
    assert!(initiating_events(&security, BUILD_EVENTS).is_err());
    assert!(initiating_events(valid, SECURITY_EVENTS).is_err());
    for event in SECURITY_EVENTS {
        assert!(
            initiating_events(
                &security.replace(&format!("  {event}\n"), ""),
                SECURITY_EVENTS
            )
            .is_err()
        );
    }
    for event in ["pull_request_target:", "workflow_run:", "schedule:"] {
        assert!(
            initiating_events(
                &security.replace("jobs:", &format!("  {event}\njobs:")),
                SECURITY_EVENTS
            )
            .is_err()
        );
    }
}

fn release_acceptance(workflow: &str) -> Result<(), &'static str> {
    for (name, field, expected) in [
        ("acceptance", "uses", "./.github/workflows/ci.yml"),
        (
            "secret-acceptance",
            "uses",
            "./.github/workflows/gitleaks.yml",
        ),
        (
            "signing",
            "needs",
            "[package, acceptance, secret-acceptance]",
        ),
        ("publish", "needs", "signing"),
    ] {
        let heading = format!("\n  {name}:\n");
        let (_, remaining) = workflow.split_once(&heading).ok_or("missing release job")?;
        let block = remaining
            .lines()
            .take_while(|line| {
                line.trim().is_empty() || !line.starts_with("  ") || line.starts_with("   ")
            })
            .collect::<Vec<_>>();
        let property = format!("    {field}: {expected}");
        if block.iter().filter(|line| **line == property).count() != 1 {
            return Err("release job lost its required acceptance dependency");
        }
        if block.iter().any(|line| {
            line.trim_start().starts_with("continue-on-error:")
                || line.starts_with("    if:") && line.contains("always()")
        }) {
            return Err("release acceptance failures must block signing and publication");
        }
        if field == "uses" && !block.contains(&"      release_acceptance: true") {
            return Err("reused workflow must enforce release acceptance");
        }
    }
    Ok(())
}

#[test]
fn signing_and_publication_require_complete_ci_and_secret_acceptance() {
    let release = include_str!("../../.github/workflows/release.yml");
    assert!(release_acceptance(release).is_ok());
    for (old, new) in [
        (
            "needs: [package, acceptance, secret-acceptance]",
            "needs: package",
        ),
        ("needs: signing", "needs: package"),
        (
            "./.github/workflows/ci.yml",
            "./.github/workflows/release.yml",
        ),
        ("release_acceptance: true", "release_acceptance: false"),
        ("  signing:\n", "  signing:\n    continue-on-error: true\n"),
    ] {
        assert!(
            release_acceptance(&release.replace(old, new)).is_err(),
            "{old}"
        );
    }
    let security = include_str!("../../.github/workflows/gitleaks.yml");
    assert!(security.contains("if: inputs.release_acceptance || github.event_name != 'push'"));
    for source in [include_str!("../../.github/workflows/ci.yml"), security] {
        assert!(source.contains("inputs.release_acceptance && 'release' || 'standalone'"));
    }
}

#[test]
fn standalone_runtime_builds_and_packages_on_every_retained_platform_without_python() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let workflow = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let production = workflow
        .split_once("\n  native-build:")
        .unwrap()
        .1
        .split_once("\n  worker:")
        .unwrap()
        .0;
    for platform in ["linux-x64", "linux-arm64", "windows-x64", "windows-arm64"] {
        assert!(production.contains(&format!("platform: {platform}")));
    }
    let platforms = production
        .lines()
        .filter_map(|line| line.trim().strip_prefix("platform: "))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        platforms,
        std::collections::BTreeSet::from([
            "linux-arm64",
            "linux-x64",
            "windows-arm64",
            "windows-x64",
        ])
    );
    assert!(!production.contains("platform: linux-x86"));
    assert!(!production.contains("platform: windows-x86"));
    assert!(!production.contains("setup-python"));
    assert!(!production.contains("PYO3_PYTHON"));
    let compiler = required_step_run(production, "Select Rust compiler").unwrap();
    assert!(
        compiler
            .lines()
            .any(|line| line == "rustup default 1.100.0")
    );
    assert!(
        compiler
            .lines()
            .any(|line| line == "rustup component add rustfmt clippy --toolchain 1.100.0")
    );
    assert!(
        production.find("name: Select Rust compiler").unwrap()
            < production.find("name: Rust format and lint").unwrap()
    );
    for prerequisite in [
        "Install Linux video prerequisites",
        "Install Windows video prerequisites",
    ] {
        assert!(step_run(production, prerequisite).is_ok());
    }
    let video = required_step_run(production, "Require actual video tools").unwrap();
    assert!(video.lines().any(|line| line == "ffmpeg -version"));
    assert!(video.lines().any(|line| line == "ffprobe -version"));
    let build = required_step_run(production, "Build standalone release executables").unwrap();
    assert_eq!(
        build,
        "cargo build --manifest-path runtime/Cargo.toml --locked --release --bins"
    );
    let tests =
        required_step_run(production, "All Rust tests through the isolated runner").unwrap();
    assert_eq!(
        tests,
        "mkdir -p logs/checkpoint\ncargo test --config .cargo/runtime-tests.toml --manifest-path runtime/Cargo.toml --locked --release --no-fail-fast -- --test-threads=1 2>&1 | tee logs/checkpoint/runtime-tests.log"
    );
    let runner = std::fs::read_to_string(root.join(".cargo/runtime-tests.toml")).unwrap();
    assert!(runner.contains("runtime/target/release/numera-runtime-tests.exe"));
    assert!(runner.contains("runtime/target/release/numera-runtime-tests\""));
    let helper = std::fs::read_to_string(root.join("runtime/src/test_main.rs")).unwrap();
    for guard in [
        "OwnedProcess::spawn_descendants",
        "network::run",
        "Duration::from_secs(900)",
        "Duration::from_secs(180)",
        ".env_clear()",
        "8 * 1024 * 1024",
        "tooling_environment::physical_root(root)",
        "migration_bundle::resolved(Path::new(executable))",
    ] {
        assert!(
            helper.contains(guard),
            "standalone runtime guard missing: {guard}"
        );
    }
    let release = std::fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    let package = release
        .split_once("\n  package:")
        .unwrap()
        .1
        .split_once("\n  publish:")
        .unwrap()
        .0;
    let platforms = package
        .lines()
        .filter_map(|line| line.trim().strip_prefix("platform: "))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        platforms,
        std::collections::BTreeSet::from([
            "linux-arm64",
            "linux-x64",
            "windows-arm64",
            "windows-x64",
        ])
    );
    assert!(!package.contains("platform: linux-x86"));
    assert!(!package.contains("platform: windows-x86"));
    assert!(!package.contains("setup-python"));
    assert!(!package.contains("maturin"));
    assert!(!package.contains(".venv"));
    for platform in ["linux-x64", "linux-arm64", "windows-x64", "windows-arm64"] {
        assert!(package.contains(&format!("platform: {platform}")));
    }
    assert!(
        required_step_run(
            package,
            "Offline package integrity, replacement and native conversion"
        )
        .unwrap()
        .contains("--release --test standalone_package")
    );
    assert!(!release.contains("\n  compatibility:"));
}

fn full_discovery(run: &str, manifest: &str) -> Result<(), &'static str> {
    if run.trim() != FULL_NATIVE {
        return Err("native acceptance must run unfiltered Cargo test discovery");
    }
    for line in manifest
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
    {
        if let Some((key, value)) = line.split_once('=')
            && matches!(
                key.trim(),
                "autotests" | "autolib" | "autobins" | "test" | "doctest"
            )
            && value.split('#').next().unwrap_or("").trim() == "false"
        {
            return Err("manifest disables default test coverage");
        }
    }
    Ok(())
}

const FULL_NATIVE: &str = "mkdir -p logs/checkpoint\ncargo test --config .cargo/runtime-tests.toml --manifest-path runtime/Cargo.toml --locked --release --no-fail-fast -- --test-threads=1 2>&1 | tee logs/checkpoint/runtime-tests.log";

#[test]
fn coverage_guard_rejects_comments_disabled_discovery_and_compile_only_commands() {
    let workflow = std::fs::read_to_string(root().join(".github/workflows/ci.yml")).unwrap();
    let manifest = std::fs::read_to_string(root().join("runtime/Cargo.toml")).unwrap();
    let actual =
        required_step_run(&workflow, "All Rust tests through the isolated runner").unwrap();
    assert!(full_discovery(&actual, &manifest).is_ok());
    assert!(full_discovery(FULL_NATIVE, "[package]\n").is_ok());
    assert!(full_discovery(&format!("# {FULL_NATIVE}"), "[package]\n").is_err());
    for command in [
        "cargo test --manifest-path runtime/Cargo.toml --locked --test codec_worker",
        "cargo test --manifest-path runtime/Cargo.toml --locked --no-run",
        "cargo test --manifest-path runtime/Cargo.toml --locked --lib",
    ] {
        assert!(full_discovery(command, "[package]\n").is_err());
    }
    for key in ["autotests", "autolib", "autobins", "test", "doctest"] {
        assert!(full_discovery(FULL_NATIVE, &format!("[package]\n{key} = false\n")).is_err());
    }
    let workflow = "jobs:\n  native:\n    steps:\n      - name: Checks\n        run: |\n          echo checked\n      - name: Elsewhere\n        run: cargo test\n";
    assert_eq!(step_run(workflow, "Checks").unwrap(), "echo checked");
    assert!(step_run(workflow, "Missing").is_err());
    let security = include_str!("../../.github/workflows/gitleaks.yml");
    let runtime = include_str!("../../.github/workflows/ci.yml");
    for (source, name) in [
        (security, "Rust security tooling behavior tests"),
        (runtime, "Build standalone release executables"),
        (runtime, "All Rust tests through the isolated runner"),
    ] {
        let positive = required_step_run(source, name).unwrap();
        let heading = format!("      - name: {name}\n");
        assert_eq!(source.matches(&heading).count(), 1);
        for field in [
            "if: false",
            "if : false",
            "'if': false",
            "\"if\" : false",
            "if: |\n          false",
            "continue-on-error: true",
            "'continue-on-error' : true",
            "continue-on-error: ${{ true }}",
        ] {
            let changed = source.replace(&heading, &format!("{heading}        {field}\n"));
            assert_eq!(step_run(&changed, name).unwrap(), positive);
            assert!(
                required_step_run(&changed, name).is_err(),
                "{name}: {field}"
            );
        }
        let explicit_false = source.replace(
            &heading,
            &format!("{heading}        continue-on-error: false\n"),
        );
        assert_eq!(required_step_run(&explicit_false, name).unwrap(), positive);
    }
}
