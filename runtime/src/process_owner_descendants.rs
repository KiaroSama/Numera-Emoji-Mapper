//! Dedicated Linux subreaper scope; normal application group/session owners are unchanged.
use super::platform::{Member, member};
use std::collections::BTreeSet;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);
const MAX_PROCESSES: usize = 65_536;

// GNU Linux x86_64: glibc2.39 bits/sigaction.h and Linux __sigset_t.
#[repr(C)]
#[derive(Default)]
struct SignalAction {
    handler: usize,
    mask: [u64; 16],
    flags: i32,
    restorer: usize,
}

#[cfg(all(
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64"
))]
const _: () = {
    assert!(std::mem::size_of::<SignalAction>() == 152);
    assert!(std::mem::offset_of!(SignalAction, flags) == 136);
    assert!(std::mem::offset_of!(SignalAction, restorer) == 144);
};

unsafe extern "C" {
    fn sigaction(signal: i32, action: *const SignalAction, previous: *mut SignalAction) -> i32;
    fn prctl(option: i32, ...) -> i32;
    fn pidfd_open(pid: i32, flags: u32) -> i32;
    fn pidfd_send_signal(fd: i32, signal: i32, info: *const std::ffi::c_void, flags: u32) -> i32;
    fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
    fn waitid(kind: i32, id: u32, info: *mut std::ffi::c_void, options: i32) -> i32;
}

fn no_children() -> Result<bool, String> {
    // Linux glibc siginfo_t is128 bytes; u64 storage also supplies its required alignment.
    let mut info = [0_u64; 16];
    // SAFETY: writable128-byte aligned buffer, P_ALL0, WEXITED4|WNOHANG1|WNOWAIT.
    // Never consume a wait status: std Child alone owns the root's status.
    let result = unsafe { waitid(0, 0, info.as_mut_ptr().cast(), 4 | 1 | 0x0100_0000) };
    if result == 0 {
        return Ok(false);
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(10) => Ok(true), // ECHILD proves absence; a /proc snapshot alone cannot.
        Some(4) => Ok(false), // EINTR is not absence.
        _ => Err("cannot verify kernel test-child absence".into()),
    }
}

pub(super) struct Descendants {
    parent: i32,
    previous: i32,
    root_reaped: AtomicBool,
    restored: AtomicBool,
}

fn snapshot() -> Result<Vec<Member>, String> {
    let mut values = Vec::new();
    for entry in std::fs::read_dir("/proc").map_err(|_| "cannot enumerate test descendants")? {
        let entry = entry.map_err(|_| "cannot enumerate test descendant identity")?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
        else {
            continue;
        };
        if let Some(value) = member(pid)? {
            values.push(value);
            if values.len() > MAX_PROCESSES {
                return Err("test descendant accounting exceeds65536 processes".into());
            }
        }
    }
    values.sort_by_key(|value| value.pid);
    Ok(values)
}

