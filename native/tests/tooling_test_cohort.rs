//! A real dedicated runner must reclaim an orphan even after it creates another Linux session.
#![cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64"
))]
use _native::{logging::RunLog, tooling_process, tooling_test_runner};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// The supported package ABI is GNU Linux x86_64; no signal mutation occurs outside
// the exact-selected fixture process, which has one libtest worker and no children.
#[repr(C)]
#[derive(Default)]
struct SignalAction {
    handler: usize,
    mask: [u64; 16],
    flags: i32,
    restorer: usize,
}
const _: () = {
    assert!(std::mem::size_of::<SignalAction>() == 152);
    assert!(std::mem::offset_of!(SignalAction, flags) == 136);
    assert!(std::mem::offset_of!(SignalAction, restorer) == 144);
};

unsafe extern "C" {
    fn sigaction(signal: i32, action: *const SignalAction, previous: *mut SignalAction) -> i32;
    fn setsid() -> i32;
    fn pidfd_open(pid: i32, flags: u32) -> i32;
    fn pidfd_send_signal(fd: i32, signal: i32, info: *const std::ffi::c_void, flags: u32) -> i32;
    fn poll(descriptors: *mut PollFd, count: usize, timeout: i32) -> i32;
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

fn exited(fd: &OwnedFd, timeout: i32) -> Result<bool, String> {
    let mut descriptor = PollFd {
        fd: fd.as_raw_fd(),
        events: 1,
        revents: 0,
    };
    // SAFETY: one live descriptor and writable pollfd, finite nonnegative timeout.
    let ready = unsafe { poll(&mut descriptor, 1, timeout) };
    if ready < 0 {
        if std::io::Error::last_os_error().raw_os_error() == Some(4) {
            return Ok(false);
        }
        return Err("cannot poll pinned fixture termination".into());
    }
    if descriptor.revents & (8 | 32) != 0 {
        return Err("pinned fixture descriptor returned invalid/error state".into());
    }
    Ok(ready > 0 && descriptor.revents & (1 | 16) != 0)
}

fn selected(name: &str) -> bool {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| pair == ["--exact", name])
}

fn record(pid: u32) -> Result<Option<(u32, u64)>, String> {
    let raw = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(raw) => raw,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(3) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(format!("cannot inspect fixture identity: {error}")),
    };
    let (head, fields) = raw
        .rsplit_once(") ")
        .ok_or("malformed fixture process accounting")?;
    if head
        .split_whitespace()
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        != Some(pid)
    {
        return Err("fixture process accounting returned another PID".into());
    }
    let fields = fields.split_whitespace().collect::<Vec<_>>();
    let session = fields
        .get(3)
        .ok_or("missing fixture session")?
        .parse()
        .map_err(|_| "invalid fixture session")?;
    let birth = fields
        .get(19)
        .ok_or("missing fixture birth")?
        .parse()
        .map_err(|_| "invalid fixture birth")?;
    Ok(Some((session, birth)))
}

struct RestoreSignal(SignalAction);
impl Drop for RestoreSignal {
    fn drop(&mut self) {
        // SAFETY: exact prior action, restored inside the same isolated fixture process.
        assert_eq!(unsafe { sigaction(17, &self.0, std::ptr::null_mut()) }, 0);
    }
}

