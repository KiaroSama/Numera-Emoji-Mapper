//! Ninety-five original coin assertions share the admitted corpus and historical worker.
use _native::{source_git_inventory::Source, source_provider_tests as oracle};
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = oracle::source_paths()
        .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    oracle::source_identity(&sources).unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let missing = parent.join(format!("provider source refusal {}", std::process::id()));
    assert!(!missing.exists());
    for index in 0..sources.len() {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let original = sources[index];
            let mut changed = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "tests/not-original-provider.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                _ => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
            }
            let mut admitted = sources;
            admitted[index] = &changed;
            assert!(
                runtime
                    .block_on(oracle::observe_owned(root, &missing, &missing, &admitted))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!missing.exists());
        }
    }
    let workspace = parent.join(format!(
        "original provider assertions {}",
        std::process::id()
    ));
    std::fs::create_dir(&workspace).unwrap();
    let caller = workspace.join("caller");
    std::fs::write(&caller, b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(std::fs::read(&caller).unwrap(), b"caller bytes");
    assert!(
        oracle::observe(root, root, &sources)
            .unwrap_err()
            .contains("inside project scratch")
    );
    std::fs::remove_file(caller).unwrap();
    #[cfg(unix)]
    {
        let link = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &link).unwrap();
        assert!(
            oracle::observe(root, &link, &sources)
                .unwrap_err()
                .contains("linked/reparse")
        );
        std::fs::remove_file(link).unwrap();
    }
    let actual = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-compat")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(actual["source_revision"], sources[0].baseline_revision);
    assert_eq!(actual["source_runtime"], "CPython3.11.9");
    assert_eq!(actual["Pillow"], "12.3.0");
    assert_eq!(actual["tests_run"], 97);
    assert_eq!(actual["primary_contexts"], 85);
    assert_eq!(actual["replacement_contexts"], 19);
    assert_eq!(actual["verify_contexts"], 8);
    assert_eq!(actual["verify_loads"], 1);
    assert_eq!(actual["remap_contexts"], 14);
    assert_eq!(actual["NumPy"], "2.4.6");
    assert_eq!(actual["map_loads"], 6);
    assert_eq!(actual["environment_loads"], 4);
    assert_eq!(actual["http_patches"], 1);
    let proof = actual["proof_contexts"].as_u64().unwrap();
    assert!(proof > 0);
    let replacement_proof = actual["replacement_proof_contexts"].as_u64().unwrap();
    assert!(replacement_proof > 0);
    assert_eq!(
        actual["temporary_calls"].as_u64().unwrap(),
        86 + proof + replacement_proof
    );
    let verified = [
        "test_a_failed_add_is_neither_mapped_nor_counted_as_added",
        "test_a_full_set_rolls_over_and_the_new_set_is_recorded_after_it_exists",
        "test_a_rebuild_holding_the_family_lock_blocks_a_top_up",
        "test_a_second_publisher_is_refused_instead_of_appending_too",
        "test_an_ambiguous_add_is_resolved_from_live_state_not_re_sent",
        "test_an_unidentifiable_sticker_is_not_mapped",
        "test_the_id_is_ours_even_when_another_sticker_appears",
        "test_the_live_count_is_sent_as_expected_before",
    ];
    let mut canonical = verified.to_vec();
    canonical.extend([
        "test_a_map_writer_blocks_the_publisher",
        "test_a_ticker_mapped_while_we_waited_is_not_uploaded_again",
        "test_neither_writers_ids_are_lost",
        "test_the_rest_of_the_batch_is_still_published",
    ]);
    canonical.sort_unstable();
    let recovery = [
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
    let cases = [
        ("VerifiedPublish", verified.as_slice()),
        ("TheCanonicalMapIsRereadUnderTheLock", canonical.as_slice()),
        ("UnverifiedUploadIsRecovered", recovery.as_slice()),
        (
            "TheProvidersAndTheRebuildShareOneStateFile",
            [
                "test_a_provider_top_up_does_not_brick_the_next_rebuild",
                "test_every_set_record_the_providers_write_carries_live",
                "test_the_provider_intent_cannot_collide_with_the_rebuild_s",
                "test_the_tally_never_counts_one_upload_twice",
            ]
            .as_slice(),
        ),
        (
            "CommandExitCodes",
            [
                "test_a_blank_provider_logo_exits_nonzero",
                "test_a_clean_run_exits_zero_and_records_the_id",
                "test_a_failed_add_exits_nonzero",
            ]
            .as_slice(),
        ),
        (
            "AnOutageIsNotNoMatch",
            [
                "test_an_unreachable_provider_leaves_the_cache_alone_and_exits_partial",
                "test_retry_unmatched_searches_a_cached_miss_again",
            ]
            .as_slice(),
        ),
        (
            "OneInventoryImplementation",
            [
                "test_an_unmapped_ticker_is_blanked_not_left_stale",
                "test_indentation_and_ids_survive_the_round_trip",
                "test_parse_missing_skips_what_is_already_mapped",
                "test_the_suffix_rules_are_unchanged",
            ]
            .as_slice(),
        ),
        (
            "AliasMapAmbiguity",
            [
                "test_ambiguous_name_is_reported_not_guessed",
                "test_unique_name_is_still_applied",
            ]
            .as_slice(),
        ),
        (
            "CanonicalMapWritersCannotLoseAnUpdate",
            [
                "test_a_held_lock_stops_the_map_tools_without_writing",
                "test_alias_map_keeps_the_other_writers_ids",
                "test_enhance_map_keeps_the_other_writers_ids",
                "test_remap_apply_backs_up_and_replaces_under_the_lock",
            ]
            .as_slice(),
        ),
        (
            "DownloadLiveTest",
            [
                "test_appended_sticker_is_picked_up",
                "test_changed_manifest_invalidates_cache_and_prunes_stale_ids",
                "test_failed_sticker_leaves_set_incomplete_and_is_retried",
                "test_legacy_cache_keeps_signatures_but_re_verifies_sets",
            ]
            .as_slice(),
        ),
        (
            "NearestTest",
            [
                "test_identical_rows_never_produce_negative_or_nan_distance",
                "test_single_candidate_has_infinite_margin",
            ]
            .as_slice(),
        ),
        (
            "MainApplyTest",
            [
                "test_a_short_result_from_nearest_raises_instead_of_mapping_fewer",
                "test_apply_refuses_ambiguous_matches",
                "test_apply_refuses_to_overwrite_canonical_map_from_incomplete_data",
                "test_apply_without_max_distance_is_a_usage_error",
                "test_apply_writes_map_when_complete_and_unambiguous",
                "test_far_match_is_omitted_not_mapped",
                "test_no_local_logos_fails_instead_of_crashing",
            ]
            .as_slice(),
        ),
        (
            "ApplyIsSerialisedAgainstThePackFamily",
            [
                "test_a_held_pack_lock_stops_apply_before_it_reads_anything",
                "test_a_provider_cannot_land_a_map_entry_during_the_live_read",
            ]
            .as_slice(),
        ),
        (
            "TheSignatureMustNotBeBlindToBlackOnTransparent",
            [
                "test_a_black_mark_carries_information_at_all",
                "test_colour_still_separates_two_marks_of_the_same_shape",
                "test_two_different_black_marks_are_not_identical",
            ]
            .as_slice(),
        ),
        (
            "ACacheFromAnOlderSignatureIsDiscarded",
            ["test_signatures_of_the_wrong_length_are_dropped"].as_slice(),
        ),
        (
            "VerifyLogosMainContracts",
            [
                "test_a_clear_intent_lets_the_run_proceed",
                "test_a_requested_fix_that_failed_exits_nonzero",
                "test_an_unresolved_intent_blocks_every_replacement",
                "test_missing_owner_id_is_a_usage_error_not_a_traceback",
                "test_missing_token_is_a_usage_error",
                "test_non_numeric_owner_id_is_a_usage_error",
                "test_the_set_list_is_read_after_the_pack_lock_not_before",
                "test_unreadable_state_file_is_a_usage_error",
            ]
            .as_slice(),
        ),
        (
            "VerifyLogosFix",
            [
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
            ]
            .as_slice(),
        ),
        (
            "LogoReviewDistance",
            [
                "test_a_genuinely_different_image_is_still_far",
                "test_the_same_mark_inverted_is_not_reported_as_different",
            ]
            .as_slice(),
        ),
    ]
    .into_iter()
    .flat_map(|(class, methods)| methods.iter().map(move |name| format!("{class}.{name}")))
    .collect::<Vec<_>>();
    assert_eq!(actual["cases"], serde_json::json!(cases));
    let inherited: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/coverage-inherited.json"
    ))
    .unwrap();
    let inherited = inherited["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["source_file"] == "tests/test_coin_providers.py")
        .collect::<Vec<_>>();
    assert_eq!(inherited.len(), 8);
    for row in inherited {
        assert_eq!(row["status"], "pending");
        assert!(row.get("evidence").is_none());
        assert!(
            actual["cases"]
                .as_array()
                .unwrap()
                .contains(&row["source_case"])
        );
        assert_eq!(
            row["source_hash"],
            actual["source_hashes"]["tests/test_coin_providers.py"]
        );
    }
    assert_eq!(
        actual["excluded"],
        serde_json::json!([
            "TheProvidersAndTheRebuildShareOneStateFile.test_both_tools_name_the_same_state_file",
            "CommandExitCodes.test_both_fetchers_publish_through_the_same_helper",
            "OneInventoryImplementation.test_the_explicit_aliases_now_reach_the_fetchers",
            "OneInventoryImplementation.test_the_re_fill_loop_has_exactly_one_definition",
            "OneInventoryImplementation.test_every_tool_re_fills_through_the_same_function",
            "AliasMapAmbiguity.test_an_interrupted_write_leaves_the_canonical_map_intact",
            "PackFamilyLockTest.test_it_is_the_same_lock_the_pack_mutators_take",
            "NumpyIsAnExtra.test_a_missing_numpy_says_which_file_to_install",
            "NumpyIsAnExtra.test_nothing_else_first_party_imports_numpy",
        ])
    );
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(actual[field], 0, "{field}");
    }
    for source in sources {
        assert_eq!(actual["source_hashes"][&source.path], source.sha256);
    }
    assert!(std::fs::read_dir(&workspace).unwrap().next().is_none());
    std::fs::remove_dir(workspace).unwrap();
}
