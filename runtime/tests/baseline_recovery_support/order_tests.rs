use crate::support::order;

// Original: tests.test_lock_order.TheCheckerActuallySeesAViolation.test_it_flags_a_reentrant_nest
#[test]
fn thecheckeractuallyseesaviolation_test_it_flags_a_reentrant_nest() {
    let f = order::functions(
        "fn f(){let _map=canonical_map_lock();{let _again=canonical_map_lock();}}",
    );
    assert!(f[0].pairs.contains(&(order::MAP.into(), order::MAP.into())));
}

// Original: tests.test_lock_order.TheCheckerActuallySeesAViolation.test_it_flags_an_inverted_order
#[test]
fn thecheckeractuallyseesaviolation_test_it_flags_an_inverted_order() {
    let f =
        order::functions("fn f(){let _map=canonical_map_lock();{let _pack=exclusive_lock(P);}}");
    assert!(
        f[0].pairs
            .contains(&(order::MAP.into(), order::PACK.into()))
    );
}

// Original: tests.test_lock_order.TheCheckerActuallySeesAViolation.test_it_reads_one_with_statement_as_nesting
#[test]
fn thecheckeractuallyseesaviolation_test_it_reads_one_with_statement_as_nesting() {
    let f = order::functions("fn f(){let _pack=exclusive_lock(P);let _map=canonical_map_lock();}");
    assert!(
        f[0].pairs
            .contains(&(order::PACK.into(), order::MAP.into()))
    );
}

// Original: tests.test_lock_order.TheCheckerActuallySeesAViolation.test_it_scans_a_real_and_non_empty_set_of_tools
#[test]
fn thecheckeractuallyseesaviolation_test_it_scans_a_real_and_non_empty_set_of_tools() {
    let sources = order::sources();
    for name in [
        "ticker_build",
        "coin_provider_recover",
        "coin_verify_cli",
        "coin_rebuild_cli",
        "coin_remap",
    ] {
        assert!(sources.contains_key(name), "missing mutator {name}");
        assert!(!order::functions(&sources[name]).is_empty());
    }
    assert!(sources.len() > 30);
}

// Original: tests.test_lock_order.TheCheckerActuallySeesAViolation.test_the_documented_order_is_not_flagged
#[test]
fn thecheckeractuallyseesaviolation_test_the_documented_order_is_not_flagged() {
    let f =
        order::functions("fn f(){let _pack=exclusive_lock(P);{let _map=canonical_map_lock();}}");
    assert_eq!(f[0].pairs, vec![(order::PACK.into(), order::MAP.into())]);
}

// Original: tests.test_lock_order.TheDocumentedLockOrderHolds.test_a_lock_holder_never_calls_a_helper_that_takes_the_same_lock
#[test]
fn thedocumentedlockorderholds_test_a_lock_holder_never_calls_a_helper_that_takes_the_same_lock() {
    let mut count = 0;
    for (name, source) in order::sources() {
        count += order::functions(&source).len();
        let bad = order::helper_clashes(&source);
        assert!(bad.is_empty(), "{name}: {bad:?}");
    }
    assert!(count > 20);
    let bad = order::helper_clashes(
        "fn caller(){let _map=canonical_map_lock();repoint();} fn repoint(){let _map=canonical_map_lock();}",
    );
    assert_eq!(
        bad.len(),
        1,
        "checker did not observe same-lock helper acquisition"
    );
    assert!(bad[0].contains("caller") && bad[0].contains("repoint"));
    assert!(order::helper_clashes("fn caller(){let _pack=exclusive_lock(P);repoint();} fn repoint(){let _map=canonical_map_lock();}").is_empty());
}

// Original: tests.test_lock_order.TheDocumentedLockOrderHolds.test_no_function_nests_a_lock_it_already_holds
#[test]
fn thedocumentedlockorderholds_test_no_function_nests_a_lock_it_already_holds() {
    let mut acquired = 0;
    for (name, source) in order::sources() {
        for f in order::functions(&source) {
            acquired += f.acquired.len();
            assert!(
                !f.pairs.iter().any(|(a, b)| a == b),
                "{name}:{} {:?}",
                f.name,
                f.pairs
            );
        }
    }
    assert!(acquired > 5);
}

// Original: tests.test_lock_order.TheDocumentedLockOrderHolds.test_the_map_lock_is_never_taken_before_a_pack_lock
#[test]
fn thedocumentedlockorderholds_test_the_map_lock_is_never_taken_before_a_pack_lock() {
    let mut seen = 0;
    for (name, source) in order::sources() {
        for f in order::functions(&source) {
            seen += f.pairs.len();
            assert!(
                !f.pairs
                    .iter()
                    .any(|(a, b)| a == order::MAP && b == order::PACK),
                "{name}:{} {:?}",
                f.name,
                f.pairs
            );
        }
    }
    assert!(seen > 0, "checker saw no production lock pair");
}
