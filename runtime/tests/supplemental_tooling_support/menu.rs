//! Scripted public menu workflows preserve argv, confirmations and failure stops without children.
use numera_runtime::tooling_menu::{self, Request};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "../support/cli_reference.rs"]
mod cli_reference;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            numera_runtime::tooling_environment::confined_directory(project, "logs/tmp", true)
                .unwrap();
        let path = parent.join(format!(
            "native menu café {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join("input with spaces")).unwrap();
        Self(numera_runtime::tooling_environment::physical_root(&path).unwrap())
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
        include_str!("../../../docs/guide-cli.md"),
        include_str!("../../../docs/guide-cli-panel-coin.md")
    );
    assert!(!guide.contains(".venv\\Scripts\\.\\native\\runtime"));
    let sections = cli_reference::sections(&guide).unwrap();
    let commands = cli_reference::inventory(&sections).unwrap();
    assert!(commands.len() >= 23);
    assert!(!commands.contains("emoji-bot"));
    for command in [
        "coin-fetch-logos",
        "coin-build-keywords",
        "coin-fetch-paprika",
        "coin-fetch-cmc",
        "coin-rebuild",
        "coin-rebuild-map",
        "coin-remap-ids",
        "coin-verify-logos",
        "coin-check-packs",
        "coin-write-manifests",
        "coin-enhance-map",
        "coin-alias-map",
    ] {
        assert!(
            commands.contains(command),
            "documented native coin command missing: {command}"
        );
    }
}

#[test]
fn cli_reference_empty_missing_and_stale_canaries_fail_closed() {
    cli_reference::canaries();
}

#[test]
fn every_menu_command_and_flag_exists_in_real_native_help() {
    use numera_runtime::{logging::RunLog, tooling_process};
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
    std::fs::copy(
        project.join("assets/panel.html"),
        fixture.0.join("assets/panel.html"),
    )
    .unwrap();
    let binary = fixture
        .0
        .join(format!("numera-emoji{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &binary).unwrap();
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
        include_str!("../../../docs/guide-cli.md"),
        include_str!("../../../docs/guide-cli-panel-coin.md")
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
    let environment =
        numera_runtime::tooling_launch::scrubbed_environment(&std::env::vars().collect(), "");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(&fixture.0, "menu-help-test", vec![]);
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let invoke = |executable: &Path, arguments: &[&str]| {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "native help cohort exceeded its deadline"
        );
        let mut argv = vec![executable.as_os_str().to_owned()];
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
    let top = invoke(&binary, &["--help"]);
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
    let contradictory = invoke(&binary, &["identity-repair", "report", "restore"]);
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
        let result = invoke(&binary, &[command.as_str(), "--help"]);
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
    let developer = fixture
        .0
        .join(format!("numera-dev{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_numera-dev"), &developer).unwrap();
    let sandbox_help = invoke(&developer, &["--help"]);
    assert_eq!(sandbox_help.code, 0, "{}", sandbox_help.stderr);
    assert!(sandbox_help.stdout.contains("panel-sandbox"));
    cli_reference::check_with_sandbox_help(&sections, &help, &sandbox_help.stdout).unwrap();
    assert_eq!(help.len(), 29, "each safe native entry point runs once");
    let worker = invoke(&binary, &["codec-worker"]);
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
