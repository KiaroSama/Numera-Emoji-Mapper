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
    #[cfg(target_os = "macos")]
    {
        #[link(name = "proc")]
        unsafe extern "C" {
            fn proc_pidinfo(
                pid: i32,
                flavor: i32,
                arg: u64,
                buffer: *mut std::ffi::c_void,
                size: i32,
            ) -> i32;
            fn __error() -> *mut i32;
        }
        let mut info = [0_u32; 16];
        // SAFETY: initialized short-BSD ABI buffer and thread-local errno, fixture PID only.
        let (count, errno) = unsafe {
            *__error() = 0;
            let count = proc_pidinfo(pid.try_into().unwrap(), 13, 1, info.as_mut_ptr().cast(), 64);
            (count, *__error())
        };
        if count == 0 {
            assert_eq!(errno, 3);
        } else {
            assert_eq!(count, 64);
            assert_eq!(info[0], pid);
            assert_eq!(info[3], 5);
        }
    }
}
