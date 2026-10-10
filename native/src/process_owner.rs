//! Codec child ownership: terminate containment, reap the root, then remove scratch.
#![allow(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

const CLEANUP: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(100);

pub struct OwnedProcess {
    child: Option<Child>,
    tree: platform::Tree,
    workspace: Option<PathBuf>,
}

impl OwnedProcess {
    pub fn spawn(command: &mut Command, workspace: PathBuf) -> Result<Self, String> {
        // Establish the outer-workspace retention signal before any child exists.
        // If it cannot be persisted, fail without spawning an unmarked process.
        std::fs::write(
            workspace.join("codec-cleanup-pending.json"),
            r#"{"version":1,"reason":"codec owner active"}"#,
        )
        .map_err(|_| "cannot record codec process ownership".to_owned())?;
        let (child, tree) = match platform::spawn(command) {
            Ok(value) => value,
            Err(error) => {
                // Platform spawn retains scratch on an unverified spawn/kill failure.
                retain(&workspace, &error);
                return Err(error);
            }
        };
        Ok(Self {
            child: Some(child),
            tree,
            workspace: Some(workspace),
        })
    }

    pub async fn wait(&mut self, wall: Duration, idle: Duration) -> Result<ExitStatus, String> {
        let started = Instant::now();
        let mut advanced = started;
        let mut previous = self.tree.sample()?;
        loop {
            if let Some(status) = self
                .child
                .as_mut()
                .ok_or("codec owner has no child")?
                .try_wait()
                .map_err(|_| "cannot inspect codec child".to_owned())?
            {
                return Ok(status);
            }
            let sample = self.tree.sample()?;
            if sample != previous {
                advanced = Instant::now();
                previous = sample;
            }
            if started.elapsed() >= wall {
                return Err(format!(
                    "media adapter exceeded {}s wall budget",
                    wall.as_secs()
                ));
            }
            if advanced.elapsed() >= idle {
                return Err(format!(
                    "media adapter made no CPU/IO progress for {}s",
                    idle.as_secs()
                ));
            }
            // Bounded monitor cadence, not an unbounded readiness sleep.
            tokio::time::sleep(POLL).await;
        }
    }

    pub fn finish(&mut self) -> Result<(), String> {
        if self.workspace.is_none() {
            return Ok(());
        }
        self.tree.terminate()?;
        let deadline = Instant::now() + CLEANUP;
        loop {
            let root_done = match self.child.as_mut() {
                Some(child) => child
                    .try_wait()
                    .map_err(|_| "cannot reap codec root".to_owned())?
                    .is_some(),
                None => true,
            };
            if root_done && self.tree.quiescent()? {
                self.child.take();
                let workspace = self.workspace.as_ref().ok_or("missing codec workspace")?;
                std::fs::remove_dir_all(workspace)
                    .map_err(|_| "codec tree stopped but scratch cleanup failed".to_owned())?;
                self.workspace.take();
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(
                    "codec tree termination/reaping could not be verified within10s".into(),
                );
            }
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        // Drop cannot await or spawn a detached cleanup task: cancellation must
        // finish containment before an enclosing workspace is allowed to vanish.
        // A marker covers unwinding while native termination/accounting itself fails.
        if let Some(workspace) = &self.workspace {
            retain(workspace, "codec owner finalization pending");
        }
        if let Err(error) = self.finish() {
            if let Some(workspace) = &self.workspace {
                retain(workspace, &error);
            }
            eprintln!("ERROR: {error}; codec scratch retained for recovery");
        }
    }
}

fn retain(workspace: &Path, reason: &str) {
    // The outer CLI must honor this marker before removing its whole scratch root.
    // No input paths, credentials or child output belong in this marker.
    let marker = workspace.join("codec-cleanup-pending.json");
    let _ = std::fs::write(
        marker,
        serde_json::json!({"version":1,"reason":reason}).to_string(),
    );
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectBasicAndIoAccountingInformation, JobObjectExtendedLimitInformation,
        QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
    };

    pub struct Tree(OwnedHandle);

