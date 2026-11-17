#![allow(dead_code)]
#[path = "baseline_cli_status_support/audit.rs"]
mod audit;
#[path = "baseline_catalog_support/mod.rs"]
mod baseline_catalog_support;
#[path = "baseline_cli_status_support/collection.rs"]
mod collection;
#[path = "baseline_cli_status_support/consumers.rs"]
mod consumers;
#[path = "baseline_cli_status_support/contracts.rs"]
mod contracts;
#[path = "baseline_cli_status_support/conversion.rs"]
mod conversion;
#[path = "baseline_cli_status_support/exports.rs"]
mod exports;
#[path = "baseline_cli_status_support/identity.rs"]
mod identity;
#[path = "baseline_cli_status_support/migration_commands.rs"]
mod migration_commands;
#[path = "baseline_cli_status_support/oracles.rs"]
mod oracles;
#[path = "baseline_cli_status_support/plan_commands.rs"]
mod plan_commands;
#[path = "baseline_cli_status_support/polling.rs"]
mod polling;
#[path = "baseline_cli_status_support/publish_commands.rs"]
mod publish_commands;
#[path = "baseline_cli_status_support/reports.rs"]
mod reports;
#[path = "baseline_cli_status_support/roster_command.rs"]
mod roster_command;
#[path = "baseline_cli_status_support/state_package.rs"]
mod state_package;
#[path = "baseline_cli_status_support/mod.rs"]
mod support;
#[path = "baseline_cli_status_support/telegram_contracts.rs"]
mod telegram_contracts;
#[path = "baseline_cli_status_support/video_collection.rs"]
mod video_collection;

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
