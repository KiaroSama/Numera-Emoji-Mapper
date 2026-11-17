// Original-main Rust test; never counted toward1516 Python IDs.
// Source native/tests/process_lifecycle.rs @9c3cbb43305b; SHA256 b31a316f3803b4aff4d703e876a16b6a51ded473b7f4b877136ed9b92724afab
extern crate numera_runtime as _native;
#[path = "original_main_rust_support/mod.rs"]
mod support;
// Real child ownership: a child launched by a child cannot outlive cancellation.
use _native::process_owner::OwnedProcess;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn cancellation_stops_descendant_before_workspace_removal() {
    let scratch = support::Scratch::new("process-lifecycle");
    let executable = scratch.worker("tree-worker");
    let directory = scratch.0.join("owned");
    std::fs::create_dir(&directory).unwrap();
    let ready = directory.join("ready.txt");
    let mut command = Command::new(executable);
    command
        .arg("tree")
        .arg(&ready)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut owner = OwnedProcess::spawn(&mut command, directory.clone()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.is_file() {
        assert!(Instant::now() < deadline, "descendant did not report ready");
        runtime.block_on(async { tokio::time::sleep(Duration::from_millis(10)).await });
    }
    let descendant: u32 = std::fs::read_to_string(&ready).unwrap().parse().unwrap();
    let error = runtime
        .block_on(owner.wait(Duration::from_secs(3), Duration::from_millis(150)))
        .unwrap_err();
    assert!(error.contains("no CPU/IO progress"), "{error}");
    drop(owner);
    assert!(
        !directory.exists(),
        "cleanup did not verify tree termination"
    );
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, WAIT_OBJECT_0,
        };
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        };
        // SAFETY: query only the fixture PID; a live handle is closed exactly once.
        unsafe {
            let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, descendant);
            if process.is_null() {
                assert_eq!(GetLastError(), ERROR_INVALID_PARAMETER);
            } else {
                let state = WaitForSingleObject(process, 0);
                CloseHandle(process);
                assert_eq!(
                    state, WAIT_OBJECT_0,
                    "descendant survived owner cancellation"
                );
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{descendant}/stat"));
        assert!(stat.is_err() || stat.unwrap().rsplit_once(") ").unwrap().1.starts_with('Z'));
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
