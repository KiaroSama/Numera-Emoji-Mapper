//! Fixed original coin cohort identities and case roster; exclusions never count as execution.
pub(super) const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
pub(super) const IDENTITIES: [(&str, &str); 25] = [
    (
        "coins/_provider_publish.py",
        "4bce8ae96c8ec69e22fa135bb622c26409100b124c578fed94f2b04cb8b114fc",
    ),
    (
        "tests/reference/packstate.py",
        "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
    ),
    (
        "emojikit/identity.py",
        "8cbefd550e306e671df1e52af2dd9fe6b4e3647c3aa1b13dc98bf35080d36e89",
    ),
    (
        "emojikit/telegram_api.py",
        "712554abbfeb63f72f38c4a27261b973bc81a7497991615223df44bec29265e8",
    ),
    (
        "emojikit/cli_env.py",
        "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
    ),
    (
        "tests/_coin_fixtures.py",
        "eb95df6b040a2b57ba521ec7f84405dad33cf3be7c7de61d8ccffb3d50475389",
    ),
    (
        "tests/test_coin_providers.py",
        "57694a3668a8bde79ca081053ac1da5a43da3e5bdc78803e5db45e8fcf842012",
    ),
    (
        "tests/test_coin_recovery.py",
        "1babc1c591f57c2db70633dfde43297d9fa667cdd161c9ac149cf73b3b5817cb",
    ),
    (
        "emojikit/media.py",
        "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53",
    ),
    (
        "coins/_paprika_api.py",
        "34742583025adaad3c7513ce0031c13d32759e29cfa8076609150a0badeb1acc",
    ),
    (
        "coins/_dedup_plan.py",
        "d5fe1edbd9ecf37cc59eada51b2108a756d49edece5b1fd47db4a1b325ec5bd3",
    ),
    (
        "coins/fetch_paprika.py",
        "360809a22b57c19808b146016fe388769046f7337986684fd4d9c4db6f268b30",
    ),
    (
        "coins/_inventory.py",
        "46985e570f4d9e7ee8c9ae0d326f0517033a3a3822f3b9d594ca07a30a079f47",
    ),
    (
        "coins/_env.py",
        "e6c2dedf03011c9b7d2c60563c924f337bd89bc9691fd4a8df5f2830ae0ba6f7",
    ),
    (
        "emojikit/operator_config.py",
        "a19f804a7bdd37baccb474eac96df6aa4d46439dd956e457c06adf3c180b9591",
    ),
    (
        "emojikit/errors.py",
        "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
    ),
    (
        "tests/test_coin_ticker_map.py",
        "4308c8c38a774820223b7645f8ff8dfe8e74e3dc309342a2d4d264a7ba3eed91",
    ),
    (
        "coins/alias_map.py",
        "e9361cc868a2dd13f5f9e450f55a95399d1cea24438223fa9e84533cb19f20ff",
    ),
    (
        "coins/enhance_map.py",
        "3ed2ccca178ec4a68547de0c4dca77e76e8b9775d2a493f006fcca418be5cd25",
    ),
    (
        "coins/remap_ids.py",
        "b66136d42adc3448d44e67c36c9f8c827a4e63d6ce259af5d6b437299b043ed7",
    ),
    (
        "tests/test_remap_ids.py",
        "3c877100f9af4a86e3cb93465b75683e9e6014adbe85b41880b7f4a2cbea65d8",
    ),
    (
        "coins/verify_logos.py",
        "86f17862ebe5019e05965fa7aa30380a20226b3632f192f051525290ae7f7031",
    ),
    (
        "tests/test_verify_logos.py",
        "aa1a7ae193adf684062d4318182c94f5b6a6962fb8bb0c13313bac3868690136",
    ),
    (
        "tests/_cli_fixtures.py",
        "25fccc28b1662f1b37e1505d9c359b598e642b5254184afc4daf751b5ef390cd",
    ),
    (
        "tests/oracles/telegram_api.py",
        "59f5c958a2f7a2f7ed32b93669861066bf3a3f6a6a859623f7456405b5c56aa3",
    ),
];
pub(super) const VERIFIED: &[&str] = &[
    "test_a_failed_add_is_neither_mapped_nor_counted_as_added",
    "test_a_full_set_rolls_over_and_the_new_set_is_recorded_after_it_exists",
    "test_a_rebuild_holding_the_family_lock_blocks_a_top_up",
    "test_a_second_publisher_is_refused_instead_of_appending_too",
    "test_an_ambiguous_add_is_resolved_from_live_state_not_re_sent",
    "test_an_unidentifiable_sticker_is_not_mapped",
    "test_the_id_is_ours_even_when_another_sticker_appears",
    "test_the_live_count_is_sent_as_expected_before",
];
pub(super) const CANONICAL: &[&str] = &[
    "test_a_map_writer_blocks_the_publisher",
    "test_a_ticker_mapped_while_we_waited_is_not_uploaded_again",
    "test_neither_writers_ids_are_lost",
    "test_the_rest_of_the_batch_is_still_published",
];
pub(super) const RECOVERY: &[&str] = &[
    "test_a_definitive_rejection_still_clears_the_intent",
    "test_a_re_downloaded_source_does_not_unmake_the_recovery",
    "test_a_skipped_ticker_does_not_replace_the_published_logo",
    "test_an_unexpected_add_failure_keeps_the_intent_and_stops",
    "test_an_unresolved_upload_keeps_its_source_for_the_next_run",
    "test_an_upload_that_never_landed_is_retried_once",
    "test_another_process_cannot_swap_the_image_mid_upload",
    "test_nothing_else_is_mutated_while_an_outcome_is_unresolved",
    "test_recovery_never_promotes_an_image_it_cannot_prove",
    "test_the_unverified_upload_is_recorded_and_not_repeated",
];
pub(super) const SHARED: &[&str] = &[
    "test_a_provider_top_up_does_not_brick_the_next_rebuild",
    "test_every_set_record_the_providers_write_carries_live",
    "test_the_provider_intent_cannot_collide_with_the_rebuild_s",
    "test_the_tally_never_counts_one_upload_twice",
];
pub(super) const COMMAND: &[&str] = &[
    "test_a_blank_provider_logo_exits_nonzero",
    "test_a_clean_run_exits_zero_and_records_the_id",
    "test_a_failed_add_exits_nonzero",
];
pub(super) const OUTAGE: &[&str] = &[
    "test_an_unreachable_provider_leaves_the_cache_alone_and_exits_partial",
    "test_retry_unmatched_searches_a_cached_miss_again",
];
pub(super) const INVENTORY: &[&str] = &[
    "test_an_unmapped_ticker_is_blanked_not_left_stale",
    "test_indentation_and_ids_survive_the_round_trip",
    "test_parse_missing_skips_what_is_already_mapped",
    "test_the_suffix_rules_are_unchanged",
];
pub(super) const ALIAS: &[&str] = &[
    "test_ambiguous_name_is_reported_not_guessed",
    "test_unique_name_is_still_applied",
];
pub(super) const MAP: &[&str] = &[
    "test_a_held_lock_stops_the_map_tools_without_writing",
    "test_alias_map_keeps_the_other_writers_ids",
    "test_enhance_map_keeps_the_other_writers_ids",
    "test_remap_apply_backs_up_and_replaces_under_the_lock",
];

