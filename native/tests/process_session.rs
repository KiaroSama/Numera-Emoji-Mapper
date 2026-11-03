//! Optional session ownership and conservative recovery-marker admission.
use _native::process_owner::cleanup_pending;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("logs/tmp")
            .join(format!(
                "session fixture {}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn clean(self) {
        // Call only after all exact child identities and nested owners were checked.
        // No Drop cleanup: any panic/error retains the entire diagnostic fixture.
        assert!(
            !cleanup_pending(&self.0),
            "fixture still has ownership evidence"
        );
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn recovery_query_never_treats_an_unreadable_or_nonphysical_container_as_absent() {
    let root = Root::new();
    assert!(!cleanup_pending(&root.0.join("absent")));
    assert!(!cleanup_pending(&root.0));
    let owner = root.0.join("owner");
    std::fs::create_dir(&owner).unwrap();
    assert!(!cleanup_pending(&root.0));
    for directory in [&root.0, &owner] {
        let marker = directory.join("codec-cleanup-pending.json");
        std::fs::write(&marker, b"not parsed").unwrap();
        assert!(cleanup_pending(&root.0));
        std::fs::remove_file(&marker).unwrap();
        std::fs::create_dir(&marker).unwrap();
        assert!(cleanup_pending(&root.0));
        std::fs::remove_dir(&marker).unwrap();
    }
    std::fs::remove_dir(&owner).unwrap();
    std::fs::write(&owner, b"not a directory").unwrap();
    assert!(cleanup_pending(&root.0));
    std::fs::remove_file(&owner).unwrap();
    let file = root.0.join("file");
    std::fs::write(&file, b"not a workspace").unwrap();
    assert!(cleanup_pending(&file));
    // Invalid OS path is an inspection failure, never proof of absence.
    assert!(cleanup_pending(&root.0.join("invalid\0path")));
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(root.0.join("missing"), &owner).unwrap();
        assert!(cleanup_pending(&root.0));
        std::fs::remove_file(&owner).unwrap();
        let marker = root.0.join("codec-cleanup-pending.json");
        symlink(root.0.join("missing"), &marker).unwrap();
        assert!(cleanup_pending(&root.0));
        std::fs::remove_file(&marker).unwrap();
        let linked = root.0.join("linked");
        symlink(&root.0, &linked).unwrap();
        assert!(cleanup_pending(&linked));
        std::fs::remove_file(linked).unwrap();
    }
    assert!(!cleanup_pending(&root.0));
    root.clean();
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
mod lifecycle {
    use super::*;
    use _native::process_owner::OwnedProcess;
    use std::future::{Future, poll_fn};
    use std::process::{Command, Stdio};
    use std::task::Poll;
    use std::time::{Duration, Instant};

    const FIXTURE: &str = "lifecycle::fixture_process";
    const CONTROL: &str = "NUMERA_SESSION_FIXTURE";
    const ROLE: &str = "NUMERA_SESSION_ROLE";
    const READY: Duration = Duration::from_secs(5);

    fn command(control: &Path, role: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.env_clear();
        for key in [
            "PATH",
            "SystemRoot",
            "WINDIR",
            "LD_LIBRARY_PATH",
            "DYLD_LIBRARY_PATH",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "PYTHONHOME",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .args(["--exact", FIXTURE, "--nocapture", "--test-threads=1"])
            .env(CONTROL, control)
            .env(ROLE, role)
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("TEMP", control)
            .env("TMP", control)
            .env("TMPDIR", control)
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(control.join(format!("{role}-stdout.log"))).unwrap())
            .stderr(std::fs::File::create(control.join(format!("{role}-stderr.log"))).unwrap());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        command
    }

    fn ready(control: &Path, role: &str) -> u32 {
        let deadline = Instant::now() + READY;
        loop {
            if let Ok(text) = std::fs::read_to_string(control.join(format!("{role}-ready")))
                && let Ok(pid) = text.parse::<u32>()
            {
                return pid;
            }
            assert!(Instant::now() < deadline, "{role} readiness exceeded5s");
            // A bounded poll observes an explicit handshake, never a fixed readiness delay.
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn fixture_process() {
        let Some(control) = std::env::var_os(CONTROL) else {
            return;
        };
        let control = PathBuf::from(control);
        let role = std::env::var(ROLE).unwrap();
        let mut nested = None;
        let mut leaf = None;
        match role.as_str() {
            "root" => {
                let workspace = control.join("nested-owner");
                std::fs::create_dir(&workspace).unwrap();
                nested =
                    Some(OwnedProcess::spawn(&mut command(&control, "middle"), workspace).unwrap());
                ready(&control, "leaf");
            }
            "middle" => {
                leaf = Some(command(&control, "leaf").spawn().unwrap());
                ready(&control, "leaf");
            }
            "late-root" | "leaf" | "control" => {}
            _ => panic!("unknown session fixture role"),
        }
        std::fs::write(
            control.join(format!("{role}-ready")),
            std::process::id().to_string(),
        )
        .unwrap();
        let deadline =
            Instant::now() + Duration::from_secs(if role == "control" { 75 } else { 30 });
        while !control.join(format!("{role}-release")).is_file() {
            if Instant::now() >= deadline {
                std::fs::write(
                    control.join(format!("{role}-fail-safe")),
                    b"owner failed to stop fixture",
                )
                .unwrap();
                panic!("session fixture outlived owner");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if role == "late-root" {
            let workspace = control.join("nested-owner");
            std::fs::create_dir(&workspace).unwrap();
            let _nested = OwnedProcess::spawn(&mut command(&control, "middle"), workspace).unwrap();
            ready(&control, "leaf");
            // The outer owner sampled only root before releasing this late fork.
            std::fs::write(control.join("late-exit-ready"), b"exiting").unwrap();
            std::process::exit(0);
        }
        if role == "root" {
            // Deliberately skip the inner Owner's Drop. Only the outer session can
            // stop the nested private group after a successful root exits on POSIX.
            std::process::exit(0);
        }
        drop(leaf);
        drop(nested);
    }

    #[cfg(unix)]
    mod identity {
        use super::*;
        unsafe extern "C" {
            fn getsid(pid: i32) -> i32;
            fn getpgid(pid: i32) -> i32;
            fn kill(pid: i32, signal: i32) -> i32;
        }
        pub struct Pinned {
            pid: i32,
            birth: u64,
            pub group: i32,
            pub session: i32,
        }
        fn birth(pid: i32) -> Result<Option<u64>, String> {
            #[cfg(target_os = "macos")]
            {
                _native::process_owner::process_birth(pid as u32)
            }
            #[cfg(target_os = "linux")]
            {
                match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                    Ok(stat) => {
                        let fields: Vec<_> = stat
                            .rsplit_once(") ")
                            .ok_or("bad fixture stat")?
                            .1
                            .split_whitespace()
                            .collect();
                        if fields.len() < 20 {
                            return Err("incomplete fixture stat".into());
                        }
                        if matches!(fields[0], "Z" | "X") {
                            return Ok(None);
                        }
                        Ok(Some(fields[19].parse().map_err(|_| "bad fixture birth")?))
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::NotFound
                            || error.raw_os_error() == Some(3) =>
                    {
                        Ok(None)
                    }
                    Err(error) => Err(format!("cannot inspect fixture birth: {error}")),
                }
            }
        }
        impl Pinned {
            pub fn open(pid: u32) -> Self {
                let pid: i32 = pid.try_into().unwrap();
                let first = birth(pid).unwrap().expect("acknowledged fixture is live");
                // SAFETY: queries only; readiness keeps this fixture waiting for release.
                let (group, session) = unsafe { (getpgid(pid), getsid(pid)) };
                assert!(group > 0 && session > 0);
                assert_eq!(birth(pid).unwrap(), Some(first));
                Self {
                    pid,
                    birth: first,
                    group,
                    session,
                }
            }
            pub fn alive(&self) -> bool {
                let current = birth(self.pid).unwrap();
                if current != Some(self.birth) {
                    return false;
                }
                // SAFETY: queries never signal a recycled process.
                let (group, session) = unsafe { (getpgid(self.pid), getsid(self.pid)) };
                if birth(self.pid).unwrap() != Some(self.birth) {
                    return false;
                }
                assert_eq!(group, self.group, "fixture changed group");
                assert_eq!(session, self.session, "fixture changed session");
                true
            }
        }
        impl Drop for Pinned {
            fn drop(&mut self) {
                // Failure-only rescue is birth/SID/PGID checked, never a blind PID kill.
                // POSIX has a residual check/signal race; this is not a kernel handle.
                if birth(self.pid).ok() != Some(Some(self.birth)) {
                    return;
                }
                if unsafe { getsid(self.pid) } != self.session
                    || unsafe { getpgid(self.pid) } != self.group
                {
                    return;
                }
                if birth(self.pid).ok() != Some(Some(self.birth)) {
                    return;
                }
                let result = unsafe { kill(self.pid, 9) };
                assert!(result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(3));
                let deadline = Instant::now() + READY;
                while birth(self.pid).unwrap() == Some(self.birth) {
                    assert!(
                        Instant::now() < deadline,
                        "fixture rescue did not stop its pinned process"
                    );
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    #[cfg(windows)]
    mod identity {
        use super::*;
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess,
            WaitForSingleObject,
        };
        pub struct Pinned(OwnedHandle);
        impl Pinned {
            pub fn open(pid: u32) -> Self {
                let raw = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, 0, pid) };
                assert!(!raw.is_null(), "cannot pin acknowledged fixture");
                Self(unsafe { OwnedHandle::from_raw_handle(raw) })
            }
            pub fn alive(&self) -> bool {
                match unsafe { WaitForSingleObject(self.0.as_raw_handle(), 0) } {
                    WAIT_TIMEOUT => true,
                    WAIT_OBJECT_0 => false,
                    value => panic!("unknown fixture handle state: {value}"),
                }
            }
        }
        impl Drop for Pinned {
            fn drop(&mut self) {
                if self.alive() {
                    assert_ne!(unsafe { TerminateProcess(self.0.as_raw_handle(), 1) }, 0);
                    assert_eq!(
                        unsafe { WaitForSingleObject(self.0.as_raw_handle(), 5000) },
                        WAIT_OBJECT_0
                    );
                }
            }
        }
    }

    #[test]
    fn session_stops_nested_private_groups_after_root_success_cancellation_and_timeout() {
        use identity::Pinned;
        let started = Instant::now();
        let root = Root::new();
        let control = root.0.join("unrelated");
        std::fs::create_dir(&control).unwrap();
        let control_workspace = control.join("owner");
        std::fs::create_dir(&control_workspace).unwrap();
        let mut unrelated = OwnedProcess::spawn_session(
            &mut command(&control, "control"),
            control_workspace.clone(),
        )
        .unwrap();
        let negative = Pinned::open(ready(&control, "control"));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut modes = vec!["success", "cancel", "timeout"];
        if cfg!(unix) {
            // Windows closes the nested kill-on-close Job when root exits.
            modes.push("late-success");
        }
        for mode in modes {
            let case = root.0.join(mode);
            std::fs::create_dir(&case).unwrap();
            let workspace = case.join("owner");
            std::fs::create_dir(&workspace).unwrap();
            let role = if mode == "late-success" {
                "late-root"
            } else {
                "root"
            };
            let mut owner =
                OwnedProcess::spawn_session(&mut command(&case, role), workspace.clone()).unwrap();
            let parent = Pinned::open(ready(&case, role));
            if mode == "late-success" {
                // First observation has no descendants. Stop monitoring, then let
                // root fork and exit; it must remain unreaped during enrollment.
                let mut waiting = Box::pin(owner.wait(READY, READY));
                runtime.block_on(poll_fn(|cx| {
                    assert!(waiting.as_mut().poll(cx).is_pending());
                    Poll::Ready(())
                }));
                drop(waiting);
                std::fs::write(case.join("late-root-release"), b"fork then exit").unwrap();
            }
            let middle = Pinned::open(ready(&case, "middle"));
            let leaf = Pinned::open(ready(&case, "leaf"));
            let nested_marker = case.join("nested-owner/codec-cleanup-pending.json");
            let nested_bytes = std::fs::read(&nested_marker).unwrap();
            assert!(!nested_bytes.is_empty());
            if mode == "late-success" {
                let deadline = Instant::now() + READY;
                while parent.alive() {
                    assert!(Instant::now() < deadline, "late root did not exit within5s");
                    std::thread::sleep(Duration::from_millis(10));
                }
                assert!(case.join("late-exit-ready").is_file());
            } else {
                assert!(parent.alive());
            }
            assert!(middle.alive() && leaf.alive());
            #[cfg(unix)]
            {
                assert_eq!(parent.session, middle.session);
                assert_eq!(parent.session, leaf.session);
                assert_ne!(
                    parent.group, middle.group,
                    "fixture must exercise a separately owned PGID"
                );
                assert_eq!(middle.group, leaf.group);
                assert_ne!(parent.session, negative.session);
            }
            // Poll the real monitor once while all members are acknowledged and
            // before root can exit, establishing the original identity cohort.
            if mode != "late-success" {
                let mut waiting = Box::pin(owner.wait(READY, READY));
                runtime.block_on(poll_fn(|cx| {
                    assert!(waiting.as_mut().poll(cx).is_pending());
                    Poll::Ready(())
                }));
            }
            match mode {
                "success" | "late-success" => {
                    if mode == "success" {
                        std::fs::write(case.join("root-release"), b"root success").unwrap();
                    }
                    assert!(
                        runtime
                            .block_on(owner.wait(READY, READY))
                            .unwrap()
                            .success()
                    );
                    #[cfg(unix)]
                    assert!(
                        middle.alive() && leaf.alive(),
                        "inner Drop must not mask missing session cleanup"
                    );
                    owner.finish().unwrap();
                }
                "cancel" => {
                    let mut waiting = Box::pin(owner.wait(READY, READY));
                    runtime.block_on(poll_fn(|cx| {
                        assert!(waiting.as_mut().poll(cx).is_pending());
                        Poll::Ready(())
                    }));
                    drop(waiting);
                    drop(owner);
                }
                "timeout" => {
                    let error = runtime
                        .block_on(owner.wait(Duration::from_millis(200), READY))
                        .unwrap_err();
                    assert!(error.contains("wall budget"), "{error}");
                    owner.finish().unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                !parent.alive() && !middle.alive() && !leaf.alive(),
                "session member survived {mode}"
            );
            assert!(!workspace.exists(), "cleanup did not prove {mode} closure");
            for role in [role, "middle", "leaf"] {
                assert!(
                    !case.join(format!("{role}-fail-safe")).exists(),
                    "fail-safe masked missing cleanup"
                );
            }
            assert!(
                negative.alive(),
                "session cleanup touched the unrelated control"
            );
            // Root intentionally bypassed its inner Drop; retain that inner marker
            // until every pinned identity and the outer closure are verified.
            assert_eq!(std::fs::read(&nested_marker).unwrap(), nested_bytes);
            assert!(cleanup_pending(&case.join("nested-owner")));
            drop(leaf);
            drop(middle);
            drop(parent);
            std::fs::remove_dir_all(&case).unwrap();
        }
        assert!(negative.alive());
        unrelated.finish().unwrap();
        assert!(!negative.alive());
        assert!(!control_workspace.exists());
        assert!(!control.join("control-fail-safe").exists());
        assert!(
            started.elapsed() < Duration::from_secs(40),
            "normal session fixture exceeded its bounded flow"
        );
        drop(negative);
        drop(unrelated);
        root.clean();
    }
}
