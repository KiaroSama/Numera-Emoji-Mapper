//! One discoverable Rust case for each original recovery/locking baseline identity.
#![allow(dead_code)]
#[path = "baseline_catalog_support/mod.rs"]
mod baseline_catalog_support;
#[path = "baseline_publication_support/mod.rs"]
mod baseline_publication_support;
#[path = "baseline_recovery_support/locks_tests.rs"]
mod locks;
#[path = "baseline_recovery_support/order_tests.rs"]
mod order_tests;
#[path = "baseline_recovery_support/reconcile_tests.rs"]
mod reconcile_tests;
#[path = "baseline_recovery_support/mod.rs"]
mod support;

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
