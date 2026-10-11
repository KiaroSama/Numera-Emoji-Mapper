//! Darwin codec group accounting; a successful signal alone never permits cleanup.
use super::*;
use std::ffi::c_void;
use std::os::unix::process::CommandExt;

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
}

fn group_pids(group: i32) -> Result<Vec<i32>, String> {
    if group <= 0 {
        return Err("invalid Darwin codec process group".into());
    }
    let mut capacity = 256;
    loop {
        let mut pids = vec![0_i32; capacity];
        let size = std::mem::size_of_val(pids.as_slice()) as i32;
        // SAFETY: bounded initialized pid_t array; errno belongs to this thread.
        // libproc returns zero for BOTH an empty list and a failed syscall.
        let (bytes, errno) = unsafe {
            *__error() = 0;
            let bytes = proc_listpids(PROC_PGRP_ONLY, group as u32, pids.as_mut_ptr().cast(), size);
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

pub struct Tree {
    group: i32,
}

impl Tree {
    pub fn sample(&self) -> Result<Vec<u64>, String> {
        let mut values = Vec::new();
        for pid in group_pids(self.group)? {
            let info = match task_info(pid)? {
                Some(info) => info,
                None => {
                    // ESRCH from task-all can mean zombie OR gone. Verify with the
                    // zombie-aware flavor; a live-but-unreadable member is unknown.
                    match bsd_info(pid)? {
                        None => continue,
                        Some(info) if info.status == SZOMB => continue,
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
        // group_pids sorts by PID, so enumeration order is never false progress.
        Ok(values)
    }

    pub fn terminate(&self) -> Result<(), String> {
        if self.quiescent()? {
            return Ok(());
        }
        // SAFETY: process_group(0) gave this invocation a private positive PGID;
        // the verified Python bridge keeps FFmpeg inside it. Root reaping and
        // fresh libproc quiescence, not this signal, authorize scratch deletion.
        if unsafe { kill(-self.group, 9) } != 0 {
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

pub fn spawn(command: &mut Command) -> Result<(Child, Tree), String> {
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
    group_pids(
        own.pgid
            .try_into()
            .map_err(|_| "invalid Darwin owner process group")?,
    )?;
    command
        .process_group(0)
        .env("NUMERA_CODEC_OWNED_GROUP", "1");
    let child = command
        .spawn()
        .map_err(|_| "cannot spawn required codec child".to_owned())?;
    // Darwin pid_t is a positive signed int; no fallible step may abandon this child.
    let tree = Tree {
        group: child.id() as i32,
    };
    Ok((child, tree))
}
