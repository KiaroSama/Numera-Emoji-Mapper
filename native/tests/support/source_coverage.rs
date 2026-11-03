//! Authored source-case cohorts and inspected target files; no runtime or parity claim.
pub const TARGETS: &[&str] = &[
    "native/tests/catalog_integrity.rs",
    "native/tests/ci_coverage.rs",
    "native/tests/source_lock_order.rs",
    "native/tests/catalog_paths.rs",
    "native/tests/sqlite_snapshot.rs",
    "native/tests/state_artifacts.rs",
    "native/src/migration_crash_tests.rs",
    "native/tests/coin_audit.rs",
    "native/tests/coin_audit_cli.rs",
    "native/tests/coin_keywords.rs",
    "native/tests/coin_provider_cli.rs",
    "native/tests/coin_http.rs",
    "native/tests/coin_provider_wire.rs",
    "native/tests/coin_gecko_fetch.rs",
    "native/tests/coin_provider_image.rs",
    "native/tests/coin_provider_effects.rs",
    "native/tests/coin_provider_restart.rs",
    "native/tests/coin_state.rs",
    "native/tests/coin_provider_cache.rs",
    "native/tests/coin_inventory.rs",
    "native/tests/coin_map_cli.rs",
    "native/src/coin_map_cli_tests.rs",
    "native/tests/coin_remap_cli.rs",
    "native/tests/coin_remap_cache.rs",
    "native/tests/coin_remap_math.rs",
    "native/tests/coin_remap.rs",
    "native/tests/coin_verify_cli.rs",
    "native/tests/coin_replace_effects.rs",
    "native/tests/coin_replace_proof.rs",
    "native/tests/coin_logo_distance.rs",
    "native/tests/coin_rebuild_effects.rs",
    "native/tests/coin_rebuild_add_cli.rs",
    "native/tests/coin_plan_disk.rs",
    "native/tests/coin_plan.rs",
    "native/tests/coin_rebuild_map_cli.rs",
    "native/tests/coin_rebuild_recovery_disk.rs",
    "native/src/catalog.rs",
    "native/src/collector.rs",
    "native/src/atomic_tests.rs",
    "native/src/binding_similarity_tests.rs",
    "native/src/binding_domain_tests.rs",
    "native/src/binding_ingest_tests.rs",
    "native/src/binding_intent_tests.rs",
    "native/src/tooling_convert_tests.rs",
    "native/src/publish_cli.rs",
    "native/src/config.rs",
    "native/src/lock_diagnostics.rs",
    "native/src/telegram_tests.rs",
    "native/tests/telegram_transport.rs",
    "native/tests/pack_locks.rs",
    "native/tests/atomic_failure.rs",
    "native/tests/state_compat_parity.rs",
    "native/tests/intent_parity.rs",
    "native/tests/bot_roster_parity.rs",
    "native/tests/bot_commands.rs",
    "native/tests/reconcile_commit.rs",
    "native/tests/reconcile_identity.rs",
    "native/tests/publication_scratch.rs",
    "native/tests/operator_config.rs",
    "native/tests/sync_order.rs",
    "native/tests/archive_state.rs",
    "native/tests/archive_commands.rs",
    "native/tests/plan_removal.rs",
    "native/tests/plan_steps_parity.rs",
    "native/tests/pack_export.rs",
    "native/tests/status.rs",
    "native/tests/roster_state.rs",
    "native/tests/roster_commands.rs",
    "native/tests/pack_reports.rs",
    "native/tests/logging_parity.rs",
    "native/tests/ingest_parity.rs",
    "native/tests/codec_static.rs",
    "native/tests/codec_lottie_file.rs",
    "native/tests/codec_reencode.rs",
    "native/tests/codec_video_operations.rs",
    "native/tests/codec_compare.rs",
    "native/tests/codec_lottie_prepare.rs",
    "native/tests/codec_tint.rs",
    "native/tests/repaint_gate.rs",
    "native/tests/media_native_consumers.rs",
    "native/tests/media_native_identity.rs",
    "native/tests/media_native_convert.rs",
    "native/tests/media_native_collect.rs",
    "native/tests/media_native_ids.rs",
    "native/tests/fetch_ids_cli.rs",
    "native/tests/local_format.rs",
    "native/tests/emoji_ids.rs",
    "native/src/codec_image.rs",
    "native/tests/codec_process_policy.rs",
    "native/tests/codec_video.rs",
    "native/tests/codec_video_fidelity.rs",
    "native/tests/announce_binding.rs",
    "native/tests/source_git_inventory.rs",
    "native/tests/tooling_menu.rs",
    "native/tests/tooling_test_runner.rs",
    "native/tests/tooling_install.rs",
    "native/tests/tooling_build.rs",
    "native/tests/tooling_policy.rs",
    "native/tests/tooling_runtime.rs",
    "native/tests/tooling_process.rs",
    "native/tests/tooling_launch.rs",
    "native/tests/tooling_sandbox.rs",
    "native/src/tooling_sandbox_tests.rs",
    "native/tests/tooling_sandbox_serve.rs",
    "native/tests/panel_browser.rs",
    "native/tests/panel_branding.rs",
    "native/tests/preview_warm.rs",
    "native/tests/media_native_lifecycle.rs",
    "native/tests/reconcile_view_parity.rs",
    "native/src/panel_save_tests.rs",
    "native/tests/wheel_package.rs",
];

