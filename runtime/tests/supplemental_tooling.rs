//! Supplemental native tooling properties, separate from the original1516-ID accounting.
#[path = "supplemental_tooling_support/build.rs"]
mod build;
#[path = "supplemental_tooling_support/commands.rs"]
mod commands;
#[path = "supplemental_tooling_support/compiler_storage.rs"]
mod compiler_storage;
#[path = "supplemental_tooling_support/convert_cli.rs"]
mod convert_cli;
#[path = "supplemental_tooling_support/launch.rs"]
mod launch;
#[path = "supplemental_tooling_support/menu.rs"]
mod menu;
#[path = "supplemental_tooling_support/policy.rs"]
mod policy;
#[path = "supplemental_native_support/process_liveness.rs"]
#[allow(unsafe_code)]
mod process_liveness;
#[path = "support/retained_compiler_directory.rs"]
mod retained_directory;
#[path = "supplemental_tooling_support/runner.rs"]
mod runner;
#[path = "supplemental_tooling_support/sandbox.rs"]
mod sandbox;
#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
