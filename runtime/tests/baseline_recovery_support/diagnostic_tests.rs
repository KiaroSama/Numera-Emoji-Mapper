use super::Presence;
use std::io;

// Original: tests.test_pack_locks.PosixStaleLockReclaim.test_a_running_process_is_alive
#[test]
fn posixstalelockreclaim_test_a_running_process_is_alive() {
    assert_eq!(Presence::from_posix_probe(Ok(())), Presence::Present);
    assert_eq!(
        super::recorded_pid(i64::from(std::process::id())),
        Presence::Present
    );
}

// Original: tests.test_pack_locks.PosixStaleLockReclaim.test_no_such_process_is_reported_dead
#[test]
fn posixstalelockreclaim_test_no_such_process_is_reported_dead() {
    assert_eq!(
        Presence::from_posix_probe(Err(io::Error::from_raw_os_error(3))),
        Presence::Gone
    );
}

// Original: tests.test_pack_locks.PosixStaleLockReclaim.test_permission_denied_means_it_exists
#[test]
fn posixstalelockreclaim_test_permission_denied_means_it_exists() {
    assert_eq!(
        Presence::from_posix_probe(Err(io::ErrorKind::PermissionDenied.into())),
        Presence::Present
    );
}

// Original: tests.test_pack_locks.PosixStaleLockReclaim.test_unknown_failure_stays_conservative
#[test]
fn posixstalelockreclaim_test_unknown_failure_stays_conservative() {
    assert_eq!(
        Presence::from_posix_probe(Err(io::Error::other("weird"))),
        Presence::Unknown
    );
}