pub const SOURCES: &[(&str, usize)] = &[
    ("tests/test_catalog.py", 22),
    ("tests/test_ci_coverage.py", 9),
    ("tests/test_suite_guard.py", 2),
    ("tests/test_fake_contracts.py", 3),
    ("tests/test_contracts_shared.py", 2),
    ("tests/test_lock_order.py", 8),
    ("tests/test_catalog_order.py", 4),
    ("tests/test_native_similarity.py", 8),
    ("tests/test_native_bot.py", 1),
    ("tests/test_native_catalog.py", 1),
    ("tests/test_native_real_catalog.py", 1),
    ("tests/test_native_ingest.py", 1),
    ("tests/test_native_reconcile.py", 1),
    ("tests/test_native_panel.py", 3),
    ("tests/test_native_plan_steps.py", 4),
    ("tests/test_native_state.py", 4),
    ("tests/test_native_ticker_cli.py", 5),
    ("tests/test_native_consumer_boundary.py", 5),
    ("tests/test_entry_point_contracts.py", 14),
    ("tests/test_sqlite_snapshot.py", 4),
    ("tests/test_state_artifacts.py", 5),
    ("tests/test_check_all_packs.py", 7),
    ("tests/test_coin_keywords.py", 3),
    ("tests/test_coin_logo_cache.py", 5),
    ("tests/test_coin_cli_args.py", 4),
    ("tests/test_coin_http.py", 14),
    ("tests/test_coin_providers.py", 37),
    ("tests/test_coin_recovery.py", 10),
    ("tests/test_coin_ticker_map.py", 14),
    ("tests/test_remap_ids.py", 22),
    ("tests/test_verify_logos.py", 29),
    ("tests/test_rebuild_dedup_locks.py", 4),
    ("tests/test_rebuild_dedup_state.py", 8),
    ("tests/test_rebuild_dedup_plan.py", 32),
    ("tests/test_rebuild_dedup_resume.py", 16),
    ("tests/test_rebuild_dedup_map.py", 17),
    ("tests/test_media_paths.py", 9),
    ("tests/test_pack_locks.py", 23),
    ("tests/test_pack_locks_exclusion.py", 10),
    ("tests/test_native_state_compat.py", 1),
    ("tests/test_native_storage_recovery.py", 2),
    ("tests/test_native_preview_codec.py", 1),
    ("tests/test_native_identity_cli.py", 1),
    ("tests/test_native_migration_cli.py", 3),
    ("tests/test_native_roster.py", 2),
    ("tests/test_atomic_state_files.py", 8),
    ("tests/test_logsetup.py", 17),
    ("tests/test_cli_env.py", 1),
    ("tests/test_cli_docs.py", 2),
    ("tests/test_resume_safety.py", 38),
    ("tests/test_native_local_cli.py", 2),
    ("tests/test_native_command_help.py", 2),
    ("tests/test_native_publish_cli.py", 3),
    ("tests/test_native_publication.py", 6),
    ("tests/test_native_video_visibility.py", 1),
    ("tests/test_preflight_outcomes.py", 11),
    ("tests/test_publish_cli.py", 38),
    ("tests/test_publish_dedup.py", 10),
    ("tests/test_publish_contracts.py", 29),
    ("tests/test_publish_invariants.py", 6),
    ("tests/test_build_collection_state.py", 50),
    ("tests/test_reconcile_identity.py", 17),
    ("tests/test_reconcile_uniqueness.py", 11),
    ("tests/test_emoji_bot.py", 44),
    ("tests/test_native_bot_poll.py", 3),
    ("tests/test_operator_config.py", 8),
    ("tests/test_sync_order.py", 13),
    ("tests/test_pack_archive.py", 14),
    ("tests/test_pack_export.py", 4),
    ("tests/test_status.py", 4),
    ("tests/test_pack_manifest.py", 24),
    ("tests/test_pack_rows.py", 7),
    ("tests/test_native_roster_cli.py", 1),
    ("tests/test_native_status_cli.py", 1),
    ("tests/test_native_archive_cli.py", 1),
    ("tests/test_native_export_cli.py", 1),
    ("tests/test_native_remove_cli.py", 4),
    ("tests/test_plan_status.py", 5),
    ("tests/test_native_plan_status_cli.py", 1),
    ("tests/test_native_plan_cli.py", 1),
    ("tests/test_plan_apply.py", 20),
    ("tests/test_telegram_client.py", 35),
    ("tests/test_telegram_body_shape.py", 2),
    ("tests/test_unresolved_mutation.py", 17),
    ("tests/test_build_pack_announce.py", 2),
    ("tests/test_native_telegram.py", 2),
    ("tests/test_native_telegram_compat.py", 2),
    ("tests/test_native_announce_compat.py", 1),
    ("tests/test_native_reorder_cli.py", 1),
    ("tests/test_media_bounds.py", 12),
    ("tests/test_media_fitting.py", 8),
    ("tests/test_media_reencode.py", 21),
    ("tests/test_media.py", 42),
    ("tests/test_make_emoji_pngs.py", 19),
    ("tests/test_identity_video.py", 14),
    ("tests/test_video_identity_fidelity.py", 12),
    ("tests/test_video_timeline_comparison.py", 10),
    ("tests/test_lottie_repaint.py", 13),
    ("tests/test_brand_logo.py", 11),
    ("tests/test_identity_migration.py", 33),
    ("tests/test_migration_provenance.py", 9),
    ("tests/test_migration_lifecycle.py", 20),
    ("tests/test_native_convert_cli.py", 4),
    ("tests/test_media_boundary.py", 2),
    ("tests/test_fetch_pack_limit.py", 3),
    ("tests/test_fetch_pack_ids.py", 3),
    ("tests/test_native_fetch_cli.py", 1),
    ("tests/test_fetch_emoji_ids.py", 12),
    ("tests/test_repaintable.py", 26),
    ("tests/test_native_collect_partial.py", 1),
    ("tests/test_native_ids_cli.py", 1),
    ("tests/test_video_collision_ingest.py", 14),
    ("tests/test_ingest_retention.py", 1),
    ("tests/test_launcher_contract.py", 3),
    ("tests/test_prebuilt_environment.py", 3),
    ("tests/test_native_install.py", 4),
    ("tests/test_native_package.py", 1),
    ("tests/test_entry_points.py", 8),
    ("tests/test_build_process_owner.py", 2),
    ("tests/test_panel_view.py", 23),
    ("tests/test_panel_assets.py", 7),
    ("tests/test_panel_logging.py", 2),
    ("tests/test_panel_preview.py", 7),
    ("tests/test_panel.py", 10),
    ("tests/test_panel_guard.py", 35),
    ("tests/test_panel_server.py", 13),
    ("tests/test_panel_instance.py", 3),
    ("tests/test_panel_startup.py", 2),
    ("tests/test_panel_sandbox.py", 41),
    ("tests/test_sandbox_lifecycle.py", 14),
    ("tests/test_panel_page_actions.py", 34),
    ("tests/test_panel_page_grid.py", 25),
    ("tests/test_panel_plan.py", 12),
    ("tests/test_panel_save_scope.py", 8),
    ("tests/test_panel_intent_persistence.py", 18),
    ("tests/test_panel_browser.py", 29),
    ("tests/test_panel_queues.py", 23),
    ("tests/test_panel_curation.py", 32),
    ("tests/test_panel_perf.py", 13),
];

