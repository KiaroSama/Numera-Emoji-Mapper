//! CI policy: automatic builds and Cargo discovery, not a stale list of native suite names.
use _native::source_ci_claims::step_run;
use std::path::Path;

const FULL_NATIVE: &str = "native/target/debug/numera-emoji test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked --no-fail-fast -- --test-threads=1";

fn initiating_events(workflow: &str, automatic: bool) -> Result<(), &'static str> {
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
    let expected = if automatic {
        vec!["pull_request:", "push:", "workflow_dispatch:"]
    } else {
        vec!["workflow_dispatch:"]
    };
    if events != expected {
        return Err("CI and package builds must run on every push and pull request");
    }
    Ok(())
}

fn full_discovery(run: &str, manifest: &str) -> Result<(), &'static str> {
    if !run.lines().any(|line| line.trim() == FULL_NATIVE) {
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
            let automatic = matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("ci.yml" | "release.yml")
            );
            initiating_events(&text, automatic)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            count += 1;
        }
    }
    assert!(
        count >= 3,
        "workflow discovery must not silently match nothing"
    );
    let valid = "on:\n  push:\n  pull_request:\n  workflow_dispatch:\njobs:\n";
    assert!(initiating_events(valid, true).is_ok());
    for event in ["push:", "pull_request:", "workflow_dispatch:"] {
        assert!(initiating_events(&valid.replace(&format!("  {event}\n"), ""), true).is_err());
    }
    for event in [
        "pull_request_target:",
        "schedule:",
        "workflow_run:",
        "push:",
    ] {
        assert!(
            initiating_events(&valid.replace("jobs:", &format!("  {event}\njobs:")), true).is_err()
        );
    }
    assert!(initiating_events("on:\n  # workflow_dispatch:\n  push:\njobs:\n", true).is_err());
}

#[test]
fn full_native_acceptance_automatically_claims_new_suites_and_the_fixture_worker() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let workflow = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let manifest = std::fs::read_to_string(root.join("native/Cargo.toml")).unwrap();
    let run = step_run(&workflow, "Rust format, lint and unit checks").unwrap();
    full_discovery(&run, &manifest).unwrap();
    let acceptance = workflow
        .split_once("\n  build:")
        .expect("primary native acceptance job")
        .1
        .split_once("\n  windows-safety:")
        .expect("Windows job follows primary acceptance")
        .0;
    let build = step_run(
        acceptance,
        "Build native backend executable for command parity",
    )
    .unwrap();
    assert!(build.contains("--lib --bin numera-emoji --bin numera-build"));
    assert!(run.lines().any(|line| {
        line.starts_with("native/target/debug/numera-emoji source-lottie-render --output-dir ")
    }));
    assert!(run.lines().any(|line| {
        line.starts_with("native/target/debug/numera-emoji source-static-images --output-dir ")
    }));
    assert!(!run.contains("python -m tests._lottie_preview_oracle"));
    assert!(!run.contains("python -m tests._static_convert_oracle"));
    assert!(run.lines().any(|line| {
        line.starts_with("rustc --edition=2024 native/tests/fixtures/media_worker.rs -o ")
    }));
    assert!(
        run.lines()
            .any(|line| line.starts_with("export NUMERA_MEDIA_FIXTURE_WORKER="))
    );
    // Collection imports original browser definitions but never starts their harness.
    let windows = workflow.split_once("\n  windows-safety:").unwrap().1;
    for (job, step) in [
        (acceptance, "Install dependencies"),
        (windows, "Install isolated test dependencies"),
    ] {
        let install = step_run(job, step).unwrap();
        assert!(
            install
                .split_whitespace()
                .any(|token| token == "requirements-dev.txt"),
            "original runtime collection needs the browser API package: {step}"
        );
        assert!(!install.contains("playwright install"));
    }
    let requirements = std::fs::read_to_string(root.join("requirements-dev.txt")).unwrap();
    let pin = requirements
        .lines()
        .find(|line| line.starts_with("playwright=="))
        .unwrap();
    let checkpoint =
        std::fs::read_to_string(root.join(".github/workflows/native-checkpoint.yml")).unwrap();
    let install = step_run(&checkpoint, "Install unchanged external static codec").unwrap();
    assert!(
        install
            .split_whitespace()
            .any(|token| token.trim_matches('"') == pin)
    );
    assert!(!install.contains("playwright install"));
    let suites = std::fs::read_dir(root.join("native/tests"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "rs"))
        .count();
    assert!(
        suites > 0,
        "Cargo suite discovery must not silently match nothing"
    );
}

