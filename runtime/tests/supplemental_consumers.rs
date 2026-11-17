// Retained native codec consumer compositions, using frozen media and owned loopback APIs.
extern crate numera_runtime as _native;
#[path = "supplemental_consumers_support/support/bounded_media.rs"]
mod bounded_media;
#[path = "supplemental_native_support/worker_fixture.rs"]
pub mod fixture;
#[path = "supplemental_consumers_support/media_native_collect.rs"]
mod native_collect;
#[path = "supplemental_consumers_support/media_native_identity.rs"]
mod native_identity;
#[path = "supplemental_consumers_support/media_native_ids.rs"]
mod native_ids;
#[path = "support/read_only_api.rs"]
mod read_only_api;
#[path = "supplemental_consumers_support/support/retained_cli.rs"]
mod retained_cli;
#[path = "support/test_admission.rs"]
mod test_admission;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