pub(super) const REMAP_DOWNLOAD: &[&str] = &[
    "test_appended_sticker_is_picked_up",
    "test_changed_manifest_invalidates_cache_and_prunes_stale_ids",
    "test_failed_sticker_leaves_set_incomplete_and_is_retried",
    "test_legacy_cache_keeps_signatures_but_re_verifies_sets",
];
pub(super) const REMAP_NEAREST: &[&str] = &[
    "test_identical_rows_never_produce_negative_or_nan_distance",
    "test_single_candidate_has_infinite_margin",
];
pub(super) const REMAP_MAIN: &[&str] = &[
    "test_a_short_result_from_nearest_raises_instead_of_mapping_fewer",
    "test_apply_refuses_ambiguous_matches",
    "test_apply_refuses_to_overwrite_canonical_map_from_incomplete_data",
    "test_apply_without_max_distance_is_a_usage_error",
    "test_apply_writes_map_when_complete_and_unambiguous",
    "test_far_match_is_omitted_not_mapped",
    "test_no_local_logos_fails_instead_of_crashing",
];
pub(super) const REMAP_FAMILY: &[&str] = &[
    "test_a_held_pack_lock_stops_apply_before_it_reads_anything",
    "test_a_provider_cannot_land_a_map_entry_during_the_live_read",
];
pub(super) const REMAP_SIGNATURE: &[&str] = &[
    "test_a_black_mark_carries_information_at_all",
    "test_colour_still_separates_two_marks_of_the_same_shape",
    "test_two_different_black_marks_are_not_identical",
];
pub(super) const REMAP_CACHE: &[&str] = &["test_signatures_of_the_wrong_length_are_dropped"];

