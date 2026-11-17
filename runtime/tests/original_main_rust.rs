//! Original Rust main tests; these identities are separate from the1516 Python cases.
extern crate numera_runtime as _native;

#[test]
fn stable_ties_and_empty_input() {
    // Original native/src/lib.rs::tests::stable_ties_and_empty_input @9c3cbb43305b.
    use _native::similarity::greedy;
    assert!(greedy(&[]).is_empty());
    assert_eq!(greedy(&[0, 1, 2, 3]), [0, 1, 3, 2]);
    assert_eq!(greedy(&[u64::MAX, u64::MAX, 0]), [0, 1, 2]);
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
