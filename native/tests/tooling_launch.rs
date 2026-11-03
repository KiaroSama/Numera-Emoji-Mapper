//! Public native launch and sandbox argument/environment boundaries; no live process or operator input.
use serde_json::Value;

#[test]
fn sandbox_scrub_and_arguments_match_independent_pinned_source() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(project.join("tests/fixtures/native/tooling-sandbox-source.json"))
            .expect("primary must capture pinned sandbox policy before target verification"),
    )
    .unwrap();
    assert_eq!(
        fixture["sourceHash"],
        "a37b7609521a78ebca9a6d0a2d1dae7b70fccce2fa50af8d5a16f2d60dfef2ae"
    );
    let original = fixture["original"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), value.as_str().unwrap().into()))
        .collect();
    assert_eq!(
        serde_json::to_value(tooling_launch::scrubbed_environment(
            &original,
            fixture["template"].as_str().unwrap()
        ))
        .unwrap(),
        fixture["scrubbed"]
    );
    for input in [
        vec!["--data-dir".into(), "collection".into()],
        vec!["--sour".into(), "collection".into()],
        vec!["--port".into(), "9450".into()],
        vec!["--port".into(), "0".into()],
        vec!["--port".into()],
    ] {
        assert!(tooling_launch::sandbox_options(&input, 9450).is_err());
    }
    let cases = fixture["arguments"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    for case in cases {
        let options = SandboxOptions {
            port: case["port"].as_i64().unwrap(),
            all: case["all"].as_bool().unwrap(),
            with_pack: case["with_pack"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap())
                .collect(),
            bot_username: case["bot_username"].as_str().unwrap().into(),
        };
        let input = case["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let (source, parsed) = tooling_launch::sandbox_options(&input, 9450).unwrap();
        assert_eq!(source, Path::new("collection"));
        assert_eq!(parsed.port, options.port);
        assert_eq!(parsed.all, options.all);
        assert_eq!(parsed.with_pack, options.with_pack);
        assert_eq!(parsed.bot_username, options.bot_username);
        let actual = tooling_launch::panel_arguments(&parsed, Path::new("$CLONE"), 9450).unwrap();
        assert_eq!(
            serde_json::to_value(&actual[1..]).unwrap(),
            case["expected"]
        );
    }
}
use _native::tooling_launch::{self, SandboxOptions};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};
#[path = "support/process_liveness.rs"]
mod process_liveness;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[test]
fn check_plan_keeps_original_python_guard_lint_and_source_selection() {
    let root = tooling_fixture::Root::new("check plan café");
    let sources = vec!["emojikit/catalog.py".into(), "tests/test_catalog.py".into()];
    let python = root.0.join("python with spaces");
    let ruff = root.0.join("ruff with spaces");
    let steps = _native::tooling_check::steps(&python, &ruff, &sources).unwrap();
    let argv = steps
        .iter()
        .map(|step| {
            step.argv
                .iter()
                .map(|value| value.to_str().unwrap())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        argv[0],
        [
            python.to_str().unwrap(),
            "-X",
            "utf8",
            "-m",
            "compileall",
            "-q",
            "emojikit/catalog.py",
            "tests/test_catalog.py"
        ]
    );
    assert_eq!(argv[1], [ruff.to_str().unwrap(), "check", "."]);
    assert_eq!(
        argv[2],
        [
            python.to_str().unwrap(),
            "-X",
            "utf8",
            "-m",
            "unittest",
            "discover",
            "-s",
            "tests",
            "-t",
            ".",
            "-p",
            "test_*.py"
        ]
    );
    assert!(
        steps
            .iter()
            .all(|step| !step.wall.is_zero() && !step.idle.is_zero() && step.idle <= step.wall)
    );
    for sources in [
        vec![],
        vec!["../foreign.py".into()],
        vec!["/absolute.py".into()],
        vec!["--option.py".into()],
    ] {
        assert!(_native::tooling_check::steps(&python, &ruff, &sources).is_err());
    }
    root.clean().unwrap();
}

#[test]
fn original_check_subset_skip_and_timeout_keep_the_tests_package_guard() {
    use _native::tooling_check::{Arguments, Options, arguments, plan};
    use std::ffi::OsString;
    let strings = |values: &[&str]| {
        values
            .iter()
            .map(|value| (*value).into())
            .collect::<Vec<_>>()
    };
    let Arguments::Check(options) = arguments(&strings(&[
        "--tests",
        "test_catalog, tests.test_panel_plan.py",
        "test_catalog.Case.test_method",
        "--skip-compile",
        "--skip-lint",
        "--timeout-seconds=7",
    ]))
    .unwrap() else {
        panic!("check options must produce a plan")
    };
    let python = Path::new("selected interpreter café");
    let steps = plan(python, None, &[], &options).unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(
        steps[0].argv,
        [
            "selected interpreter café",
            "-X",
            "utf8",
            "-m",
            "unittest",
            "tests.test_catalog",
            "tests.test_panel_plan",
            "tests.test_catalog.Case.test_method",
            "-v",
        ]
        .map(OsString::from)
    );
    assert_eq!(steps[0].wall, Duration::from_secs(7));
    assert_eq!(steps[0].idle, Duration::from_secs(7));
    let Arguments::Check(repeated_prefix) = arguments(&strings(&[
        "--tests=tests.tests.module.py",
        "--skip-compile",
        "--skip-lint",
    ]))
    .unwrap() else {
        panic!("selected tests expected")
    };
    let repeated = plan(python, None, &[], &repeated_prefix).unwrap();
    assert_eq!(
        repeated[0].argv[5],
        OsString::from("tests.tests.module"),
        "normalize the original prefix exactly once"
    );
    assert!(plan(python, None, &[], &Options::default()).is_err());
    let mut compile_only = options.clone();
    compile_only.skip_compile = false;
    assert!(plan(python, None, &[], &compile_only).is_err());
    let mut lint = options.clone();
    lint.skip_lint = false;
    assert!(plan(python, None, &[], &lint).is_err());
    let steps = plan(python, Some(Path::new("ruff")), &[], &lint).unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].argv, ["ruff", "check", "."].map(OsString::from));
    for input in [
        vec!["--tests"],
        vec!["--tests= , "],
        vec!["--tests=../foreign.py"],
        vec!["--tests=-v"],
        vec!["--timeout-seconds=0"],
        vec!["--timeout-seconds=-1"],
        vec!["--timeout-seconds=bad"],
        vec!["--skip"],
        vec!["--unknown"],
    ] {
        assert!(arguments(&strings(&input)).is_err(), "accepted {input:?}");
    }
    for input in [vec!["--help"], vec!["-h"]] {
        assert!(matches!(
            arguments(&strings(&input)).unwrap(),
            Arguments::Exit(0)
        ));
    }
    for input in [
        vec!["--python", "selected interpreter café"],
        vec!["--python=selected interpreter café"],
    ] {
        let Arguments::Check(selected) = arguments(&strings(&input)).unwrap() else {
            panic!("explicit interpreter must produce check options")
        };
        assert_eq!(
            selected.python.as_deref(),
            Some(Path::new("selected interpreter café"))
        );
    }
    for input in [
        vec!["--python"],
        vec!["--python="],
        vec!["--python", "--skip-lint"],
    ] {
        assert!(arguments(&strings(&input)).is_err(), "accepted {input:?}");
    }
    assert!(Options::default().python.is_none());
}

