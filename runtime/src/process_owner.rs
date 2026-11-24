//! Codec child ownership: terminate containment, reap the root, then remove scratch.
#![allow(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::{Duration, Instant};

const CLEANUP: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(100);

#[derive(Debug)]
enum InventoryPoll<T> {
    Ready(T),
    #[cfg(any(windows, test))]
    Pending(String),
}

#[cfg(any(windows, test))]
impl<T> InventoryPoll<T> {
    fn require_complete(self) -> Result<T, String> {
        match self {
            Self::Ready(value) => Ok(value),
            Self::Pending(reason) => Err(reason),
        }
    }
}

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

#[cfg(any(windows, test))]
fn complete_job_inventory<T>(
    slots: u32,
    query: impl FnMut() -> Result<(u32, u32, T), String>,
) -> Result<(usize, T), String> {
    poll_job_inventory(slots, query)?.require_complete()
}

#[cfg(any(windows, test))]
fn poll_job_inventory<T>(
    slots: u32,
    mut query: impl FnMut() -> Result<(u32, u32, T), String>,
) -> Result<InventoryPoll<(usize, T)>, String> {
    const ATTEMPTS: usize = 8;
    let mut last = (0, 0);
    for attempt in 1..=ATTEMPTS {
        let (assigned, listed, inventory) = query()?;
        if assigned > slots || listed > slots {
            return Err(format!(
                "codec job member inventory exceeds its bound (assigned={assigned}, listed={listed}, slots={slots}, attempts={attempt})"
            ));
        }
        if assigned == listed {
            return Ok(InventoryPoll::Ready((listed as usize, inventory)));
        }
        last = (assigned, listed);
        if attempt < ATTEMPTS {
            // A changing Job can return different header/list counts. Give its
            // updates a chance to settle, without sleeping or adding a deadline.
            std::thread::yield_now();
        }
    }
    Ok(InventoryPoll::Pending(format!(
        "codec job member inventory remains incomplete (assigned={}, listed={}, slots={slots}, attempts={ATTEMPTS})",
        last.0, last.1
    )))
}

#[cfg(test)]
mod job_settlement_tests {
    use super::{complete_job_inventory, job_settled};

    #[test]
    fn zero_active_count_cannot_certify_a_still_unsignaled_pinned_process() {
        assert!(!job_settled(0, [Ok(false)]).unwrap());
        assert!(!job_settled(1, [Ok(true)]).unwrap());
        assert!(job_settled(0, [Ok(true), Ok(true)]).unwrap());
        assert!(job_settled(0, []).unwrap());
        assert!(job_settled(0, [Ok(false), Err("unknown process signal".into())]).is_err());
    }

    #[test]
    fn partial_job_inventory_is_retried_and_never_returned() {
        for assigned in [1, 3] {
            let mut queries = 0;
            let complete = complete_job_inventory(8192, || {
                queries += 1;
                Ok(if queries == 1 {
                    (assigned, 2, vec![10, 20])
                } else {
                    (2, 2, vec![20, 30])
                })
            })
            .unwrap();
            assert_eq!(queries, 2);
            assert_eq!(complete, (2, vec![20, 30]));
        }
    }

    #[test]
    fn persistent_partial_job_inventory_is_bounded_and_reports_counts() {
        let mut queries = 0;
        let error = complete_job_inventory(8192, || {
            queries += 1;
            Ok((4, 3, ()))
        })
        .unwrap_err();
        assert_eq!(queries, 8);
        assert!(error.contains("assigned=4, listed=3, slots=8192, attempts=8"));
    }

