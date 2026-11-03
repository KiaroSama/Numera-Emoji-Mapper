//! A recorded PID is a diagnostic hint, never authority to release or reclaim an OS lease.
#![allow(unsafe_code)]
use std::io;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    Present,
    Gone,
    Unknown,
}

impl Presence {
    pub fn from_posix_probe(result: io::Result<()>) -> Self {
        match result {
            Ok(()) => Self::Present,
            Err(error) if error.raw_os_error() == Some(3) => Self::Gone,
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Self::Present,
            Err(_) => Self::Unknown,
        }
    }
}

pub fn recorded_pid(pid: i64) -> Presence {
    if pid <= 0 {
        return Presence::Gone;
    }
    match u32::try_from(pid) {
        Ok(pid) => probe(pid),
        Err(_) => Presence::Unknown,
    }
}

#[cfg(unix)]
fn probe(pid: u32) -> Presence {
    let Ok(pid) = i32::try_from(pid) else {
        return Presence::Unknown;
    };
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    // Positive pid_t only; signal zero neither sends a signal nor proves process birth identity.
    let result = unsafe { kill(pid, 0) };
    Presence::from_posix_probe(if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    })
}

#[cfg(windows)]
fn probe(pid: u32) -> Presence {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    // Noninheritable, synchronization-only access cannot terminate or modify the observed process.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return Presence::Unknown;
    }
    // OpenProcess returned one owned handle; RAII closes it after every nonblocking wait outcome.
    let _owned = unsafe { OwnedHandle::from_raw_handle(handle) };
    match unsafe { WaitForSingleObject(handle, 0) } {
        WAIT_OBJECT_0 => Presence::Gone,
        WAIT_TIMEOUT => Presence::Present,
        _ => Presence::Unknown,
    }
}

#[cfg(not(any(unix, windows)))]
fn probe(_pid: u32) -> Presence {
    Presence::Unknown
}

pub fn busy_message(path: &Path, error: &io::Error) -> String {
    let record = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text.trim()).ok());
    describe(record.as_ref(), error, path, recorded_pid)
}

fn describe(
    record: Option<&serde_json::Value>,
    error: &io::Error,
    path: &Path,
    observe: impl FnOnce(i64) -> Presence,
) -> String {
    let pid = record.and_then(|record| record["pid"].as_i64());
    let who = match pid.filter(|pid| *pid > 0) {
        Some(pid) => match observe(pid) {
            Presence::Present => format!(
                "record names pid {pid} (started {}; present when checked)",
                record
                    .and_then(|record| record["started"].as_str())
                    .unwrap_or("unknown")
            ),
            Presence::Gone => format!(
                "another run (the lock file names pid {pid}, which was not running when this was checked -- it either finished just now or never wrote its own record)"
            ),
            Presence::Unknown => {
                format!("another run (record names pid {pid}; liveness could not be determined)")
            }
        },
        None => "another run".into(),
    };
    format!(
        "{} is held by {who}. Refusing to mutate the same pack family concurrently. [{error}]",
        path.file_name().unwrap_or_default().to_string_lossy()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn original_posix_outcomes_remain_conservative_without_granting_a_lease() {
        for (result, expected, original_alive) in [
            (Err(io::Error::from_raw_os_error(3)), Presence::Gone, false),
            (
                Err(io::Error::from(io::ErrorKind::PermissionDenied)),
                Presence::Present,
                true,
            ),
            (Err(io::Error::other("weird")), Presence::Unknown, true),
            (Ok(()), Presence::Present, true),
        ] {
            let actual = Presence::from_posix_probe(result);
            assert_eq!(actual, expected);
            assert_eq!(actual != Presence::Gone, original_alive);
            let message = describe(
                Some(&json!({"pid":4242,"started":"fixture UTC"})),
                &io::Error::from(io::ErrorKind::WouldBlock),
                Path::new("family.lock"),
                |pid| {
                    assert_eq!(pid, 4242);
                    actual
                },
            );
            assert!(message.contains("Refusing to mutate the same pack family concurrently."));
            match actual {
                Presence::Gone => {
                    assert!(message.contains("was not running when this was checked"))
                }
                Presence::Present => assert!(message.contains("present when checked")),
                Presence::Unknown => assert!(message.contains("could not be determined")),
            }
        }
        for pid in [i64::MIN, -1, 0] {
            assert_eq!(recorded_pid(pid), Presence::Gone);
        }
        assert_eq!(recorded_pid(i64::MAX), Presence::Unknown);
        let unknown = Presence::from_posix_probe(Err(io::Error::from_raw_os_error(2)));
        assert_eq!(unknown, Presence::Unknown, "ENOENT is not ESRCH");
        for record in [
            None,
            Some(json!({})),
            Some(json!({"pid":0})),
            Some(json!({"pid":-1})),
        ] {
            let message = describe(
                record.as_ref(),
                &io::Error::other("controlled refusal"),
                Path::new("family.lock"),
                |_| panic!("invalid record must not invoke a process probe"),
            );
            assert!(message.starts_with("family.lock is held by another run."));
        }
    }

    #[test]
    fn current_process_is_observed_without_starting_a_probe_child() {
        assert_eq!(
            recorded_pid(i64::from(std::process::id())),
            Presence::Present
        );
    }
}