pub fn binding_fixture(source: &str, name: &str) -> Option<(&'static str, &'static str)> {
    Some(match (source, name) {
        (
            "tests/test_native_migration_cli.py",
            "NativeMigrationCLI.test_generated_video_bundle_replays_between_backends",
        ) => (
            "tests/fixtures/native/video-fingerprint.json",
            "5cd389f29767999b245c53f3fb1ea7e317c1d207bf0235d825c6f17237a7c32f",
        ),
        (
            "tests/test_native_bot.py",
            "NativeBotPayloads.test_render_parse_extract_and_copy_batches_match_source",
        ) => (
            "tests/fixtures/backend/bot.json",
            "00f3a5e7f703fc95fb027bb6ad1278a428c98f801f6619214a1a5933b37c04ba",
        ),
        (
            "tests/test_native_ingest.py",
            "NativeIngestParity.test_nominations_unknown_ambiguity_and_collision_match_source",
        ) => (
            "tests/fixtures/backend/ingest.json",
            "2ec808a2a1210be99d3d161f3a14df5c8a3ab2b0b6895571cd44ec8fec646818",
        ),
        (
            "tests/test_native_reconcile.py",
            "NativeReconcileParity.test_near_candidates_and_unknown_results_match_source",
        ) => (
            "tests/fixtures/backend/reconcile.json",
            "bc430cc2de230c5e5791601be1e9297e0d6f4c9702134e355f3d56a5465a27a7",
        ),
        (
            "tests/test_native_state.py",
            "NativeIntent.test_intent_matches_pinned_source"
            | "NativeIntent.test_invalid_targets_are_refused",
        ) => (
            "tests/fixtures/backend/intent.json",
            "9217bd704f96051c163d2b889fc1dac122830f0b83e35ee97b7c96f56d361b0b",
        ),
        ("tests/test_native_state.py", "NativeIntent.test_collection_resume_matches_source") => (
            "tests/fixtures/backend/resume.json",
            "ffd211b0b760b906e6b8d9d84dfde12c274e26730f52bdabe4e802beb1088b9a",
        ),
        (
            "tests/test_native_roster.py",
            "NativeRosterRows.test_ids_format_and_history_match_live_source_contract",
        ) => (
            "tests/fixtures/backend/roster.json",
            "039eee42bf316e1f8ad21f6d6f2dca7d8a2be99970381dadfaa9583256f907b1",
        ),
        (
            "tests/test_native_plan_steps.py",
            "NativePlanSteps.test_moves_holding_reorder_and_capacity_match_source"
            | "NativePlanSteps.test_empty_duplicated_or_unannotated_cases_cannot_pass",
        ) => (
            "tests/fixtures/backend/steps.json",
            "63e38f56182a4e98aa3883e9ababd1cba53a7baac3b645f2125be87e35a000c3",
        ),
        (
            "tests/test_native_panel.py",
            "NativePanelView.test_visibility_membership_and_labels_match_source",
        ) => (
            "tests/fixtures/backend/view.json",
            "4c54a770153bc4c3887512215f689f571e37ae2311a22dd5aa6bdcba93359122",
        ),
        _ => return None,
    })
}

