//! Incomplete Windows inventories remain pending only inside existing bounded callers.
use super::{CLEANUP, CleanupMonitor, InventoryPoll, WaitMonitor, WaitOutcome, poll_job_inventory};
use std::cell::Cell;
use std::time::{Duration, Instant};

#[test]
fn complete_inventory_can_arrive_after_multiple_strict_rounds_without_early_reaping() {
    let start = Instant::now();
    let mut monitor = WaitMonitor::new(Duration::from_secs(2), Duration::from_secs(1), start);
    let queries = Cell::new(0);
    let progress_calls = Cell::new(0);
    let reap_calls = Cell::new(0);
    for round in 0..3 {
        let inventory = poll_job_inventory(8192, || {
            queries.set(queries.get() + 1);
            Ok(if queries.get() <= 16 {
                (4, 3, vec![10, 20, 30])
            } else {
                (4, 4, vec![10, 20, 30, 40])
            })
        })
        .map(|inventory| match inventory {
            InventoryPoll::Ready((count, values)) => {
                assert_eq!(count, 4);
                InventoryPoll::Ready(values)
            }
            InventoryPoll::Pending(reason) => {
                assert!(reason.contains("assigned=4, listed=3, slots=8192, attempts=8"));
                InventoryPoll::Pending(reason)
            }
        });
        let now = start + Duration::from_millis(100 * round);
        let outcome = monitor
            .poll(
                inventory,
                || {
                    assert_eq!(round, 2, "pending inventory must not call progress");
                    progress_calls.set(progress_calls.get() + 1);
                    Ok(false)
                },
                || {
                    assert_eq!(round, 2, "pending inventory must not reap the root");
                    reap_calls.set(reap_calls.get() + 1);
                    Ok(None)
                },
                || now,
            )
            .unwrap();
        assert!(outcome.is_none());
        if round < 2 {
            assert_eq!(queries.get(), 8 * (round + 1));
            assert_eq!(monitor.advanced, start);
            assert!(monitor.previous.is_none());
        }
    }
    assert_eq!(queries.get(), 17);
    assert_eq!(progress_calls.get(), 1);
    assert_eq!(reap_calls.get(), 1);
    assert_eq!(monitor.previous, Some(vec![10, 20, 30, 40]));
}

#[test]
fn pending_and_late_complete_inventory_cannot_restart_wall_or_idle_budgets() {
    for wall_limited in [false, true] {
        for previously_complete in [false, true] {
            for late_complete in [false, true] {
                let start = Instant::now();
                let limit = Duration::from_millis(200);
                let unlimited = Duration::from_secs(u64::MAX);
                let (wall, idle) = if wall_limited {
                    (limit, unlimited)
                } else {
                    (unlimited, limit)
                };
                let mut monitor = WaitMonitor::new(wall, idle, start);
                if previously_complete {
                    assert!(
                        monitor
                            .poll(
                                Ok(InventoryPoll::Ready(vec![1])),
                                || Ok(false),
                                || Ok(None),
                                || start,
                            )
                            .unwrap()
                            .is_none()
                    );
                }
                for elapsed in [100, 199] {
                    let now = start + Duration::from_millis(elapsed);
                    assert!(monitor.pending_expired(now).is_none());
                    assert!(
                        monitor
                            .poll(
                                Ok(InventoryPoll::Pending("still incomplete".into())),
                                || panic!("pending inventory is not external progress"),
                                || panic!("pending inventory cannot authorize reaping"),
                                || now,
                            )
                            .unwrap()
                            .is_none()
                    );
                    assert_eq!(monitor.advanced, start);
                }
                let now = start + limit;
                assert!(monitor.pending_expired(now).is_some());
                let observation = if late_complete {
                    InventoryPoll::Ready(vec![2])
                } else {
                    InventoryPoll::Pending("still incomplete".into())
                };
                let outcome = monitor
                    .poll(
                        Ok(observation),
                        || panic!("late complete inventory must not restart an expired budget"),
                        || panic!("late complete inventory must not reap after an expired budget"),
                        || now,
                    )
                    .unwrap();
                assert!(match outcome {
                    Some(WaitOutcome::Wall(value)) => wall_limited && value == limit,
                    Some(WaitOutcome::Idle(value)) => !wall_limited && value == limit,
                    _ => false,
                });
                assert_eq!(monitor.advanced, start);
                assert_eq!(monitor.previous, previously_complete.then(|| vec![1]));
            }
        }
    }
}

