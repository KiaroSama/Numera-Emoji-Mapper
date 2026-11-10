//! The original credential guard must precede native suite execution, not a Python import.
use _native::{logging::RunLog, tooling_process, tooling_test_runner};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
#[path = "support/tooling_observation.rs"]
mod tooling_observation;

#[test]
fn guarded_configuration_ignores_dotenv_and_preserves_project_paths_and_parent_environment() {
    let root = tooling_fixture::Root::new("guarded configuration café");
    let path = root.0.join(".env");
    let parent_before = std::env::vars_os().collect::<BTreeMap<_, _>>();
    let inherited = BTreeMap::from([
        ("GENERAL_BOT_TOKEN".into(), "synthetic-parent-token".into()),
        ("OTHER_API_KEY_SUFFIX".into(), "synthetic-parent-key".into()),
        ("PACK_OWNER_USER_ID".into(), "222222222".into()),
    ]);
    let before = inherited.clone();
    let guarded = tooling_test_runner::scrubbed_environment(&inherited);
    for bytes in [
        b"GENERAL_BOT_TOKEN=synthetic-dotenv-token\nPACK_OWNER_USER_ID=222222222\nUNLISTED_VALUE=synthetic-dotenv-only\n".as_slice(),
        b"GENERAL_BOT_TOKEN=\xff".as_slice(),
    ] {
        std::fs::write(&path, bytes).unwrap();
        let config = _native::config::Config::load_with_environment(root.0.clone(), &guarded)
            .unwrap();
        for key in ["GENERAL_BOT_TOKEN", "OTHER_API_KEY_SUFFIX", "PACK_OWNER_USER_ID", "UNLISTED_VALUE"] {
            assert_eq!(config.value(key), "", "guard loaded {key}");
        }
        assert_eq!(config.value("NUMERA_EMOJI_MAPPER_NO_DOTENV"), "1");
        assert_eq!(config.root, root.0);
        assert_eq!(config.path("state_emoji_bot.json"), root.0.join("state_emoji_bot.json"));
        assert_eq!(config.path("logs"), root.0.join("logs"));
        assert_eq!(config.path("collection/catalog.db"), root.0.join("collection/catalog.db"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    }
    assert_eq!(inherited, before);
    assert_eq!(
        std::env::vars_os().collect::<BTreeMap<_, _>>(),
        parent_before
    );
    assert!(!root.0.join("collection").exists());
    assert!(!root.0.join("state_emoji_bot.json").exists());
    root.clean().unwrap();
}

#[test]
fn test_environment_keeps_only_runtime_prerequisites_and_synthetic_operator() {
    let original = [
        ("PATH", "runtime path"),
        ("SystemRoot", "system root"),
        ("PYO3_PYTHON", "python"),
        ("PYTHONHOME", "external runtime"),
        ("NUMERA_STATIC_CONVERT_ORACLE", "original observations"),
        (
            "NUMERA_REAL_CATALOG_FIXTURE",
            "explicit local catalog fixture",
        ),
        (
            "NUMERA_REAL_COIN_DIRECTORY",
            "explicit local coin directory",
        ),
        (
            "NUMERA_REAL_VIDEO_FIXTURE",
            "explicit local real-video fixture",
        ),
        ("GENERAL_BOT_TOKEN", "synthetic-parent-token"),
        ("OTHER_API_KEY_SUFFIX", "synthetic-parent-key"),
        ("mixed_Secret", "synthetic-parent-secret"),
        ("PASSWORD", "synthetic-parent-password"),
        ("PACK_OWNER_USER_ID", "222222222"),
        ("BOT_ALLOWED_USER_IDS", "222222222"),
        ("PACK_LINKS_CHAT_ID", "222222222"),
        ("BRAND_LOGO_PATH", "operator artwork"),
        ("BRAND_LOGO_BOTS", "RealOperatorBot"),
        ("COIN_PACK_BASE", "operator pack"),
        ("WORKER_PUBLISH_URL", "https://example.invalid/publish"),
        ("HTTP_PROXY", "http://example.invalid"),
        ("PYTHONPATH", "untrusted imports"),
        ("GIT_CONFIG_COUNT", "1"),
        ("NUMERA_UNRECOGNIZED_SETTING", "not a prerequisite"),
        ("NUMERA_TEST_WORKSPACE", "untrusted previous owner"),
        ("PLAYWRIGHT_BROWSERS_PATH", "foreign browser cache"),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value.into()))
    .collect::<BTreeMap<_, _>>();
    let before = original.clone();
    let child = tooling_test_runner::scrubbed_environment(&original);
    for key in [
        "PATH",
        "SystemRoot",
        "PYO3_PYTHON",
        "PYTHONHOME",
        "NUMERA_STATIC_CONVERT_ORACLE",
        "NUMERA_REAL_CATALOG_FIXTURE",
        "NUMERA_REAL_COIN_DIRECTORY",
        "NUMERA_REAL_VIDEO_FIXTURE",
    ] {
        assert_eq!(child[key], original[key]);
    }
    for key in [
        "GENERAL_BOT_TOKEN",
        "OTHER_API_KEY_SUFFIX",
        "mixed_Secret",
        "PASSWORD",
        "PACK_OWNER_USER_ID",
        "BOT_ALLOWED_USER_IDS",
        "PACK_LINKS_CHAT_ID",
        "BRAND_LOGO_PATH",
        "WORKER_PUBLISH_URL",
        "HTTP_PROXY",
        "PYTHONPATH",
        "GIT_CONFIG_COUNT",
        "NUMERA_UNRECOGNIZED_SETTING",
        "NUMERA_TEST_WORKSPACE",
        "PLAYWRIGHT_BROWSERS_PATH",
    ] {
        assert!(!child.contains_key(key), "inherited {key}");
    }
    assert_eq!(
        child.len(),
        18,
        "an unreviewed variable entered the allowlist"
    );
    assert_eq!(child["TELEGRAM_API_BASE"], "http://127.0.0.1:9");
    assert_eq!(child["NUMERA_EMOJI_MAPPER_NO_DOTENV"], "1");
    assert_eq!(child["BRAND_LOGO_BOTS"], "");
    assert_eq!(child["COLLECTION_PACK_BASE"], "YourBrand_Emoji_Packs");
    assert_eq!(child["COIN_PACK_BASE"], "cryptoemoji");
    assert_eq!(child["COIN_PACK_TITLE"], "@YourBrand Crypto Emoji");
    for key in [
        "PYTHONUTF8",
        "PYTHONNOUSERSITE",
        "PYTHONSAFEPATH",
        "RUST_BACKTRACE",
    ] {
        assert_eq!(child[key], "1");
    }
    assert_eq!(original, before, "the parent environment is borrowed only");
}

#[test]
fn borrowed_runtime_key_spelling_matches_the_host_without_admitting_other_variables() {
    let original = [
        ("Path", "selected executable search"),
        ("sYsTeMrOoT", "loader prerequisite"),
        ("pYo3_PyThOn", "selected interpreter"),
        ("numera_real_catalog_fixture", "explicit local fixture"),
        ("gEnErAl_BoT_tOkEn", "synthetic-parent-token"),
        ("hTtP_pRoXy", "http://example.invalid"),
        ("bRaNd_LoGo_BoTs", "operator-only"),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value.into()))
    .collect::<BTreeMap<_, _>>();
    let before = original.clone();
    let child = tooling_test_runner::scrubbed_environment(&original);
    for (key, value) in [
        ("PATH", "selected executable search"),
        ("SystemRoot", "loader prerequisite"),
        ("PYO3_PYTHON", "selected interpreter"),
        ("NUMERA_REAL_CATALOG_FIXTURE", "explicit local fixture"),
    ] {
        assert_eq!(
            child.get(key).map(String::as_str),
            cfg!(windows).then_some(value)
        );
    }
    for key in original.keys() {
        assert!(
            !child.contains_key(key),
            "noncanonical or private key survived"
        );
    }
    assert_eq!(child["BRAND_LOGO_BOTS"], "");
    assert_eq!(child.len(), if cfg!(windows) { 14 } else { 10 });
    assert_eq!(original, before);
}