pub fn selected(source: &str, name: &str) -> bool {
    match source {
        "tests/test_coin_providers.py" => {
            name.starts_with("BlankProviderLogo.")
                || name.starts_with("VerifiedPublish.")
                || name.starts_with("TheCanonicalMapIsRereadUnderTheLock.")
                || matches!(
                    name,
                    "TheCanonicalMapIsRereadUnderTheLock.test_neither_writers_ids_are_lost"
                        | "TheCanonicalMapIsRereadUnderTheLock.test_a_ticker_mapped_while_we_waited_is_not_uploaded_again"
                        | "TheCanonicalMapIsRereadUnderTheLock.test_the_rest_of_the_batch_is_still_published"
                        | "TheCanonicalMapIsRereadUnderTheLock.test_a_map_writer_blocks_the_publisher"
                        | "TheProvidersAndTheRebuildShareOneStateFile.test_the_provider_intent_cannot_collide_with_the_rebuild_s"
                        | "TheProvidersAndTheRebuildShareOneStateFile.test_a_provider_top_up_does_not_brick_the_next_rebuild"
                        | "TheProvidersAndTheRebuildShareOneStateFile.test_the_tally_never_counts_one_upload_twice"
                        | "TheProvidersAndTheRebuildShareOneStateFile.test_every_set_record_the_providers_write_carries_live"
                        | "CommandExitCodes.test_a_failed_add_exits_nonzero"
                        | "CommandExitCodes.test_a_blank_provider_logo_exits_nonzero"
                        | "CommandExitCodes.test_a_clean_run_exits_zero_and_records_the_id"
                        | "VerifyLogosModule.test_it_uses_the_shared_pack_lock"
                        | "VerifyLogosModule.test_module_imports_cleanly"
                        | "OnePackFamilyOneLock.test_every_coin_tool_locks_on_the_pack_base"
                        | "CommandExitCodes.test_both_fetchers_publish_through_the_same_helper"
                        | "TheProvidersAndTheRebuildShareOneStateFile.test_both_tools_name_the_same_state_file"
                        | "AnOutageIsNotNoMatch.test_an_unreachable_provider_leaves_the_cache_alone_and_exits_partial"
                        | "AnOutageIsNotNoMatch.test_retry_unmatched_searches_a_cached_miss_again"
                )
        }
        "tests/test_coin_recovery.py" => matches!(
            name,
            "UnverifiedUploadIsRecovered.test_nothing_else_is_mutated_while_an_outcome_is_unresolved"
                | "UnverifiedUploadIsRecovered.test_a_definitive_rejection_still_clears_the_intent"
                | "UnverifiedUploadIsRecovered.test_the_unverified_upload_is_recorded_and_not_repeated"
                | "UnverifiedUploadIsRecovered.test_a_re_downloaded_source_does_not_unmake_the_recovery"
                | "UnverifiedUploadIsRecovered.test_a_skipped_ticker_does_not_replace_the_published_logo"
                | "UnverifiedUploadIsRecovered.test_an_unresolved_upload_keeps_its_source_for_the_next_run"
                | "UnverifiedUploadIsRecovered.test_recovery_never_promotes_an_image_it_cannot_prove"
                | "UnverifiedUploadIsRecovered.test_an_upload_that_never_landed_is_retried_once"
                | "UnverifiedUploadIsRecovered.test_another_process_cannot_swap_the_image_mid_upload"
                | "UnverifiedUploadIsRecovered.test_an_unexpected_add_failure_keeps_the_intent_and_stops"
        ),
        "tests/test_coin_ticker_map.py" => matches!(
            name,
            "AliasMapAmbiguity.test_ambiguous_name_is_reported_not_guessed"
                | "AliasMapAmbiguity.test_unique_name_is_still_applied"
                | "AliasMapAmbiguity.test_an_interrupted_write_leaves_the_canonical_map_intact"
                | "CanonicalMapWritersCannotLoseAnUpdate.test_alias_map_keeps_the_other_writers_ids"
                | "CanonicalMapWritersCannotLoseAnUpdate.test_enhance_map_keeps_the_other_writers_ids"
                | "CanonicalMapWritersCannotLoseAnUpdate.test_a_held_lock_stops_the_map_tools_without_writing"
                | "CanonicalMapWritersCannotLoseAnUpdate.test_remap_apply_backs_up_and_replaces_under_the_lock"
                | "OneInventoryImplementation.test_the_suffix_rules_are_unchanged"
                | "OneInventoryImplementation.test_an_unmapped_ticker_is_blanked_not_left_stale"
                | "OneInventoryImplementation.test_indentation_and_ids_survive_the_round_trip"
                | "OneInventoryImplementation.test_parse_missing_skips_what_is_already_mapped"
                | "OneInventoryImplementation.test_the_explicit_aliases_now_reach_the_fetchers"
                | "OneInventoryImplementation.test_the_re_fill_loop_has_exactly_one_definition"
                | "OneInventoryImplementation.test_every_tool_re_fills_through_the_same_function"
        ),
        "tests/test_verify_logos.py" => {
            name.starts_with("VerifyLogosFix.")
                || name.starts_with("LogoReviewDistance.")
                || matches!(
                    name,
                    "VerifyLogosMainContracts.test_a_requested_fix_that_failed_exits_nonzero"
                        | "VerifyLogosMainContracts.test_missing_owner_id_is_a_usage_error_not_a_traceback"
                        | "VerifyLogosMainContracts.test_non_numeric_owner_id_is_a_usage_error"
                        | "VerifyLogosMainContracts.test_missing_token_is_a_usage_error"
                        | "VerifyLogosMainContracts.test_unreadable_state_file_is_a_usage_error"
                        | "VerifyLogosMainContracts.test_an_unresolved_intent_blocks_every_replacement"
                        | "VerifyLogosMainContracts.test_a_clear_intent_lets_the_run_proceed"
                        | "VerifyLogosMainContracts.test_the_set_list_is_read_after_the_pack_lock_not_before"
                )
        }
        "tests/test_rebuild_dedup_locks.py"
        | "tests/test_rebuild_dedup_state.py"
        | "tests/test_rebuild_dedup_plan.py"
        | "tests/test_rebuild_dedup_resume.py" => true,
        "tests/test_rebuild_dedup_map.py" => true,
        "tests/test_remap_ids.py" => true,
        "tests/test_resume_safety.py" => {
            name.starts_with("SafeConfigParsing.")
                || name.starts_with("StateShapeValidation.")
                || name.starts_with("AtomicJsonWrite.")
                || name.starts_with("ExitCodes.")
                || name.starts_with("AddMediaExitCode.")
                || name.starts_with("PerSetLimit.")
                || name.starts_with("ResumeAfterSkippedImage.")
                || name.starts_with("LinksDestination.")
        }
        "tests/test_publish_dedup.py" => {
            name.starts_with("PublishDedupTest.") || name.starts_with("VerifiedRetryTest.")
        }
        "tests/test_publish_contracts.py" => {
            name.starts_with("PackTitlesAreOneSequence.")
                || name.starts_with("VideoBlankCheck.")
                || name.starts_with("EveryPublisherSharesOneAnnouncer.")
                || name.starts_with("AFilledPackAnnouncesAgain.")
                || name.starts_with("AnnouncementRoutesThroughTheWorker.")
                || name.starts_with("MixedPublishesOneFamily.")
                || name.starts_with("ThePublishFollowsThePanelOrderNow.")
        }
        "tests/test_publish_invariants.py" => {
            name.starts_with("AClosedSetIsNeverAppendedTo.")
                || name.starts_with("MixedFamiliesStillCheckEachItemsOwnFormat.")
        }
        "tests/test_publish_cli.py" => {
            name.starts_with("CliContract.")
                || matches!(
                    name,
                    "PublishThroughMain.test_publish_then_resume_uploads_each_emoji_once"
                        | "PublishThroughMain.test_a_set_emptied_between_runs_stops_the_resume"
                        | "PublishThroughMain.test_unreachable_telegram_is_retryable_not_a_failure"
                        | "PublishThroughMain.test_new_set_starts_the_next_pack_while_this_one_is_half_empty"
                        | "PublishThroughMain.test_into_pack_tops_up_an_older_pack_that_still_has_room"
                        | "PublishThroughMain.test_the_record_written_is_the_pack_actually_uploaded_to"
                        | "PublishThroughMain.test_into_pack_refuses_a_number_that_is_not_there"
                        | "PublishThroughMain.test_into_pack_refuses_a_full_pack"
                        | "PublishThroughMain.test_into_pack_and_new_set_together_are_rejected"
                        | "PublishThroughMain.test_an_older_recorded_set_that_disappeared_fails_closed"
                        | "PublishThroughMain.test_an_older_set_that_cannot_be_read_is_retryable"
                        | "PublishThroughMain.test_each_successful_upload_is_logged"
                        | "PublishThroughMain.test_a_file_telegram_will_never_accept_stops_being_retried"
                        | "PublishThroughMain.test_the_dry_run_counts_what_will_publish_not_the_plan"
                        | "PublishThroughMain.test_a_run_that_lost_an_emoji_does_not_announce_the_pack"
                        | "PublishThroughMain.test_a_clean_run_announces_normally"
                        | "PublishThroughMain.test_a_transient_failure_is_still_retried"
                        | "PublishThroughMain.test_a_run_where_every_upload_failed_exits_non_zero"
                        | "PublishThroughMain.test_a_partly_failed_run_is_partial"
                        | "PublishThroughMain.test_a_foreign_tail_sticker_sends_the_next_emoji_to_a_new_set"
                        | "PublishThroughMain.test_without_new_set_the_next_emoji_still_fills_the_current_pack"
                        | "PublishThroughMain.test_the_manifest_counts_the_pack_not_the_catalog_rows"
                        | "PublishThroughMain.test_a_set_without_a_logo_still_counts_plainly"
                        | "PublishThroughMain.test_preflight_refuses_early_and_publishes_nothing"
                        | "PublishThroughMain.test_preflight_passes_a_clean_queue_and_still_uploads_nothing"
                        | "PublishThroughMain.test_a_transport_error_is_not_reported_as_a_bad_file"
                        | "PublishThroughMain.test_skipped_blank_media_is_not_counted_as_a_failure"
                        | "PublishThroughMain.test_drift_while_adopting_a_set_stops_the_run_instead_of_retrying"
                )
        }
        "tests/test_build_collection_state.py" => {
            name.starts_with("StateFileContract.")
                || [
                    "LiveSetDrift.",
                    "ForeignIdentityOnARecordedPosition.",
                    "UnattributedTail.",
                    "OurEmojiBehindAForeignSticker.",
                    "FreshUploadIdentity.",
                ]
                .iter()
                .any(|prefix| name.starts_with(prefix))
        }
        "tests/test_telegram_client.py" => {
            matches!(
                name,
                "TokenRedaction.test_exception_text_never_carries_the_token"
                    | "TokenRedaction.test_network_retry_output_is_redacted"
                    | "StickerSetInvalidScope.test_lookup_fails_fast_without_sleeping"
                    | "StickerSetInvalidScope.test_create_still_waits_for_the_name_lock"
            ) || name.starts_with("AddedCheckUsesIdentity.")
                || name.starts_with("CreateAdoptionVerifiesContent.")
                || name.starts_with("PostAddSizeIsMeasured.")
                || name.starts_with("TriStateLiveReads.")
                || name.starts_with("RepaintingIsCreationOnly.")
                || name.starts_with("WaitsReachTheLogFile.")
                || name.starts_with("BookkeepingFollowsLiveState.")
        }
        "tests/test_panel.py" => {
            name.starts_with("InertItemJson.")
                || name.starts_with("CatalogUnavailable.")
                || name.starts_with("SavingFromAFilteredGridKeepsHiddenChoices.")
                || name.starts_with("SaveDuringReorder.")
                || name.starts_with("TheMediaMapIsNeverEmpty.")
        }
        "tests/test_panel_guard.py" => true,
        "tests/test_panel_startup.py" => name.starts_with("StartUpDoesNotWaitOnTelegram."),
        "tests/test_panel_sandbox.py" => true,
        "tests/test_sandbox_lifecycle.py" => true,
        "tests/test_native_migration_cli.py" => matches!(
            name,
            "NativeMigrationCLI.test_version_two_apply_restore_and_late_edit_refusal"
                | "NativeMigrationCLI.test_generated_video_bundle_replays_between_backends"
                | "NativeMigrationCLI.test_real_operator_video_copy_migrates_and_source_can_restore_bundle"
        ),
        "tests/test_panel_server.py" => {
            name.starts_with("LoopbackCheck.")
                || name.starts_with("ConnectionsAreReused.")
                || name.starts_with("TheBusyPortMessageNamesTheProcess.")
                || name.starts_with("TheSandboxCannotTakeTheRealPanelsPort.")
                || name.starts_with("OnlyOnePanelPerPort.")
        }
        "tests/test_media_fitting.py" => true,
        "tests/test_media_reencode.py" => true,
        "tests/test_entry_points.py" => true,
        "tests/test_build_process_owner.py" => true,
        "tests/test_lottie_repaint.py" => true,
        "tests/test_brand_logo.py" => {
            name.starts_with("BrandLogoPrepare.")
                || name.starts_with("AnUnusableLogoStopsThePublish.")
                || name.starts_with("PublishFormatLogoFirst.")
                || name.starts_with("TheRepositoryShipsNoOperatorLogo.")
        }
        "tests/test_identity_migration.py" => matches!(
            name,
            "EveryDurableReferenceMoves.test_the_state_file_is_rewritten_not_just_the_tables"
                | "EveryDurableReferenceMoves.test_the_telegram_identifiers_are_preserved"
                | "EveryDurableReferenceMoves.test_a_hash_only_correction_is_applied_even_when_the_key_holds"
                | "EveryDurableReferenceMoves.test_an_archived_file_is_renamed_to_match_its_new_key"
                | "EveryDurableReferenceMoves.test_a_second_migration_is_a_verified_no_op"
                | "EveryDurableReferenceMoves.test_the_migration_takes_the_pack_family_lock"
                | "MigratedPathsStayDataRelative.test_unchanged_rows_stay_byte_identical_and_renames_stay_relative"
                | "NothingIsAppliedFromAnIncompletePicture.test_a_missing_file_is_not_reported_as_unchanged"
                | "NothingIsAppliedFromAnIncompletePicture.test_a_collision_refuses_instead_of_merging"
                | "TheBackupIsWalSafe.test_a_snapshot_taken_with_an_open_wal_holds_the_committed_rows"
                | "TheBackupIsWalSafe.test_the_restored_copy_equals_the_pre_migration_catalog"
                | "APreexistingPartialMigrationIsRecoverable.test_the_map_is_recovered_from_the_backup"
                | "APreexistingPartialMigrationIsRecoverable.test_applying_it_repairs_the_state_file"
                | "APreexistingPartialMigrationIsRecoverable.test_a_key_absent_from_the_backup_is_left_alone"
                | "APreexistingPartialMigrationIsRecoverable.test_an_ambiguous_pairing_is_not_guessed"
                | "EveryDurableReferenceMoves.test_multiple_bases_all_move"
                | "NothingIsAppliedFromAnIncompletePicture.test_report_mode_says_incomplete_rather_than_clean"
                | "NothingIsAppliedFromAnIncompletePicture.test_some_missing_still_blocks_the_whole_migration"
                | "NothingIsAppliedFromAnIncompletePicture.test_all_undecodable_blocks_it_too"
                | "NothingIsAppliedFromAnIncompletePicture.test_colliding_with_a_row_that_is_not_moving_also_refuses"
                | "NothingIsAppliedFromAnIncompletePicture.test_an_empty_catalog_is_clean_not_pending"
                | "NothingIsAppliedFromAnIncompletePicture.test_a_clean_catalog_reports_clean"
                | "NothingIsAppliedFromAnIncompletePicture.test_a_pending_catalog_reports_stale_and_writes_nothing"
                | "NothingIsAppliedFromAnIncompletePicture.test_only_obsolete_plan_entries_are_informational"
                | "AnInterruptedMigrationIsResumable.test_report_names_an_interrupted_migration"
                | "AnInterruptedMigrationIsResumable.test_an_unreadable_journal_is_not_deleted_blindly"
                | "MigratedPathsStayDataRelative.test_rows_written_absolute_by_the_old_code_still_verify"
                | "AnInterruptedMigrationIsResumable.test_a_crash_after_the_backup_resumes"
                | "AnInterruptedMigrationIsResumable.test_a_crash_after_the_database_stage_resumes"
                | "AnInterruptedMigrationIsResumable.test_a_crash_after_the_state_stage_resumes"
                | "TheBackupIsWalSafe.test_a_rollback_journal_catalog_backs_up_too"
                | "TheBackupIsWalSafe.test_two_attempts_in_the_same_second_do_not_share_a_file"
                | "TheBackupIsWalSafe.test_an_unverifiable_backup_stops_before_any_change"
        ),
        "tests/test_migration_lifecycle.py" => matches!(
            name,
            "MigrationCannotCertifyDamage.test_actual_publisher_lock_prevents_migration"
                | "MigrationCannotCertifyDamage.test_unrelated_rename_destination_is_never_overwritten"
                | "MigrationCannotCertifyDamage.test_required_stale_manifest_cannot_be_called_plan_noise"
                | "MigrationCannotCertifyDamage.test_late_missing_media_retains_journal_and_refuses_success"
                | "MigrationCannotCertifyDamage.test_completed_migration_restores_files_state_and_identifiers"
                | "MigrationCannotCertifyDamage.test_restore_preserves_an_identical_preexisting_destination"
                | "MigrationCannotCertifyDamage.test_restore_refuses_later_catalog_or_file_changes"
                | "MigrationCannotCertifyDamage.test_journal_bound_to_another_directory_refuses_without_mutation"
                | "MigrationCannotCertifyDamage.test_late_decode_failure_cannot_retire_the_journal"
                | "MigrationCannotCertifyDamage.test_orphaned_identifiers_prevent_a_clean_report_or_noop"
                | "MigrationCannotCertifyDamage.test_restore_after_a_move_before_database_path_commit"
                | "MigrationCannotCertifyDamage.test_existing_link_after_interruption_is_replayed_without_losing_bytes"
                | "MigrationCannotCertifyDamage.test_a_second_catalog_connection_cannot_write_during_migration"
                | "ActualWriterOwnership.test_canonical_alias_and_second_migration_use_the_same_native_lock"
                | "MigrationCannotCertifyDamage.test_partial_legacy_migration_requires_full_backup_coverage"
                | "ActualWriterOwnership.test_pending_journal_blocks_ingest_archive_publisher_and_panel_save"
                | "ActualWriterOwnership.test_real_catalog_order_and_unchanged_pack_resume_survive_migration_and_restore"
                | "MigrationCannotCertifyDamage.test_cli_recovers_after_each_state_replacement_and_journal_retirement"
                | "MigrationCannotCertifyDamage.test_native_death_after_second_move_replays_via_cli"
                | "ActualWriterOwnership.test_first_family_publisher_and_catalog_exclude_migration_in_real_children"
        ),
        "tests/test_media_boundary.py" => true,
        "tests/test_repaintable.py" => true,
        "tests/test_video_collision_ingest.py" => true,
        "tests/test_migration_provenance.py" => true,
        "tests/test_native_consumer_boundary.py" => matches!(
            name,
            "NativeConsumerBoundary.test_converter_watchdog_passes_use_the_installed_native_command"
                | "NativeConsumerBoundary.test_converter_idle_timeout_stops_owned_tree_without_waiting_minutes"
                | "NativeConsumerBoundary.test_relocated_references_preserve_source_behavior_and_pinned_oracles"
                | "NativeConsumerBoundary.test_all_references_import_with_retired_production_modules_blocked"
                | "NativeConsumerBoundary.test_sandbox_imports_without_the_python_panel_application"
        ),
        "tests/test_native_similarity.py" => matches!(
            name,
            "NativeSimilarity.test_greedy_matches_source_fixtures"
                | "NativeSimilarity.test_near_matches_source_fixtures"
                | "NativeSimilarity.test_source_fixtures_do_not_drift"
                | "NativeSimilarity.test_invalid_native_inputs_fail_before_computation"
                | "NativeSimilarity.test_independent_concurrent_calls_have_no_shared_state"
                | "NativeSimilarity.test_missing_module_never_uses_a_python_fallback"
                | "NativeSimilarity.test_missing_native_cannot_create_catalog_state"
                | "NativeSimilarity.test_installer_fails_usefully_without_rust"
        ),
        _ => true,
    }
}