    #[test]
    fn job_inventory_bound_and_unknown_query_still_fail_closed() {
        for (assigned, listed) in [(8193, 8192), (8192, 8193)] {
            let mut queries = 0;
            let error = complete_job_inventory(8192, || {
                queries += 1;
                Ok((assigned, listed, ()))
            })
            .unwrap_err();
            assert_eq!(queries, 1);
            assert!(error.contains("exceeds its bound"));
            assert!(error.contains("slots=8192, attempts=1"));
        }
        let mut queries = 0;
        let error = complete_job_inventory(8192, || {
            queries += 1;
            if queries == 1 {
                Ok((2, 1, ()))
            } else {
                Err("cannot inspect inventory".into())
            }
        })
        .unwrap_err();
        assert_eq!(queries, 2);
        assert_eq!(error, "cannot inspect inventory");
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

struct WaitMonitor {
    started: Instant,
    advanced: Instant,
    previous: Option<Vec<u64>>,
    wall: Duration,
    idle: Duration,
    pending: bool,
}

impl WaitMonitor {
    fn new(wall: Duration, idle: Duration, started: Instant) -> Self {
        Self {
            started,
            advanced: started,
            previous: None,
            wall,
            idle,
            pending: false,
        }
    }

    fn expired(&self, now: Instant) -> Option<WaitOutcome> {
        // Elapsed comparisons also support budgets too large to add to Instant.
        if now.duration_since(self.started) >= self.wall {
            Some(WaitOutcome::Wall(self.wall))
        } else if now.duration_since(self.advanced) >= self.idle {
            Some(WaitOutcome::Idle(self.idle))
        } else {
            None
        }
    }

    fn pending_expired(&self, now: Instant) -> Option<WaitOutcome> {
        if self.pending {
            self.expired(now)
        } else {
            None
        }
    }

    fn poll(
        &mut self,
        inventory: Result<InventoryPoll<Vec<u64>>, String>,
        progress: impl FnOnce() -> Result<bool, String>,
        reap: impl FnOnce() -> Result<Option<ExitStatus>, String>,
        mut now: impl FnMut() -> Instant,
    ) -> Result<Option<WaitOutcome>, String> {
        let inventory = inventory?;
        // A complete response arriving after pending must not restart an expired
        // idle budget, including when there has never been a complete sample.
        if let Some(outcome) = self.pending_expired(now()) {
            return Ok(Some(outcome));
        }
        match inventory {
            InventoryPoll::Ready(sample) => {
                let external = progress()?;
                if self.previous.as_ref() != Some(&sample) || external {
                    self.advanced = now();
                    self.previous = Some(sample);
                }
                self.pending = false;
                if let Some(status) = reap()? {
                    return Ok(Some(WaitOutcome::Finished(status)));
                }
            }
            #[cfg(any(windows, test))]
            InventoryPoll::Pending(_) => self.pending = true,
        }
        Ok(self.expired(now()))
    }
}

struct CleanupMonitor {
    deadline: Instant,
    pending: bool,
}

impl CleanupMonitor {
    fn new(started: Instant) -> Self {
        Self {
            deadline: started + CLEANUP,
            pending: false,
        }
    }

    fn check_deadline(&self, now: Instant) -> Result<(), String> {
        if now >= self.deadline {
            Err("codec tree termination/reaping could not be verified within10s".into())
        } else {
            Ok(())
        }
    }

    fn check_pending_deadline(&self, now: Instant) -> Result<(), String> {
        if self.pending {
            self.check_deadline(now)?;
        }
        Ok(())
    }

    fn poll(
        &mut self,
        inventory: Result<InventoryPoll<()>, String>,
        settled: impl FnOnce() -> Result<bool, String>,
        mut now: impl FnMut() -> Instant,
    ) -> Result<bool, String> {
        let inventory = inventory?;
        self.check_pending_deadline(now())?;
        match inventory {
            InventoryPoll::Ready(()) => {
                self.pending = false;
                if settled()? {
                    return Ok(true);
                }
            }
            #[cfg(any(windows, test))]
            InventoryPoll::Pending(_) => self.pending = true,
        }
        self.check_deadline(now())?;
        Ok(false)
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
    #[cfg(any(windows, target_os = "linux"))]
    DescendantSession,
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

    /// Dedicated single-child automation only: Linux subreaper or Windows Job descendants.
    /// No other child owner may share that Linux process; abrupt owner death is not covered.
    #[cfg(any(windows, target_os = "linux"))]
    pub fn spawn_descendants(command: &mut Command, workspace: PathBuf) -> Result<Self, String> {
        Self::spawn_scope(command, workspace, Scope::Descendants)
    }

    /// Dedicated single-child automation only: preserve session interruption while
    /// also owning nested sessions through the existing descendant custodian.
    /// Linux admission requires no other child owner; it cannot prevent unrelated
    /// future library spawns or contain abrupt death of this owning process.
    #[cfg(any(windows, target_os = "linux"))]
    pub(crate) fn spawn_descendant_session(
        command: &mut Command,
        workspace: PathBuf,
    ) -> Result<Self, String> {
        Self::spawn_scope(command, workspace, Scope::DescendantSession)
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
        let mut monitor = WaitMonitor::new(wall, idle, Instant::now());
        loop {
            if let Some(outcome) = monitor.pending_expired(Instant::now()) {
                return Ok(outcome);
            }
            // Enroll the complete current cohort before try_wait can reap the root.
            let inventory = self.tree.poll_sample();
            if let Some(outcome) =
                monitor.poll(inventory, &mut progress, || self.try_reap(), Instant::now)?
            {
                return Ok(outcome);
            }
            // Bounded monitor cadence, not an unbounded readiness sleep.
            tokio::time::sleep(POLL).await;
        }
    }

    /// User-driven commands may wait for input or network indefinitely; their automated
    /// callers provide wall/idle bounds. Cleanup always retains its existing10s ceiling.
    pub async fn wait_console(&mut self) -> Result<ExitStatus, String> {
        loop {
            // No caller deadline exists here: keep incomplete inspection a strict refusal.
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
        let mut monitor = CleanupMonitor::new(Instant::now());
        loop {
            monitor.check_pending_deadline(Instant::now())?;
            // A member can fork another private group during the previous snapshot.
            // Repeat verified signals within the existing bound, then sample afresh.
            let inventory = self.tree.poll_terminate();
            if monitor.poll(
                inventory,
                || {
                    let root_done = self.try_reap()?.is_some();
                    Ok(root_done && self.tree.quiescent()?)
                },
                Instant::now,
            )? {
                let workspace = self.workspace.as_ref().ok_or("missing codec workspace")?;
                std::fs::remove_dir_all(workspace)
                    .map_err(|_| "codec tree stopped but scratch cleanup failed".to_owned())?;
                self.child.take();
                self.workspace.take();
                return Ok(());
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
#[path = "process_owner/windows.rs"]
mod platform;

#[cfg(test)]
#[path = "../tests/baseline_lifecycle_support/process_owner_windows_units.rs"]
mod baseline_lifecycle;

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

#[cfg(target_os = "linux")]
#[path = "process_owner_descendants.rs"]
mod descendants;

#[cfg(target_os = "linux")]
#[path = "process_owner_linux.rs"]
mod platform;

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::*;
    pub struct Tree;
    impl Tree {
        pub fn ready_to_reap(&self) -> Result<bool, String> {
            Ok(true)
        }

        pub fn root_reaped(&self) {}
        pub fn sample(&self) -> Result<Vec<u64>, String> {
            Err("codec process ownership supports Windows/Linux only".into())
        }
        pub fn interrupt(&self) -> Result<(), String> {
            Err("console process ownership supports Windows/Linux only".into())
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
            "codec process ownership supports Windows/Linux only; this platform is unverified"
                .into(),
        )
    }
}

#[cfg(test)]
#[path = "supplemental_process_progress.rs"]
mod supplemental_progress_tests;

#[cfg(not(windows))]
impl platform::Tree {
    fn poll_sample(&self) -> Result<InventoryPoll<Vec<u64>>, String> {
        self.sample().map(InventoryPoll::Ready)
    }

    fn poll_terminate(&self) -> Result<InventoryPoll<()>, String> {
        self.terminate().map(InventoryPoll::Ready)
    }
}

#[cfg(test)]
#[path = "supplemental_process_inventory.rs"]
mod supplemental_inventory_tests;