#[test]
fn pending_cleanup_requires_a_complete_inventory_before_the_original_deadline() {
    let start = Instant::now();
    let mut recovered = CleanupMonitor::new(start);
    assert!(
        !recovered
            .poll(
                Ok(InventoryPoll::Pending("incomplete".into())),
                || panic!("pending cleanup must not reap or inspect quiescence"),
                || start,
            )
            .unwrap()
    );
    assert!(
        recovered
            .poll(
                Ok(InventoryPoll::Ready(())),
                || Ok(true),
                || start + Duration::from_millis(100),
            )
            .unwrap()
    );

    for late_complete in [false, true] {
        let mut expired = CleanupMonitor::new(start);
        assert_eq!(expired.deadline.duration_since(start), CLEANUP);
        assert!(
            !expired
                .poll(
                    Ok(InventoryPoll::Pending("incomplete".into())),
                    || panic!("pending cleanup must retain custody"),
                    || start + CLEANUP - Duration::from_millis(1),
                )
                .unwrap()
        );
        assert!(expired.check_pending_deadline(start + CLEANUP).is_err());
        let observation = if late_complete {
            InventoryPoll::Ready(())
        } else {
            InventoryPoll::Pending("incomplete".into())
        };
        let error = expired
            .poll(
                Ok(observation),
                || panic!("an expired pending cleanup must not authorize deletion"),
                || start + CLEANUP,
            )
            .unwrap_err();
        assert_eq!(
            error,
            "codec tree termination/reaping could not be verified within10s"
        );
    }
}

