use super::{NpmCommand, discover_in};
use crate::developer::{self, Command, Options, Step, Tools};
use crate::{logging::RunLog, tooling_process};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

mod fixture;
use fixture::Root;

fn file(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    // Expected native invocation paths include Windows' canonical verbatim prefix.
    path.canonicalize().unwrap()
}

fn installed_web_dependencies(root: &Path) {
    for relative in [
        "worker/node_modules/typescript/package.json",
        "worker/node_modules/vitest/package.json",
        "assets/node_modules/@playwright/test/package.json",
    ] {
        file(root, relative, b"{}");
    }
}

fn check_plan(root: &Path, npm: NpmCommand) -> Vec<Step> {
    developer::plan(
        root,
        &Command::Check(Options::default()),
        &Tools {
            cargo: "unused cargo".into(),
            rustc: "unused rustc".into(),
            npm: Some(npm),
            driver: "unused developer".into(),
        },
    )
    .unwrap()
}

#[test]
fn standard_windows_npm_layout_admits_every_available_web_check() {
    let root = Root::new("npm bundled café & literal");
    let node = file(&root.0, "Node tools/node.exe", b"native executable fixture");
    file(&root.0, "Node tools/npm.cmd", b"unused shell shim fixture");
    let cli = file(
        &root.0,
        "Node tools/node_modules/npm/bin/npm-cli.js",
        b"unused npm entry fixture",
    );
    installed_web_dependencies(&root.0);
    let npm = discover_in(&[root.0.join("Node tools")], true)
        .expect("the standard node.exe + npm.cmd installation must be available");
    assert_eq!(
        npm,
        NpmCommand::Node {
            executable: node.clone(),
            cli: cli.clone(),
        }
    );
    let steps = check_plan(&root.0, npm);
    let web = steps.iter().skip(4).collect::<Vec<_>>();
    assert_eq!(
        web.iter()
            .map(|step| step.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Worker type check",
            "Browser script type check",
            "Worker tests",
            "Browser tests",
        ]
    );
    for step in web {
        assert_eq!(step.argv[0], node.as_os_str());
        assert_eq!(step.argv[1], cli.as_os_str());
    }
}

#[test]
fn windows_npm_uses_the_first_complete_path_entry_and_its_sibling_node() {
    let root = Root::new("npm PATH order");
    let first = root.0.join("global npm");
    let second = root.0.join("Node tools");
    let other = root.0.join("unrelated Node");
    file(&first, "npm.cmd", b"unused shell shim fixture");
    let first_cli = file(&first, "node_modules/npm/bin/npm-cli.js", b"npm fixture");
    let fallback_node = file(&other, "node.exe", b"fallback node fixture");
    let sibling_node = file(&second, "node.exe", b"bundled node fixture");
    file(&second, "npm.cmd", b"unused shell shim fixture");
    let second_cli = file(&second, "node_modules/npm/bin/npm-cli.js", b"npm fixture");
    assert_eq!(
        discover_in(&[first.clone(), other.clone(), second.clone()], true),
        Some(NpmCommand::Node {
            executable: fallback_node,
            cli: first_cli,
        })
    );
    assert_eq!(
        discover_in(&[other, second, first], true),
        Some(NpmCommand::Node {
            executable: sibling_node,
            cli: second_cli,
        })
    );
}

#[test]
fn incomplete_windows_npm_installations_do_not_admit_web_checks() {
    let root = Root::new("npm unavailable");
    let tools = root.0.join("tools");
    assert_eq!(discover_in(std::slice::from_ref(&tools), true), None);
    file(&tools, "npm.cmd", b"unused shell shim fixture");
    let cli = file(&tools, "node_modules/npm/bin/npm-cli.js", b"npm fixture");
    assert_eq!(discover_in(std::slice::from_ref(&tools), true), None);
    let node = file(&tools, "node.exe", b"native executable fixture");
    std::fs::remove_file(&cli).unwrap();
    assert_eq!(discover_in(std::slice::from_ref(&tools), true), None);
    std::fs::create_dir(&cli).unwrap();
    assert_eq!(discover_in(std::slice::from_ref(&tools), true), None);
    std::fs::remove_dir(&cli).unwrap();
    std::fs::write(&cli, b"npm fixture").unwrap();
    std::fs::remove_file(&node).unwrap();
    std::fs::create_dir(&node).unwrap();
    assert_eq!(discover_in(&[tools], true), None);
}

