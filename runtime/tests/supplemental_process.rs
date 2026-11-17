// Retained native process/capture regressions; technical child drivers remain explicit.
extern crate numera_runtime as _native;
#[path = "supplemental_process_support/capture_contracts.rs"]
mod capture_contracts;
#[path = "supplemental_process_support/tooling_test_cohort.rs"]
mod cohort;
#[path = "supplemental_native_support/worker_fixture.rs"]
pub mod fixture;
#[path = "supplemental_process_support/process_session.rs"]
mod session;
#[path = "support/test_admission.rs"]
mod test_admission;
#[path = "supplemental_process_support/tooling_process.rs"]
mod tooling;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
