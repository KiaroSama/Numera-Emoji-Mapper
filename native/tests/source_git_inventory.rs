//! Mechanical source identity comes from pinned Git, never the editable case ledger.
use serde_json::Value;
use sha2::{Digest, Sha256};
const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
#[path = "support/retained_cli.rs"]
mod retained_cli;
#[path = "support/source_archive_tests_replay.rs"]
mod source_archive_tests_replay;
#[path = "support/source_benchmark_replay.rs"]
mod source_benchmark_replay;
#[path = "support/source_bot_replay.rs"]
mod source_bot_replay;
#[path = "support/source_catalog_replay.rs"]
mod source_catalog_replay;
#[path = "support/source_coin_audit_replay.rs"]
mod source_coin_audit_replay;
#[path = "support/source_coin_cli_replay.rs"]
mod source_coin_cli_replay;
#[path = "support/source_config_replay.rs"]
mod source_config_replay;
#[path = "support/source_coverage.rs"]
mod source_coverage;
#[path = "support/source_emoji_ids_replay.rs"]
mod source_emoji_ids_replay;
#[path = "support/source_export_replay.rs"]
mod source_export_replay;
#[path = "support/source_http_replay.rs"]
mod source_http_replay;
#[path = "support/source_identity_replay.rs"]
mod source_identity_replay;
#[path = "support/source_import_replay.rs"]
mod source_import_replay;
#[path = "support/source_keywords_replay.rs"]
mod source_keywords_replay;
#[path = "support/source_local_ingest_replay.rs"]
mod source_local_ingest_replay;
#[path = "support/source_logging_replay.rs"]
mod source_logging_replay;
#[path = "support/source_logo_cache_replay.rs"]
mod source_logo_cache_replay;
#[path = "support/source_lottie_replay.rs"]
mod source_lottie_replay;
#[path = "support/source_oracle_evidence.rs"]
mod source_oracle_evidence;
#[path = "support/source_oracle_replay.rs"]
mod source_oracle_replay;
#[path = "support/source_panel_view_replay.rs"]
mod source_panel_view_replay;
#[path = "support/source_prebuilt_replay.rs"]
mod source_prebuilt_replay;
#[path = "support/source_preflight_replay.rs"]
mod source_preflight_replay;
#[path = "support/source_provider_image_replay.rs"]
mod source_provider_image_replay;
#[path = "support/source_provider_replay.rs"]
mod source_provider_replay;
#[path = "support/source_publication_replay.rs"]
mod source_publication_replay;
#[path = "support/source_real_video_replay.rs"]
mod source_real_video_replay;
#[path = "support/source_rebuild_replay.rs"]
mod source_rebuild_replay;
#[path = "support/source_reconcile_replay.rs"]
mod source_reconcile_replay;
#[path = "support/source_reference_replay.rs"]
mod source_reference_replay;
#[path = "support/source_roster_replay.rs"]
mod source_roster_replay;
#[path = "support/source_rows_replay.rs"]
mod source_rows_replay;
#[path = "support/source_snapshot_replay.rs"]
mod source_snapshot_replay;
#[path = "support/source_state_compat_replay.rs"]
mod source_state_compat_replay;
#[path = "support/source_static_replay.rs"]
mod source_static_replay;
#[path = "support/source_status_replay.rs"]
mod source_status_replay;
#[path = "support/source_sync_order_replay.rs"]
mod source_sync_order_replay;
#[path = "support/source_tooling_replay.rs"]
mod source_tooling_replay;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[cfg(unix)]
#[test]
fn source_snapshot_refuses_linked_roots_before_git_or_scratch() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let scratch = root
        .join("logs/tmp")
        .join(format!("source root links {}", std::process::id()));
    std::fs::create_dir_all(scratch.parent().unwrap()).unwrap();
    std::fs::create_dir(&scratch).unwrap();
    let link = scratch.join("linked");
    std::os::unix::fs::symlink(root, &link).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let error = runtime
        .block_on(_native::source_git_inventory::snapshot(&link, BASELINE))
        .err()
        .expect("linked root must refuse");
    assert!(error.contains("linked/reparse"), "{error}");
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 1);
    std::fs::remove_file(link).unwrap();
    std::fs::remove_dir(scratch).unwrap();
}

