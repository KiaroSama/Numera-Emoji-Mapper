//! Exact foreground and sandbox-scrub tails over the active native launch owner.
use numera_runtime::tooling_launch;

#[test]
fn sandbox_argv_has_one_owner_selected_clone_and_no_browser_open() {
    let real_port = numera_runtime::panel_session::DEFAULT_PORT;
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
    let options = tooling_launch::SandboxOptions {
        port: 9451,
        all: true,
        with_pack: vec![2, 3],
        bot_username: "YourEmojiBot".into(),
    };
    assert_eq!(
        tooling_launch::panel_arguments(&options, std::path::Path::new("clone with spaces"), 9450)
            .unwrap(),
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
    assert_eq!(source, std::path::Path::new("collection"));
    let argv = tooling_launch::panel_arguments(
        &parsed,
        std::path::Path::new("clone with spaces"),
        real_port,
    )
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
        let options = tooling_launch::SandboxOptions {
            port,
            all: false,
            with_pack: vec![],
            bot_username: String::new(),
        };
        assert!(
            tooling_launch::panel_arguments(&options, std::path::Path::new("clone"), 9450).is_err()
        );
    }
}

use crate::process_liveness;
use crate::tooling_fixture;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
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
    let log = numera_runtime::logging::RunLog::open(&root.0, "foreground-test", vec![]);
    let mut environment = tooling_launch::scrubbed_environment(&std::env::vars().collect(), "");
    if let Ok(marker) = std::env::var("NUMERA_RUNTIME_TEST_ADMISSION") {
        environment.insert("NUMERA_RUNTIME_TEST_ADMISSION".into(), marker);
    }
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
            "launch::foreground_fixture".into(),
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
    let template = include_str!("../../../.env.example");
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