#[test]
fn native_browser_acceptance_runs_once_in_its_browser_bucket() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let workflow = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let browser = workflow
        .split_once("\n  panel-browser:")
        .unwrap()
        .1
        .split_once("\n  build:")
        .unwrap()
        .0;
    assert!(!browser.contains("NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS:"));
    assert!(
        browser.contains("PLAYWRIGHT_BROWSERS_PATH: ${{ github.workspace }}/.cache/ms-playwright")
    );
    let run = step_run(browser, "Native panel browser tests").unwrap();
    assert_eq!(
        run,
        "native/target/debug/numera-emoji test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked --test panel_browser -- --test-threads=1 --nocapture"
    );
    assert_eq!(workflow.matches("--test panel_browser ").count(), 1);
    assert!(root.join("native/tests/panel_browser.rs").is_file());
    let cohort = std::fs::read_to_string(root.join("native/tests/panel_browser.rs")).unwrap();
    for responsibility in ["layout", "queues", "curation", "perf"] {
        assert!(cohort.contains(&format!("mod {responsibility};")));
        assert_eq!(
            cohort.matches(&format!("{responsibility}::run(")).count(),
            1,
            "browser assertions must be invoked, not merely declared: {responsibility}"
        );
    }
    let primary = workflow
        .split_once("\n  build:")
        .unwrap()
        .1
        .split_once("\n  windows-safety:")
        .unwrap()
        .0;
    assert!(primary.contains("NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS: \"1\""));
    let original = step_run(browser, "Panel browser tests").unwrap();
    for suite in [
        "tests.test_panel_browser",
        "tests.test_panel_queues",
        "tests.test_panel_curation",
        "tests.test_panel_perf",
    ] {
        assert_eq!(
            original
                .split_whitespace()
                .filter(|token| *token == suite)
                .count(),
            1
        );
    }
}

fn ordinary_module(library: &str, module: &str) -> usize {
    let mut previous = "";
    let mut found = 0;
    for line in library
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if (line == format!("pub mod {module};") || line == format!("mod {module};"))
            && !previous.starts_with("#[")
        {
            found += 1;
        }
        previous = line;
    }
    found
}

fn test_module_wiring(
    library: &str,
    module: &str,
    source: &str,
    parents: &[(&str, &str)],
) -> Result<(), &'static str> {
    let tests = source
        .lines()
        .map(str::trim)
        .any(|line| line == "#[test]" || line.starts_with("#[tokio::test"));
    if tests {
        let mut owners = library
            .matches(&format!("#[cfg(test)]\nmod {module};"))
            .count();
        for (parent, source) in parents {
            if ordinary_module(library, parent) != 1 {
                continue;
            }
            let lines = source.lines().map(str::trim).collect::<Vec<_>>();
            owners += lines
                .windows(3)
                .filter(|lines| {
                    lines[0] == "#[cfg(test)]"
                        && lines[1] == format!("#[path = \"{module}.rs\"]")
                        && lines[2]
                            .strip_prefix("mod ")
                            .and_then(|line| line.strip_suffix(';'))
                            .is_some_and(|name| {
                                !name.is_empty()
                                    && name
                                        .bytes()
                                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                            })
                })
                .count();
        }
        if owners != 1 {
            return Err("crate-private test module is not wired");
        }
        return Ok(());
    }
    let path = format!("#[path = \"{module}.rs\"]");
    let mut owners = ordinary_module(library, module);
    for (parent, source) in parents {
        if ordinary_module(library, parent) != 1 {
            continue;
        }
        let lines = source.lines().map(str::trim).collect::<Vec<_>>();
        owners += lines
            .windows(3)
            .filter(|lines| {
                lines[0] == "#[cfg(all(test, any(windows, target_os = \"linux\")))]"
                    && lines[1] == path
                    && lines[2].starts_with("pub(crate) mod ")
                    && lines[2].ends_with(';')
            })
            .count();
        owners += lines
            .windows(2)
            .filter(|pair| {
                pair[0] == path
                    && pair[1]
                        .strip_prefix("mod ")
                        .and_then(|value| value.strip_suffix(';'))
                        .is_some_and(|name| {
                            !name.is_empty()
                                && name
                                    .bytes()
                                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                        })
            })
            .count();
    }
    if owners != 1 {
        return Err("historical assertion helper needs exactly one ordinary library owner");
    }
    Ok(())
}

