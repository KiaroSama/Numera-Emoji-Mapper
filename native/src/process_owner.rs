//! Codec child ownership: terminate containment, reap the root, then remove scratch.
#![allow(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

const CLEANUP: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(100);

#[cfg(any(windows, test))]
fn job_settled(
    active: u32,
    signals: impl IntoIterator<Item = Result<bool, String>>,
) -> Result<bool, String> {
    let mut settled = active == 0;
    // Inspect every handle: an earlier live member must not hide an unknown signal.
    for signal in signals {
        settled &= signal?;
    }
    Ok(settled)
}

#[cfg(test)]
mod job_settlement_tests {
    use super::job_settled;

    #[test]
    fn zero_active_count_cannot_certify_a_still_unsignaled_pinned_process() {
        assert!(!job_settled(0, [Ok(false)]).unwrap());
        assert!(!job_settled(1, [Ok(true)]).unwrap());
        assert!(job_settled(0, [Ok(true), Ok(true)]).unwrap());
        assert!(job_settled(0, []).unwrap());
        assert!(job_settled(0, [Ok(false), Err("unknown process signal".into())]).is_err());
    }
}

pub(crate) enum WaitOutcome {
    Finished(ExitStatus),
    Wall(Duration),
    Idle(Duration),
}
impl WaitOutcome {
    pub(crate) fn status(self) -> Result<ExitStatus, String> {
        match self {
            Self::Finished(status) => Ok(status),
            Self::Wall(limit) => Err(format!(
                "media adapter exceeded {}s wall budget",
                limit.as_secs()
            )),
            Self::Idle(limit) => Err(format!(
                "media adapter made no CPU/IO progress for {}s",
                limit.as_secs()
            )),
        }
    }
}

/// Unknown inspection is pending cleanup, not permission to remove outer scratch.
/// Inspect only the two ownership layouts; never follow links or parse markers.
pub fn cleanup_pending(workspace: &Path) -> bool {
    for directory in [workspace.to_path_buf(), workspace.join("owner")] {
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        return true;
                    }
                }
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return true;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return true,
        }
        match std::fs::symlink_metadata(directory.join("codec-cleanup-pending.json")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return true,
        }
    }
    false
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Group,
    Session,
    Console,
    #[cfg(any(windows, target_os = "linux"))]
    Descendants,
}

#[cfg(unix)]
struct Session {
    root: i32,
    reaped: std::sync::atomic::AtomicBool,
    known: std::sync::Mutex<std::collections::BTreeMap<i32, [u64; 2]>>,
}

#[cfg(unix)]
impl Session {
    fn new(root: i32) -> Self {
        Self {
            root,
            reaped: std::sync::atomic::AtomicBool::new(false),
            known: Default::default(),
        }
    }