#[test]
fn pinned_git_corpus_matches_every_original_source_hash_and_rejects_omission() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut environment = std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>();
    let unavailable = root.join("logs/tmp/not-a-source-repository");
    for key in [
        "GIT_DIR",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        environment.insert(key.into(), unavailable.as_os_str().to_owned());
    }
    environment.insert("GIT_WORK_TREE".into(), root.as_os_str().to_owned());
    environment.insert("GIT_CONFIG_COUNT".into(), "1".into());
    environment.insert("GIT_CONFIG_KEY_0".into(), "core.worktree".into());
    environment.insert(
        "GIT_CONFIG_VALUE_0".into(),
        unavailable.as_os_str().to_owned(),
    );
    let unchanged = environment.clone();
    let sources = runtime
        .block_on(_native::source_git_inventory::snapshot_with_environment(
            root,
            BASELINE,
            &environment,
        ))
        .unwrap();
    assert_eq!(
        environment, unchanged,
        "caller environment must not be mutated"
    );
    let inventory: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    assert_eq!(sources.len(), 277);
    _native::source_git_inventory::verify(&sources, &inventory).unwrap();
    let single = runtime
        .block_on(_native::source_git_inventory::read_source(
            root,
            BASELINE,
            "tests/_similarity_oracle.py",
        ))
        .unwrap();
    let original = sources
        .iter()
        .find(|source| source.path == single.path)
        .unwrap();
    assert_eq!(single.baseline_revision, original.baseline_revision);
    assert_eq!(single.sha256, original.sha256);
    assert_eq!(single.text, original.text);
    let selected = [
        "tests/_similarity_oracle.py",
        "tests/_reference_behavior.py",
    ];
    let batch = runtime
        .block_on(_native::source_git_inventory::read_sources(
            root, BASELINE, &selected,
        ))
        .unwrap();
    assert_eq!(batch.len(), selected.len());
    for source in batch {
        assert!(selected.contains(&source.path.as_str()));
        let original = sources
            .iter()
            .find(|original| original.path == source.path)
            .unwrap();
        assert_eq!(source.baseline_revision, original.baseline_revision);
        assert_eq!(source.sha256, original.sha256);
        assert_eq!(source.text, original.text);
    }
    assert!(
        runtime
            .block_on(_native::source_git_inventory::read_source(
                root,
                BASELINE,
                "tests/not-an-original-source.py",
            ))
            .is_err()
    );
    assert!(
        runtime
            .block_on(_native::source_git_inventory::read_sources(
                root,
                BASELINE,
                &[selected[0], "tests/not-an-original-source.py"],
            ))
            .is_err(),
        "a missing batch member must not return a partial selection"
    );
    let claims = _native::source_ci_claims::inspect(&sources).unwrap();
    let workflow = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    _native::source_ci_claims::check(&claims, &workflow).unwrap();
    let lock_order = _native::source_lock_order::inspect(&sources).unwrap();
    _native::source_lock_order::check(&lock_order).unwrap();
    let fakes = _native::source_fake_contracts::inspect(&sources).unwrap();
    _native::source_fake_contracts::check(&fakes).unwrap();
    let entry_points = _native::source_entry_points::inspect(&sources).unwrap();
    _native::source_entry_points::check(&entry_points).unwrap();
    assert_eq!(entry_points.modules.len(), 35);
    assert_eq!(entry_points.commands.len(), 26);
    assert_eq!(entry_points.support.len(), 8);
    assert!(!entry_points.commands.contains("codec-worker"));
    let coin_contracts = _native::source_coin_contracts::inspect(&sources).unwrap();
    _native::source_coin_contracts::check(&coin_contracts).unwrap();
    assert_eq!(coin_contracts.files.len(), 19);
    _native::source_media_contracts::check(&sources).unwrap();
    _native::source_publication_contracts::check(&sources).unwrap();
    // Every replay owns a separate worker and scratch directory. Keep running
    // independent cohorts after a refusal so one broken fixture cannot conceal
    // the remaining parity failures in CI; every panic still fails this target.
    let mut replay_failures = Vec::new();
    macro_rules! replay {
        ($module:ident) => {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                $module::check(root, &runtime, &sources)
            }))
            .is_err()
            {
                replay_failures.push(stringify!($module));
            }
        };
    }
    replay!(source_oracle_replay);
    replay!(source_panel_view_replay);
    replay!(source_snapshot_replay);
    replay!(source_identity_replay);
    replay!(source_bot_replay);
    replay!(source_reconcile_replay);
    replay!(source_publication_replay);
    replay!(source_preflight_replay);
    replay!(source_catalog_replay);
    replay!(source_reference_replay);
    replay!(source_real_video_replay);
    replay!(source_benchmark_replay);
    replay!(source_rows_replay);
    replay!(source_lottie_replay);
    replay!(source_static_replay);
    replay!(source_state_compat_replay);
    replay!(source_logging_replay);
    replay!(source_config_replay);
    replay!(source_sync_order_replay);
    replay!(source_archive_tests_replay);
    replay!(source_export_replay);
    replay!(source_status_replay);
    replay!(source_roster_replay);
    replay!(source_emoji_ids_replay);
    replay!(source_local_ingest_replay);
    replay!(source_coin_audit_replay);
    replay!(source_keywords_replay);
    replay!(source_http_replay);
    replay!(source_coin_cli_replay);
    replay!(source_logo_cache_replay);
    replay!(source_provider_image_replay);
    replay!(source_provider_replay);
    replay!(source_rebuild_replay);
    replay!(source_prebuilt_replay);
    replay!(source_tooling_replay);
    assert!(
        replay_failures.is_empty(),
        "Historical replay failures: {}",
        replay_failures.join(", ")
    );
    let discovered = _native::source_git_inventory::inventory(&sources).unwrap();
    assert_eq!(discovered["baseline_revision"], BASELINE);
    assert_eq!(discovered["files"].as_array().unwrap().len(), 277);
    let assertion_fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-assertions.json"
    ))
    .unwrap();
    assert_eq!(assertion_fixture["source_revision"], BASELINE);
    let actual_cases = assertion_fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| !case["source"].is_string())
        .collect::<Vec<_>>();
    assert_eq!(actual_cases.len(), 2);
    for case in actual_cases {
        let file = discovered["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["source"] == case["path"])
            .unwrap();
        assert_eq!(file["sha256"], case["sha256"]);
        assert_eq!(
            file["assertions"]["methods"], case["methods"],
            "{}",
            case["path"]
        );
        assert_eq!(file["assertions"]["runtime_variants_complete"], false);
    }
    // Static declarations cannot certify decorators, imported bases or load_tests.
    assert_eq!(discovered["discovery_complete"], false);
    let mut coverage: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/coverage.json")).unwrap();
    assert_eq!(coverage["cases"].as_array().unwrap().len(), 1489);
    let inherited_coverage: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coverage-inherited.json"
    ))
    .unwrap();
    assert_eq!(inherited_coverage["baseline_revision"], BASELINE);
    let inherited_rows = inherited_coverage["cases"].as_array().unwrap();
    assert_eq!(inherited_rows.len(), 26);
    assert!(inherited_rows.iter().all(|row| row["status"] == "pending"
        && row.get("evidence").is_none()
        && row["origin"].is_object()));
    let declaration_report =
        _native::source_migration_coverage::validate(&discovered, &coverage, false).unwrap();
    coverage["cases"]
        .as_array_mut()
        .unwrap()
        .extend(inherited_rows.iter().cloned());
    let report =
        _native::source_migration_coverage::validate(&discovered, &coverage, false).unwrap();
    assert_eq!(report["pending"].as_array().unwrap().len(), 1515);
    assert_eq!(report["verified"], 0);
    source_import_replay::check(root, &runtime, &sources, &discovered, &coverage);
    let targets = source_coverage::TARGETS
        .iter()
        .map(|&path| (path, std::fs::read_to_string(root.join(path)).unwrap()))
        .collect::<Vec<_>>();
    let baseline_hash =
        _native::tooling_manifest::digest(&root.join("tests/fixtures/native/source-baseline.json"))
            .unwrap();
    let audit_fixtures = [
        (
            "tests/fixtures/native/coin-audit.json",
            "74d21e13f82d61d2cd3f4b137633f869195b09196808d61312b1a46b58e9e9ec",
        ),
        (
            "tests/fixtures/native/coin-audit-image.json",
            "6f1fed3b033a76dc7bcd5a6c01520347c917ca34953016764b33d8c5fa2351a4",
        ),
    ];
    for (fixture, hash) in audit_fixtures {
        let text = std::fs::read_to_string(root.join(fixture))
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(
            Sha256::digest(text.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            hash
        );
    }
    assert_eq!(
        report["candidates"].as_u64().unwrap(),
        source_coverage::SOURCES
            .iter()
            .map(|(_, count)| *count as u64)
            .sum::<u64>(),
        "every candidate must belong to an inspected source cohort"
    );
    for &(source, count) in source_coverage::SOURCES {
        let cases = coverage["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["source_file"] == source && case.get("candidate").is_some())
            .filter(|case| {
                case["source_case"]
                    .as_str()
                    .is_some_and(|name| source_coverage::selected(source, name))
            })
            .collect::<Vec<_>>();
        assert_eq!(cases.len(), count, "{source}");
        let original = discovered["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["source"] == source)
            .unwrap();
        for case in cases {
            assert_eq!(case["status"], "pending");
            assert!(
                case.get("evidence").is_none(),
                "authored mapping must not claim an observed run"
            );
            let evidence = original["case_assertions"]["cases"]
                .as_array()
                .unwrap()
                .iter()
                .chain(
                    original["case_assertions"]["unresolved_declarations"]
                        .as_array()
                        .unwrap(),
                )
                .filter(|entry| entry["source_case"] == case["source_case"])
                .collect::<Vec<_>>();
            assert_eq!(
                evidence.len(),
                1,
                "candidate requires unique original body evidence: {}",
                case["source_case"]
            );
            let assertion = evidence[0];
            if assertion.get("evidence_kind").is_some() {
                assert_eq!(assertion["evidence_kind"], "declaration-only");
                assert_eq!(assertion["binding_resolved"], false);
                assert_eq!(assertion["defined_in_file"], source);
                assert_eq!(original["discovery"]["discovery_complete"], false);
                assert!(
                    case.get("origin").is_none(),
                    "declaration cannot stand in for inherited binding evidence"
                );
            }
            // These pinned original smoke cases assert success by returning
            // normally. Preserve their exact operation instead of inventing
            // assertion calls that never appeared in the source.
            let successful_call = match (source, case["source_case"].as_str().unwrap()) {
                (
                    "tests/test_resume_safety.py",
                    "StateShapeValidation.test_a_sound_state_passes",
                ) => Some("ps.validate_state_shape(self._ok(), base=\"t\", per_set=200)"),
                (
                    "tests/test_lottie_repaint.py",
                    "ATimelineMustBeFiniteNumbers.test_a_valid_timeline_still_passes",
                ) => Some("media.validate_tgs(_tgs(VALID, self.path))"),
                (
                    "tests/test_media.py",
                    "SubtractMasksAreRefusedBeforeTelegramRefusesThem.test_an_ADD_mask_is_still_allowed",
                ) => Some(
                    "media.validate_tgs(self._masked(\"a\", inside_precomp=True))\n        media.validate_tgs(self._masked(\"a\", inside_precomp=False))",
                ),
                ("tests/test_media.py", "TestVideo.test_compliant_webm_is_accepted") => {
                    Some("media.validate_video(self._lavfi(\"ok.webm\", *self.VP9, \"-an\"))")
                }
                ("tests/test_entry_points.py", "SuiteIsHermetic.test_loopback_still_works") => {
                    Some(
                        "with socket.create_connection(srv.getsockname(), timeout=5):\n                pass",
                    )
                }
                _ => None,
            };
            let validator_success = successful_call.is_some_and(|call| {
                sources.iter().any(|original| {
                    original.path == source
                        && original.sha256 == case["source_hash"].as_str().unwrap()
                        && original.text.contains(call)
                })
            });
            assert!(
                !assertion["method"]["assertions"]
                    .as_array()
                    .unwrap()
                    .is_empty()
                    || assertion["helper_assertions"]
                        .as_array()
                        .is_some_and(|helpers| {
                            helpers.iter().any(|helper| {
                                helper["defined_in_hash"] == case["source_hash"]
                                    && helper["defined_in_identity"]
                                        == assertion["defined_in_identity"]
                                    && helper["direct_assertions"].as_array().is_some_and(|rows| {
                                        !rows.is_empty()
                                            && rows.iter().all(|row| {
                                                helper["method"]["assertions"]
                                                    .as_array()
                                                    .is_some_and(|all| all.contains(row))
                                            })
                                    })
                                    && original["assertions"]["methods"]
                                        .as_array()
                                        .unwrap()
                                        .iter()
                                        .any(|method| method == &helper["method"])
                            })
                        })
                    || validator_success
                    || (source == "tests/test_pack_locks.py"
                        && !assertion["method"]["contexts"]
                            .as_array()
                            .unwrap()
                            .is_empty()),
                "original case {source}::{} must retain inline/helper assertions or acquisition contexts",
                case["source_case"]
            );
            assert_eq!(assertion["defined_in_hash"], case["source_hash"]);
            let candidate = &case["candidate"];
            let target = candidate["target_file"].as_str().unwrap();
            let target_case = candidate["target_case"].as_str().unwrap();
            let (_, body) = targets
                .iter()
                .find(|(path, _)| *path == target)
                .expect("candidate must name an inspected native test source");
            let function = target_case.rsplit("::").next().unwrap();
            assert!(body.contains(&format!("fn {function}(")), "{target_case}");
            let (fixture, hash) = if source == "tests/test_check_all_packs.py" {
                let image = matches!(
                    case["source_case"].as_str(),
                    Some(
                        "CheckAllPacksTest.test_blank_sticker_is_still_reported_with_live_position"
                            | "CheckAllPacksTest.test_blank_is_the_pipeline_wide_rule_not_a_local_copy"
                            | "CheckAllPacksListingRetry.test_each_set_boundary_checkpoints_the_audit"
                    )
                );
                audit_fixtures[usize::from(image)]
            } else if source == "tests/test_coin_recovery.py"
                && matches!(
                    case["source_case"].as_str(),
                    Some(
                        "UnverifiedUploadIsRecovered.test_a_re_downloaded_source_does_not_unmake_the_recovery"
                            | "UnverifiedUploadIsRecovered.test_recovery_never_promotes_an_image_it_cannot_prove"
                    )
                )
            {
                (
                    "tests/fixtures/native/static-codec.json",
                    "b179fa97ab139b8f1ae0a695838ac8f9fd65b906185d07f7b56b706ef6fdab1b",
                )
            } else if source == "tests/test_rebuild_dedup_plan.py"
                && case["source_case"]
                    == "TruncatedPlanFailsClosed.test_the_plan_and_the_report_are_written_atomically"
            {
                (
                    "tests/fixtures/native/coin-plan-disk.json",
                    "215c81c421dd8d3ecf2153cd2931de9a2a0bfcda61a6078f2ece0919c368415a",
                )
            } else if source == "tests/test_rebuild_dedup_plan.py"
                && case["source_case"]
                    == "RetryableFailureKeepsThePlanPosition.test_a_media_rejection_is_still_skipped_for_good"
            {
                (
                    "tests/fixtures/native/coin-rebuild-walk.json",
                    "6d6486918e9ab4dc7143e75f4732c1b4062709da6b61488bc937fb99ad595b21",
                )
            } else if source == "tests/test_contracts_shared.py" {
                let (fixture, hash) = match case["source_case"].as_str().unwrap() {
                    "ThePublishBodyIsWhatTheWorkerValidates.test_announce_via_worker_sends_the_fixture_body" => {
                        (
                            "tests/fixtures/contracts/publish_request.json",
                            "dbc24993190ed27e80c3ed55c22b311e3ba40d8775106603ae9b28087f8e4ae3",
                        )
                    }
                    "TheIdSyntaxIsSharedWithTheWorker.test_every_case" => (
                        "tests/fixtures/contracts/emoji_messages.json",
                        "25e3883b746f98dd366361536b327a143583d8fedde2a7003cbb9d50a1e3d7db",
                    ),
                    _ => panic!("unadmitted shared Worker contract"),
                };
                assert_eq!(
                    _native::tooling_manifest::digest(&root.join(fixture)).unwrap(),
                    hash
                );
                (fixture, hash)
            } else if let Some((fixture, hash)) =
                source_coverage::binding_fixture(source, case["source_case"].as_str().unwrap())
            {
                assert_eq!(
                    _native::tooling_manifest::digest(&root.join(fixture)).unwrap(),
                    hash
                );
                (fixture, hash)
            } else {
                (
                    "tests/fixtures/native/source-baseline.json",
                    baseline_hash.as_str(),
                )
            };
            assert_eq!(candidate["fixture"], fixture);
            assert_eq!(candidate["fixture_hash"], hash);
        }
    }
    let browser_map: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/browser-cases.json"
    ))
    .unwrap();
    assert_eq!(browser_map["baseline_revision"], BASELINE);
    assert_eq!(browser_map["status"], "authored-unverified");
    let mut browser_rows = 0;
    for source in browser_map["sources"].as_array().unwrap() {
        let original = coverage["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["source_file"] == source["file"])
            .collect::<Vec<_>>();
        let rows = source["cases"].as_array().unwrap();
        assert_eq!(rows.len(), original.len());
        let mut names = std::collections::BTreeSet::new();
        for row in rows {
            let name = row[0].as_str().unwrap();
            assert!(names.insert(name), "duplicate browser source identity");
            let case = original
                .iter()
                .find(|case| case["source_case"] == name)
                .unwrap();
            assert_eq!(case["source_hash"], source["sha256"]);
            assert!(case.get("candidate").is_some());
            assert!(!row[1].as_array().unwrap().is_empty());
            browser_rows += 1;
        }
    }
    assert_eq!(browser_rows, 97);
    for (path, name, actual) in [
        (
            "tests/_panel_browser_fixtures.py",
            "STUB",
            include_str!("fixtures/panel-browser-stub.js"),
        ),
        (
            "tests/test_panel_perf.py",
            "PROBE",
            include_str!("fixtures/panel-browser-perf.js"),
        ),
        (
            "tests/test_panel_perf.py",
            "REFERENCE",
            include_str!("fixtures/panel-browser-bulk-reference.js"),
        ),
    ] {
        let source = sources.iter().find(|source| source.path == path).unwrap();
        let marker = format!("{name} = \"\"\"");
        let start = source
            .text
            .find(&marker)
            .expect("pinned original JavaScript declaration")
            + marker.len();
        let end = source.text[start..]
            .find("\"\"\"")
            .expect("pinned JavaScript terminator")
            + start;
        assert_eq!(
            actual,
            &source.text[start..end],
            "browser JavaScript fixture drift: {name}"
        );
    }
    // Keep the declaration-only omission visible after adding explicitly pending inheritance.
    // No syntax candidate or authored replay becomes verified runtime coverage.
    let inherited =
        "TheCanonicalMapIsRereadUnderTheLock.test_the_live_count_is_sent_as_expected_before";
    let providers = discovered["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["source"] == "tests/test_coin_providers.py")
        .unwrap();
    assert!(
        providers["test_cases"]
            .as_array()
            .unwrap()
            .iter()
            .any(|case| case == inherited)
    );
    let identity = serde_json::json!(format!("tests/test_coin_providers.py::{inherited}"));
    assert!(
        declaration_report["unmapped"]
            .as_array()
            .unwrap()
            .contains(&identity)
    );
    assert!(report["pending"].as_array().unwrap().contains(&identity));
    assert!(!report["unmapped"].as_array().unwrap().contains(&identity));
    assert!(
        providers["discovery"]["case_origins"]
            .as_array()
            .unwrap()
            .iter()
            .any(|origin| {
                origin["binding"] == "TheCanonicalMapIsRereadUnderTheLock"
                    && origin["method"] == "test_the_live_count_is_sent_as_expected_before"
                    && origin["defined_in_identity"] == "tests/test_coin_providers.py:91"
            })
    );
    let inherited_assertions = providers["case_assertions"]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["source_case"] == inherited)
        .unwrap();
    assert_eq!(
        inherited_assertions["defined_in_identity"],
        "tests/test_coin_providers.py:91"
    );
    assert_eq!(inherited_assertions["defined_in_hash"], providers["sha256"]);
    let original_assertions = providers["assertions"]["methods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|method| {
            method["class"] == "VerifiedPublish"
                && method["method"] == "test_the_live_count_is_sent_as_expected_before"
        });
    assert!(
        original_assertions.is_some(),
        "original declaring method must remain in inventory"
    );
    assert_eq!(
        inherited_assertions["method"],
        *original_assertions.unwrap()
    );
    assert_eq!(
        providers["case_assertions"]["runtime_variants_complete"],
        false
    );
    assert!(report["candidates"].as_u64().unwrap() >= 9);
    assert_eq!(report["release_ready"], false);
    assert!(_native::source_migration_coverage::validate(&discovered, &coverage, true).is_err());
    let mut removed = inventory.clone();
    removed["files"].as_array_mut().unwrap().pop();
    assert!(_native::source_git_inventory::verify(&sources, &removed).is_err());
    let mut drift = inventory.clone();
    drift["files"][0]["sha256"] = serde_json::json!("0".repeat(64));
    assert!(_native::source_git_inventory::verify(&sources, &drift).is_err());
    for revision in ["HEAD", "main", "../outside", "a\ncommand", ""] {
        assert!(
            runtime
                .block_on(_native::source_git_inventory::snapshot(root, revision))
                .is_err()
        );
    }
}
