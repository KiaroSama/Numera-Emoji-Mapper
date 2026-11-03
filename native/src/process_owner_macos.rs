//! Darwin group/session accounting; a successful signal alone never permits cleanup.
use super::*;
use std::ffi::c_void;
use std::os::unix::process::CommandExt;

const PROC_ALL_PIDS: u32 = 1;
const PROC_PGRP_ONLY: u32 = 2;
const PROC_PIDTASKALLINFO: i32 = 2;
const PROC_PIDTBSDINFO: i32 = 3;
const ESRCH: i32 = 3;
const SZOMB: u32 = 5;
const MAX_PIDS: usize = 65_536;

// Apple sys/proc_info.h, 64-bit Intel/ARM Darwin. No pointer-sized fields.
#[repr(C)]
#[derive(Default)]
struct BsdInfo {
    flags: u32,
    status: u32,
    xstatus: u32,
    pid: u32,
    ppid: u32,
    credentials: [u32; 7],
    comm: [u8; 16],
    name: [u8; 32],
    nfiles: u32,
    pgid: u32,
    pjobc: u32,
    tty_device: u32,
    tty_pgid: u32,
    nice: i32,
    birth_sec: u64,
    birth_usec: u64,
}

#[repr(C)]
#[derive(Default)]
struct TaskInfo {
    virtual_size: u64,
    resident_size: u64,
    user: u64,
    system: u64,
    threads_user: u64,
    threads_system: u64,
    scheduler: [i32; 12],
}

#[repr(C)]
#[derive(Default)]
struct TaskAllInfo {
    bsd: BsdInfo,
    task: TaskInfo,
}

#[cfg(all(
    target_pointer_width = "64",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
const _: () = {
    assert!(std::mem::size_of::<BsdInfo>() == 136);
    assert!(std::mem::offset_of!(BsdInfo, pgid) == 100);
    assert!(std::mem::offset_of!(BsdInfo, birth_sec) == 120);
    assert!(std::mem::size_of::<TaskInfo>() == 96);
    assert!(std::mem::offset_of!(TaskInfo, user) == 16);
    assert!(std::mem::size_of::<TaskAllInfo>() == 232);
};

#[link(name = "proc")]
unsafe extern "C" {
    fn proc_listpids(kind: u32, info: u32, buffer: *mut c_void, size: i32) -> i32;
    fn proc_pidinfo(pid: i32, flavor: i32, arg: u64, buffer: *mut c_void, size: i32) -> i32;
    fn __error() -> *mut i32;
    fn kill(pid: i32, signal: i32) -> i32;
    fn getsid(pid: i32) -> i32;
    fn setsid() -> i32;
}

fn pids(group: Option<i32>) -> Result<Vec<i32>, String> {
    if group.is_some_and(|value| value <= 0) {
        return Err("invalid Darwin codec process group".into());
    }
    let (kind, selector) = match group {
        Some(group) => (PROC_PGRP_ONLY, group as u32),
        None => (PROC_ALL_PIDS, 0),
    };
    let mut capacity = 256;
    loop {
        let mut pids = vec![0_i32; capacity];
        let size = std::mem::size_of_val(pids.as_slice()) as i32;
        // SAFETY: bounded initialized pid_t array; errno belongs to this thread.
        // libproc returns zero for BOTH an empty list and a failed syscall.
        let (bytes, errno) = unsafe {
            *__error() = 0;
            let bytes = proc_listpids(kind, selector, pids.as_mut_ptr().cast(), size);
            (bytes, *__error())
        };
        if bytes < 0 || (bytes == 0 && errno != 0) {
            return Err(format!(
                "cannot enumerate Darwin codec group (errno={errno})"
            ));
        }
        if bytes > size || bytes % 4 != 0 {
            return Err("invalid Darwin codec PID-list size".into());
        }
        // A full buffer may be truncated. Grow within a fixed memory/call ceiling,
        // never accept a partial list as proof there are no remaining descendants.
        if bytes == size {
            if capacity == MAX_PIDS {
                return Err("Darwin codec group exceeds bounded PID accounting".into());
            }
            capacity *= 2;
            continue;
        }
        pids.truncate(bytes as usize / 4);
        // PROC_ALL_PIDS includes kernel_task PID0, not a signalable user process.
        if group.is_none() {
            pids.retain(|pid| *pid != 0);
        }
        pids.sort_unstable();
        if pids.iter().any(|pid| *pid <= 0) || pids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("invalid Darwin codec PID-list entries".into());
        }
        return Ok(pids);
    }
}