#[test]
fn foreground_fixture() {
    let Some(path) = std::env::var_os("NUMERA_FOREGROUND_FIXTURE") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    assert_eq!(std::env::current_dir().unwrap(), path);
    assert_eq!(std::env::var("NUMERA_FOREGROUND_TEXT").unwrap(), "café 😀");
    std::fs::write(
        path.join("foreground-ready"),
        std::process::id().to_string(),
    )
    .unwrap();
    if std::env::var("NUMERA_FOREGROUND_WAIT").as_deref() == Ok("1") {
        let (_send, receive) = std::sync::mpsc::channel::<()>();
        receive.recv_timeout(Duration::from_secs(8)).unwrap();
    } else {
        std::process::exit(23);
    }
}

#[test]
fn foreground_preserves_exit_and_cancellation_reaps_before_removing_workspace() {
    let root = tooling_fixture::Root::new("foreground café");
    let log = _native::logging::RunLog::open(&root.0, "foreground-test", vec![]);
    let mut environment = tooling_launch::scrubbed_environment(&std::env::vars().collect(), "");
    environment.insert(
        "NUMERA_FOREGROUND_FIXTURE".into(),
        root.0.to_str().unwrap().into(),
    );
    environment.insert("NUMERA_FOREGROUND_TEXT".into(), "café 😀".into());
    let mut launch = tooling_launch::Launch {
        root: root.0.clone(),
        argv: vec![
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            "foreground_fixture".into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ],
        environment,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let code = runtime.block_on(async {
        tokio::time::timeout(
            Duration::from_secs(6),
            tooling_launch::foreground(&launch, std::future::pending::<Result<(), String>>(), &log),
        )
        .await
        .unwrap()
        .unwrap()
    });
    assert_eq!(code, 23);
    let first_pid = std::fs::read_to_string(root.0.join("foreground-ready"))
        .unwrap()
        .parse()
        .unwrap();
    process_liveness::assert_stopped(first_pid);
    std::fs::remove_file(root.0.join("foreground-ready")).unwrap();
    launch
        .environment
        .insert("NUMERA_FOREGROUND_WAIT".into(), "1".into());
    let code = runtime.block_on(async {
        let cancel = async {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if std::fs::read_to_string(root.0.join("foreground-ready"))
                    .ok()
                    .and_then(|value| value.parse::<u32>().ok())
                    .is_some()
                {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err("foreground readiness expired".into());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        tokio::time::timeout(
            Duration::from_secs(7),
            tooling_launch::foreground(&launch, cancel, &log),
        )
        .await
        .unwrap()
        .unwrap()
    });
    assert_eq!(code, 130, "interruption must not become successful work");
    let cancelled_pid = std::fs::read_to_string(root.0.join("foreground-ready"))
        .unwrap()
        .parse()
        .unwrap();
    process_liveness::assert_stopped(cancelled_pid);
    let error = runtime
        .block_on(tooling_launch::foreground(
            &launch,
            async { Err("cancellation channel failed".into()) },
            &log,
        ))
        .unwrap_err();
    assert_eq!(error, "cancellation channel failed");
    assert_eq!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    drop(log);
    root.clean().unwrap();
}

#[test]
fn sandbox_scrub_preserves_machine_environment_and_drops_template_and_secret_shapes() {
    let original = [
        "PATH",
        "SystemRoot",
        "KEEP",
        "CUSTOM_SETTING",
        "EXPORTED_TOKEN",
        "misc_Secret",
        "OWNER_ID",
        "PRIVATE_KEY",
        "password_extra",
    ]
    .into_iter()
    .map(|key| (key.into(), "synthetic".into()))
    .collect::<BTreeMap<_, _>>();
    let child =
        tooling_launch::scrubbed_environment(&original, "# comment=value\nCUSTOM_SETTING=\n");
    assert_eq!(
        child,
        ["PATH", "SystemRoot", "KEEP"]
            .into_iter()
            .map(|key| (key.into(), "synthetic".into()))
            .chain([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())])
            .collect::<BTreeMap<_, _>>()
    );
    assert_eq!(
        original.len(),
        9,
        "caller environment must remain untouched"
    );
    let before = std::env::vars().collect::<BTreeMap<_, _>>();
    let template = include_str!("../../.env.example");
    let names = template
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| line.split_once('=').map(|(key, _)| key.trim()))
        .filter(|key| !key.is_empty())
        .collect::<Vec<_>>();
    assert!(!names.is_empty(), "an empty template cannot prove a scrub");
    // Never copy parent credentials into assertions or their failure output.
    let mut machine = before
        .keys()
        .map(|key| (key.clone(), "synthetic-machine-value".into()))
        .collect::<BTreeMap<_, _>>();
    for key in &names {
        machine.insert((*key).into(), "synthetic-template-value".into());
    }
    machine.insert(
        "GENERAL_BOT_TOKEN".into(),
        "synthetic-exported-token".into(),
    );
    machine.insert("COIN_BOT_TOKEN".into(), "synthetic-exported-token".into());
    machine.insert("EMOJI_SANDBOX_CANARY".into(), "kept".into());
    let unchanged = machine.clone();
    let scrubbed = tooling_launch::scrubbed_environment(&machine, template);
    assert!(names.iter().all(|key| !scrubbed.contains_key(*key)));
    assert!(!scrubbed.contains_key("GENERAL_BOT_TOKEN"));
    assert!(!scrubbed.contains_key("COIN_BOT_TOKEN"));
    assert_eq!(scrubbed["NUMERA_EMOJI_MAPPER_NO_DOTENV"], "1");
    assert_eq!(scrubbed["EMOJI_SANDBOX_CANARY"], "kept");
    assert!(scrubbed.keys().any(|key| key.eq_ignore_ascii_case("PATH")));
    #[cfg(windows)]
    assert!(
        scrubbed
            .keys()
            .any(|key| key.eq_ignore_ascii_case("SYSTEMROOT"))
    );
    assert!(
        scrubbed.len() > 5,
        "machine environment was wiped, not scrubbed"
    );
    assert_eq!(machine, unchanged);
    assert!(
        std::env::vars().collect::<BTreeMap<_, _>>() == before,
        "parent environment changed"
    );
}

#[test]
fn sandbox_argv_has_one_owner_selected_clone_and_no_browser_open() {
    let real_port = _native::panel_session::DEFAULT_PORT;
    let (_, derived) = tooling_launch::sandbox_options(&[], real_port).unwrap();
    assert_eq!(derived.port, i64::from(real_port) + 1);
    assert_ne!(derived.port, i64::from(real_port));
    let refused =
        tooling_launch::sandbox_options(&["--port".into(), real_port.to_string()], real_port)
            .err()
            .expect("the real panel port must be refused before cloning");
    assert!(refused.contains(&real_port.to_string()));
    let (_, shifted) = tooling_launch::sandbox_options(&[], 12000).unwrap();
    assert_eq!(
        shifted.port, 12001,
        "a changed real port must change the sandbox default"
    );
    let options = SandboxOptions {
        port: 9451,
        all: true,
        with_pack: vec![2, 3],
        bot_username: "YourEmojiBot".into(),
    };
    assert_eq!(
        tooling_launch::panel_arguments(&options, Path::new("clone with spaces"), 9450).unwrap(),
        [
            "panel",
            "--data-dir",
            "clone with spaces",
            "--port",
            "9451",
            "--no-open",
            "--all",
            "--with-pack",
            "2",
            "--with-pack",
            "3",
            "--bot-username",
            "YourEmojiBot"
        ]
    );
    for arguments in [
        vec!["--data-dir", "collection"],
        vec!["--nonsense"],
        vec!["--sou", "x"],
        vec!["--wi", "3"],
        vec!["--port", "0"],
        vec!["--port", "70000"],
        vec!["--port", "not-a-number"],
    ] {
        assert!(
            tooling_launch::sandbox_options(
                &arguments.into_iter().map(str::to_owned).collect::<Vec<_>>(),
                real_port,
            )
            .is_err()
        );
    }
    let (source, parsed) = tooling_launch::sandbox_options(
        &[
            "--all",
            "--with-pack",
            "3",
            "--with-pack",
            "4",
            "--bot-username",
            "b",
        ]
        .map(str::to_owned),
        real_port,
    )
    .unwrap();
    assert_eq!(source, Path::new("collection"));
    let argv = tooling_launch::panel_arguments(&parsed, Path::new("clone with spaces"), real_port)
        .unwrap();
    assert_eq!(
        argv.iter()
            .filter(|arg| arg.as_str() == "--data-dir")
            .count(),
        1
    );
    assert_eq!(
        argv.iter()
            .filter(|arg| arg.as_str() == "--with-pack")
            .count(),
        2
    );
    assert!(argv.contains(&"--all".into()));
    assert_eq!(argv.last().unwrap(), "b");
    for port in [0, 9450, 65536] {
        let options = SandboxOptions {
            port,
            all: false,
            with_pack: vec![],
            bot_username: String::new(),
        };
        assert!(tooling_launch::panel_arguments(&options, Path::new("clone"), 9450).is_err());
    }
}
