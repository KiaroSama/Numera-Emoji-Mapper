use super::Download;
use crate::baseline_recovery_support::Scratch;
// Original: tests.test_reconcile_identity.ScratchFilesArePrivateAndAlwaysRemoved.test_two_calls_never_share_a_path
#[test]
fn scratch_two_calls_never_share_a_path() {
    let s = Scratch::new();
    let a = Download::new(&s.0, "verify", b"").unwrap();
    let b = Download::new(&s.0, "verify", b"").unwrap();
    assert_ne!(a.path, b.path);
    assert!(a.path.is_file() && b.path.is_file());
}
// Original: tests.test_reconcile_identity.ScratchFilesArePrivateAndAlwaysRemoved.test_the_file_exists_before_the_caller_writes_to_it
#[test]
fn scratch_the_file_exists_before_the_caller_writes_to_it() {
    let s = Scratch::new();
    let p = Download::new(&s.0, "verify", b"").unwrap();
    assert!(p.path.is_file());
    assert_eq!(std::fs::read(&p.path).unwrap(), b"");
}
// Original: tests.test_reconcile_identity.ScratchFilesArePrivateAndAlwaysRemoved.test_it_is_removed_when_the_block_raises
#[test]
fn scratch_it_is_removed_when_the_block_raises() {
    let s = Scratch::new();
    let mut leaked = None;
    let raised = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let owned = Download::new(&s.0, "reconcile", b"").unwrap();
        leaked = Some(owned.path.clone());
        panic!("ambiguous, or any other refusal");
    }));
    assert!(raised.is_err());
    assert!(!leaked.unwrap().exists());
    assert!(std::fs::read_dir(s.0.join("tmp")).unwrap().next().is_none());
}
// Original: tests.test_reconcile_identity.ScratchFilesArePrivateAndAlwaysRemoved.test_nothing_is_left_behind_after_a_normal_call
#[test]
fn scratch_nothing_is_left_behind_after_a_normal_call() {
    let s = Scratch::new();
    let p = Download::new(&s.0, "verify", b"").unwrap();
    let path = p.path.clone();
    std::fs::write(&path, b"downloaded").unwrap();
    p.finish(Ok(())).unwrap();
    assert!(!path.exists());
    assert!(std::fs::read_dir(s.0.join("tmp")).unwrap().next().is_none());
}