pub(super) const VERIFY_FIX: &[&str] = &[
    "test_a_busy_map_lock_stops_the_fix_before_it_touches_the_pack",
    "test_a_definite_failure_leaves_the_oracle_untouched",
    "test_a_lost_confirmation_read_leaves_a_recoverable_intent",
    "test_a_map_writer_cannot_repoint_the_ticker_mid_replacement",
    "test_a_replacement_whose_image_is_not_ours_is_refused",
    "test_a_shifted_set_is_not_trusted_as_the_replacement",
    "test_an_intent_cannot_be_reconciled_into_a_different_map",
    "test_an_intent_is_bound_to_its_state_file_too",
    "test_an_intent_with_no_recorded_target_is_refused",
    "test_an_unchanged_set_is_not_trusted_either",
    "test_an_unreadable_set_leaves_the_intent_for_the_run_after",
    "test_every_entry_sharing_the_old_cid_is_repointed",
    "test_fix_locks_on_the_pack_family_not_on_this_script",
    "test_recovery_is_idempotent_after_the_map_was_already_repointed",
    "test_recovery_refuses_when_the_live_image_is_not_ours",
    "test_the_next_run_recovers_the_replacement_from_the_intent",
    "test_the_uploaded_body_is_bytes_a_retry_can_resend",
    "test_timeout_after_apply_is_verified_not_resent",
    "test_verified_new_cid_rules",
];
pub(super) const VERIFY_DISTANCE: &[&str] = &[
    "test_a_genuinely_different_image_is_still_far",
    "test_the_same_mark_inverted_is_not_reported_as_different",
];

pub(super) const VERIFY_MAIN: &[&str] = &[
    "test_a_clear_intent_lets_the_run_proceed",
    "test_a_requested_fix_that_failed_exits_nonzero",
    "test_an_unresolved_intent_blocks_every_replacement",
    "test_missing_owner_id_is_a_usage_error_not_a_traceback",
    "test_missing_token_is_a_usage_error",
    "test_non_numeric_owner_id_is_a_usage_error",
    "test_the_set_list_is_read_after_the_pack_lock_not_before",
    "test_unreadable_state_file_is_a_usage_error",
];

pub(super) fn exclusions(class: &str) -> &'static [&'static str] {
    match class {
        "TheProvidersAndTheRebuildShareOneStateFile" => {
            &["test_both_tools_name_the_same_state_file"]
        }
        "CommandExitCodes" => &["test_both_fetchers_publish_through_the_same_helper"],
        "OneInventoryImplementation" => &[
            "test_the_explicit_aliases_now_reach_the_fetchers",
            "test_the_re_fill_loop_has_exactly_one_definition",
            "test_every_tool_re_fills_through_the_same_function",
        ],
        "AliasMapAmbiguity" => &["test_an_interrupted_write_leaves_the_canonical_map_intact"],
        "PackFamilyLockTest" => &["test_it_is_the_same_lock_the_pack_mutators_take"],
        "NumpyIsAnExtra" => &[
            "test_a_missing_numpy_says_which_file_to_install",
            "test_nothing_else_first_party_imports_numpy",
        ],
        _ => &[],
    }
}