fn exact_fixture(name: &str) -> bool {
    let arguments = std::env::args().collect::<Vec<_>>();
    arguments.windows(2).any(|pair| pair == ["--exact", name])
}

#[test]
fn runner_environment_fixture() {
    if !exact_fixture("runner_environment_fixture") {
        return;
    }
    for key in [
        "GENERAL_BOT_TOKEN",
        "OTHER_API_KEY_SUFFIX",
        "UNRELATED_PRIVATE_VALUE",
        "PYTHONPATH",
        "HTTP_PROXY",
    ] {
        assert!(std::env::var_os(key).is_none(), "inherited {key}");
    }
    assert_eq!(std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").unwrap(), "1");
    assert_eq!(
        std::env::var("TELEGRAM_API_BASE").unwrap(),
        "http://127.0.0.1:9"
    );
    assert_eq!(std::env::var("COIN_PACK_BASE").unwrap(), "cryptoemoji");
    let root = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    for key in ["TEMP", "TMP", "TMPDIR"] {
        let path = PathBuf::from(std::env::var_os(key).unwrap());
        assert!(path.starts_with(root.join("logs/tmp")));
    }
    assert_eq!(
        PathBuf::from(std::env::var_os("PLAYWRIGHT_BROWSERS_PATH").unwrap()),
        root.join(".cache/ms-playwright")
    );
    let workspace = _native::tooling_environment::physical_root(Path::new(
        &std::env::var_os("NUMERA_TEST_WORKSPACE").unwrap(),
    ))
    .unwrap();
    assert_eq!(
        workspace.parent(),
        Some(root.join("logs/tmp/tooling").as_path())
    );
    assert_eq!(std::fs::read_dir(workspace).unwrap().count(), 0);
    assert!(
        _native::telegram::Telegram::new("synthetic".into(), "https://example.invalid".into())
            .is_err()
    );
    assert!(_native::coin_http::endpoint("https://example.invalid", true, None).is_err());
    println!("runner UTF-8: café 🦀");
}

#[cfg(target_os = "linux")]
fn assert_loopback_only_network() {
    use std::io::{Read, Write};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
    for ip in [
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    ] {
        let listener = TcpListener::bind(SocketAddr::new(ip, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let mut sent = TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
        sent.set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        sent.write_all(b"L").unwrap();
        listener.set_nonblocking(true).unwrap();
        let (mut received, peer) = listener.accept().unwrap();
        assert!(peer.ip().is_loopback());
        received
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut byte = [0];
        received.read_exact(&mut byte).unwrap();
        assert_eq!(byte, *b"L");
        let socket = UdpSocket::bind(SocketAddr::new(ip, 0)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        socket.send_to(b"U", socket.local_addr().unwrap()).unwrap();
        let (length, peer) = socket.recv_from(&mut byte).unwrap();
        assert_eq!((length, byte), (1, *b"U"));
        assert!(peer.ip().is_loopback());
    }
    // Documentation-only destinations must fail at the local routing boundary, not by timeout.
    for (local, destination) in [("0.0.0.0:0", "192.0.2.1:9"), ("[::]:0", "[2001:db8::1]:9")] {
        let destination: SocketAddr = destination.parse().unwrap();
        let error = TcpStream::connect_timeout(&destination, Duration::from_secs(1)).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::ENETUNREACH));
        let socket = UdpSocket::bind(local).unwrap();
        let error = socket.send_to(b"denied", destination).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::ENETUNREACH));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn runner_network_descendant_fixture() {
    if !exact_fixture("runner_network_descendant_fixture") {
        return;
    }
    assert_loopback_only_network();
    println!("separate-session loopback-only network verified");
}

#[cfg(target_os = "linux")]
#[test]
fn runner_network_fixture() {
    if !exact_fixture("runner_network_fixture") {
        return;
    }
    // The outer trace passes an intentionally inheritable socket number as an argument.
    let arguments = std::env::args().collect::<Vec<_>>();
    let ancestor_identity =
        if let Some(index) = arguments.iter().position(|argument| argument == "--skip") {
            let mut observation = arguments[index + 1]
                .strip_prefix("closed-fd-")
                .unwrap()
                .split(':');
            let descriptor = observation.next().unwrap().parse::<i32>().unwrap();
            let device = observation.next().unwrap().parse::<u64>().unwrap();
            let inode = observation.next().unwrap().parse::<u64>().unwrap();
            assert!(observation.next().is_none());
            // SAFETY: F_GETFD is a read-only descriptor query; no unrelated descriptor is closed.
            let result = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
            assert_eq!(result, -1);
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::EBADF)
            );
            (device, inode)
        } else {
            panic!("inherited descriptor observation is missing");
        };
    {
        use std::os::unix::fs::MetadataExt;
        let before = std::fs::metadata("/proc/self/ns/net").unwrap();
        assert_ne!((before.dev(), before.ino()), ancestor_identity);
        // Cross-user-namespace ptrace admission rejects opening the ancestor namespace itself.
        let parent = unsafe { libc::getppid() };
        let error = std::fs::File::open(format!("/proc/{parent}/ns/net")).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::EACCES));
        let after = std::fs::metadata("/proc/self/ns/net").unwrap();
        assert_eq!((after.dev(), after.ino()), (before.dev(), before.ino()));
    }
    assert_loopback_only_network();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture = tooling_fixture::Root::new("isolated network descendant");
    let owner_path = fixture.0.join("owner");
    std::fs::create_dir(&owner_path).unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "runner_network_descendant_fixture",
            "--nocapture",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut owner =
        _native::process_owner::OwnedProcess::spawn_session(&mut command, owner_path).unwrap();
    drop(command);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let status = runtime.block_on(owner.wait(Duration::from_secs(5), Duration::from_secs(3)));
    let finished = owner.finish();
    finished.unwrap();
    assert!(
        status.unwrap().success(),
        "isolated descendant network assertions failed"
    );
    assert!(fixture.0.starts_with(project));
    fixture.clean().unwrap();
    println!("root and separate-session loopback-only network verified");
}

