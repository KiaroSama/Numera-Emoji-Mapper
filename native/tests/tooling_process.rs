//! Actual native child at the public finite-command owner seam; no Python code strings.
use _native::{logging::RunLog, tooling_process};
use std::collections::BTreeMap;
use std::ffi::OsString;
#[cfg(windows)]
use std::path::{Path, PathBuf};
use std::time::Duration;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

#[test]
fn fixture_cleanup_retains_uncertain_child_evidence_even_during_unwind() {
    let root = Root::new("tooling process");
    let children = root.0.join("logs/tmp/tooling");
    let workspace = children.join("child-fixture");
    let marker = workspace.join("owner/codec-cleanup-pending.json");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, b"unresolved fixture owner").unwrap();
    assert!(root.clean().unwrap_err().contains("retained"));
    assert_eq!(std::fs::read(&marker).unwrap(), b"unresolved fixture owner");
    let path = root.0.clone();
    let panic = std::panic::catch_unwind(|| {
        let _root = root;
        panic!("original fixture failure");
    })
    .unwrap_err();
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"original fixture failure")
    );
    assert_eq!(std::fs::read(&marker).unwrap(), b"unresolved fixture owner");
    // This marker is synthetic and has never owned a process.
    std::fs::remove_dir_all(workspace).unwrap();
    std::fs::remove_dir(&children).unwrap();
    std::fs::write(&children, b"not a directory").unwrap();
    let root = Root(path);
    assert!(root.clean().unwrap_err().contains("retained"));
    assert_eq!(std::fs::read(&children).unwrap(), b"not a directory");
    std::fs::remove_file(children).unwrap();
    root.clean().unwrap();
    let diagnostic = Root::new("failed tooling evidence");
    std::fs::write(
        diagnostic.0.join("response.txt"),
        b"failed assertion diagnostic",
    )
    .unwrap();
    let retained = diagnostic.0.clone();
    assert!(
        std::panic::catch_unwind(|| {
            let _fixture = diagnostic;
            panic!("controlled failed observation");
        })
        .is_err()
    );
    assert_eq!(
        std::fs::read(retained.join("response.txt")).unwrap(),
        b"failed assertion diagnostic"
    );
    // This owned fixture never started a child; release after verifying retention.
    Root(retained).clean().unwrap();
}

