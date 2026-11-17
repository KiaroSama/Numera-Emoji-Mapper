#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);
pub fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .into()
}
pub struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new(label: &str) -> Self {
        let root = project().join("logs/tmp").join(format!(
            "original-main-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        std::fs::create_dir(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
    pub fn worker(&self, name: &str) -> PathBuf {
        let path = self
            .0
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        let compiler = option_env!("NUMERA_BASELINE_RUSTC").unwrap_or("rustc");
        let result = Command::new(compiler)
            .args(["--edition=2024", "--crate-name=original_main_fixture"])
            .arg(project().join("runtime/tests/original_main_rust_support/worker.rs"))
            .arg("-o")
            .arg(&path)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "fixture compile failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained {}", self.0.display());
        } else {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
pub async fn ready(path: &Path) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !path.is_file() {
        assert!(
            std::time::Instant::now() < deadline,
            "active worker readiness timed out"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
pub fn terminated(pid: u32) {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"));
        assert!(
            stat.is_err() || stat.unwrap().rsplit_once(") ").unwrap().1.starts_with('Z'),
            "fixture process{pid} survived cancellation"
        );
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, WAIT_OBJECT_0,
        };
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        };
        // SAFETY: only the observed fixture PID is queried and each acquired handle is closed.
        unsafe {
            let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if process.is_null() {
                assert_eq!(GetLastError(), ERROR_INVALID_PARAMETER);
            } else {
                let status = WaitForSingleObject(process, 0);
                CloseHandle(process);
                assert_eq!(status, WAIT_OBJECT_0);
            }
        }
    }
}