#[test]
fn native_npm_executables_and_unix_shims_keep_their_existing_invocation() {
    let root = Root::new("npm native controls");
    let tools = root.0.join("tools");
    let windows = file(&tools, "npm.exe", b"native npm fixture");
    let unix = file(&tools, "npm", b"Unix npm shim fixture");
    assert_eq!(
        discover_in(std::slice::from_ref(&tools), true),
        Some(NpmCommand::Executable(windows))
    );
    assert_eq!(
        discover_in(&[tools], false),
        Some(NpmCommand::Executable(unix))
    );
}

#[test]
fn windows_npm_path_order_applies_to_native_and_powershell_installations() {
    let root = Root::new("npm mixed PATH order");
    let first = root.0.join("first");
    let second = root.0.join("second");
    file(&first, "npm.ps1", b"unused PowerShell shim fixture");
    let node = file(&first, "node.exe", b"native node fixture");
    let cli = file(&first, "node_modules/npm/bin/npm-cli.js", b"npm fixture");
    let native = file(&second, "npm.exe", b"native npm fixture");
    assert_eq!(
        discover_in(&[first.clone(), second.clone()], true),
        Some(NpmCommand::Node {
            executable: node,
            cli,
        })
    );
    assert_eq!(
        discover_in(&[second, first], true),
        Some(NpmCommand::Executable(native))
    );
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn native_fixture(root: &Path) -> PathBuf {
    let node = root.join("Node tools/node.exe");
    std::fs::create_dir_all(node.parent().unwrap()).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/developer_npm_support/argv_fixture.rs.in");
    let argv = vec![
        env!("NUMERA_BASELINE_RUSTC").into(),
        "--edition=2024".into(),
        "--crate-name=developer_npm_argv_fixture".into(),
        source.into_os_string(),
        "-o".into(),
        node.clone().into_os_string(),
    ];
    let environment = std::env::vars().collect::<BTreeMap<_, _>>();
    let log = RunLog::open(root, "npm-fixture-compiler", vec![]);
    let result = runtime()
        .block_on(tooling_process::capture(
            root,
            &argv,
            &environment,
            Duration::from_secs(60),
            Duration::from_secs(30),
            &log,
        ))
        .unwrap();
    assert_eq!(result.code, 0, "{}", result.stderr);
    node
}

fn read_arguments(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| {
            let (pairs, remainder) = line.as_bytes().as_chunks::<2>();
            assert!(
                remainder.is_empty(),
                "fixture argument hex must be complete"
            );
            let bytes = pairs
                .iter()
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            String::from_utf8(bytes).unwrap()
        })
        .collect()
}

