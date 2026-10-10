//! Real child ownership: a child launched by a child cannot outlive cancellation.
use _native::process_owner::OwnedProcess;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn cancellation_stops_descendant_before_workspace_removal() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let python = std::env::var_os("PYO3_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    assert!(
        !python.is_absolute() || python.is_file(),
        "Python required by real lifecycle fixture"
    );
    let directory = root
        .join("logs")
        .join(format!("owner-test-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let script = "import pathlib,subprocess,sys,threading\np=subprocess.Popen([sys.executable,'-c','import threading;threading.Event().wait()'])\npathlib.Path(sys.argv[1]).write_text(str(p.pid),encoding='utf-8')\nthreading.Event().wait()\n";
    let ready = directory.join("ready.txt");
    let mut command = Command::new(python);
    command
        .args(["-c", script])
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