#[test]
fn runner_failure_fixture() {
    if exact_fixture("runner_failure_fixture") {
        println!("ok runner_failure_fixture");
        panic!("intentional runner fixture failure");
    }
}

#[test]
fn runner_early_exit_fixture() {
    if exact_fixture("runner_early_exit_fixture") {
        println!("ok runner_early_exit_fixture");
        std::process::exit(0);
    }
}

#[test]
fn actual_runner_scrubs_before_child_and_preserves_failure_without_leaking_scratch() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let base = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let scratch = base.join(format!("test runner trace {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let log = RunLog::open(&scratch, "runner-trace", vec![]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut environment = tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    for key in [
        "GENERAL_BOT_TOKEN",
        "OTHER_API_KEY_SUFFIX",
        "UNRELATED_PRIVATE_VALUE",
        "PYTHONPATH",
        "HTTP_PROXY",
    ] {
        environment.insert(key.into(), "synthetic-never-forwarded".into());
    }
    if !cfg!(any(windows, target_os = "linux", target_os = "macos")) {
        let argv = [
            OsString::from(env!("CARGO_BIN_EXE_numera-emoji")),
            "test-runner".into(),
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            "runner_failure_fixture".into(),
        ];
        let result = runtime
            .block_on(tooling_process::capture(
                &scratch,
                &argv,
                &environment,
                Duration::from_secs(8),
                Duration::from_secs(4),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, 1);
        assert!(
            result
                .stderr
                .contains("nested-session cleanup is not verified; no child started")
        );
        assert!(!result.stdout.contains("intentional runner fixture failure"));
        assert_eq!(
            std::fs::read_dir(scratch.join("logs/tmp/tooling"))
                .unwrap()
                .count(),
            0
        );
        drop(log);
        std::fs::remove_dir_all(scratch).unwrap();
        return;
    }
    let plan_output = scratch.join("not-created-plan.json");
    for (command, tail, expected) in [
        ("test-plan", vec!["--help"], 0),
        (
            "test-plan",
            vec![
                "--baseline",
                "invalid",
                "--output",
                plan_output.to_str().unwrap(),
            ],
            1,
        ),
        ("test-cargo", vec!["--help"], 0),
        ("test-cargo", vec!["--baseline", "invalid", "--"], 1),
    ] {
        let mut argv = vec![
            OsString::from(env!("CARGO_BIN_EXE_numera-emoji")),
            command.into(),
        ];
        argv.extend(tail.into_iter().map(OsString::from));
        let result = runtime
            .block_on(tooling_process::capture(
                &scratch,
                &argv,
                &environment,
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, expected, "{}{}", result.stdout, result.stderr);
        if expected == 0 {
            assert!(result.stdout.contains(&format!("{command} --baseline")));
        }
        assert!(!plan_output.exists());
    }
    let manifest = tooling_observation::manifest(root, &scratch);
    let (cargo_plan, cargo_build) = tooling_observation::cargo_inputs(root, &manifest);
    let mut cases = vec![
        ("runner_environment_fixture", 0, "runner UTF-8: café 🦀"),
        (
            "runner_early_exit_fixture",
            1,
            "ok runner_early_exit_fixture",
        ),
        (
            "runner_failure_fixture",
            101,
            "intentional runner fixture failure",
        ),
    ];
    if cfg!(target_os = "linux") {
        cases.push((
            "runner_network_fixture",
            0,
            "root and separate-session loopback-only network verified",
        ));
    }
    for (fixture, expected_code, text) in cases {
        #[cfg(target_os = "linux")]
        let inherited_socket = if fixture == "runner_network_fixture" {
            use std::os::fd::{AsRawFd, FromRawFd};
            let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            // SAFETY: duplicate our own socket at a high descriptor without CLOEXEC for this tracer.
            let descriptor = unsafe { libc::fcntl(socket.as_raw_fd(), libc::F_DUPFD, 200) };
            assert!(descriptor >= 200);
            Some(unsafe { std::os::fd::OwnedFd::from_raw_fd(descriptor) })
        } else {
            None
        };
        let mut argv = vec![
            OsString::from(env!("CARGO_BIN_EXE_numera-emoji")),
            "test-runner".into(),
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            fixture.into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ];
        if fixture == "runner_environment_fixture" {
            argv.splice(
                2..2,
                [
                    "--observe-cargo".into(),
                    cargo_plan.clone().into_os_string(),
                    cargo_build.clone().into_os_string(),
                ],
            );
        } else if fixture != "runner_network_fixture" {
            argv.splice(
                2..2,
                ["--observe".into(), manifest.clone().into_os_string()],
            );
        }
        #[cfg(target_os = "linux")]
        if let Some(socket) = &inherited_socket {
            use std::os::fd::AsRawFd;
            use std::os::unix::fs::MetadataExt;
            let namespace = std::fs::metadata("/proc/self/ns/net").unwrap();
            argv.extend([
                "--skip".into(),
                format!(
                    "closed-fd-{}:{}:{}",
                    socket.as_raw_fd(),
                    namespace.dev(),
                    namespace.ino()
                )
                .into(),
            ]);
        }
        let result = runtime
            .block_on(tooling_process::capture(
                &scratch,
                &argv,
                &environment,
                Duration::from_secs(20),
                Duration::from_secs(10),
                &log,
            ))
            .unwrap();
        assert_eq!(
            result.code, expected_code,
            "{}{}",
            result.stdout, result.stderr
        );
        assert!(format!("{}{}", result.stdout, result.stderr).contains(text));
        if fixture != "runner_network_fixture" {
            tooling_observation::check(root, fixture, result.code, &result.stderr);
        }
        assert_eq!(
            std::fs::read_dir(scratch.join("logs/tmp/tooling"))
                .unwrap()
                .count(),
            0
        );
    }
    #[cfg(target_os = "linux")]
    {
        let target = std::env::current_exe().unwrap();
        for tail in [
            vec![
                "--".into(),
                target.into_os_string(),
                "--exact".into(),
                "runner_failure_fixture".into(),
            ],
            vec!["--help".into()],
        ] {
            // Default piped stdout/stderr are deliberately inadmissible bootstrap stdio.
            let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_numera-emoji"));
            command
                .arg("test-net-bootstrap")
                .args(tail)
                .current_dir(&scratch)
                .env_clear()
                .envs(&environment)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped());
            let owned = tooling_process::workspace(&scratch).unwrap();
            let mut owner =
                _native::process_owner::OwnedProcess::spawn(&mut command, owned).unwrap();
            drop(command);
            let waited =
                runtime.block_on(owner.wait(Duration::from_secs(5), Duration::from_secs(3)));
            let finished = owner.finish();
            finished.unwrap();
            assert_eq!(
                waited.unwrap().code(),
                Some(2),
                "setup failure must precede the failing target"
            );
        }
    }
    drop(log);
    // Failure above retains the exact trace rather than erasing unresolved ownership.
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn suite_workspace_waits_for_outer_custody_and_retains_failed_or_unready_evidence() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let base = _native::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let root = base.join(format!("suite workspace policy {}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let owned = tooling_process::workspace(&root).unwrap();
    let fixture = owned.join("native-browser-fixture");
    std::fs::create_dir(&fixture).unwrap();
    let diagnostic = fixture.join("trace.zip");
    std::fs::write(&diagnostic, b"synthetic failure evidence").unwrap();
    tooling_test_runner::clean_suite_workspace(&root, &owned, false).unwrap();
    assert_eq!(
        std::fs::read(&diagnostic).unwrap(),
        b"synthetic failure evidence"
    );
    assert!(tooling_test_runner::clean_suite_workspace(&root, &owned, true).is_err());
    std::fs::write(fixture.join("cleanup-ready"), b"native-browser-stopped-v1").unwrap();
    std::fs::write(
        fixture.join("codec-cleanup-pending.json"),
        b"unresolved synthetic owner",
    )
    .unwrap();
    assert!(tooling_test_runner::clean_suite_workspace(&root, &owned, true).is_err());
    assert_eq!(
        std::fs::read(&diagnostic).unwrap(),
        b"synthetic failure evidence"
    );
    // No process was created by this policy fixture.
    std::fs::remove_file(fixture.join("codec-cleanup-pending.json")).unwrap();
    tooling_test_runner::clean_suite_workspace(&root, &owned, true).unwrap();
    assert!(!owned.exists());
    let empty = tooling_process::workspace(&root).unwrap();
    tooling_test_runner::clean_suite_workspace(&root, &empty, false).unwrap();
    assert!(!empty.exists());
    let foreign = root.join("not-owned");
    std::fs::create_dir(&foreign).unwrap();
    assert!(tooling_test_runner::clean_suite_workspace(&root, &foreign, true).is_err());
    assert!(foreign.is_dir());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn libtest_arguments_preserve_selection_and_only_lower_parallelism() {
    for (input, expected) in [
        (
            vec!["--exact", "named"],
            vec!["--exact", "named", "--test-threads=2"],
        ),
        (
            vec!["--test-threads=8", "named"],
            vec!["--test-threads=2", "named"],
        ),
        (vec!["--test-threads", "1"], vec!["--test-threads", "1"]),
    ] {
        let input = input.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(tooling_test_runner::arguments(&input, 2).unwrap(), expected);
    }
    for input in [
        vec!["--test-threads=0"],
        vec!["--test-threads"],
        vec!["--test-threads=bad"],
    ] {
        let input = input.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert!(tooling_test_runner::arguments(&input, 2).is_err());
    }
}
