use super::{required_step_run, root, step_run};

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
    let security = include_str!("../../../.github/workflows/gitleaks.yml");
    let runtime = include_str!("../../../.github/workflows/ci.yml");
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
