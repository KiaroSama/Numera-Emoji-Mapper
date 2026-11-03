//! Scripted public menu workflows preserve argv, confirmations and failure stops without children.
use _native::tooling_menu::{self, Request};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "support/cli_reference.rs"]
mod cli_reference;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            _native::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let path = parent.join(format!(
            "native menu café {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join("input with spaces")).unwrap();
        Self(_native::tooling_environment::physical_root(&path).unwrap())
    }
    fn session(&self, text: &str, codes: &[i32]) -> (Vec<Request>, String) {
        let mut calls = Vec::new();
        let mut output = Vec::new();
        let started = std::time::Instant::now();
        tooling_menu::run(
            &self.0,
            &mut Cursor::new(text.as_bytes()),
            &mut output,
            false,
            |request| {
                assert!(started.elapsed() < std::time::Duration::from_secs(2));
                assert!(calls.len() < 16, "unexpected repeated menu command");
                let code = *codes.get(calls.len()).unwrap_or(&0);
                calls.push(request.clone());
                Ok(code)
            },
        )
        .unwrap();
        (calls, String::from_utf8(output).unwrap())
    }
    fn state(&self, name: &str, value: &[u8]) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, value).unwrap();
    }
    fn clean(self) {
        std::fs::remove_dir_all(self.0).unwrap();
    }
}
fn command(args: &[&str]) -> Request {
    Request::Command(args.iter().map(|value| (*value).into()).collect())
}

#[test]
fn original_menu_keys_quit_and_eof_never_start_a_command() {
    let fixture = Fixture::new();
    for input in ["exit\n", "q\n", "0\n", ""] {
        let (calls, output) = fixture.session(input, &[]);
        assert!(calls.is_empty());
        for key in [
            "A1", "A2", "A3", "B0", "B1", "B2", "B3", "B4", "B5", "C1", "D1", "D2", "D3", "D4",
        ] {
            assert!(output.contains(key), "missing {key}");
        }
        assert!(output.contains("Bye."));
        assert!(!output.contains("\u{1b}"));
    }
    fixture.clean();
}

#[test]
fn conversion_wizard_back_reasks_one_field_and_keeps_paths_as_single_arguments() {
    let fixture = Fixture::new();
    let (calls, output) = fixture.session(
        "a2\ninput with spaces\n0\ninput with spaces\noutput café\nexit\n",
        &[],
    );
    assert_eq!(
        calls,
        [command(&[
            "make-emoji-pngs",
            "--in",
            "input with spaces",
            "--out",
            "output café"
        ])]
    );
    assert!(output.contains("Conversion finished."));
    let (calls, _) = fixture.session("a2\ninput with spaces\n\nexit\n", &[]);
    assert_eq!(
        calls,
        [command(&["make-emoji-pngs", "--in", "input with spaces"])]
    );
    fixture.clean();
}

#[test]
fn confirmation_back_returns_to_the_previous_wizard_step_instead_of_cancelling() {
    let fixture = Fixture::new();
    let (calls, _) = fixture.session(
        "a1\ninput with spaces\nmyset\nMy Emoji\n😀\n0\n🎯\ny\nexit\n",
        &[0, 0, 0],
    );
    assert_eq!(calls.len(), 3);
    let Request::Command(args) = &calls[1] else {
        panic!("command expected")
    };
    assert!(args.windows(2).any(|pair| pair == ["--emoji", "🎯"]));
    let (calls, _) = fixture.session(
        "b3\nmyset\nMy Set\n\n0\nCUSTOM_TOKEN\ny\n0\nn\ny\nexit\n",
        &[0, 0, 0],
    );
    assert_eq!(calls.len(), 3);
    for call in calls {
        let Request::Command(args) = call else {
            panic!("command expected")
        };
        assert!(!args.contains(&"--mixed".into()));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--token-env", "CUSTOM_TOKEN"])
        );
    }
    let (calls, _) = fixture.session("a3\n", &[]);
    assert!(
        calls.is_empty(),
        "EOF must not accept a default-yes confirmation"
    );
    fixture.clean();
}

