// Retained helper from native/tests/support/process_liveness.rs; source SHA256 21da98478b8ee212b486fd6be23e62598964d47ff1879255ff1e9e6c5ab162da.
//! Read-only post-stop checks for acknowledged fixture PIDs; never signals or reclaims a process.
pub fn assert_stopped(pid: u32) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{
            CloseHandle, ERROR_INVALID_PARAMETER, GetLastError, WAIT_OBJECT_0,
        };
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        };
        // SAFETY: inspect only the acknowledged fixture PID and close its handle exactly once.
        unsafe {
            let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if handle.is_null() {
                assert_eq!(GetLastError(), ERROR_INVALID_PARAMETER);
            } else {
                let state = WaitForSingleObject(handle, 0);
                CloseHandle(handle);
                assert_eq!(state, WAIT_OBJECT_0, "fixture survived");
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => assert!(
                stat.rsplit_once(") ").unwrap().1.starts_with('Z'),
                "fixture survived"
            ),
            Err(error) => assert!(
                error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(3),
                "cannot account fixture: {error}"
            ),
        }
    }
}
