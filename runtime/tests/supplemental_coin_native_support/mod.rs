//! Unwired retained supplemental scenarios, separate from the1516 original identities.
#[path = "../supplemental_native_support/bot_api.rs"]
pub mod bot_api;
mod coin_audit;
mod coin_audit_cli;
mod coin_map_cli;
mod coin_provider_cli;
mod coin_provider_restart;
mod coin_provider_wire;
mod coin_rebuild_add_cli;
mod coin_rebuild_effects;
mod coin_rebuild_links_cli;
mod coin_rebuild_map_cli;
mod coin_rebuild_recovery_disk;
mod coin_replace_effects;
mod coin_verify_cli;
pub mod frozen_audit;
pub mod frozen_static;
#[path = "../support/read_only_api.rs"]
pub mod read_only_api;
#[path = "../support/tooling_fixture.rs"]
pub mod tooling_fixture;

/// Keep genuine outer admission and OS prerequisites; operator configuration never enters fixtures.
pub fn guarded_environment(
    original: &std::collections::BTreeMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    let scrubbed = numera_runtime::tooling_launch::scrubbed_environment(original, "");
    let mut child = std::collections::BTreeMap::new();
    for key in [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "PATHEXT",
        "LD_LIBRARY_PATH",
        "LANG",
        "LC_ALL",
        "NUMERA_RUNTIME_TEST_ADMISSION",
    ] {
        let value = scrubbed.get(key).or_else(|| {
            cfg!(windows)
                .then(|| {
                    scrubbed
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case(key))
                        .map(|(_, value)| value)
                })
                .flatten()
        });
        if let Some(value) = value {
            child.insert(key.to_owned(), value.clone());
        }
    }
    for (key, value) in [
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1"),
        ("TELEGRAM_API_BASE", "http://127.0.0.1:9"),
        ("BRAND_LOGO_BOTS", ""),
        ("COLLECTION_PACK_BASE", "YourBrand_Emoji_Packs"),
        ("COIN_PACK_BASE", "cryptoemoji"),
        ("COIN_PACK_TITLE", "@YourBrand Crypto Emoji"),
        ("RUST_BACKTRACE", "1"),
    ] {
        child.insert(key.into(), value.into());
    }
    child
}