    impl Tree {
        fn new() -> Result<Self, String> {
            // SAFETY: no inherited handle or pointer to borrowed security/name data.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err("cannot create codec process job".into());
            }
            // SAFETY: newly created non-null handle has exactly one owner.
            let tree = Self(unsafe { OwnedHandle::from_raw_handle(handle) });
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: owned handle and initialized correctly sized native structure.
            let ok = unsafe {
                SetInformationJobObject(
                    tree.0.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32,
                )
            };
            if ok == 0 {
                return Err("cannot configure codec process job".into());
            }
            Ok(tree)
        }

        fn attach_and_resume(&self, child: &Child) -> Result<(), String> {
            // SAFETY: child is suspended; both process/job handles remain owned.
            if unsafe { AssignProcessToJobObject(self.0.as_raw_handle(), child.as_raw_handle()) }
                == 0
            {
                return Err("cannot contain codec child in a process job".into());
            }
            // SAFETY: no pointer arguments. Snapshot only enumerates thread identity.
            let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
            if raw == INVALID_HANDLE_VALUE {
                return Err("cannot inspect suspended codec thread".into());
            }
            // SAFETY: newly created valid snapshot has exactly one owner.
            let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
            let mut entry = THREADENTRY32 {
                dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
                ..Default::default()
            };
            // SAFETY: initialized writable entry and live snapshot handle.
            let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) } != 0;
            while found {
                if entry.th32OwnerProcessID == child.id() {
                    // SAFETY: suspended child's PID cannot be recycled while its handle is held.
                    let raw = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                    if raw.is_null() {
                        return Err("cannot open suspended codec thread".into());
                    }
                    // SAFETY: fresh valid thread handle transferred to one owner.
                    let thread = unsafe { OwnedHandle::from_raw_handle(raw) };
                    // SAFETY: no child instruction ran before job assignment; resume exactly once.
                    let before = unsafe { ResumeThread(thread.as_raw_handle()) };
                    if before != 1 {
                        return Err("unexpected codec thread suspension state".into());
                    }
                    return Ok(());
                }
                // SAFETY: snapshot and writable entry remain live through enumeration.
                found = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) } != 0;
            }
            Err("suspended codec primary thread was not found".into())
        }

        fn accounting(&self) -> Result<JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION, String> {
            let mut info = JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION::default();
            // SAFETY: initialized output buffer matches requested native information class.
            let ok = unsafe {
                QueryInformationJobObject(
                    self.0.as_raw_handle(),
                    JobObjectBasicAndIoAccountingInformation,
                    (&mut info as *mut JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION).cast(),
                    std::mem::size_of_val(&info) as u32,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err("cannot inspect codec job progress".into());
            }
            Ok(info)
        }

        pub fn sample(&self) -> Result<Vec<u64>, String> {
            let info = self.accounting()?;
            Ok(vec![
                info.BasicInfo.TotalUserTime as u64,
                info.BasicInfo.TotalKernelTime as u64,
                info.IoInfo.ReadTransferCount,
                info.IoInfo.WriteTransferCount,
                info.BasicInfo.TotalProcesses as u64,
            ])
        }
        pub fn terminate(&self) -> Result<(), String> {
            if self.quiescent()? {
                return Ok(());
            }
            // SAFETY: this private job contains only this codec invocation's tree.
            if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
                return Err("cannot terminate codec process job".into());
            }
            Ok(())
        }
        pub fn quiescent(&self) -> Result<bool, String> {
            Ok(self.accounting()?.BasicInfo.ActiveProcesses == 0)
        }
    }

    pub fn spawn(command: &mut Command) -> Result<(Child, Tree), String> {
        let tree = Tree::new()?;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        let mut child = command
            .spawn()
            .map_err(|_| "cannot spawn required codec child".to_owned())?;
        if let Err(error) = tree.attach_and_resume(&child) {
            // Assignment failure happens before execution, so no descendants can escape.
            let _ = tree.terminate();
            let _ = child.kill();
            let deadline = Instant::now() + CLEANUP;
            while !child
                .try_wait()
                .map_err(|_| "cannot reap suspended codec child")?
                .is_some()
            {
                if Instant::now() >= deadline {
                    return Err("suspended codec child could not be reaped".into());
                }
                std::thread::sleep(POLL);
            }
            return Err(error);
        }
        Ok((child, tree))
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::io::ErrorKind;
    use std::os::unix::process::CommandExt;

    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    pub struct Tree {
        group: i32,
    }

    impl Tree {
        pub fn sample(&self) -> Result<Vec<u64>, String> {
            let mut values = Vec::new();
            let entries =
                std::fs::read_dir("/proc").map_err(|_| "codec ownership requires Linux /proc")?;
            for entry in entries {
                let entry = entry.map_err(|_| "cannot enumerate codec process group")?;
                let Some(pid) = entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.parse::<u64>().ok())
                else {
                    continue;
                };
                let path = entry.path().join("stat");
                let stat = match std::fs::read_to_string(&path) {
                    Ok(value) => value,
                    Err(error) if error.kind() == ErrorKind::NotFound => continue,
                    Err(_) => return Err("cannot verify codec process-group membership".into()),
                };
                let (_, fields) = stat
                    .rsplit_once(") ")
                    .ok_or("malformed Linux process accounting")?;
                let fields: Vec<&str> = fields.split_whitespace().collect();
                if fields.len() < 20 {
                    return Err("incomplete Linux process accounting".into());
                }
                let group: i32 = fields[2]
                    .parse()
                    .map_err(|_| "invalid Linux process group")?;
                if group != self.group || fields[0] == "Z" || fields[0] == "X" {
                    continue;
                }
                values.extend([
                    pid,
                    fields[19]
                        .parse::<u64>()
                        .map_err(|_| "invalid process birth time")?,
                    fields[11]
                        .parse::<u64>()
                        .map_err(|_| "invalid process CPU time")?,
                    fields[12]
                        .parse::<u64>()
                        .map_err(|_| "invalid process CPU time")?,
                ]);
            }
            // /proc enumeration order is not progress; preserve a deterministic signature.
            let mut rows: Vec<Vec<u64>> = values.chunks_exact(4).map(|row| row.to_vec()).collect();
            rows.sort();
            Ok(rows.into_iter().flatten().collect())
        }
        pub fn terminate(&self) -> Result<(), String> {
            if self.quiescent()? {
                return Ok(());
            }
            // SAFETY: process_group(0) established a private PGID equal to this child's PID.
            // Adapter-only inherited-group mode must prevent FFmpeg creating another session.
            if unsafe { kill(-self.group, 9) } != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(3) {
                    return Err("cannot terminate codec process group".into());
                }
            }
            Ok(())
        }
        pub fn quiescent(&self) -> Result<bool, String> {
            Ok(self.sample()?.is_empty())
        }
    }

    pub fn spawn(command: &mut Command) -> Result<(Child, Tree), String> {
        if !Path::new("/proc/self/stat").is_file() {
            return Err("Linux codec lifecycle requires readable /proc accounting".into());
        }
        command
            .process_group(0)
            .env("NUMERA_CODEC_OWNED_GROUP", "1");
        let child = command
            .spawn()
            .map_err(|_| "cannot spawn required codec child".to_owned())?;
        let group =
            i32::try_from(child.id()).map_err(|_| "codec process id out of range".to_owned())?;
        Ok((child, Tree { group }))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::*;
    pub struct Tree;
    impl Tree {
        pub fn sample(&self) -> Result<Vec<u64>, String> {
            Err("codec process ownership supports Windows/Linux only".into())
        }
        pub fn terminate(&self) -> Result<(), String> {
            Ok(())
        }
        pub fn quiescent(&self) -> Result<bool, String> {
            Ok(true)
        }
    }
    pub fn spawn(_command: &mut Command) -> Result<(Child, Tree), String> {
        Err(
            "codec process ownership supports Windows/Linux only; this platform is unverified"
                .into(),
        )
    }
}