fn bsd_info(pid: i32) -> Result<Option<BsdInfo>, String> {
    let mut info = BsdInfo::default();
    let size = std::mem::size_of::<BsdInfo>() as i32;
    // SAFETY: initialized repr(C) buffer matches PROC_PIDTBSDINFO exactly.
    // arg=1 includes zombies, unlike the task-all flavor.
    let (bytes, errno) = unsafe {
        *__error() = 0;
        let bytes = proc_pidinfo(
            pid,
            PROC_PIDTBSDINFO,
            1,
            (&mut info as *mut BsdInfo).cast(),
            size,
        );
        (bytes, *__error())
    };
    if bytes == 0 && errno == ESRCH {
        return Ok(None);
    }
    if bytes != size {
        return Err(format!(
            "cannot inspect Darwin codec identity (errno={errno}, bytes={bytes})"
        ));
    }
    validate_identity(pid, &info)?;
    Ok(Some(info))
}

fn task_info(pid: i32) -> Result<Option<TaskAllInfo>, String> {
    let mut info = TaskAllInfo::default();
    let size = std::mem::size_of::<TaskAllInfo>() as i32;
    // SAFETY: initialized repr(C) buffer matches PROC_PIDTASKALLINFO exactly.
    let (bytes, errno) = unsafe {
        *__error() = 0;
        let bytes = proc_pidinfo(
            pid,
            PROC_PIDTASKALLINFO,
            0,
            (&mut info as *mut TaskAllInfo).cast(),
            size,
        );
        (bytes, *__error())
    };
    if bytes == 0 && errno == ESRCH {
        return Ok(None);
    }
    if bytes != size {
        return Err(format!(
            "cannot inspect Darwin codec CPU accounting (errno={errno}, bytes={bytes})"
        ));
    }
    validate_identity(pid, &info.bsd)?;
    Ok(Some(info))
}

fn validate_identity(pid: i32, info: &BsdInfo) -> Result<(), String> {
    if pid <= 0
        || info.pid != pid as u32
        || !(1..=SZOMB).contains(&info.status)
        || info.birth_sec == 0
        || info.birth_usec >= 1_000_000
    {
        return Err("invalid Darwin codec process identity/accounting ABI".into());
    }
    Ok(())
}

pub fn process_birth(pid: u32) -> Result<Option<u64>, String> {
    let pid = i32::try_from(pid).map_err(|_| "Darwin parent PID out of range")?;
    match bsd_info(pid)? {
        Some(info) if info.status != SZOMB => info
            .birth_sec
            .checked_mul(1_000_000)
            .and_then(|value| value.checked_add(info.birth_usec))
            .map(Some)
            .ok_or_else(|| "Darwin parent birth overflow".into()),
        _ => Ok(None),
    }
}

pub struct Tree {
    group: i32,
    session: Option<Session>,
}

fn session_id(pid: i32) -> Result<Option<i32>, String> {
    // SAFETY: query only; errno is thread-local and immediately captured.
    let sid = unsafe { getsid(pid) };
    if sid < 0 {
        let errno = std::io::Error::last_os_error().raw_os_error();
        return if errno == Some(ESRCH) {
            Ok(None)
        } else {
            Err(format!("cannot inspect Darwin session (errno={errno:?})"))
        };
    }
    Ok(Some(sid))
}

fn session_identity(pid: i32) -> Result<Option<(BsdInfo, i32)>, String> {
    let Some(before) = bsd_info(pid)? else {
        return Ok(None);
    };
    let Some(sid) = session_id(pid)? else {
        return match bsd_info(pid)? {
            None => Ok(None),
            Some(info) if info.status == SZOMB => Ok(None),
            Some(_) => Err("live Darwin process has unknown session identity".into()),
        };
    };
    let Some(after) = bsd_info(pid)? else {
        return Ok(None);
    };
    if (before.birth_sec, before.birth_usec) != (after.birth_sec, after.birth_usec) {
        return Err("Darwin process identity changed during session inspection".into());
    }
    Ok(Some((after, sid)))
}