#[test]
fn native_child_capture_preserves_utf8_and_code_then_removes_owned_workspace() {
    let root = Root::new("tooling process");
    let log = RunLog::open(&root.0, "tooling-process-test", vec![]);
    let environment = std::env::vars().collect::<BTreeMap<_, _>>();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let command = [
        OsString::from(env!("CARGO_BIN_EXE_numera-compat")),
        "--help".into(),
    ];
    let actual = runtime
        .block_on(tooling_process::capture(
            &root.0,
            &command,
            &environment,
            Duration::from_secs(5),
            Duration::from_secs(3),
            &log,
        ))
        .unwrap();
    assert_eq!(actual.code, 0);
    assert!(actual.stdout.contains("Numera Emoji Mapper native backend"));
    assert_eq!(actual.stderr, "");
    assert_eq!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    let absent = root.0.join("no such executable");
    assert_eq!(
        std::fs::symlink_metadata(&absent).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    let missing = [absent.clone().into_os_string()];
    assert!(
        runtime
            .block_on(tooling_process::capture(
                &root.0,
                &missing,
                &environment,
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log
            ))
            .is_err()
    );
    // This exclusive, absent executable could not start a child. Assert its
    // conservative recovery marker before explicitly cleaning only that fixture.
    assert_eq!(
        std::fs::symlink_metadata(&absent).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    let retained = std::fs::read_dir(root.0.join("logs/tmp/tooling"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(retained.len(), 1);
    assert!(
        retained[0]
            .join("owner/codec-cleanup-pending.json")
            .is_file()
    );
    assert!(root.clean().unwrap_err().contains("retained"));
    std::fs::remove_dir_all(&retained[0]).unwrap();
    drop(log);
    root.clean().unwrap();
}

#[test]
fn scripted_input_refuses_invalid_files_before_creating_child_workspace() {
    let root = Root::new("scripted input café");
    let foreign = Root::new("foreign scripted input");
    let log = RunLog::open(&root.0, "input-admission-test", vec![]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let input = root.0.join("input.txt");
    std::fs::write(&input, "child café 😀\n").unwrap();
    let oversized = root.0.join("oversized.txt");
    let file = std::fs::File::create(&oversized).unwrap();
    file.set_len(256 * 1024 + 1).unwrap();
    drop(file);
    let outside = foreign.0.join("input.txt");
    std::fs::write(&outside, b"foreign").unwrap();
    let environment = BTreeMap::new();
    for (path, expected) in [
        (root.0.join("missing.txt"), "cannot resolve tooling input"),
        (root.0.clone(), "regular project-contained file"),
        (outside, "regular project-contained file"),
        (oversized, "regular project-contained file"),
        (input.clone(), "Tool command is empty"),
    ] {
        let error = runtime
            .block_on(tooling_process::capture_with_input(
                &root.0,
                &[],
                &environment,
                &path,
                Duration::from_secs(1),
                Duration::from_secs(1),
                &log,
            ))
            .err()
            .expect("invalid input must refuse before spawn");
        if cfg!(any(windows, target_os = "linux")) {
            assert!(error.contains(expected), "{error}");
        } else {
            assert!(
                error.contains("requires verified descendant custody"),
                "{error}"
            );
        }
        assert!(!root.0.join("logs/tmp/tooling").exists());
    }
    #[cfg(unix)]
    {
        let link = root.0.join("linked.txt");
        std::os::unix::fs::symlink(&input, &link).unwrap();
        let error = runtime
            .block_on(tooling_process::capture_with_input(
                &root.0,
                &[],
                &environment,
                &link,
                Duration::from_secs(1),
                Duration::from_secs(1),
                &log,
            ))
            .err()
            .expect("invalid input must refuse before spawn");
        let expected = if cfg!(target_os = "linux") {
            "tooling input cannot follow links"
        } else {
            "requires verified descendant custody"
        };
        assert!(error.contains(expected), "{error}");
        std::fs::remove_file(link).unwrap();
    }
    assert_eq!(std::fs::read_to_string(input).unwrap(), "child café 😀\n");
    assert!(!root.0.join("logs/tmp/tooling").exists());
    drop(log);
    foreign.clean().unwrap();
    root.clean().unwrap();
}

#[cfg(windows)]
mod successful_build {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::Instant;
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
    };

    const FIXTURE: &str = "successful_build::fixture_process";
    const CONTROL: &str = "NUMERA_TOOLING_BUILD_CONTROL";
    const ROLE: &str = "NUMERA_TOOLING_BUILD_ROLE";

    struct Pinned(OwnedHandle);
    impl Pinned {
        fn open(pid: u32) -> Self {
            // SAFETY: root is still waiting for our acknowledgement, so this PID
            // is the live fixture child. All later inspection/cleanup uses its handle.
            let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, 0, pid) };
            assert!(!handle.is_null(), "cannot pin live fixture descendant");
            // SAFETY: OpenProcess returned a fresh owned process handle.
            Self(unsafe { OwnedHandle::from_raw_handle(handle) })
        }
        fn state(&self) -> u32 {
            // SAFETY: this fixture handle remains owned for the entire call.
            unsafe { WaitForSingleObject(self.0.as_raw_handle(), 0) }
        }
    }
    impl Drop for Pinned {
        fn drop(&mut self) {
            if self.state() != WAIT_OBJECT_0 {
                // Failure-only cleanup of the already pinned fixture; never a
                // re-open/kill by PID after its identity could have been recycled.
                unsafe {
                    TerminateProcess(self.0.as_raw_handle(), 1);
                    let state = WaitForSingleObject(self.0.as_raw_handle(), 5000);
                    assert_eq!(
                        state, WAIT_OBJECT_0,
                        "fixture cleanup could not be verified"
                    );
                }
            }
        }
    }

    #[test]
    fn fixture_process() {
        let Some(control) = std::env::var_os(CONTROL) else {
            return;
        };
        let control = PathBuf::from(control);
        let role = std::env::var(ROLE).unwrap();
        if role == "descendant" {
            std::fs::write(
                control.join("descendant-ready"),
                std::process::id().to_string(),
            )
            .unwrap();
            // Deliberately idle, with an independent fail-safe ceiling. The outer
            // owned Job must end this before the successful build returns.
            let (_sender, receiver) = std::sync::mpsc::channel::<()>();
            if receiver.recv_timeout(Duration::from_secs(8)).is_err() {
                std::fs::write(
                    control.join("descendant-expired"),
                    b"owner failed to stop child",
                )
                .unwrap();
                panic!("fixture outlived owner");
            }
            return;
        }
        assert_eq!(role, "root");
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", FIXTURE, "--nocapture", "--test-threads=1"])
            .env(ROLE, "descendant")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        while !control.join("root-release").is_file() {
            assert!(Instant::now() < deadline, "fixture root was not released");
            std::thread::sleep(Duration::from_millis(10));
        }
        // Child::drop closes the Windows handle without killing. Returning root
        // success with a closed-stdio live child is the original failing pattern.
        drop(child);
    }

    /// Migrates BuildProcessOwner.test_success_reaps_detached_stdio_descendant.
    #[test]
    fn successful_command_reaps_closed_stdio_descendant_before_returning() {
        let root = Root::new("tooling process");
        let log = RunLog::open(&root.0, "successful-build-test", vec![]);
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut environment = _native::tooling_launch::scrubbed_environment(
            &std::env::vars().collect(),
            &std::fs::read_to_string(project.join(".env.example")).unwrap(),
        );
        environment.insert(CONTROL.into(), root.0.to_str().unwrap().into());
        environment.insert(ROLE.into(), "root".into());
        let command = [
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            FIXTURE.into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ];
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let pinned = runtime.block_on(async {
            let mut capture = Box::pin(tooling_process::capture(
                &root.0, &command, &environment,
                Duration::from_secs(6), Duration::from_secs(4), &log,
            ));
            let deadline = Instant::now() + Duration::from_secs(3);
            let pid = loop {
                if let Ok(text) = std::fs::read_to_string(root.0.join("descendant-ready"))
                    && let Ok(pid) = text.parse::<u32>()
                {
                    break pid;
                }
                assert!(Instant::now() < deadline, "fixture descendant did not become ready");
                tokio::select! {
                    result = &mut capture => panic!("root finished before descendant acknowledgement: {}", result.err().unwrap_or_default()),
                    _ = tokio::time::sleep(Duration::from_millis(10)) => {}
                }
            };
            let pinned = Pinned::open(pid);
            assert_eq!(pinned.state(), WAIT_TIMEOUT, "descendant was not alive before root success");
            std::fs::write(root.0.join("root-release"), b"released").unwrap();
            let completed = capture.await.unwrap();
            assert_eq!(completed.code, 0, "{}", completed.stderr);
            assert!(completed.stdout.contains("1 passed"), "{}", completed.stdout);
            assert_eq!(pinned.state(), WAIT_OBJECT_0, "successful build leaked its descendant");
            assert!(!root.0.join("descendant-expired").exists(), "child fail-safe must not produce a false cleanup success");
            pinned
        });
        assert_eq!(
            std::fs::read_dir(root.0.join("logs/tmp/tooling"))
                .unwrap()
                .count(),
            0
        );
        drop(pinned);
        drop(log);
        root.clean().unwrap();
    }
}