#[test]
fn general_build_stops_after_each_failed_step_and_never_skips_dry_run() {
    let fixture = Fixture::new();
    for (codes, expected) in [(&[1][..], 1), (&[0, 2][..], 2), (&[0, 0, 3][..], 3)] {
        let (calls, output) =
            fixture.session("a1\ninput with spaces\nmyset\nMy Emoji\n\n\nexit\n", codes);
        assert_eq!(calls.len(), expected);
        assert_eq!(
            calls[0],
            command(&[
                "make-emoji-pngs",
                "--in",
                "input with spaces",
                "--out",
                "build/myset"
            ])
        );
        if expected > 1 {
            assert_eq!(
                calls[1],
                command(&[
                    "build-pack",
                    "--base",
                    "myset",
                    "--title",
                    "My Emoji",
                    "--source-dir",
                    "build/myset",
                    "--token-env",
                    "GENERAL_BOT_TOKEN",
                    "--emoji",
                    "😀",
                    "--dry-run"
                ])
            );
        }
        if expected == 3 {
            let Request::Command(args) = &calls[2] else {
                panic!("command expected")
            };
            assert!(!args.contains(&"--dry-run".into()));
            assert!(output.contains("partially finished"));
        }
    }
    let (calls, _) = fixture.session("a1\ninput with spaces\nmyset\nMy Emoji\n\nn\nexit\n", &[]);
    assert!(calls.is_empty());
    fixture.clean();
}

#[test]
fn collection_keeps_recorded_mode_and_requires_dry_run_then_preflight() {
    let fixture = Fixture::new();
    fixture.state(
        "collection/publish_myset.json",
        br#"{"sets":[{"fmt":"mixed","index":1}]}"#,
    );
    let (calls, _) = fixture.session("b3\nmyset\nMy Set\n\n\nexit\n", &[0, 0, 0]);
    assert_eq!(calls.len(), 3);
    for (index, call) in calls.iter().enumerate() {
        let Request::Command(args) = call else {
            panic!("command expected")
        };
        assert_eq!(
            &args[..7],
            [
                "build-collection",
                "--base",
                "myset",
                "--title",
                "My Set",
                "--token-env",
                "GENERAL_BOT_TOKEN"
            ]
        );
        assert!(args.contains(&"--mixed".into()));
        assert_eq!(args.contains(&"--dry-run".into()), index == 0);
        assert_eq!(args.contains(&"--preflight".into()), index == 1);
    }
    let (calls, output) = fixture.session("b3\nmyset\nMy Set\n\n\nexit\n", &[0, 4]);
    assert_eq!(calls.len(), 2);
    assert!(output.contains("Nothing was published"));
    fixture.state(
        "collection/publish_myset.json",
        br#"{"sets":[{"fmt":"mixed"},{"fmt":"static"}]}"#,
    );
    let (calls, output) = fixture.session("b3\nmyset\nMy Set\n\nexit\n", &[]);
    assert!(calls.is_empty());
    assert!(output.contains("ambiguous"));
    fixture.clean();
}

