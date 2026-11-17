//! Additional activation gates; these are not counted as original source case mappings.
use super::baseline_tooling_support::*;
use std::path::Path;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
pub(super) struct Codec {
    pid: u32,
    birth: u64,
}
#[cfg(target_os = "linux")]
impl Codec {
    fn observe(root: &Path) -> Option<Self> {
        for entry in std::fs::read_dir("/proc").ok()? {
            let entry = entry.ok()?;
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|value| value.parse::<u32>().ok())
            else {
                continue;
            };
            if std::fs::read_link(entry.path().join("exe")).ok().as_deref()
                != Some(root.join(app_name()).as_path())
            {
                continue;
            }
            let args = std::fs::read(entry.path().join("cmdline")).ok()?;
            if !args
                .split(|byte| *byte == 0)
                .any(|arg| arg == b"codec-worker")
            {
                continue;
            }
            let (birth, state) = Self::state(pid)?;
            if matches!(state, 'Z' | 'X' | 'x') {
                continue;
            }
            return Some(Self { pid, birth });
        }
        None
    }
    fn state(pid: u32) -> Option<(u64, char)> {
        let raw = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let fields = raw
            .rsplit_once(')')?
            .1
            .split_whitespace()
            .collect::<Vec<_>>();
        Some((
            fields.get(19)?.parse().ok()?,
            fields.first()?.chars().next()?,
        ))
    }
    fn alive(&self) -> bool {
        Self::state(self.pid)
            .is_some_and(|(birth, state)| birth == self.birth && !matches!(state, 'Z' | 'X' | 'x'))
    }
    pub(super) fn gone(&self) -> bool {
        Self::state(self.pid).is_none_or(|(birth, _)| birth != self.birth)
    }
}

#[cfg(windows)]
pub(super) struct Codec(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl Codec {
    fn observe(_root: &Path) -> Option<Self> {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
            System::{
                Diagnostics::ToolHelp::*,
                Threading::{OpenProcess, PROCESS_SYNCHRONIZE},
            },
        };
        // Observe only the current test's owned driver -> worker -> panel -> codec chain.
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return None;
            }
            let mut value: PROCESSENTRY32W = std::mem::zeroed();
            value.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut rows = vec![];
            let mut present = Process32FirstW(snapshot, &mut value);
            while present != 0 {
                let end = value
                    .szExeFile
                    .iter()
                    .position(|ch| *ch == 0)
                    .unwrap_or(value.szExeFile.len());
                rows.push((
                    value.th32ProcessID,
                    value.th32ParentProcessID,
                    String::from_utf16_lossy(&value.szExeFile[..end]),
                ));
                present = Process32NextW(snapshot, &mut value);
            }
            CloseHandle(snapshot);
            let child = |parent, name: &str| {
                rows.iter()
                    .find(|(_, p, n)| *p == parent && n.eq_ignore_ascii_case(name))
                    .map(|(pid, _, _)| *pid)
            };
            let driver = child(std::process::id(), dev_name())?;
            let worker = child(driver, dev_name())?;
            let panel = child(worker, app_name())?;
            let codec = child(panel, app_name())?;
            let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, codec);
            if handle.is_null() {
                None
            } else {
                Some(Self(handle))
            }
        }
    }
    fn alive(&self) -> bool {
        unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(self.0, 0)
                == windows_sys::Win32::Foundation::WAIT_TIMEOUT
        }
    }
    pub(super) fn gone(&self) -> bool {
        !self.alive()
    }
}
#[cfg(windows)]
impl Drop for Codec {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

pub(super) fn observe_active_codec(root: &Root) -> Codec {
    let deadline = Instant::now() + Duration::from_secs(10);
    let codec = loop {
        if let Some(codec) = Codec::observe(&root.0) {
            break codec;
        }
        assert!(
            Instant::now() < deadline,
            "did not observe the actual native codec process"
        );
        std::thread::sleep(Duration::from_millis(2));
    };
    assert!(codec.alive());
    codec
}

#[test]
fn actual_deadline_and_owner_cleanup_failure_retain_recovery_with_both_diagnostics() {
    let root = Root::new();
    root.install();
    let source = root.source(&["s:a"], false);
    let original = std::fs::read(source.join("catalog.db")).unwrap();
    let mut live = Live::wrapper_budget(&root, &source, port(), 5);
    live.ready();
    let clone = active_clone(&root);
    // A real filesystem fault in this wrapper's own owner workspace, after the
    // actual panel is serving. It does not fake a process or cleanup result.
    let owners = std::fs::read_dir(root.0.join("logs/tmp/tooling"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.join("stdout").is_file()
                && path.join("stderr").is_file()
                && path.join("owner/codec-cleanup-pending.json").is_file()
        })
        .collect::<Vec<_>>();
    assert_eq!(owners.len(), 1);
    std::fs::remove_dir_all(owners[0].join("owner")).unwrap();
    assert_eq!(live.finish_code(), 1);
    let diagnostic = live.diagnostics();
    assert!(diagnostic.contains("wall budget"), "{diagnostic}");
    assert!(diagnostic.contains("cleanup also failed"), "{diagnostic}");
    assert!(
        diagnostic.contains("recovery copy retained"),
        "{diagnostic}"
    );
    assert!(clone.join("catalog.db").is_file());
    assert!(clone.join(".sandbox-recovery.json").is_file());
    assert_eq!(
        numera_runtime::tooling_sandbox::sweep_stale(clone.parent().unwrap()).unwrap(),
        0
    );
    assert!(
        get(&format!("{}/api/ping", live.url)).is_none(),
        "original listener did not stop"
    );
    assert_eq!(std::fs::read(source.join("catalog.db")).unwrap(), original);
}
