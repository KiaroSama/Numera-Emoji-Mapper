//! Historical injected atomic-I/O contracts exercise the crate-private production seam.
use crate::baseline_catalog_support as support;
use serde_json::json;
use std::collections::BTreeSet;
use std::sync::{Condvar, Mutex};
use std::time::Duration;
#[cfg(windows)]
use std::time::Instant;
use std::{fs::File, io};
use support::*;

// tests.test_atomic_state_files.AtomicStateFiles.test_fsync_failure_retains_old_state_and_removes_only_owned_temp
#[test]
fn atomic_fsync_failure_retains_old_state_and_removes_only_owned_temp() {
    let (root, path) = atomic_fixture();
    let error = crate::atomic::write_bytes_with(
        &path,
        br#"{"new":true}"#,
        |_| Err(io::Error::other("disk failure")),
        |_, _| panic!("replace after failed fsync"),
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "disk failure");
    assert_eq!(read_json(&path), json!({"old":true}));
    atomic_members(&root);
}
// tests.test_atomic_state_files.AtomicStateFiles.test_replace_failure_retains_old_state_and_removes_only_owned_temp
#[test]
fn atomic_replace_failure_retains_old_state_and_removes_only_owned_temp() {
    let (root, path) = atomic_fixture();
    let error =
        crate::atomic::write_bytes_with(&path, br#"{"new":true}"#, File::sync_all, |_, _| {
            Err(io::Error::other("destination busy"))
        })
        .unwrap_err();
    assert_eq!(error.to_string(), "destination busy");
    assert_eq!(read_json(&path), json!({"old":true}));
    atomic_members(&root);
}
// tests.test_atomic_state_files.AtomicStateFiles.test_a_transient_windows_refusal_is_retried_to_completion
#[cfg_attr(not(windows), ignore = "Windows' transient replace refusal")]
#[test]
fn atomic_a_transient_windows_refusal_is_retried_to_completion() {
    #[cfg(windows)]
    {
        let (root, path) = atomic_fixture();
        let mut calls = 0;
        crate::atomic::write_bytes_with(&path, br#"{"new":true}"#, File::sync_all, |src, dst| {
            calls += 1;
            if calls <= 2 {
                Err(io::Error::from_raw_os_error(5))
            } else {
                std::fs::rename(src, dst)
            }
        })
        .unwrap();
        assert_eq!(calls, 3);
        assert_eq!(read_json(&path), json!({"new":true}));
        atomic_members(&root);
    }
}
// tests.test_atomic_state_files.AtomicStateFiles.test_a_persistent_refusal_still_fails_and_cleans_up
#[cfg_attr(not(windows), ignore = "Windows' transient replace refusal")]
#[test]
fn atomic_a_persistent_refusal_still_fails_and_cleans_up() {
    #[cfg(windows)]
    {
        let (root, path) = atomic_fixture();
        let started = Instant::now();
        let error =
            crate::atomic::write_bytes_with(&path, br#"{"new":true}"#, File::sync_all, |_, _| {
                Err(io::Error::from_raw_os_error(5))
            })
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(read_json(&path), json!({"old":true}));
        atomic_members(&root);
    }
}
// tests.test_atomic_state_files.AtomicStateFiles.test_overlapping_writes_use_independent_temps_and_publish_complete_json
#[test]
fn atomic_overlapping_writes_use_independent_temps_and_publish_complete_json() {
    let (root, path) = atomic_fixture();
    let barrier = (Mutex::new(0), Condvar::new());
    let sources = Mutex::new(BTreeSet::new());
    let bodies = [
        json!({"writer":1,"value":"A".repeat(10_000)}),
        json!({"writer":2,"value":"B".repeat(1000)}),
    ];
    std::thread::scope(|scope| {
        let mut writers = Vec::new();
        for body in &bodies {
            let bytes = serde_json::to_vec(body).unwrap();
            let path = &path;
            let barrier = &barrier;
            let sources = &sources;
            writers.push(scope.spawn(move || {
                crate::atomic::write_bytes_with(
                    path,
                    &bytes,
                    File::sync_all,
                    |source, destination| {
                        let first = sources.lock().unwrap().insert(source.to_owned());
                        if first {
                            let mut count = barrier.0.lock().unwrap();
                            *count += 1;
                            barrier.1.notify_all();
                            let (guard, timeout) = barrier
                                .1
                                .wait_timeout_while(count, Duration::from_secs(5), |count| {
                                    *count < 2
                                })
                                .unwrap();
                            drop(guard);
                            if timeout.timed_out() {
                                return Err(io::Error::new(
                                    io::ErrorKind::TimedOut,
                                    "second writer did not reach replacement",
                                ));
                            }
                        }
                        std::fs::rename(source, destination)
                    },
                )
            }));
        }
        for writer in writers {
            writer.join().unwrap().unwrap();
        }
    });
    assert_eq!(sources.lock().unwrap().len(), 2);
    assert!(bodies.contains(&read_json(&path)));
    atomic_members(&root);
}