#[test]
fn no_child_wait_fixture() {
    if !selected("no_child_wait_fixture") {
        return;
    }
    let action = SignalAction {
        flags: 2,
        ..Default::default()
    };
    let mut previous = SignalAction::default();
    // SAFETY: supported ABI, initialized action/output; fixture has no other child owner.
    assert_eq!(unsafe { sigaction(17, &action, &mut previous) }, 0);
    let restore = RestoreSignal(previous);
    let mut observed = SignalAction::default();
    // SAFETY: query only into a live correctly sized action.
    assert_eq!(unsafe { sigaction(17, std::ptr::null(), &mut observed) }, 0);
    assert_eq!(observed.handler, 0);
    assert_ne!(observed.flags & 2, 0);
    let root = std::env::current_dir().unwrap();
    let missing = root.join("must-not-spawn");
    assert_eq!(
        std::fs::symlink_metadata(&missing).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    let workspace = root.join("refused-owner");
    std::fs::create_dir(&workspace).unwrap();
    let result = _native::process_owner::OwnedProcess::spawn_descendants(
        &mut Command::new(missing),
        workspace.clone(),
    );
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("SA_NOCLDWAIT unexpectedly admitted a child"),
    };
    assert_eq!(
        error,
        "test descendant owner requires default SIGCHLD without SA_NOCLDWAIT"
    );
    drop(restore);
    // Refusal precedes spawn, and the deliberately absent executable cannot run.
    std::fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn descendant_admission_rejects_default_sigchld_with_no_child_wait() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let scratch = parent.join(format!("signal admission {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let log = RunLog::open(&scratch, "signal-admission", vec![]);
    let environment = tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    let argv = [
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        "no_child_wait_fixture".into(),
        "--nocapture".into(),
        "--test-threads=1".into(),
    ];
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
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
    assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    assert!(!scratch.join("refused-owner").exists());
    assert_eq!(
        std::fs::read_dir(scratch.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    drop(log);
    std::fs::remove_dir_all(scratch).unwrap();
}

#[test]
fn orphan_fixture() {
    if !selected("orphan_fixture") {
        return;
    }
    let root = std::env::current_dir().unwrap();
    let pid = std::process::id();
    let (session, birth) = record(pid).unwrap().expect("live fixture identity");
    assert_eq!(session, pid, "fixture must establish a distinct session");
    std::fs::write(root.join("orphan-ready"), format!("{pid} {birth}")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::fs::write(
        root.join("orphan-expired"),
        b"runner failed to reclaim orphan",
    )
    .unwrap();
    panic!("orphan fixture expired");
}

#[test]
fn root_fixture() {
    if !selected("root_fixture") {
        return;
    }
    let root = std::env::current_dir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "orphan_fixture",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid is the only child-side operation and is async-signal-safe.
    unsafe {
        command.pre_exec(|| {
            if setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join("root-release").is_file() {
        assert!(Instant::now() < deadline, "root was not acknowledged");
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(child);
    // No Rust destructor runs; this is the failure shape a group-only owner misses.
    std::process::exit(37);
}

struct Rescue {
    fd: OwnedFd,
}
impl Drop for Rescue {
    fn drop(&mut self) {
        if exited(&self.fd, 0) != Ok(true) {
            // SAFETY: the owned pidfd pins only this acknowledged fixture, even if /proc is unreadable.
            let result = unsafe { pidfd_send_signal(self.fd.as_raw_fd(), 9, std::ptr::null(), 0) };
            assert!(result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(3));
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if exited(&self.fd, 20).expect("cannot verify pinned fixture rescue") {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "pinned fixture rescue exceeded 5s"
                );
            }
        }
    }
}

#[test]
fn dedicated_runner_reaps_separately_sessioned_orphan_and_preserves_root_status() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let scratch = parent.join(format!("test cohort {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let log = RunLog::open(&scratch, "cohort-test", vec![]);
    let environment = tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    let argv = [
        PathBuf::from(env!("CARGO_BIN_EXE_numera-compat")).into_os_string(),
        "test-runner".into(),
        std::env::current_exe().unwrap().into_os_string(),
        "--exact".into(),
        "root_fixture".into(),
        "--nocapture".into(),
        "--test-threads=1".into(),
    ];
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let rescue = runtime.block_on(async {
        let mut captured = Box::pin(tooling_process::capture(
            &scratch, &argv, &environment, Duration::from_secs(20), Duration::from_secs(10), &log,
        ));
        let deadline = Instant::now() + Duration::from_secs(5);
        let (pid, birth) = loop {
            if let Ok(raw) = std::fs::read_to_string(scratch.join("orphan-ready")) {
                let fields = raw.split_whitespace().collect::<Vec<_>>();
                if fields.len() == 2
                    && let (Ok(pid), Ok(birth)) = (fields[0].parse::<u32>(), fields[1].parse::<u64>()) { break (pid, birth); }
            }
            assert!(Instant::now() < deadline, "orphan did not become ready");
            tokio::select! {
                result = &mut captured => panic!("runner ended before acknowledgement: {:?}", result.err()),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
            }
        };
        // SAFETY: root is held by our release marker; this live fixture has acknowledged its birth.
        let fd = unsafe { pidfd_open(pid as i32, 0) };
        assert!(fd >= 0);
        let rescue = Rescue { fd: unsafe { OwnedFd::from_raw_fd(fd) } };
        assert_eq!(record(pid).unwrap(), Some((pid, birth)));
        assert!(!exited(&rescue.fd, 0).unwrap(), "fixture exited before release");
        std::fs::write(scratch.join("root-release"), b"exit without cleanup").unwrap();
        let result = captured.await.unwrap();
        assert_eq!(result.code, 37, "{}{}", result.stdout, result.stderr);
        assert!(exited(&rescue.fd, 0).unwrap(), "runner leaked pinned orphan");
        assert_ne!(record(pid).unwrap().map(|(_, current)| current), Some(birth), "runner left an unreaped orphan");
        assert!(!scratch.join("orphan-expired").exists());
        rescue
    });
    drop(rescue);
    drop(log);
    assert_eq!(
        std::fs::read_dir(scratch.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    std::fs::remove_dir_all(scratch).unwrap();
}
