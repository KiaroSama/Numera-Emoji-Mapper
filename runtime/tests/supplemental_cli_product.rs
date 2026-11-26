//! Supplemental native product properties, separate from the1516 original-ID mapping.
#[path = "supplemental_cli_product_support/archive_commands.rs"]
mod archive_commands;
#[path = "supplemental_cli_product_support/archive_freshness.rs"]
mod archive_freshness;
#[path = "supplemental_cli_product_support/bot_commands.rs"]
mod bot_commands;
#[path = "supplemental_cli_product_support/external_catalog.rs"]
mod external_catalog;
#[path = "supplemental_cli_product_support/family_merge.rs"]
mod family_merge;
#[path = "supplemental_cli_product_support/fetch_ids_commands.rs"]
mod fetch_ids_commands;
#[path = "supplemental_cli_product_support/gallery_receipts.rs"]
mod gallery_receipts;
#[allow(dead_code)]
#[path = "baseline_cli_status_support/http.rs"]
mod http;
#[path = "supplemental_cli_product_support/installed.rs"]
mod installed;
#[path = "supplemental_cli_product_support/operator_commands.rs"]
mod operator_commands;
#[path = "supplemental_cli_product_support/operator_edges.rs"]
mod operator_edges;
#[path = "supplemental_cli_product_support/payload_matrix.rs"]
mod payload_matrix;
#[path = "supplemental_cli_product_support/plan_structure.rs"]
mod plan_structure;
#[path = "support/read_only_api.rs"]
mod read_only_api;
#[path = "supplemental_cli_product_support/removal_settlement.rs"]
mod removal_settlement;
#[path = "support/tooling_fixture.rs"]
mod root;
#[path = "supplemental_cli_product_support/scripted.rs"]
mod scripted;
#[path = "supplemental_cli_product_support/status_commands.rs"]
mod status_commands;
#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
#[path = "supplemental_cli_product_support/unit_tails.rs"]
mod unit_tails;

// Same allowlist and synthetic defaults as the real supplemental worker fixture;
// this cohort needs its guarded child environment, without a fixture compiler.
fn test_environment() -> std::collections::BTreeMap<String, String> {
    let mut environment = std::collections::BTreeMap::new();
    for name in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "PATHEXT",
        "LANG",
        "LC_ALL",
        "LD_LIBRARY_PATH",
        "NUMERA_RUNTIME_TEST_ADMISSION",
    ] {
        if let Ok(value) = std::env::var(name) {
            environment.insert(name.to_owned(), value);
        }
    }
    environment.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
    environment.insert("TELEGRAM_API_BASE".into(), "http://127.0.0.1:9".into());
    environment
}
