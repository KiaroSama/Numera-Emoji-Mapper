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
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION, JOBOBJECT_BASIC_PROCESS_ID_LIST,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAndIoAccountingInformation,
    JobObjectBasicProcessIdList, JobObjectExtendedLimitInformation, QueryInformationJobObject,
    SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenProcess, OpenThread, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, ResumeThread, THREAD_SUSPEND_RESUME, WaitForSingleObject,
};

pub struct Tree(
    OwnedHandle,
    std::sync::Mutex<std::collections::BTreeMap<u32, OwnedHandle>>,
    #[cfg(test)] std::sync::atomic::AtomicBool,
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
            #[cfg(test)]
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

    #[cfg(test)]
    pub(super) fn fail_next_member_query(&self) {
        self.2.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(super) fn owned_handles(
        &self,
    ) -> (
        std::os::windows::io::RawHandle,
        std::os::windows::io::RawHandle,
    ) {
        let pinned = self.1.lock().unwrap();
        (
            self.0.as_raw_handle(),
            pinned
                .values()
                .next()
                .expect("real Job member must be pinned")
                .as_raw_handle(),
        )
    }

    fn attach_and_resume(&self, child: &Child) -> Result<(), String> {
        // SAFETY: child is suspended; both process/job handles remain owned.
        if unsafe { AssignProcessToJobObject(self.0.as_raw_handle(), child.as_raw_handle()) } == 0 {
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

    fn prune_finished_members(&self) -> Result<(), String> {
        let mut pinned = self
            .1
            .lock()
            .map_err(|_| "codec member accounting lock is poisoned")?;
        // A confirmed terminal process can never become live again. Release
        // its handle before querying: retaining it can keep the process in
        // the Job's inventory throughout a long compiler invocation.
        let mut finished = Vec::new();
        for (&pid, process) in pinned.iter() {
            if Self::signaled(process)? {
                finished.push(pid);
            }
        }
        for pid in finished {
            pinned.remove(&pid);
        }
        Ok(())
    }

    fn record_members(&self) -> Result<(), String> {
        #[cfg(test)]
        if self.2.swap(false, std::sync::atomic::Ordering::SeqCst) {
            return Err("fixture member query failed".into());
        }
        const SLOTS: u32 = 8192;
        let (count, storage) = complete_job_inventory(SLOTS, || {
            self.prune_finished_members()?;
            // usize storage supplies alignment for the flexible PID array.
            let mut storage = vec![0usize; SLOTS as usize + 2];
            let length = std::mem::size_of_val(storage.as_slice());
            // SAFETY: initialized, aligned writable storage covers the header
            // and SLOTS PIDs on both 32-bit and 64-bit Windows.
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
                return Err(format!(
                    "cannot inspect complete codec job member inventory (slots={SLOTS}, error={})",
                    unsafe { GetLastError() }
                ));
            }
            // SAFETY: successful query initialized the SDK header in aligned storage.
            let header = unsafe { &*storage.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() };
            Ok((
                header.NumberOfAssignedProcesses,
                header.NumberOfProcessIdsInList,
                storage,
            ))
        })?;
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
        for &id in members {
            let pid = u32::try_from(id).map_err(|_| "invalid codec job member identity")?;
            if let Some(process) = pinned.get(&pid) {
                if !Self::signaled(process)? {
                    continue; // A live pinned process cannot have its PID recycled.
                }
                pinned.remove(&pid);
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
            if member != 0 && !Self::signaled(&process)? {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::Stdio;

    // Technical self-child only: the ordinary suite returns immediately.
    // The shared pre-main constructor still requires real inherited test
    // admission before this test can acknowledge the selected child role.
    #[test]
    fn short_job_child_fixture() {
        let Some(role) = std::env::var_os("NUMERA_WINDOWS_SHORT_JOB_CHILD") else {
            return;
        };
        assert_eq!(role, "1");
    }

    #[test]
    fn one_job_samples_many_short_children_without_retaining_terminal_handles() {
        let tree = Tree::new().unwrap();
        let command = std::env::current_exe().unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut previous_total = 0;
        for iteration in 0..256 {
            assert!(Instant::now() < deadline, "child workload did not finish");
            let mut child = Command::new(&command)
                .args([
                    "--exact",
                    "process_owner::platform::tests::short_job_child_fixture",
                    "--test-threads=1",
                ])
                .env_clear()
                .envs(
                    ["PATH", "SystemRoot", "WINDIR", "PATHEXT"]
                        .into_iter()
                        .filter_map(|key| std::env::var_os(key).map(|value| (key, value))),
                )
                .env(
                    "NUMERA_RUNTIME_TEST_ADMISSION",
                    std::env::var_os("NUMERA_RUNTIME_TEST_ADMISSION")
                        .expect("actual inherited test admission"),
                )
                .env(
                    "NUMERA_EMOJI_MAPPER_NO_DOTENV",
                    std::env::var_os("NUMERA_EMOJI_MAPPER_NO_DOTENV")
                        .expect("actual inherited dotenv guard"),
                )
                .env("NUMERA_WINDOWS_SHORT_JOB_CHILD", "1")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW)
                .spawn()
                .unwrap();
            tree.attach_and_resume(&child).unwrap();
            loop {
                tree.sample().unwrap();
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success(), "child {iteration} failed");
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "child {iteration} did not finish"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            // The independently owned child handle proves termination before
            // the Tree is permitted to discard its handle to that object.
            assert_eq!(
                unsafe { WaitForSingleObject(child.as_raw_handle(), 0) },
                WAIT_OBJECT_0
            );
            let root = child.id();
            let settlement = deadline.min(Instant::now() + CLEANUP);
            loop {
                tree.record_members().unwrap();
                let pinned = tree
                    .1
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(&pid, process)| {
                        let mut member = 0;
                        // SAFETY: both handles stay owned throughout this inspection.
                        assert_ne!(
                            unsafe {
                                IsProcessInJob(
                                    process.as_raw_handle(),
                                    tree.0.as_raw_handle(),
                                    &mut member,
                                )
                            },
                            0
                        );
                        let signaled = Tree::signaled(process).unwrap();
                        assert!(
                            member != 0 || signaled,
                            "live pinned process {pid} left its Job"
                        );
                        (pid, signaled)
                    })
                    .collect::<Vec<_>>();
                // Keep the original child handle until after this check: its
                // PID cannot be reused to identify another live Job member.
                assert!(
                    pinned.iter().all(|&(pid, _)| pid != root),
                    "terminated child {iteration} ({root}) was retained: {pinned:?}"
                );
                let settled = tree.quiescent().unwrap();
                let info = tree.accounting().unwrap().BasicInfo;
                assert!(
                    Instant::now() < settlement,
                    "child {iteration} ({root}) left an unsettled Job: active={}, total={}, pinned(pid, signaled)={pinned:?}",
                    info.ActiveProcesses,
                    info.TotalProcesses
                );
                if settled && pinned.is_empty() {
                    break;
                }
                // Root termination does not prove that every helper process
                // has signaled. Continue sampling; production retains any
                // unsignaled handle until that independent proof arrives.
                std::thread::sleep(Duration::from_millis(1));
            }
            drop(child);
            assert!(tree.1.lock().unwrap().is_empty());
            assert!(tree.quiescent().unwrap());
            let info = tree.accounting().unwrap().BasicInfo;
            assert_eq!(info.ActiveProcesses, 0);
            assert!(info.TotalProcesses > previous_total);
            previous_total = info.TotalProcesses;
        }
        let info = tree.accounting().unwrap().BasicInfo;
        assert!(info.TotalProcesses >= 256);
        assert_eq!(info.ActiveProcesses, 0);
        // Direct stderr keeps the actual helper-inclusive count visible in
        // successful CI logs, even when libtest captures printing macros.
        writeln!(
            std::io::stderr().lock(),
            "Windows Job stress settled: roots=256, total={}, active=0, pinned=0",
            info.TotalProcesses
        )
        .unwrap();
    }
}