#[test]
fn windows_npm_process_preserves_literal_arguments_and_failure_exit() {
    let root = Root::new("npm process café & literal");
    let node = native_fixture(&root.0);
    file(&root.0, "Node tools/npm.cmd", b"unused shell shim fixture");
    let cli = file(
        &root.0,
        "Node tools/node_modules/npm/bin/npm-cli.js",
        b"unused npm entry fixture",
    );
    installed_web_dependencies(&root.0);
    let npm = discover_in(&[root.0.join("Node tools")], true).unwrap();
    let mut step = check_plan(&root.0, npm)
        .into_iter()
        .find(|step| step.label == "Browser script type check")
        .unwrap();
    step.argv.extend([
        OsString::from(""),
        OsString::from("literal \"quote\" & semicolon; %PATH% !caret^"),
        OsString::from("متن فارسی café"),
    ]);
    let marker = root.0.join("arguments.hex");
    let mut environment = BTreeMap::from([
        (
            "NUMERA_NPM_FIXTURE_RECORD".into(),
            marker.to_str().unwrap().into(),
        ),
        ("NUMERA_NPM_FIXTURE_EXIT".into(), "7".into()),
    ]);
    #[cfg(windows)]
    environment.insert("SystemRoot".into(), std::env::var("SystemRoot").unwrap());
    #[cfg(not(windows))]
    environment.insert("LANG".into(), "C.UTF-8".into());
    let log = RunLog::open(&root.0, "npm-argv-exit", vec![]);
    let result = runtime()
        .block_on(developer::execute(&root.0, &[step], &environment, &log))
        .unwrap();
    assert_eq!(result, 7);
    assert_eq!(
        read_arguments(&marker),
        [
            cli.to_str().unwrap(),
            "--prefix",
            "worker",
            "exec",
            "--",
            "tsc",
            "-p",
            root.0.join("assets/jsconfig.json").to_str().unwrap(),
            "--noEmit",
            "",
            "literal \"quote\" & semicolon; %PATH% !caret^",
            "متن فارسی café",
        ]
    );
    assert!(node.is_file());
    drop(log);
    root.clean().unwrap();
}

#[cfg(windows)]
#[test]
fn actual_windows_node_npm_runs_through_the_owned_process_capture() {
    let root = Root::new("actual Windows npm");
    let npm =
        NpmCommand::discover().expect("Windows contributor checks require installed Node/npm");
    let step = npm.step(
        "Actual Windows npm",
        &["--version"],
        Duration::from_secs(20),
    );
    let environment = developer::environment(&root.0, &std::env::vars().collect()).unwrap();
    let log = RunLog::open(&root.0, "actual-windows-npm", vec![]);
    let result = runtime()
        .block_on(tooling_process::capture(
            &root.0,
            &step.argv,
            &environment,
            step.wall,
            step.idle,
            &log,
        ))
        .unwrap();
    assert_eq!(result.code, 0, "{}", result.stderr);
    let parts = result.stdout.trim().split('.').collect::<Vec<_>>();
    assert_eq!(parts.len(), 3, "npm must print its real version");
    assert!(parts.iter().all(|part| part.parse::<u32>().is_ok()));
    drop(log);
    root.clean().unwrap();
}

#[test]
fn windows_npm_process_keeps_the_bounded_timeout_and_cleans_its_owned_child() {
    let root = Root::new("npm process timeout");
    native_fixture(&root.0);
    file(&root.0, "Node tools/npm.cmd", b"unused shell shim fixture");
    let cli = file(
        &root.0,
        "Node tools/node_modules/npm/bin/npm-cli.js",
        b"unused npm entry fixture",
    );
    let npm = discover_in(&[root.0.join("Node tools")], true).unwrap();
    let step = npm.step("Bounded npm", &["--version"], Duration::from_secs(3));
    let marker = root.0.join("timeout-arguments.hex");
    let mut environment = BTreeMap::from([
        (
            "NUMERA_NPM_FIXTURE_RECORD".into(),
            marker.to_str().unwrap().into(),
        ),
        ("NUMERA_NPM_FIXTURE_EXIT".into(), "0".into()),
        ("NUMERA_NPM_FIXTURE_SLEEP_MS".into(), "30000".into()),
    ]);
    #[cfg(windows)]
    environment.insert("SystemRoot".into(), std::env::var("SystemRoot").unwrap());
    #[cfg(not(windows))]
    environment.insert("LANG".into(), "C.UTF-8".into());
    let log = RunLog::open(&root.0, "npm-timeout", vec![]);
    let started = std::time::Instant::now();
    let error = runtime()
        .block_on(developer::execute(&root.0, &[step], &environment, &log))
        .unwrap_err();
    assert!(error.contains("wall budget"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(15));
    assert_eq!(
        read_arguments(&marker),
        [cli.to_str().unwrap(), "--version"]
    );
    drop(log);
    root.clean().unwrap();
}