impl Tree {
    pub fn ready_to_reap(&self) -> Result<bool, String> {
        let Some(session) = &self.session else {
            return Ok(true);
        };
        session.before_reap(
            || {
                let root = bsd_info(self.group)?
                    .ok_or("Darwin session root disappeared before verified reaping")?;
                Ok(root.status == SZOMB)
            },
            || self.sample().map(|_| ()),
        )
    }

    pub fn root_reaped(&self) {
        if let Some(session) = &self.session {
            session.root_reaped();
        }
    }

    fn session_members(&self) -> Result<Vec<BsdInfo>, String> {
        let mut values = Vec::new();
        for pid in pids(None)? {
            // Query SID before BSD identity: a known foreign session needs no
            // privileged CPU/identity inspection. Denied SID itself stays unknown.
            if session_id(pid)? != Some(self.group) {
                continue;
            }
            if let Some((info, sid)) = session_identity(pid)?
                && sid == self.group
            {
                values.push(info);
            }
        }
        let owner = self
            .session
            .as_ref()
            .ok_or("missing Darwin session owner")?;
        // getsid may exclude a zombie, but our still-unreaped Child prevents root
        // PID reuse. setsid succeeded before exec and a session leader cannot move
        // to another session, so its zombie BSD identity is a valid initial anchor.
        if !owner.is_reaped()
            && !values.iter().any(|info| info.pid == self.group as u32)
            && let Some(info) = bsd_info(self.group)?
            && info.status == SZOMB
        {
            values.push(info);
            values.sort_by_key(|info| info.pid);
        }
        let identities: Vec<_> = values
            .iter()
            .map(|value| (value.pid as i32, [value.birth_sec, value.birth_usec]))
            .collect();
        owner.admit(&identities, |pid| {
            if pid == self.group
                && !owner.is_reaped()
                && let Some(info) = bsd_info(pid)?
                && info.status == SZOMB
            {
                return Ok(Some([info.birth_sec, info.birth_usec]));
            }
            Ok(session_identity(pid)?
                .filter(|(_, sid)| *sid == self.group)
                .map(|(info, _)| [info.birth_sec, info.birth_usec]))
        })?;
        Ok(values)
    }

    fn session_sample(&self) -> Result<Vec<u64>, String> {
        let mut values = Vec::new();
        for expected in self.session_members()? {
            if expected.status == SZOMB {
                continue;
            }
            let pid = expected.pid as i32;
            let task = task_info(pid)?;
            let Some((current, sid)) = session_identity(pid)? else {
                continue;
            };
            if sid != self.group
                || (current.birth_sec, current.birth_usec)
                    != (expected.birth_sec, expected.birth_usec)
            {
                return Err("Darwin session member identity changed during accounting".into());
            }
            if current.status == SZOMB {
                continue;
            }
            let cpu = match task {
                Some(task)
                    if (task.bsd.birth_sec, task.bsd.birth_usec)
                        == (current.birth_sec, current.birth_usec) =>
                {
                    [task.task.user, task.task.system]
                }
                None if current.flags & 4 != 0 => [0, 0],
                _ => return Err("Darwin session member has unknown CPU accounting".into()),
            };
            values.extend([
                pid as u64,
                current.birth_sec,
                current.birth_usec,
                cpu[0],
                cpu[1],
            ]);
        }
        Ok(values)
    }

    pub fn sample(&self) -> Result<Vec<u64>, String> {
        if self.session.is_some() {
            return self.session_sample();
        }
        let mut values = Vec::new();
        for pid in pids(Some(self.group))? {
            let info = match task_info(pid)? {
                Some(info) => info,
                None => {
                    // ESRCH from task-all can mean zombie OR gone. Verify with the
                    // zombie-aware flavor; a live-but-unreadable member is unknown.
                    match bsd_info(pid)? {
                        None => continue,
                        Some(info) if info.status == SZOMB => continue,
                        Some(info) if info.flags & 4 != 0 => {
                            // PROC_FLAG_INEXIT can precede SZOMB after task teardown.
                            // Keep ownership non-quiescent until the PID is gone/zombie.
                            if info.pgid != self.group as u32 {
                                return Err("Darwin codec member changed group during exit".into());
                            }
                            values.extend([pid as u64, info.birth_sec, info.birth_usec, 0, 0]);
                            continue;
                        }
                        Some(_) => {
                            return Err("Darwin codec member has no live CPU accounting".into());
                        }
                    }
                }
            };
            if info.bsd.status == SZOMB {
                continue;
            }
            if info.bsd.pgid != self.group as u32 {
                return Err("Darwin codec member changed group during accounting".into());
            }
            values.extend([
                pid as u64,
                info.bsd.birth_sec,
                info.bsd.birth_usec,
                info.task.user,
                info.task.system,
            ]);
        }
        // pids sorts by PID, so enumeration order is never false progress.
        Ok(values)
    }