    fn root_reaped(&self) {
        self.reaped.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn is_reaped(&self) -> bool {
        self.reaped.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn before_reap(
        &self,
        exited: impl FnOnce() -> Result<bool, String>,
        sample: impl FnOnce() -> Result<(), String>,
    ) -> Result<bool, String> {
        if self.is_reaped() {
            return Ok(true);
        }
        if !exited()? {
            return Ok(false);
        }
        // A sample taken while root was live cannot authorize try_wait: root can
        // fork and exit between them. Enroll again after terminal observation,
        // while the unreaped original root still anchors the session identity.
        sample()?;
        Ok(true)
    }

    fn admit(
        &self,
        members: &[(i32, [u64; 2])],
        mut identity: impl FnMut(i32) -> Result<Option<[u64; 2]>, String>,
    ) -> Result<(), String> {
        if members.is_empty() {
            return Ok(());
        }
        let mut known = self
            .known
            .lock()
            .map_err(|_| "session identity accounting lock is poisoned")?;
        let anchor = members
            .iter()
            .find(|(pid, birth)| {
                known.get(pid) == Some(birth)
                    || (*pid == self.root && !self.is_reaped() && known.is_empty())
            })
            .copied();
        let Some((pid, birth)) = anchor else {
            return Err("cannot establish original session identity after root reaping".into());
        };
        // The anchor existed in this complete snapshot and still has the same SID
        // and birth afterwards. Numeric SID alone never enrolls a later reused cohort.
        if identity(pid)? != Some(birth) {
            return Err("session identity anchor changed during enumeration".into());
        }
        *known = members.iter().copied().collect();
        Ok(())
    }
}

pub struct OwnedProcess {
    child: Option<Child>,
    tree: platform::Tree,
    workspace: Option<PathBuf>,
}

impl OwnedProcess {
    pub fn spawn(command: &mut Command, workspace: PathBuf) -> Result<Self, String> {
        Self::spawn_scope(command, workspace, Scope::Group)
    }

    /// Own nested private process groups in one POSIX session (the same Job on Windows).
    /// Not an OS sandbox: deliberate setsid escape and abrupt owner death are not covered.
    pub fn spawn_session(command: &mut Command, workspace: PathBuf) -> Result<Self, String> {
        Self::spawn_scope(command, workspace, Scope::Session)
    }

    /// Explicit user-driven foreground child; automation keeps the hidden spawn paths.
    pub fn spawn_console(command: &mut Command, workspace: PathBuf) -> Result<Self, String> {
        Self::spawn_scope(command, workspace, Scope::Console)
    }

    /// Dedicated test-runner process only: Linux subreaper or Windows Job descendants.
    /// No other child owner may share that Linux runner; abrupt runner death is not covered.
    #[cfg(any(windows, target_os = "linux"))]
    pub fn spawn_descendants(command: &mut Command, workspace: PathBuf) -> Result<Self, String> {
        Self::spawn_scope(command, workspace, Scope::Descendants)
    }

    fn spawn_scope(
        command: &mut Command,
        workspace: PathBuf,
        scope: Scope,
    ) -> Result<Self, String> {
        // Establish the outer-workspace retention signal before any child exists.
        // If it cannot be persisted, fail without spawning an unmarked process.
        std::fs::write(
            workspace.join("codec-cleanup-pending.json"),
            r#"{"version":1,"reason":"codec owner active"}"#,
        )
        .map_err(|_| "cannot record codec process ownership".to_owned())?;
        let (child, tree) = match platform::spawn(command, scope) {
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
        self.wait_outcome(wall, idle).await?.status()
    }

    pub(crate) async fn wait_outcome(
        &mut self,
        wall: Duration,
        idle: Duration,
    ) -> Result<WaitOutcome, String> {
        self.wait_progress(wall, idle, || Ok(false)).await
    }

    /// Supplement tree activity without restarting either deadline or weakening descendant enrollment.
    pub(crate) async fn wait_progress(
        &mut self,
        wall: Duration,
        idle: Duration,
        mut progress: impl FnMut() -> Result<bool, String>,
    ) -> Result<WaitOutcome, String> {
        let started = Instant::now();
        let mut advanced = started;
        let mut previous = None;
        loop {
            // Enroll the complete current cohort before try_wait can reap the root.
            let sample = self.tree.sample()?;
            let external = progress()?;
            if previous.as_ref() != Some(&sample) || external {
                advanced = Instant::now();
                previous = Some(sample);
            }
            if let Some(status) = self.try_reap()? {
                return Ok(WaitOutcome::Finished(status));
            }
            if started.elapsed() >= wall {
                return Ok(WaitOutcome::Wall(wall));
            }
            if advanced.elapsed() >= idle {
                return Ok(WaitOutcome::Idle(idle));
            }
            // Bounded monitor cadence, not an unbounded readiness sleep.
            tokio::time::sleep(POLL).await;
        }
    }

    /// User-driven commands may wait for input or network indefinitely; their automated
    /// callers provide wall/idle bounds. Cleanup always retains its existing10s ceiling.
    pub async fn wait_console(&mut self) -> Result<ExitStatus, String> {
        loop {
            self.tree.sample()?;
            if let Some(status) = self.try_reap()? {
                return Ok(status);
            }
            tokio::time::sleep(POLL).await;
        }
    }

    pub fn interrupt(&self) -> Result<(), String> {
        self.tree.interrupt()
    }

    fn try_reap(&mut self) -> Result<Option<ExitStatus>, String> {
        if !self.tree.ready_to_reap()? {
            return Ok(None);
        }
        let status = self
            .child
            .as_mut()
            .ok_or("codec owner has no child")?
            .try_wait()
            .map_err(|_| "cannot inspect or reap codec child".to_owned())?;
        if status.is_some() {
            self.tree.root_reaped();
        }
        Ok(status)
    }

    pub fn finish(&mut self) -> Result<(), String> {
        if self.workspace.is_none() {
            return Ok(());
        }
        let deadline = Instant::now() + CLEANUP;
        loop {
            // A member can fork another private group during the previous snapshot.
            // Repeat verified signals within the existing bound, then sample afresh.
            self.tree.terminate()?;
            let root_done = self.try_reap()?.is_some();
            if root_done && self.tree.quiescent()? {
                let workspace = self.workspace.as_ref().ok_or("missing codec workspace")?;
                std::fs::remove_dir_all(workspace)
                    .map_err(|_| "codec tree stopped but scratch cleanup failed".to_owned())?;
                self.child.take();
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
    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_PARAMETER, GetLastError, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION,
        JOBOBJECT_BASIC_PROCESS_ID_LIST, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectBasicAndIoAccountingInformation, JobObjectBasicProcessIdList,
        JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
        TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenProcess, OpenThread,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, ResumeThread,
        THREAD_SUSPEND_RESUME, WaitForSingleObject,
    };

    pub struct Tree(
        OwnedHandle,
        std::sync::Mutex<std::collections::BTreeMap<u32, OwnedHandle>>,
    );

    impl Tree {
        fn new() -> Result<Self, String> {
            // SAFETY: no inherited handle or pointer to borrowed security/name data.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err("cannot create codec process job".into());
            }
            // SAFETY: newly created non-null handle has exactly one owner.
            let tree = Self(
                unsafe { OwnedHandle::from_raw_handle(handle) },
                Default::default(),
            );
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
            self.record_members()?;
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

        fn signaled(process: &OwnedHandle) -> Result<bool, String> {
            // SAFETY: the original process object stays pinned by this owned handle.
            match unsafe { WaitForSingleObject(process.as_raw_handle(), 0) } {
                WAIT_OBJECT_0 => Ok(true),
                WAIT_TIMEOUT => Ok(false),
                _ => Err("cannot verify pinned codec process termination".into()),
            }
        }

        fn record_members(&self) -> Result<(), String> {
            const SLOTS: usize = 8192;
            // usize storage supplies native alignment for the flexible PID array.
            let mut storage = vec![0usize; SLOTS + 2];
            let length = std::mem::size_of_val(storage.as_slice());
            // SAFETY: initialized, aligned writable storage covers the header and
            // SLOTS PIDs on both 32-bit and 64-bit Windows.
            if unsafe {
                QueryInformationJobObject(
                    self.0.as_raw_handle(),
                    JobObjectBasicProcessIdList,
                    storage.as_mut_ptr().cast(),
                    length as u32,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                return Err("cannot inspect complete codec job member inventory".into());
            }
            // SAFETY: successful query initialized the SDK header in aligned storage.
            let header = unsafe { &*storage.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() };
            let count = header.NumberOfProcessIdsInList as usize;
            if count > SLOTS || header.NumberOfAssignedProcesses as usize != count {
                return Err("codec job member inventory is incomplete or exceeds its bound".into());
            }
            // SAFETY: derive from the whole allocation, not the SDK's one-element
            // placeholder. The checked count fits the allocated native PID array.
            let members = unsafe {
                std::slice::from_raw_parts(
                    storage
                        .as_ptr()
                        .cast::<u8>()
                        .add(std::mem::offset_of!(
                            JOBOBJECT_BASIC_PROCESS_ID_LIST,
                            ProcessIdList
                        ))
                        .cast::<usize>(),
                    count,
                )
            };
            let mut pinned = self
                .1
                .lock()
                .map_err(|_| "codec member accounting lock is poisoned")?;
            // A confirmed terminal process can never become live again. Release
            // those handles so a long Cargo/build invocation does not retain
            // every completed compiler or test process until final cleanup.
            let mut finished = Vec::new();
            for (&pid, process) in pinned.iter() {
                if Self::signaled(process)? {
                    finished.push(pid);
                }
            }
            for pid in finished {
                pinned.remove(&pid);
            }
            for &id in members {
                let pid = u32::try_from(id).map_err(|_| "invalid codec job member identity")?;
                if let Some(process) = pinned.get(&pid)
                    && !Self::signaled(process)?
                {
                    continue; // A live pinned process cannot have its PID recycled.
                }
                // SAFETY: only inspection rights; verify private Job membership
                // before enrolling a numeric PID that might have exited/recycled.
                let raw = unsafe {
                    OpenProcess(
                        PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                        0,
                        pid,
                    )
                };
                if raw.is_null() {
                    // ERROR_INVALID_PARAMETER confirms the PID has no live object.
                    if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
                        continue;
                    }
                    return Err("cannot pin codec job member for termination verification".into());
                }
                // SAFETY: fresh process handle has exactly one owner on every path.
                let process = unsafe { OwnedHandle::from_raw_handle(raw) };
                let mut member = 0;
                if unsafe {
                    IsProcessInJob(process.as_raw_handle(), self.0.as_raw_handle(), &mut member)
                } == 0
                {
                    return Err("cannot verify pinned codec process Job membership".into());
                }
                if member != 0 {
                    pinned.insert(pid, process);
                }
                // Non-members are never enrolled or signaled: the old member
                // exited and its PID may now identify an unrelated process.
            }
            Ok(())
        }

        pub fn ready_to_reap(&self) -> Result<bool, String> {
            Ok(true)
        }

        pub fn root_reaped(&self) {}

        pub fn sample(&self) -> Result<Vec<u64>, String> {
            self.record_members()?;
            let info = self.accounting()?;
            Ok(vec![
                info.BasicInfo.TotalUserTime as u64,
                info.BasicInfo.TotalKernelTime as u64,
                info.IoInfo.ReadTransferCount,
                info.IoInfo.WriteTransferCount,
                info.BasicInfo.TotalProcesses as u64,
            ])
        }
        pub fn interrupt(&self) -> Result<(), String> {
            // The shared console already delivers Ctrl+C to the child. Never broadcast
            // another event to the whole console or suppress the child's own handler.
            Ok(())
        }

        pub fn terminate(&self) -> Result<(), String> {
            self.record_members()?;
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
            let active = self.accounting()?.BasicInfo.ActiveProcesses;
            let pinned = self
                .1
                .lock()
                .map_err(|_| "codec member accounting lock is poisoned")?;
            // Windows can decrement ActiveProcesses before the original process
            // objects signal. Retain those handles through asynchronous termination.
            job_settled(active, pinned.values().map(Self::signaled))
        }
    }

    pub fn spawn(command: &mut Command, scope: Scope) -> Result<(Child, Tree), String> {
        let tree = Tree::new()?;
        command.creation_flags(
            CREATE_SUSPENDED
                | if scope == Scope::Console && crate::tooling_interrupt::console_attached() {
                    0
                } else {
                    CREATE_NO_WINDOW
                },
        );
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

#[cfg(all(test, unix))]
mod session_tests {
    use super::Session;

    #[test]
    fn reap_gate_waits_for_terminal_root_then_enrolls_late_children_before_reaping() {
        use std::cell::Cell;
        let owner = Session::new(10);
        owner
            .admit(&[(10, [100, 0])], |_| Ok(Some([100, 0])))
            .unwrap();
        let exited = Cell::new(false);
        assert!(
            !owner
                .before_reap(
                    || {
                        // Root can fork and exit immediately after this live observation.
                        exited.set(true);
                        Ok(false)
                    },
                    || panic!("a live root must not authorize a reap"),
                )
                .unwrap()
        );
        assert!(exited.get());
        let sampled = Cell::new(false);
        assert!(
            owner
                .before_reap(
                    || Ok(exited.get()),
                    || {
                        assert!(!owner.is_reaped());
                        owner.admit(&[(10, [100, 0]), (11, [101, 0])], |_| Ok(Some([100, 0])))?;
                        sampled.set(true);
                        Ok(())
                    },
                )
                .unwrap()
        );
        assert!(sampled.get());
        owner.root_reaped();
        owner
            .admit(&[(11, [101, 0])], |_| Ok(Some([101, 0])))
            .unwrap();
        assert!(
            owner
                .before_reap(
                    || panic!("cached reaped status needs no root PID query"),
                    || panic!("cached reaped status needs no enrollment"),
                )
                .unwrap()
        );
        let failed = Session::new(20);
        assert!(
            failed
                .before_reap(|| Err("unknown root".into()), || Ok(()))
                .is_err()
        );
        assert!(
            failed
                .before_reap(|| Ok(true), || Err("unknown cohort".into()))
                .is_err()
        );
        assert!(!failed.is_reaped());
    }

    #[test]
    fn cohort_admission_refuses_reused_or_unknown_identity_after_root_reaping() {
        let owner = Session::new(10);
        let first = [(10, [100, 0]), (11, [101, 0])];
        owner
            .admit(&first, |pid| {
                Ok(first
                    .iter()
                    .find(|value| value.0 == pid)
                    .map(|value| value.1))
            })
            .unwrap();
        owner.root_reaped();
        owner
            .admit(&[(11, [101, 0]), (12, [102, 0])], |_| Ok(Some([101, 0])))
            .unwrap();
        let before = owner.known.lock().unwrap().clone();
        assert!(
            owner
                .admit(&[(11, [999, 0])], |_| panic!(
                    "no trusted anchor may be queried"
                ))
                .is_err()
        );
        assert!(
            owner
                .admit(&[(12, [102, 0])], |_| Err("permission denied".into()))
                .is_err()
        );
        assert!(owner.admit(&[(12, [102, 0])], |_| Ok(None)).is_err());
        assert!(
            owner
                .admit(&[(12, [102, 0])], |_| Ok(Some([999, 0])))
                .is_err()
        );
        assert_eq!(
            *owner.known.lock().unwrap(),
            before,
            "failed inspection must not enroll anything"
        );
        assert!(
            owner
                .admit(&[], |_| panic!("empty snapshot needs no signal"))
                .is_ok()
        );
        let never_sampled = Session::new(10);
        never_sampled.root_reaped();
        assert!(
            never_sampled
                .admit(&[(10, [100, 0])], |_| panic!("reaped root cannot pin PID"))
                .is_err()
        );
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
#[path = "process_owner_progress_tests.rs"]
pub(crate) mod progress_tests;

#[cfg(target_os = "linux")]
#[path = "process_owner_descendants.rs"]
mod descendants;

#[cfg(target_os = "linux")]
#[path = "process_owner_linux.rs"]
mod platform;

#[cfg(target_os = "macos")]
#[path = "process_owner_macos.rs"]
mod platform;

#[cfg(target_os = "macos")]
pub use platform::process_birth;

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod platform {
    use super::*;
    pub struct Tree;
    impl Tree {
        pub fn ready_to_reap(&self) -> Result<bool, String> {
            Ok(true)
        }

        pub fn root_reaped(&self) {}
        pub fn sample(&self) -> Result<Vec<u64>, String> {
            Err("codec process ownership supports Windows/Linux/macOS only".into())
        }
        pub fn interrupt(&self) -> Result<(), String> {
            Err("console process ownership supports Windows/Linux/macOS only".into())
        }
        pub fn terminate(&self) -> Result<(), String> {
            Ok(())
        }
        pub fn quiescent(&self) -> Result<bool, String> {
            Ok(true)
        }
    }
    pub fn spawn(_command: &mut Command, _scope: Scope) -> Result<(Child, Tree), String> {
        Err(
            "codec process ownership supports Windows/Linux/macOS only; this platform is unverified"
                .into(),
        )
    }
}