#[test]
fn destructive_rebuild_reorder_and_archive_default_to_no() {
    let fixture = Fixture::new();
    fixture.state("coins/rebuild_state.json", br#"{"sets":[{},{}]}"#);
    let (calls, output) = fixture.session("a3\n\nb5\nmyset\n\n\nd4\n\nexit\n", &[0, 3]);
    assert_eq!(
        calls,
        [
            command(&["sync-order", "--base", "myset"]),
            command(&["pack-archive", "--check"])
        ]
    );
    assert!(output.contains("DELETES the 2 pack(s)"));
    let (calls, _) = fixture.session("a3\ny\nb5\nmyset\n2\ny\nd4\ny\nexit\n", &[0, 0, 0, 3, 0]);
    assert_eq!(
        calls,
        [
            command(&["coin-rebuild"]),
            command(&["sync-order", "--base", "myset", "--pack", "2"]),
            command(&["sync-order", "--base", "myset", "--pack", "2", "--apply"]),
            command(&["pack-archive", "--check"]),
            command(&["pack-archive", "--sync"]),
        ]
    );
    let (calls, _) = fixture.session("b5\nmyset\n\ny\nexit\n", &[4]);
    assert_eq!(
        calls.len(),
        1,
        "failed report must not offer or execute apply"
    );
    fixture.clean();
}

#[test]
fn collection_lists_undo_one_entry_without_duplicating_it() {
    let fixture = Fixture::new();
    let (calls, _) = fixture.session(
        "b1\nremoved\n0\nkept pack\n\n\nb0\n111111111\n0\n222222222\n\nexit\n",
        &[],
    );
    assert_eq!(
        calls,
        [
            command(&[
                "fetch-pack",
                "kept pack",
                "--token-env",
                "GENERAL_BOT_TOKEN"
            ]),
            command(&["fetch-emoji-ids", "--id", "222222222"]),
        ]
    );
    let (calls, output) = fixture.session("b0\n111111111\n0\n\nexit\n", &[]);
    assert!(calls.is_empty());
    assert!(output.contains("No ids entered"));
    let (calls, _) = fixture.session("b1\nfirst\n\n0\nsecond\n\n\nexit\n", &[]);
    assert_eq!(
        calls,
        [command(&[
            "fetch-pack",
            "first",
            "second",
            "--token-env",
            "GENERAL_BOT_TOKEN"
        ])]
    );
    fixture.clean();
}

#[test]
fn panel_roster_status_bot_checks_and_local_ingest_keep_their_distinct_requests() {
    let fixture = Fixture::new();
    fixture.state(
        "collection/publish_first.json",
        br#"{"sets":[{"index":2},{"index":1},{"index":2}]}"#,
    );
    let (calls, _) = fixture.session(
        "b4\ny\nd3\n\nd2\nc1\nd1\nb2\ninput with spaces\n\nexit\n",
        &[0, 3, 0, 3, 0, 0, 0],
    );
    assert_eq!(
        calls,
        [
            command(&["panel", "--with-pack", "1", "--with-pack", "2"]),
            command(&["pack-manifest", "--check"]),
            command(&["pack-manifest", "--refresh"]),
            command(&["status"]),
            command(&["emoji-bot"]),
            Request::Checks,
            command(&["add-media", "--in", "input with spaces", "--emoji", "😀"]),
        ]
    );
    fixture.clean();
}

#[test]
fn missing_malformed_and_linked_family_evidence_never_becomes_a_publish_decision() {
    let fixture = Fixture::new();
    fixture.state("collection/publish_myset.json", b"not JSON");
    let (calls, _) = fixture.session("b3\nmyset\nMy Set\n\nexit\n", &[]);
    assert!(calls.is_empty());
    fixture.state(
        "collection/publish_myset.json",
        br#"{"sets":"not an array"}"#,
    );
    let (calls, _) = fixture.session("b3\nmyset\nMy Set\n\nexit\n", &[]);
    assert!(calls.is_empty());
    #[cfg(unix)]
    {
        let target = fixture.0.join("collection/publish_myset.json");
        std::fs::remove_file(&target).unwrap();
        std::os::unix::fs::symlink("missing.json", &target).unwrap();
        let (calls, _) = fixture.session("b3\nmyset\nMy Set\n\nexit\n", &[]);
        assert!(calls.is_empty());
        std::fs::remove_file(target).unwrap();
    }
    fixture.clean();
}

#[test]
fn original_cli_reference_discovers_native_sections_and_retained_coin_tools() {
    let guide = format!(
        "{}\n{}",
        include_str!("../../docs/guide-cli.md"),
        include_str!("../../docs/guide-cli-panel-coin.md")
    );
    assert!(!guide.contains(".venv\\Scripts\\.\\native\\runtime"));
    let sections = cli_reference::sections(&guide).unwrap();
    let commands = cli_reference::inventory(&sections).unwrap();
    assert_eq!(commands.len(), 23);
    assert!(!commands.contains("emoji-bot"));
    assert!(commands.contains("coin-rebuild-map"));
}

#[test]
fn cli_reference_empty_missing_and_stale_canaries_fail_closed() {
    cli_reference::canaries();
}

/// The original menu and GUIDE contracts share one capture per unique safe-help command.
#[test]
fn every_menu_command_and_flag_exists_in_real_native_help() {
    use _native::{logging::RunLog, tooling_install, tooling_process, tooling_test_runner};
    use std::collections::{BTreeMap, BTreeSet};
    use std::time::Duration;

    let fixture = Fixture::new();
    fixture.state("coins/rebuild_state.json", br#"{"sets":[{}]}"#);
    fixture.state(
        "collection/publish_first.json",
        br#"{"sets":[{"index":1}]}"#,
    );
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::fs::create_dir(fixture.0.join("assets")).unwrap();
    for member in ["assets/panel.html", "pyproject.toml"] {
        std::fs::copy(project.join(member), fixture.0.join(member)).unwrap();
    }
    let binary = tooling_install::install_executable(
        &fixture.0,
        Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
    )
    .unwrap();
    let mut commands = BTreeMap::<String, BTreeSet<String>>::new();
    for script in [
        "a1\ninput with spaces\nmyset\nMy Set\n\ny\nexit\n",
        "a2\ninput with spaces\noutput café\nexit\n",
        "a3\ny\nexit\n",
        "b0\n111111111\n\nexit\n",
        "b1\nexample-pack\n\nGENERAL_BOT_TOKEN\nexit\n",
        "b2\ninput with spaces\n😀\nexit\n",
        "b3\nmyset\nMy Set\nGENERAL_BOT_TOKEN\ny\ny\nexit\n",
        "b4\ny\nb5\nmyset\n1\ny\nexit\n",
        "c1\nd1\nd2\nd3\ny\nd4\ny\nexit\n",
    ] {
        let codes: &[i32] = if script.starts_with("c1\n") {
            &[0, 0, 0, 3, 0, 3, 0]
        } else {
            &[]
        };
        let (requests, _) = fixture.session(script, codes);
        for request in requests {
            let args = match request {
                Request::Command(args) => args,
                Request::Checks => vec!["check".into()],
            };
            let flags = commands.entry(args[0].clone()).or_default();
            flags.extend(
                args[1..]
                    .iter()
                    .filter(|arg| arg.starts_with("--"))
                    .cloned(),
            );
        }
    }
    // A scanner/driver that observes no requests must fail, not certify an empty set.
    for command in [
        "build-pack",
        "build-collection",
        "panel",
        "fetch-pack",
        "fetch-emoji-ids",
        "sync-order",
        "pack-manifest",
        "pack-archive",
        "status",
        "make-emoji-pngs",
        "add-media",
        "emoji-bot",
        "coin-rebuild",
        "check",
    ] {
        assert!(
            commands.contains_key(command),
            "menu never produced {command}"
        );
    }
    assert!(commands["panel"].contains("--with-pack"));
    assert!(commands["fetch-pack"].contains("--token-env"));
    let sections = cli_reference::sections(&format!(
        "{}\n{}",
        include_str!("../../docs/guide-cli.md"),
        include_str!("../../docs/guide-cli-panel-coin.md")
    ))
    .unwrap();
    for command in cli_reference::inventory(&sections).unwrap() {
        commands.entry(command).or_default();
    }
    for command in ["identity-repair", "plan-apply", "plan-status"] {
        commands.entry(command.into()).or_default();
    }
    // Invalid local configuration must remain unread under the already installed guard.
    fixture.state(".env", b"GENERAL_BOT_TOKEN=\xff");
    let environment = tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(&fixture.0, "menu-help-test", vec![]);
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let invoke = |arguments: &[&str]| {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "native help cohort exceeded its deadline"
        );
        let mut argv = vec![binary.clone().into_os_string()];
        argv.extend(
            arguments
                .iter()
                .map(|argument| std::ffi::OsString::from(*argument)),
        );
        runtime
            .block_on(tooling_process::capture(
                &fixture.0,
                &argv,
                &environment,
                Duration::from_secs(5).min(remaining),
                Duration::from_secs(3).min(remaining),
                &log,
            ))
            .unwrap()
    };
    let top = invoke(&["--help"]);
    assert_eq!(top.code, 0, "{}", top.stderr);
    assert!(top.stderr.is_empty());
    let advertised = top.stdout.split_whitespace().collect::<BTreeSet<_>>();
    for command in [
        "panel",
        "add-media",
        "fetch-pack",
        "fetch-emoji-ids",
        "build-collection",
        "build-pack",
        "emoji-bot",
        "sync-order",
        "plan-apply",
        "make-emoji-pngs",
        "plan-status",
        "pack-manifest",
        "status",
        "pack-archive",
        "identity-repair",
    ] {
        assert!(
            advertised.contains(command),
            "top-level help omitted {command}"
        );
    }
    let contradictory = invoke(&["identity-repair", "report", "restore"]);
    assert_eq!(contradictory.code, 2, "{}", contradictory.stderr);
    assert!(
        contradictory
            .stderr
            .contains("only one identity-repair command")
    );
    assert!(!contradictory.stdout.contains("no catalog"));
    let mut help = BTreeMap::new();
    for (command, flags) in commands {
        if command == "emoji-bot" {
            assert!(
                flags.is_empty(),
                "bot accepts configuration only, not command flags"
            );
            continue;
        }
        let result = invoke(&[command.as_str(), "--help"]);
        assert_eq!(
            result.code, 0,
            "{command}: {}{}",
            result.stdout, result.stderr
        );
        for flag in flags {
            assert!(
                result.stdout.contains(&flag),
                "menu passes unsupported {flag} to {command}"
            );
        }
        if command == "coin-rebuild" {
            assert!(
                result.stdout.contains("all"),
                "default coin rebuild command disappeared"
            );
        }
        assert!(help.insert(command, result.stdout).is_none());
    }
    cli_reference::check(&sections, &help).unwrap();
    assert_eq!(help.len(), 27, "each safe native entry point runs once");
    let worker = invoke(&["codec-worker"]);
    assert_eq!(worker.code, 2);
    assert!(
        worker
            .stderr
            .contains("codec-worker requires request and response paths")
    );
    assert_eq!(
        std::fs::read(fixture.0.join(".env")).unwrap(),
        b"GENERAL_BOT_TOKEN=\xff"
    );
    assert!(!fixture.0.join("state_emoji_bot.json").exists());
    assert!(std::time::Instant::now() < deadline);
    runtime.shutdown_timeout(Duration::from_secs(1));
    assert!(!fixture.0.join("collection/catalog.db").exists());
    assert!(!fixture.0.join("native/target").exists());
    assert_eq!(
        std::fs::read_dir(fixture.0.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    drop(log);
    fixture.clean();
}

#[test]
fn invalid_utf8_oversized_input_and_executor_errors_are_not_silent_success() {
    let fixture = Fixture::new();
    for bytes in [
        vec![255, b'\n'],
        vec![b'a'; 16 * 1024 + 1],
        b"d2\x1b[2J\n".to_vec(),
    ] {
        let mut output = Vec::new();
        let result = tooling_menu::run(
            &fixture.0,
            &mut Cursor::new(bytes),
            &mut output,
            false,
            |_| panic!("invalid input executed"),
        );
        assert!(result.is_err());
    }
    let mut output = Vec::new();
    let mut calls = 0;
    let result = tooling_menu::run(
        &fixture.0,
        &mut Cursor::new(b"d2\nd2\nexit\n"),
        &mut output,
        false,
        |_| {
            calls += 1;
            Err("synthetic execution refusal".into())
        },
    );
    assert_eq!(
        calls, 1,
        "uncertain execution must stop before the next action"
    );
    assert_eq!(result.unwrap_err(), "synthetic execution refusal");
    assert!(!String::from_utf8(output).unwrap().contains("Bye."));
    assert!(!fixture.0.join("native/target").exists());
    fixture.clean();
}
