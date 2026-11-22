//! Standalone application library. Its dependency graph contains no interpreter or Python ABI.
#![deny(unsafe_code)]

// The same fixture source is used by integration crates and by this crate's unit tests.
#[cfg(test)]
extern crate self as numera_runtime;

#[cfg(test)]
#[path = "../tests/supplemental_native_support/worker_fixture.rs"]
pub(crate) mod supplemental_worker_fixture;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/baseline_catalog_support/mod.rs"]
pub(crate) mod baseline_catalog_support;
#[cfg(test)]
#[path = "../tests/baseline_coin_providers_support/mod.rs"]
pub(crate) mod baseline_coin_provider_support;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/baseline_identity_migration_support/mod.rs"]
pub(crate) mod baseline_identity_support;
#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/baseline_publication_support/mod.rs"]
pub(crate) mod baseline_publication_support;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/baseline_cli_status_support/http.rs"]
pub(crate) mod baseline_cli_http;

pub mod developer;
pub mod tooling_sandbox;

#[cfg(not(all(
    any(target_arch = "x86_64", target_arch = "aarch64"),
    any(target_os = "linux", target_os = "windows")
)))]
compile_error!("Numera supports 64-bit Linux and Windows (x86_64, aarch64) only");

#[cfg(test)]
mod baseline_atomic_tests;