#[test]
fn test_module_discovery_distinguishes_assertion_replayers_without_hiding_native_tests() {
    let public = "pub mod source_example_tests;\n";
    let private = "#[cfg(test)]\nmod source_example_tests;\n";
    let helper = "pub fn observe() {}\n";
    let native = "#[test]\nfn observes() {}\n";
    let asynchronous = "#[tokio::test(flavor = \"current_thread\")]\nasync fn observes() {}\n";
    assert!(test_module_wiring(public, "source_example_tests", helper, &[]).is_ok());
    assert!(
        test_module_wiring(
            "mod source_example_tests;\n",
            "source_example_tests",
            helper,
            &[]
        )
        .is_ok()
    );
    assert!(test_module_wiring("", "source_example_tests", helper, &[]).is_err());
    assert!(test_module_wiring(private, "source_example_tests", helper, &[]).is_err());
    let parent = "#[path = \"source_example_tests.rs\"]\nmod examples;\n";
    let parents = [("source_parent", parent)];
    assert!(
        test_module_wiring(
            "pub mod source_parent;\n",
            "source_example_tests",
            helper,
            &parents
        )
        .is_ok()
    );
    for library in [
        "",
        "#[cfg(test)]\nmod source_parent;\n",
        "#[cfg(any())]\nmod source_parent;\n",
    ] {
        assert!(test_module_wiring(library, "source_example_tests", helper, &parents).is_err());
    }
    for changed in [
        "// #[path = \"source_example_tests.rs\"]\nmod examples;\n",
        "#[path = \"other.rs\"]\nmod examples;\n",
    ] {
        assert!(
            test_module_wiring(
                "pub mod source_parent;\n",
                "source_example_tests",
                helper,
                &[("source_parent", changed)]
            )
            .is_err()
        );
    }
    assert!(
        test_module_wiring(
            "pub mod source_parent;\nmod source_example_tests;\n",
            "source_example_tests",
            helper,
            &parents
        )
        .is_err()
    );
    assert!(
        test_module_wiring(
            "pub mod source_parent;\n",
            "source_example_tests",
            helper,
            &[("source_parent", parent), ("source_parent", parent)]
        )
        .is_err()
    );
    for source in [native, asynchronous] {
        let nested = format!("#[cfg(test)]\n{parent}");
        assert!(
            test_module_wiring(
                "pub mod source_parent;\n",
                "source_example_tests",
                source,
                &[("source_parent", &nested)]
            )
            .is_ok()
        );
        for invalid in [
            nested.replace("#[cfg(test)]", "// #[cfg(test)]"),
            nested.replace("#[cfg(test)]", "#[cfg(any())]"),
        ] {
            assert!(
                test_module_wiring(
                    "pub mod source_parent;\n",
                    "source_example_tests",
                    source,
                    &[("source_parent", &invalid)]
                )
                .is_err()
            );
        }
        assert!(test_module_wiring(public, "source_example_tests", source, &[]).is_err());
        assert!(test_module_wiring(private, "source_example_tests", source, &[]).is_ok());
        assert!(test_module_wiring("", "source_example_tests", source, &[]).is_err());
        assert!(
            test_module_wiring(
                "pub mod source_parent;\n",
                "source_example_tests",
                source,
                &parents
            )
            .is_err()
        );
    }
}

