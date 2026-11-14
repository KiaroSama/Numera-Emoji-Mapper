//! Original ownership CLI contracts; no source execution or real conversion in this policy tier.
use _native::{logging::RunLog, tooling_convert_cli, tooling_process};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::time::Duration;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[test]
fn fixture_cleanup_preserves_unresolved_children_and_unreadable_child_storage() {
    let root = Root::new("converter cli");
    let children = root.0.join("logs/tmp/tooling");
    let child = children.join("synthetic child");
    std::fs::create_dir_all(&child).unwrap();
    let marker = child.join("codec-cleanup-pending.json");
    let evidence = b"synthetic ownership evidence; no process started";
    std::fs::write(&marker, evidence).unwrap();
    assert!(root.clean().unwrap_err().contains("unresolved"));
    assert_eq!(std::fs::read(&marker).unwrap(), evidence);
    // Remove only this test's synthetic evidence after proving the retention branch.
    std::fs::remove_file(marker).unwrap();
    std::fs::remove_dir(child).unwrap();
    std::fs::remove_dir(&children).unwrap();
    std::fs::write(&children, evidence).unwrap();
    assert!(root.clean().unwrap_err().contains("Cannot inspect"));
    assert_eq!(std::fs::read(&children).unwrap(), evidence);
    std::fs::remove_file(children).unwrap();
    root.clean().unwrap();
    assert!(!root.0.exists());
}

#[test]
fn all_four_original_paths_are_required_without_losing_argparse_defaults() {
    use tooling_convert_cli::Arguments;
    let complete = args(&[
        "--in",
        "input with spaces",
        "--out",
        "output café",
        "--stdout",
        "stdout #.txt",
        "--stderr",
        "stderr %.txt",
    ]);
    assert_eq!(
        tooling_convert_cli::arguments(&complete).unwrap(),
        Arguments::Convert {
            source: "input with spaces".into(),
            output: "output café".into(),
            stdout: "stdout #.txt".into(),
            stderr: "stderr %.txt".into(),
        }
    );
    for omitted in 0..4 {
        let mut missing = complete.clone();
        missing.drain(omitted * 2..omitted * 2 + 2);
        assert_eq!(
            tooling_convert_cli::arguments(&missing).unwrap(),
            Arguments::Exit(2)
        );
    }
    assert_eq!(
        tooling_convert_cli::arguments(&args(&[
            "--i=-1",
            "--o=",
            "--stdou=first",
            "--stderr=errors",
            "--stdout=last",
        ]))
        .unwrap(),
        Arguments::Convert {
            source: "-1".into(),
            output: ".".into(),
            stdout: "last".into(),
            stderr: "errors".into(),
        }
    );
    for invalid in [
        vec![],
        vec!["--in"],
        vec!["--st=x"],
        vec!["--unknown"],
        vec!["positional"],
    ] {
        assert_eq!(
            tooling_convert_cli::arguments(&args(&invalid)).unwrap(),
            Arguments::Exit(2)
        );
    }
    assert_eq!(
        tooling_convert_cli::arguments(&args(&["--help"])).unwrap(),
        Arguments::Exit(0)
    );
}

#[test]
fn missing_installed_runtime_fails_without_creating_conversion_outputs() {
    let root = Root::new("converter cli");
    let log = RunLog::open(&root.0, "native_convert_owner", vec![]);
    let path = log.path.clone().unwrap();
    let result = tooling_convert_cli::run(
        &root.0,
        &args(&[
            "--in",
            "input with spaces",
            "--out",
            "output",
            "--stdout",
            "stdout",
            "--stderr",
            "stderr",
        ]),
        &log,
    );
    assert_eq!(result, 1);
    log.finish(result);
    drop(log);
    let output = std::fs::read_to_string(path).unwrap();
    assert!(output.contains("Native converter ownership failed:"));
    assert!(output.contains("Prebuilt runtime is not installed"));
    assert!(output.contains("exit=1"));
    for absent in [
        "input with spaces",
        "output",
        "stdout",
        "stderr",
        "collection",
    ] {
        assert!(
            !root.0.join(absent).exists(),
            "unexpected conversion side effect: {absent}"
        );
    }
    assert!(
        !root.0.join("logs/tmp/tooling").exists(),
        "prerequisite refusal must not spawn"
    );
    root.clean().unwrap();
}

#[test]
fn copied_public_cli_help_usage_and_missing_prerequisite_bypass_operator_config() {
    let root = Root::new("converter cli");
    std::fs::create_dir(root.0.join("assets")).unwrap();
    std::fs::write(root.0.join("assets/panel.html"), b"fixture root marker").unwrap();
    std::fs::write(
        root.0.join("pyproject.toml"),
        b"[project]\nname = 'fixture'\n",
    )
    .unwrap();
    let executable = root.0.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-compat"), &executable).unwrap();
    let mut environment = BTreeMap::new();
    // An allowlist keeps production credentials out without altering the parent environment.
    for key in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "PYTHONHOME",
        "LANG",
        "LC_ALL",
    ] {
        if let Ok(value) = std::env::var(key) {
            environment.insert(key.into(), value);
        }
    }
    for (key, value) in [
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1"),
        ("PYTHONUTF8", "1"),
        ("PYTHONNOUSERSITE", "1"),
        ("PYTHONSAFEPATH", "1"),
    ] {
        environment.insert(key.into(), value.into());
    }
    for key in ["TEMP", "TMP", "TMPDIR"] {
        environment.insert(key.into(), root.0.to_str().unwrap().into());
    }
    let log = RunLog::open(&root.0, "converter-cli-test", vec![]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for (arguments, expected, text) in [
        (args(&["--help"]), 0, "--stdout STDOUT"),
        (vec![], 2, "arguments are required"),
        (
            args(&[
                "--in",
                "input with spaces",
                "--out",
                "output",
                "--stdout",
                "out.txt",
                "--stderr",
                "err.txt",
            ]),
            1,
            "Prebuilt runtime is not installed",
        ),
    ] {
        let mut command = vec![
            executable.clone().into_os_string(),
            OsString::from("native-convert-owner"),
        ];
        command.extend(arguments.into_iter().map(OsString::from));
        let completed = runtime
            .block_on(tooling_process::capture(
                &root.0,
                &command,
                &environment,
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log,
            ))
            .unwrap();
        assert_eq!(completed.code, expected, "{}", completed.stderr);
        assert!(format!("{}{}", completed.stdout, completed.stderr).contains(text));
    }
    assert_eq!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    for absent in ["collection", "output", "out.txt", "err.txt", ".env"] {
        assert!(
            !root.0.join(absent).exists(),
            "CLI must not require operator configuration or create outputs"
        );
    }
    drop(log);
    root.clean().unwrap();
}
