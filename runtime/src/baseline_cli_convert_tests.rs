use super::*;
use crate::baseline_catalog_support::Scratch;
use std::collections::BTreeMap;
// tests.test_native_consumer_boundary.NativeConsumerBoundary.test_converter_idle_timeout_stops_owned_tree_without_waiting_minutes
#[test]
fn converter_idle_timeout_stops_real_owned_tree_without_waiting_minutes() {
    let root = Scratch::new();
    let executable = root
        .0
        .join(format!("idle-observer{}", std::env::consts::EXE_SUFFIX));
    let compiler = option_env!("NUMERA_BASELINE_RUSTC").unwrap_or("rustc");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/baseline_cli_status_support/watchdog_observer.rs");
    let result = Command::new(compiler)
        .args(["--edition=2024", "--crate-name=cli_watchdog_observer"])
        .arg(source)
        .arg("-o")
        .arg(&executable)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let mut env = std::env::vars().collect::<BTreeMap<_, _>>();
    env.insert(
        "NUMERA_CLI_WATCHDOG_ROOT".into(),
        root.0.display().to_string(),
    );
    let log = RunLog::open(&root.0, "idle-observer", vec![]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let code = runtime
        .block_on(run_attempt_with_launch(
            &root.0,
            &root.0.join("in"),
            &root.0.join("out"),
            &root.0.join("stdout"),
            &root.0.join("stderr"),
            &log,
            (
                (
                    &env,
                    Watchdog {
                        wall: Duration::from_secs(10),
                        idle: Duration::from_secs(2),
                        poll: Duration::from_millis(10),
                    },
                ),
                |root, args, env| {
                    assert_eq!(args[0], "make-emoji-pngs");
                    Ok(tooling_launch::Launch {
                        root: root.into(),
                        argv: vec![executable.into_os_string()],
                        environment: tooling_environment::environment(root, env, false)?,
                    })
                },
            ),
        ))
        .unwrap();
    assert_eq!(code, 124);
    assert_eq!(
        deadline_code(Duration::from_secs(121), Duration::from_secs(121)),
        Some(124)
    );
    for name in ["parent.pid", "descendant.pid"] {
        let pid = std::fs::read_to_string(root.0.join(name))
            .unwrap()
            .parse::<u32>()
            .unwrap();
        terminated(pid);
    }
    assert!(!crate::process_owner::cleanup_pending(&root.0));
    assert!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
}
#[cfg(target_os = "linux")]
fn terminated(pid: u32) {
    let value = std::fs::read_to_string(format!("/proc/{pid}/stat"));
    assert!(
        value.is_err() || value.unwrap().rsplit_once(") ").unwrap().1.starts_with('Z'),
        "owned process {pid} survived idle cleanup"
    );
}
#[cfg(windows)]
#[allow(unsafe_code)]
fn terminated(pid: u32) {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if process.is_null() {
            assert_eq!(GetLastError(), ERROR_INVALID_PARAMETER);
        } else {
            assert_eq!(WaitForSingleObject(process, 0), WAIT_OBJECT_0);
            CloseHandle(process);
        }
    }
}