    pub fn interrupt(&self) -> Result<(), String> {
        if self.session.is_none() {
            return Err("console interruption requires an owned session".into());
        }
        self.signal(2)
    }

    pub fn terminate(&self) -> Result<(), String> {
        self.signal(9)
    }

    fn signal(&self, signal: i32) -> Result<(), String> {
        if self.session.is_some() {
            for expected in self.session_members()? {
                if expected.status == SZOMB {
                    continue;
                }
                let Some((current, sid)) = session_identity(expected.pid as i32)? else {
                    continue;
                };
                if sid != self.group
                    || current.status == SZOMB
                    || (current.birth_sec, current.birth_usec)
                        != (expected.birth_sec, expected.birth_usec)
                {
                    continue;
                }
                // SAFETY: positive PID rechecked for birth+SID immediately before
                // signaling. This is not atomic kernel-handle pinning on Darwin.
                if unsafe { kill(current.pid as i32, signal) } != 0 {
                    let errno = std::io::Error::last_os_error().raw_os_error();
                    if errno != Some(ESRCH) {
                        return Err(format!(
                            "cannot terminate Darwin session member (errno={errno:?})"
                        ));
                    }
                }
            }
            return Ok(());
        }
        if self.quiescent()? {
            return Ok(());
        }
        // SAFETY: process_group(0) gave this invocation a private positive PGID;
        // the verified Python bridge keeps FFmpeg inside it. Root reaping and
        // fresh libproc quiescence, not this signal, authorize scratch deletion.
        if unsafe { kill(-self.group, signal) } != 0 {
            let errno = std::io::Error::last_os_error().raw_os_error();
            if errno != Some(ESRCH) {
                return Err(format!(
                    "cannot terminate Darwin codec group (errno={errno:?})"
                ));
            }
        }
        Ok(())
    }

    pub fn quiescent(&self) -> Result<bool, String> {
        Ok(self.sample()?.is_empty())
    }
}

pub fn spawn(command: &mut Command, scope: Scope) -> Result<(Child, Tree), String> {
    if !cfg!(all(
        target_pointer_width = "64",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )) {
        return Err("Darwin codec ownership requires verified 64-bit Intel/ARM libproc ABI".into());
    }
    // Refuse inaccessible/unsupported accounting BEFORE a child can exist.
    let pid = std::process::id() as i32;
    let own = bsd_info(pid)?.ok_or("cannot inspect Darwin codec owner's identity")?;
    task_info(pid)?.ok_or("cannot inspect Darwin codec owner's CPU accounting")?;
    if matches!(scope, Scope::Session | Scope::Console) {
        // Check all-PID enumeration and SID API before any child can exist.
        pids(None)?;
        session_identity(pid)?.ok_or("cannot inspect Darwin owner's session")?;
        // SAFETY: async-signal-safe setsid, only errno conversion on failure.
        // std's process_group setup runs before pre_exec, so do not set it here.
        unsafe {
            command.pre_exec(|| {
                if setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    } else {
        pids(Some(
            own.pgid
                .try_into()
                .map_err(|_| "invalid Darwin owner process group")?,
        ))?;
        command.process_group(0);
    }
    command.env("NUMERA_CODEC_OWNED_GROUP", "1");
    let child = command
        .spawn()
        .map_err(|_| "cannot spawn required codec child".to_owned())?;
    // Darwin pid_t is a positive signed int; no fallible step may abandon this child.
    let group = child.id() as i32;
    let tree = Tree {
        group,
        session: (matches!(scope, Scope::Session | Scope::Console)).then(|| Session::new(group)),
    };
    Ok((child, tree))
}