#[test]
fn crate_private_test_files_are_wired_into_the_unfiltered_library_target() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let library = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    let sources = std::fs::read_dir(root.join("src"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .map(|path| {
            (
                path.file_stem().unwrap().to_str().unwrap().to_owned(),
                std::fs::read_to_string(path).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let parents = sources
        .iter()
        .map(|(name, source)| (name.as_str(), source.as_str()))
        .collect::<Vec<_>>();
    let mut count = 0;
    for (module, source) in &sources {
        if !module.ends_with("_tests") {
            continue;
        }
        test_module_wiring(&library, module, source, &parents)
            .unwrap_or_else(|error| panic!("{error}: {module}"));
        count += 1;
    }
    assert!(
        count > 0,
        "private test discovery must not silently match nothing"
    );
}

#[test]
fn executable_log_owners_install_panic_capture_without_changing_extension_imports() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (file, owners) in [
        ("main.rs", 3),
        ("codec_worker.rs", 1),
        ("source_oracle_worker.rs", 1),
        ("source_media_oracle_cli.rs", 1),
        ("tooling_build.rs", 1),
        ("tooling_runtime.rs", 2),
        ("tooling_doctor.rs", 1),
        ("tooling_menu_cli.rs", 1),
        ("tooling_check.rs", 1),
        ("tooling_prebuilt.rs", 1),
        ("tooling_similarity_cli.rs", 1),
        ("tooling_test_runner.rs", 2),
        ("tooling_test_command.rs", 1),
    ] {
        let source = std::fs::read_to_string(root.join("src").join(file)).unwrap();
        assert_eq!(source.matches("RunLog::open(").count(), owners, "{file}");
        assert_eq!(
            source.matches("log.install_panic_hook();").count(),
            owners,
            "{file}"
        );
    }
    let extension = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    assert!(!extension.contains("install_panic_hook("));
}

#[test]
fn windows_tooling_and_codec_ownership_cases_have_a_real_platform_bucket() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let workflow = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    let windows = workflow
        .split_once("\n  windows-safety:")
        .expect("native Windows job must exist")
        .1;
    assert!(windows.contains("runs-on: windows-latest"));
    assert!(windows.contains("python-version: \"3.11.9\""));
    let build = step_run(
        windows,
        "Build native backend executable for command parity",
    )
    .unwrap();
    assert!(build.contains("--lib --bin numera-emoji --bin numera-build"));
    let archive = step_run(windows, "Native Windows archive regression").unwrap();
    assert_eq!(
        archive,
        "logs/checkpoint/test-cargo-driver.exe test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked --test archive_commands -- --test-threads=1"
    );
    let library = step_run(windows, "Native process-death migration replay").unwrap();
    assert_eq!(
        library,
        "logs/checkpoint/test-cargo-driver.exe test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked --lib -- --test-threads=1"
    );
    let checkpoint =
        std::fs::read_to_string(root.join(".github/workflows/native-checkpoint.yml")).unwrap();
    assert!(checkpoint.lines().any(|line| line.trim() == "bounded atomic-preservation native/target/debug/numera-emoji test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked --lib atomic_tests:: -- --test-threads=1"));
    assert_eq!(checkpoint.lines().filter(|line| line.trim() == "bounded collection-persistence native/target/debug/numera-emoji test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked --lib publish_cli::tests -- --test-threads=1").count(), 1);
    assert!(checkpoint.lines().any(|line| line.trim() == "bounded configuration native/target/debug/numera-emoji test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked --lib config::tests -- --test-threads=1"));
    assert!(checkpoint.lines().any(|line| line.trim() == "bounded lock-diagnostics native/target/debug/numera-emoji test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked --lib lock_diagnostics::tests -- --test-threads=1"));
    assert!(checkpoint.lines().any(|line| line.trim() == "bounded logging-panic native/target/debug/numera-emoji test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked --lib logging::tests -- --test-threads=1"));
    assert_eq!(checkpoint.lines().filter(|line| line.trim() == "bounded telegram-flood native/target/debug/numera-emoji test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked --lib telegram_tests:: -- --test-threads=1").count(), 1);
    assert_eq!(checkpoint.lines().filter(|line| line.trim() == "bounded sandbox-copy native/target/debug/numera-emoji test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- --config .cargo/tests.toml --manifest-path native/Cargo.toml --locked --lib tooling_sandbox_tests:: -- --test-threads=1").count(), 1);
    assert!(checkpoint.contains("--test source_tooling_recipe "));
    assert!(checkpoint.contains("--test source_assertions "));
    let seams = checkpoint
        .lines()
        .find(|line| line.trim().starts_with("bounded seams native/target/debug/numera-emoji test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- "))
        .unwrap();
    let tokens = seams.split_whitespace().collect::<Vec<_>>();
    for suite in [
        "repaint_gate",
        "source_fake_contracts",
        "source_reference_behavior",
        "source_lock_order",
        "media_native_collect",
        "media_native_ids",
        "fetch_ids_cli",
    ] {
        assert_eq!(
            tokens
                .windows(2)
                .filter(|pair| *pair == ["--test", suite])
                .count(),
            1,
            "checkpoint must execute each migrated contract once: {suite}"
        );
    }
    assert!(checkpoint.contains("--test bot_commands "));
    assert!(checkpoint.contains("--test reconcile_commit "));
    assert!(checkpoint.contains("--test reconcile_identity "));
    assert!(checkpoint.contains("--test publication_scratch "));
    assert!(checkpoint.contains("--test operator_config "));
    assert!(checkpoint.contains("--test sync_order "));
    assert!(checkpoint.contains("--test archive_state "));
    assert!(checkpoint.contains("--test archive_commands "));
    assert!(checkpoint.contains("--test plan_removal "));
    assert!(checkpoint.contains("--test plan_steps_parity "));
    assert!(checkpoint.contains("--test pack_export "));
    assert!(checkpoint.contains("--test status "));
    assert!(checkpoint.contains("--test roster_state "));
    assert!(checkpoint.contains("--test roster_commands "));
    assert!(checkpoint.contains("--test pack_reports "));
    assert!(
        !checkpoint.contains("--test wheel_package "),
        "real-wheel acceptance needs the full wheel-building lane"
    );
    let run = step_run(windows, "Native codec lifecycle regressions").unwrap();
    let tokens = run.split_whitespace().collect::<Vec<_>>();
    for suite in [
        "tooling_process",
        "tooling_test_runner",
        "tooling_runtime",
        "tooling_doctor",
        "tooling_menu",
        "tooling_launch",
        "tooling_build",
        "tooling_install",
        "tooling_policy",
        "wheel_package",
        "catalog_integrity",
        "catalog_paths",
        "sqlite_snapshot",
        "state_artifacts",
        "panel_branding",
        "pack_locks",
        "atomic_failure",
        "source_git_inventory",
        "source_assertions",
        "source_state_compat_recipe",
        "source_logging_admission",
        "logging_parity",
        "tooling_sandbox",
        "tooling_sandbox_serve",
        "tooling_convert_cli",
        "state_compat_parity",
        "ingest_parity",
        "telegram_transport",
        "bot_commands",
        "reconcile_view_parity",
        "reconcile_commit",
        "reconcile_identity",
        "publication_scratch",
        "operator_config",
        "sync_order",
        "archive_state",
        "archive_commands",
        "plan_removal",
        "plan_steps_parity",
        "pack_export",
        "status",
        "roster_state",
        "roster_commands",
        "pack_reports",
        "announce_binding",
        "process_session",
        "codec_prepare_tint",
        "codec_static",
        "codec_format",
        "codec_static_source_boundary",
        "codec_static_lifetime",
        "codec_svg_lifetime",
        "codec_lottie_file",
        "codec_reencode",
        "codec_compare",
        "codec_video_operations",
        "codec_process_policy",
        "codec_video",
        "codec_video_fidelity",
        "codec_lottie_lifetime",
        "codec_lottie_prepare",
        "repaint_gate",
        "source_fake_contracts",
        "source_reference_behavior",
        "source_lock_order",
        "codec_worker_error_ipc",
        "media_native_lifecycle",
        "preview_warm",
        "media_native_consumers",
        "media_native_paths",
        "media_native_identity",
        "media_native_convert",
        "media_native_collect",
        "media_native_ids",
        "fetch_ids_cli",
        "coin_audit",
        "coin_audit_cli",
        "coin_keywords",
        "coin_provider_cli",
        "coin_http",
        "coin_provider_wire",
        "coin_gecko_fetch",
        "coin_provider_image",
        "coin_provider_effects",
        "coin_provider_restart",
        "coin_state",
        "coin_provider_cache",
        "coin_inventory",
        "coin_map_cli",
        "coin_remap_cli",
        "coin_remap_cache",
        "coin_remap_math",
        "coin_remap",
        "coin_verify_cli",
        "coin_replace_effects",
        "coin_replace_proof",
        "coin_logo_distance",
        "coin_rebuild_effects",
        "coin_rebuild_add_cli",
        "coin_plan_disk",
        "coin_plan",
        "coin_rebuild_map_cli",
        "coin_rebuild_recovery_disk",
        "coin_rebuild_links_cli",
        "local_format",
        "emoji_ids",
    ] {
        assert!(root.join(format!("native/tests/{suite}.rs")).is_file());
        assert_eq!(
            tokens
                .windows(2)
                .filter(|pair| *pair == ["--test", suite])
                .count(),
            1,
            "Windows branches of {suite} must execute exactly once on Windows"
        );
    }
    let fixture = step_run(windows, "Build native Windows lifecycle fixture").unwrap();
    assert!(fixture.lines().any(|line| {
        line.starts_with("rustc --edition=2024 native/tests/fixtures/media_worker.rs -o ")
    }));
    assert!(
        fixture.lines().any(
            |line| line.contains("NUMERA_MEDIA_FIXTURE_WORKER=") && line.contains("GITHUB_ENV")
        )
    );
}

#[test]
fn cargo_suites_opt_in_to_the_guard_including_fail_closed_posix_admission() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let runner = std::fs::read_to_string(root.join(".cargo/tests.toml")).unwrap();
    assert!(
        runner.contains("runner = [\"native/target/debug/numera-emoji.exe\", \"test-runner\"]")
    );
    assert!(runner.contains("[target.'cfg(unix)']"));
    assert!(runner.contains("runner = [\"native/target/debug/numera-emoji\", \"test-runner\"]"));
    assert!(
        !root.join(".cargo/config.toml").exists(),
        "test-only runner must not change ordinary cargo run"
    );
    let mut suites = 0;
    for entry in std::fs::read_dir(root.join(".github/workflows")).unwrap() {
        let path = entry.unwrap().path();
        if !matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("yml" | "yaml")
        ) {
            continue;
        }
        let workflow = std::fs::read_to_string(&path).unwrap();
        for line in workflow
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with('#'))
        {
            if line.contains("cargo test ") && line.split_whitespace().any(|arg| arg == "--no-run")
            {
                assert!(
                    line.contains("--manifest-path native/Cargo.toml --locked"),
                    "compile checks must use the locked native manifest"
                );
                continue; // Compilation is checked separately; it never counts as executed coverage.
            }
            if line.contains("cargo test ") || line.contains(" test-cargo ") {
                assert!(
                    line.contains("cargo test --config .cargo/tests.toml ")
                        || (line.contains(" test-cargo --baseline 1a36a968b006236bd0665a1da65c541938c06061 -- --config .cargo/tests.toml ")
                            && line.contains("--manifest-path native/Cargo.toml --locked ")),
                    "unguarded native suite: {}",
                    path.display()
                );
                suites += 1;
            }
        }
    }
    assert!(
        suites >= 4,
        "native guard coverage must not silently match nothing"
    );
}

#[test]
fn coverage_guard_rejects_comments_disabled_discovery_and_compile_only_commands() {
    assert!(full_discovery(&format!("# {FULL_NATIVE}"), "[package]\n").is_err());
    for command in [
        "cargo test --manifest-path native/Cargo.toml --locked --test codec_worker",
        "cargo test --manifest-path native/Cargo.toml --locked --no-run",
        "cargo test --manifest-path native/Cargo.toml --locked --lib",
    ] {
        assert!(full_discovery(command, "[package]\n").is_err());
    }
    for key in ["autotests", "autolib", "autobins", "test", "doctest"] {
        assert!(full_discovery(FULL_NATIVE, &format!("[package]\n{key} = false\n")).is_err());
    }
    let workflow = "jobs:\n  native:\n    steps:\n      - name: Checks\n        run: |\n          echo checked\n      - name: Elsewhere\n        run: cargo test\n";
    assert_eq!(step_run(workflow, "Checks").unwrap(), "echo checked");
    assert!(step_run(workflow, "Missing").is_err());
}