pub mod announce;
pub mod archive_catalog;
pub mod archive_check;
pub mod archive_cli;
pub mod archive_move;
pub mod archive_rows;
pub mod archive_sync;
pub mod asset_version;
pub mod atomic;
pub mod bot_payloads;
pub mod brand_logo;
pub mod browser_open;
pub mod catalog;
pub mod catalog_read;
pub mod codec_compare;
pub mod codec_error;
pub mod codec_format;
pub mod codec_image;
pub mod codec_lottie_identity;
pub mod codec_lottie_package;
pub mod codec_lottie_prepare;
pub mod codec_lottie_preview;
pub mod codec_lottie_validate;
pub mod codec_process;
pub mod codec_request;
pub mod codec_tint;
pub mod codec_video;
pub mod codec_video_cache;
pub mod codec_video_encode;
pub mod codec_video_error;
pub mod codec_video_preview;
pub mod codec_video_probe;
pub mod codec_video_remux;
pub mod codec_video_timeline;
pub mod codec_video_validate;
pub mod codec_worker;
pub mod coin_aliases;
pub mod coin_audit;
pub mod coin_audit_cli;
pub mod coin_cmc;
pub mod coin_gecko;
pub mod coin_gecko_cli;
pub mod coin_gecko_fetch;
pub mod coin_http;
pub mod coin_inventory;
pub mod coin_inventory_parse;
pub mod coin_keywords;
pub mod coin_keywords_cli;
pub mod coin_logo_review;
pub mod coin_manifest;
pub mod coin_manifest_cli;
pub mod coin_map_cli;
pub mod coin_map_identity;
pub mod coin_paprika;
pub mod coin_plan;
pub mod coin_plan_disk;
pub mod coin_provider_cli;
pub mod coin_provider_effects;
pub mod coin_provider_policy;
pub mod coin_provider_proof;
pub mod coin_provider_publish;
pub mod coin_provider_recover;
pub mod coin_provider_resolve;
pub mod coin_provider_stage;
pub mod coin_rebuild_cli;
pub mod coin_rebuild_delete;
pub mod coin_rebuild_effects;
pub mod coin_rebuild_links;
pub mod coin_rebuild_map;
pub mod coin_rebuild_map_cli;
pub mod coin_rebuild_map_disk;
pub mod coin_rebuild_recovery;
pub mod coin_rebuild_recovery_disk;
pub mod coin_rebuild_walk;
pub mod coin_remap;
pub mod coin_remap_cache;
pub mod coin_remap_cli;
pub mod coin_remap_math;
pub mod coin_remap_policy;
pub mod coin_replace_disk;
pub mod coin_replace_effects;
pub mod coin_replace_proof;
pub mod coin_replace_publish;
pub mod coin_replace_recovery;
pub mod coin_state;
pub mod coin_verify_cli;
pub mod coin_workflows;
pub mod collection_manifest;
pub mod collection_names;
pub mod collection_notify;
pub mod collector;
pub mod config;
pub mod config_integer;
pub mod convert_cli;
pub mod emoji_bot;
pub mod emoji_ids;
pub mod fetch_cli;
pub mod fetch_ids_cli;
pub mod identity_cli;
pub mod identity_invariants;
pub mod identity_references;
pub mod identity_survey;
pub mod ingest;
pub mod intent;
pub mod local_cli;
pub mod locks;
pub mod logging;
pub mod media_adapter;
pub mod media_native_adapter;
pub mod media_store;
pub mod migration_apply;
pub mod migration_bundle;
pub mod migration_database;
pub mod migration_files;
pub mod migration_provenance;
pub mod migration_restore;
pub mod migration_signature;
#[allow(unsafe_code)]
pub mod native_lottie;
pub mod native_raster;
pub mod native_svg;
pub mod ownership;
pub mod pack_export;
pub mod pack_gallery;
pub mod pack_rows;
pub mod package;
pub mod panel_events;
pub mod panel_http;
pub mod panel_preview;
pub mod panel_save;
pub mod panel_session;
pub mod panel_view;
pub mod paths;
pub mod plan_apply;
pub mod plan_remove;
pub mod plan_status;
pub mod plan_steps;
pub mod process_owner;
pub mod publication;
pub mod publication_download;
pub mod publication_media;
pub mod publication_recovery;
pub mod publish_cli;
pub mod publish_collection;
pub mod python_repr;
pub mod reconcile;
pub mod repaint_gate;
pub mod resume;
pub mod roster;
pub mod roster_cli;
pub mod roster_index;
pub mod roster_markdown;
pub mod roster_provenance;
pub mod sandbox_lease;
pub mod similarity;
pub mod sqlite_snapshot;
pub mod state_artifacts;
pub mod status;
pub mod sync_order;
pub mod telegram;
pub mod telegram_mutation;
pub mod ticker_build;
pub mod ticker_keywords;
pub mod ticker_recovery;
pub mod ticker_state;
pub mod tooling_convert;
pub mod tooling_convert_cli;
pub mod tooling_doctor;
pub mod tooling_environment;
pub mod tooling_interrupt;
pub mod tooling_launch;
pub mod tooling_menu;
pub mod tooling_menu_actions;
pub mod tooling_menu_cli;
pub mod tooling_process;
pub mod upload_effect;

mod entry;
pub use entry::run as entry;

#[cfg(test)]
#[path = "baseline_migration_tests.rs"]
mod migration_crash_tests;

// Private deterministic observations for the baseline plan-write HTTP tests.
#[cfg(test)]
#[path = "../tests/baseline_panel_support/plan_write_tests.rs"]
mod panel_save_tests;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../tests/baseline_recovery_support/mod.rs"]
pub(crate) mod baseline_recovery_support;

#[cfg(test)]
#[path = "../tests/baseline_packs_support/unit_support.rs"]
pub(crate) mod baseline_packs_support;

#[cfg(test)]
#[path = "../tests/baseline_coin_state_support/mod.rs"]
pub(crate) mod baseline_coin_state_support;

mod lock_diagnostics;

#[cfg(test)]
#[path = "../tests/support/test_admission.rs"]
mod test_admission;
pub mod worker_admin;

#[cfg(test)]
#[path = "supplemental_atomic_tests.rs"]
mod supplemental_atomic_tests;

#[cfg(test)]
#[path = "../tests/supplemental_coin_native_support/map_private.rs"]
mod supplemental_coin_map_tests;

#[cfg(test)]
#[path = "../tests/supplemental_coin_native_support/telegram_private.rs"]
mod supplemental_telegram_tests;