impl Descendants {
    pub(super) fn new() -> Result<Self, String> {
        if ACTIVE
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("test descendant owner requires a dedicated process".into());
        }
        let result = (|| {
            if !cfg!(all(
                target_arch = "x86_64",
                target_env = "gnu",
                target_pointer_width = "64"
            )) {
                return Err(
                    "test descendant scope requires the supported GNU Linux x64 ABI".into(),
                );
            }
            let parent = std::process::id() as i32;
            let mut action = SignalAction::default();
            // SAFETY: supported ABI checked above; query only, with a live correctly sized output.
            if unsafe { sigaction(17, std::ptr::null(), &mut action) } != 0 {
                return Err("cannot inspect test runner SIGCHLD action".into());
            }
            if action.handler != 0 || action.flags & 2 != 0 {
                return Err(
                    "test descendant owner requires default SIGCHLD without SA_NOCLDWAIT".into(),
                );
            }
            if !no_children()? || snapshot()?.iter().any(|value| value.parent == parent) {
                return Err("test descendant owner refuses preexisting child processes".into());
            }
            let mut previous = 0_i32;
            // SAFETY: live int output, admitted LP64 ABI; glibc reads all four ulong arguments.
            if unsafe {
                prctl(
                    37,
                    (&mut previous as *mut i32) as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                )
            } != 0
            {
                return Err("cannot inspect Linux child-subreaper support".into());
            }
            // Probe the kernel capability before any child can require descriptor-based rescue.
            // SAFETY: current process is alive; flags0 opens only its close-on-exec identity.
            let probe = unsafe { pidfd_open(parent, 0) };
            if probe < 0 {
                return Err("Linux test runner requires pidfd support".into());
            }
            // SAFETY: successful pidfd_open transfers one fresh descriptor.
            let probe = unsafe { OwnedFd::from_raw_fd(probe) };
            // SAFETY: signal0 checks permission/support without delivering a signal.
            if unsafe { pidfd_send_signal(probe.as_raw_fd(), 0, std::ptr::null(), 0) } != 0 {
                return Err("Linux test runner requires pidfd signaling support".into());
            }
            // SAFETY: fixed option36 and four explicit unsigned-long arguments, no pointers.
            if unsafe {
                prctl(
                    36,
                    1 as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                )
            } != 0
            {
                return Err("cannot establish Linux test child-subreaper".into());
            }
            Ok(Self {
                parent,
                previous,
                root_reaped: AtomicBool::new(false),
                restored: AtomicBool::new(false),
            })
        })();
        if result.is_err() {
            ACTIVE.store(false, Ordering::SeqCst);
        }
        result
    }

    pub(super) fn sample(&self) -> Result<Vec<u64>, String> {
        let processes = snapshot()?;
        let mut cohort = BTreeSet::from([self.parent]);
        loop {
            let before = cohort.len();
            for value in &processes {
                if cohort.contains(&value.parent) {
                    cohort.insert(value.pid);
                }
            }
            if cohort.len() == before {
                break;
            }
        }
        Ok(processes
            .into_iter()
            .filter(|value| value.pid != self.parent && cohort.contains(&value.pid) && value.live)
            .flat_map(|value| [value.pid as u64, value.birth, value.cpu[0], value.cpu[1]])
            .collect())
    }

    pub(super) fn root_reaped(&self) {
        self.root_reaped.store(true, Ordering::SeqCst);
    }

    pub(super) fn terminate(&self, root: i32) -> Result<(), String> {
        for expected in snapshot()?
            .into_iter()
            .filter(|value| value.parent == self.parent)
        {
            // Direct children cannot be PID-recycled while this dedicated parent holds wait ownership.
            // SAFETY: positive inspected child PID; flags0 requests a close-on-exec process descriptor.
            let raw = unsafe { pidfd_open(expected.pid, 0) };
            if raw < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(3) {
                    continue;
                }
                return Err("cannot pin Linux test descendant".into());
            }
            // SAFETY: pidfd_open returned one fresh owned descriptor.
            let descriptor = unsafe { OwnedFd::from_raw_fd(raw) };
            let Some(current) = member(expected.pid)? else {
                continue;
            };
            if current.parent != self.parent || current.birth != expected.birth {
                return Err("test descendant identity changed before termination".into());
            }
            if current.live {
                // SAFETY: stable process descriptor, SIGKILL9, no caller-supplied siginfo or flags.
                if unsafe { pidfd_send_signal(descriptor.as_raw_fd(), 9, std::ptr::null(), 0) } != 0
                    && std::io::Error::last_os_error().raw_os_error() != Some(3)
                {
                    return Err("cannot terminate pinned test descendant".into());
                }
            }
            if current.pid != root || self.root_reaped.load(Ordering::SeqCst) {
                let mut status = 0_i32;
                // SAFETY: only this exact adopted child is reaped, never the std Child root or -1.
                let result = unsafe { waitpid(current.pid, &mut status, 1) };
                if result < 0 && std::io::Error::last_os_error().raw_os_error() != Some(4) {
                    return Err("cannot reap adopted test descendant".into());
                }
            }
        }
        Ok(())
    }

    pub(super) fn quiescent(&self) -> Result<bool, String> {
        if !no_children()? || snapshot()?.iter().any(|value| value.parent == self.parent) {
            return Ok(false);
        }
        if !self.restored.load(Ordering::SeqCst) {
            // SAFETY: restore only the process-local attribute observed before this sole child cohort.
            if unsafe {
                prctl(
                    36,
                    self.previous as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                )
            } != 0
            {
                return Err("cannot restore Linux child-subreaper state".into());
            }
            self.restored.store(true, Ordering::SeqCst);
            ACTIVE.store(false, Ordering::SeqCst);
        }
        Ok(true)
    }
}

impl Drop for Descendants {
    fn drop(&mut self) {
        if !self.restored.load(Ordering::SeqCst) && self.quiescent() != Ok(true) {
            // Keep subreaper custody and the active refusal after unresolved cleanup.
            eprintln!("ERROR: test descendants remain unresolved; subreaper custody retained");
        }
    }
}