#[test]
fn unknown_and_over_bound_inventory_errors_are_immediate_even_after_pending() {
    for failure in ["query failed", "pinning failed"] {
        let start = Instant::now();
        let mut wait = WaitMonitor::new(Duration::from_secs(1), Duration::from_secs(1), start);
        wait.poll(
            Ok(InventoryPoll::Pending("incomplete".into())),
            || panic!("pending progress"),
            || panic!("pending reap"),
            || start,
        )
        .unwrap();
        assert_eq!(
            wait.poll(
                Err(failure.into()),
                || panic!("an inspection error cannot become progress"),
                || panic!("an inspection error cannot authorize reaping"),
                || panic!("an inspection error must not wait for a deadline"),
            )
            .err()
            .unwrap(),
            failure
        );
        let mut cleanup = CleanupMonitor::new(start);
        assert_eq!(
            cleanup
                .poll(
                    Err(failure.into()),
                    || panic!("an inspection error cannot authorize cleanup"),
                    || panic!("an inspection error must not wait for a deadline"),
                )
                .unwrap_err(),
            failure
        );
    }
    for (assigned, listed) in [(8193, 8192), (8192, 8193)] {
        let mut queries = 0;
        let error = poll_job_inventory(8192, || {
            queries += 1;
            Ok((assigned, listed, ()))
        })
        .unwrap_err();
        assert_eq!(queries, 1);
        assert!(error.contains("exceeds its bound"));
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::process_owner::{OwnedProcess, cleanup_pending};
    use std::os::windows::io::{AsHandle, AsRawHandle};
    use std::process::{Command, Stdio};
    use windows_sys::Win32::Foundation::{GetHandleInformation, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    #[test]
    fn actual_windows_child_completes_after_two_pending_inventory_rounds() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("owner");
        std::fs::create_dir(&workspace).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "process_owner::platform::tests::short_job_child_fixture",
                "--test-threads=1",
            ])
            .env_clear()
            .envs(crate::supplemental_worker_fixture::test_environment())
            .env("NUMERA_WINDOWS_SHORT_JOB_CHILD", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut owner = OwnedProcess::spawn(&mut command, workspace.clone()).unwrap();
        let inspection = owner
            .child
            .as_ref()
            .unwrap()
            .as_handle()
            .try_clone_to_owned()
            .unwrap();
        owner.tree.defer_member_queries(16);
        let outcome =
            runtime().block_on(owner.wait(Duration::from_secs(10), Duration::from_secs(5)));
        let remaining = owner.tree.pending_member_queries();
        owner.tree.defer_member_queries(0);
        let finish = owner.finish();
        drop(owner);
        finish.unwrap();
        assert!(outcome.unwrap().success());
        assert_eq!(remaining, 0, "both incomplete rounds must actually execute");
        assert!(!workspace.exists());
        // SAFETY: this independently owned process handle remains valid after owner cleanup.
        assert_eq!(
            unsafe { WaitForSingleObject(inspection.as_raw_handle(), 0) },
            WAIT_OBJECT_0
        );
    }

    #[test]
    fn actual_windows_pending_wait_retains_handles_and_strict_console_refusal() {
        let worker = crate::supplemental_worker_fixture::worker();
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("owner");
        let ready = directory.path().join("ready");
        std::fs::create_dir(&workspace).unwrap();
        let mut command = Command::new(worker);
        command
            .arg("--capture-hold")
            .arg(&ready)
            .current_dir(directory.path())
            .env_clear()
            .envs(crate::supplemental_worker_fixture::test_environment())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut owner = OwnedProcess::spawn(&mut command, workspace.clone()).unwrap();
        let inspection = owner
            .child
            .as_ref()
            .unwrap()
            .as_handle()
            .try_clone_to_owned()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready.is_file() {
            assert!(
                Instant::now() < deadline,
                "owned child did not reach readiness"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let (job, member) = owner.tree.owned_handles();
        owner.tree.defer_member_queries(8);
        let pending_cleanup = owner.tree.poll_terminate();
        owner.tree.defer_member_queries(usize::MAX);
        let limit = Duration::from_millis(200);
        let outcome = runtime().block_on(owner.wait_outcome(limit, Duration::from_secs(5)));
        let valid = |handle| {
            let mut flags = 0;
            // SAFETY: the owner remains alive; the API validates each retained handle value.
            unsafe { GetHandleInformation(handle, &mut flags) != 0 }
        };
        let retained =
            valid(job) && valid(member) && workspace.is_dir() && cleanup_pending(&workspace);
        // SAFETY: the independent owned process handle pins the original child identity.
        let before = unsafe { WaitForSingleObject(inspection.as_raw_handle(), 0) };
        owner.tree.defer_member_queries(8);
        let console = runtime().block_on(owner.wait_console());
        owner.tree.defer_member_queries(16);
        let finish = owner.finish();
        owner.tree.defer_member_queries(0);
        drop(owner);
        finish.unwrap();
        assert!(matches!(pending_cleanup, Ok(InventoryPoll::Pending(_))));
        assert!(matches!(outcome, Ok(WaitOutcome::Wall(value)) if value == limit));
        assert!(
            retained,
            "pending inventory must preserve Job/process handles and scratch"
        );
        assert_eq!(before, WAIT_TIMEOUT);
        assert!(console.unwrap_err().contains("attempts=8"));
        assert!(!workspace.exists());
        // SAFETY: this independent owned handle still identifies the terminated child.
        assert_eq!(
            unsafe { WaitForSingleObject(inspection.as_raw_handle(), 0) },
            WAIT_OBJECT_0
        );
    }
}
