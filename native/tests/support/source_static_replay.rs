//! Fixed static/provider recipes share the admitted Git corpus and existing bounded worker.
use _native::source_git_inventory::Source;
use serde_json::json;
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, sources: &[Source]) {
    let media = sources
        .iter()
        .find(|source| source.path == "emojikit/media.py")
        .unwrap();
    let provider = sources
        .iter()
        .find(|source| source.path == "coins/_paprika_api.py")
        .unwrap();
    let (static_fixture, provider_fixture) = _native::source_static_oracle::fixtures().unwrap();
    let unavailable = root.join("logs/tmp/not-a-static-oracle-executable");
    for dependency in [false, true] {
        let original = if dependency { provider } else { media };
        for mutation in ["revision", "path", "hash", "bytes"] {
            let mut changed = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "tests/unverified_static.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let pair = if dependency {
                (media, &changed)
            } else {
                (&changed, provider)
            };
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::generate_static(
                        root,
                        &unavailable,
                        &unavailable,
                        pair,
                        (&static_fixture, &provider_fixture),
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
        }
    }
    let missing = root.join("logs/tmp/static admission must not create");
    assert!(!missing.exists());
    let mut recipe_drift = static_fixture.clone();
    recipe_drift["cases"][0]["input_sha256"] = json!("0".repeat(64));
    recipe_drift["cases"][0]["expected"] = json!({"size":[1,1]});
    assert!(
        runtime
            .block_on(_native::source_oracle_worker::generate_static(
                root,
                &unavailable,
                &missing,
                (media, provider),
                (&recipe_drift, &provider_fixture),
            ))
            .unwrap_err()
            .contains("recipe drift")
    );
    assert!(!missing.exists());
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let workspace = parent.join(format!("static source observations {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    // Caller observations do not become the acceptance oracle for size, pixels or refusals.
    let mut observations = static_fixture.clone();
    observations["cases"][0]["expected"] = json!({"size":[1,1]});
    let mut provider_observations = provider_fixture.clone();
    provider_observations["cases"][0]["expected"] = json!({"accepted":true});
    let report = runtime
        .block_on(_native::source_oracle_worker::generate_static(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            (media, provider),
            (&observations, &provider_observations),
        ))
        .unwrap();
    assert_eq!(report["cases_checked"], 12);
    assert_eq!(report["source_hash"], media.sha256);
    assert_eq!(report["dependency_source_hash"], provider.sha256);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["supplied_ports"], json!(["MediaError=RuntimeError"]));
    assert_eq!(report["platform_specific_bytes"], true);
    for (key, original) in [("static", &static_fixture), ("provider", &provider_fixture)] {
        let actual = &report[key];
        assert_eq!(
            actual["cases"].as_array().unwrap().len(),
            original["cases"].as_array().unwrap().len()
        );
        _native::source_static_oracle::fixture_identity(
            if key == "static" {
                actual
            } else {
                &static_fixture
            },
            if key == "provider" {
                actual
            } else {
                &provider_fixture
            },
        )
        .unwrap();
        for (case, expected) in actual["cases"]
            .as_array()
            .unwrap()
            .iter()
            .zip(original["cases"].as_array().unwrap())
        {
            assert_eq!(case["error"], expected["error"]);
            for field in ["accepted", "size", "mode", "rgba_sha256"] {
                assert_eq!(
                    case["expected"][field], expected["expected"][field],
                    "{} {field}",
                    case["name"]
                );
            }
            if case["expected"].get("size").is_some() {
                assert_eq!(case["expected"]["png_sha256"].as_str().unwrap().len(), 64);
                if key == "static" {
                    assert!(case["expected"]["bytes"].as_u64().unwrap() > 0);
                }
            }
        }
    }
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    std::fs::write(workspace.join("keep"), b"caller data").unwrap();
    assert!(
        _native::source_static_oracle::generate(
            root,
            &workspace,
            media,
            provider,
            &static_fixture,
            &provider_fixture,
        )
        .unwrap_err()
        .contains("empty")
    );
    assert_eq!(
        std::fs::read(workspace.join("keep")).unwrap(),
        b"caller data"
    );
    std::fs::remove_file(workspace.join("keep")).unwrap();
    std::fs::remove_dir(&workspace).unwrap();

    let scratch = parent.join(format!("static source command {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let log = _native::logging::RunLog::open(&scratch, "static-source-command", vec![]);
    let environment =
        _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    let environment =
        _native::tooling_environment::environment(&scratch, &environment, false).unwrap();
    let occupied = scratch.join("occupied");
    std::fs::create_dir(&occupied).unwrap();
    std::fs::write(occupied.join("keep"), b"caller data").unwrap();
    for (args, code, message) in [
        (vec!["--help".to_owned()], 0, "--output-dir"),
        (vec!["--unknown".to_owned()], 1, "requires --output-dir"),
        (
            vec![
                "--output-dir".to_owned(),
                occupied.to_str().unwrap().to_owned(),
            ],
            1,
            "must be new",
        ),
    ] {
        let mut argv = vec![
            std::ffi::OsString::from(env!("CARGO_BIN_EXE_numera-emoji")),
            "source-static-images".into(),
        ];
        argv.extend(args.into_iter().map(std::ffi::OsString::from));
        let actual = runtime
            .block_on(_native::tooling_process::capture(
                &scratch,
                &argv,
                &environment,
                std::time::Duration::from_secs(8),
                std::time::Duration::from_secs(4),
                &log,
            ))
            .unwrap();
        assert_eq!(actual.code, code, "{}", actual.stderr);
        assert!(format!("{}{}", actual.stdout, actual.stderr).contains(message));
        assert_eq!(
            std::fs::read(occupied.join("keep")).unwrap(),
            b"caller data"
        );
    }
    drop(log);
    assert_eq!(
        std::fs::read_dir(scratch.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    std::fs::remove_dir_all(scratch).unwrap();
    check_fitting(root, runtime, sources);
    check_brand(root, runtime, sources);
    check_reencode(root, runtime, sources);
}

fn check_fitting(root: &Path, runtime: &tokio::runtime::Runtime, sources: &[Source]) {
    let media = sources
        .iter()
        .find(|source| source.path == "emojikit/media.py")
        .unwrap();
    let test = sources
        .iter()
        .find(|source| source.path == "tests/test_media_fitting.py")
        .unwrap();
    let unavailable = root.join("logs/tmp/fitting admission must not create");
    assert!(!unavailable.exists());
    for index in 0..2 {
        for mutation in ["revision", "path", "hash", "bytes"] {
            let original = [media, test][index];
            let mut changed = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "tests/unverified_fitting.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let (media, test) = if index == 0 {
                (&changed, test)
            } else {
                (media, &changed)
            };
            assert!(
                _native::source_fitting_oracle::observe(&unavailable, &unavailable, media, test)
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::observe_fitting(
                        &unavailable,
                        &unavailable,
                        &unavailable,
                        media,
                        test,
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!unavailable.exists());
        }
    }
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let workspace = parent.join(format!(
        "fitting source observations {}",
        std::process::id()
    ));
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("keep"), b"caller data").unwrap();
    assert!(
        _native::source_fitting_oracle::observe(root, &workspace, media, test)
            .unwrap_err()
            .contains("empty")
    );
    assert_eq!(
        std::fs::read(workspace.join("keep")).unwrap(),
        b"caller data"
    );
    std::fs::remove_file(workspace.join("keep")).unwrap();
    assert!(_native::source_fitting_oracle::observe(root, root, media, test).is_err());
    #[cfg(unix)]
    {
        let linked = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &linked).unwrap();
        assert!(_native::source_fitting_oracle::observe(root, &linked, media, test).is_err());
        std::fs::remove_file(linked).unwrap();
    }
    let report = runtime
        .block_on(_native::source_oracle_worker::observe_fitting(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            media,
            test,
        ))
        .unwrap();
    assert_eq!(
        report["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["Pillow"], "12.3.0");
    assert_eq!(report["source_hashes"]["emojikit/media.py"], media.sha256);
    assert_eq!(
        report["source_hashes"]["tests/test_media_fitting.py"],
        test.sha256
    );
    assert_eq!(report["tests_run"], 7);
    assert_eq!(
        report["cases"],
        json!([
            "FittingPreservesOpacity.test_a_partly_transparent_pixel_keeps_its_colour_and_its_alpha",
            "FittingPreservesOpacity.test_opaque_art_is_the_control_that_always_passed",
            "FittingPreservesOpacity.test_a_resized_image_keeps_its_opacity",
            "FittingPreservesOpacity.test_a_cutout_stays_a_cutout",
            "FittingPreservesOpacity.test_a_transparent_border_is_trimmed_not_composited",
            "FittingPreservesOpacity.test_a_partial_alpha_gradient_survives_end_to_end",
            "CompositingOntoAnOpaqueBackdropIsStillCorrect.test_flattening_onto_white_still_blends"
        ])
    );
    for field in ["errors", "failures", "skipped"] {
        assert_eq!(report[field], 0);
    }
    assert_eq!(
        report["excluded"],
        json!(["FittingPreservesOpacity.test_every_fitting_entry_point_shares_one_implementation"])
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    std::fs::remove_dir(workspace).unwrap();
}

fn check_brand(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let sources = [
        "emojikit/media.py",
        "tests/reference/collection_state.py",
        "tests/test_brand_logo.py",
        "tests/_media_fixtures.py",
        "emojikit/errors.py",
    ]
    .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    let unavailable = root.join("logs/tmp/brand admission must not create");
    assert!(!unavailable.exists());
    assert!(
        _native::source_brand_oracle::source_identity(&sources[..4])
            .unwrap_err()
            .contains("exactly five")
    );
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
                "path" => changed.path = "tests/unverified_brand.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let mut forged = sources;
            forged[index] = &changed;
            assert!(
                _native::source_brand_oracle::observe(&unavailable, &unavailable, &forged)
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::observe_brand(
                        &unavailable,
                        &unavailable,
                        &unavailable,
                        &forged,
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!unavailable.exists());
        }
    }
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let workspace = parent.join(format!("brand source observations {}", std::process::id()));
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("keep"), b"caller data").unwrap();
    assert!(
        _native::source_brand_oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("empty")
    );
    assert_eq!(
        std::fs::read(workspace.join("keep")).unwrap(),
        b"caller data"
    );
    std::fs::remove_file(workspace.join("keep")).unwrap();
    assert!(_native::source_brand_oracle::observe(root, root, &sources).is_err());
    #[cfg(unix)]
    {
        let linked = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &linked).unwrap();
        assert!(_native::source_brand_oracle::observe(root, &linked, &sources).is_err());
        std::fs::remove_file(linked).unwrap();
    }
    let report = runtime
        .block_on(_native::source_oracle_worker::observe_brand(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(report["source_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["Pillow"], "12.3.0");
    for source in sources {
        assert_eq!(report["source_hashes"][&source.path], source.sha256);
    }
    assert_eq!(report["tests_run"], 6);
    assert_eq!(
        report["cases"],
        json!([
            "BrandLogoPrepare.test_static_logo_is_100x100_png",
            "BrandLogoPrepare.test_static_png_is_cached",
            "BrandLogoPrepare.test_missing_source_returns_none",
            "BrandLogoPrepare.test_a_corrupt_logo_raises",
            "BrandLogoPrepare.test_a_blank_logo_raises",
            "BrandLogoPrepare.test_a_truncated_cache_is_rendered_again"
        ])
    );
    for field in ["errors", "failures", "skipped"] {
        assert_eq!(report[field], 0);
    }
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    std::fs::remove_dir(workspace).unwrap();
}

fn check_reencode(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    use _native::source_reencode_tests as oracle;
    let sources = oracle::source_paths()
        .map(|path| corpus.iter().find(|source| source.path == path).unwrap());
    let unavailable = root.join("logs/tmp/reencode admission must not create");
    assert!(!unavailable.exists());
    assert!(oracle::source_identity(&sources[..5]).is_err());
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
                "path" => changed.path = "tests/unverified_reencode.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let mut admitted = sources;
            admitted[index] = &changed;
            assert!(
                runtime
                    .block_on(oracle::observe_owned(
                        &unavailable,
                        &unavailable,
                        &unavailable,
                        &admitted,
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(!unavailable.exists());
        }
    }
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let workspace = parent.join(format!(
        "original reencoding assertions {}",
        std::process::id()
    ));
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("keep"), b"caller bytes").unwrap();
    assert!(
        oracle::observe(root, &workspace, &sources)
            .unwrap_err()
            .contains("must be empty")
    );
    assert_eq!(
        std::fs::read(workspace.join("keep")).unwrap(),
        b"caller bytes"
    );
    assert!(oracle::observe(root, root, &sources).is_err());
    std::fs::remove_file(workspace.join("keep")).unwrap();
    #[cfg(unix)]
    {
        let linked = workspace.join("linked");
        std::os::unix::fs::symlink(&workspace, &linked).unwrap();
        assert!(oracle::observe(root, &linked, &sources).is_err());
        std::fs::remove_file(linked).unwrap();
    }
    let report = runtime
        .block_on(oracle::observe_owned(
            root,
            Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
            &workspace,
            &sources,
        ))
        .unwrap();
    assert_eq!(report["source_revision"], sources[0].baseline_revision);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["Pillow"], "12.3.0");
    assert_eq!(report["tests_run"], 21);
    assert_eq!(report["media_tests_run"], 42);
    assert_eq!(report["repaint_tests_run"], 12);
    assert_eq!(report["gate_tests_run"], 11);
    assert_eq!(report["temporary_calls"], 68);
    assert_eq!(report["preview_tests_run"], 1);
    assert_eq!(report["preview_requests"], 12);
    assert_eq!(
        report["preview_cases"],
        json!(["NativePreviewCodec.test_static_and_lottie_preview_bytes_match_source"])
    );
    assert_eq!(report["processes"]["preview_encodes"], 1);
    assert_eq!(report["processes"]["previews"], 4);
    assert_eq!(report["processes"]["encodes"], 4);
    assert_eq!(report["processes"]["copies"], 3);
    assert_eq!(report["processes"]["conversions"], 7);
    assert_eq!(report["processes"]["lavfi"], 6);
    assert_eq!(report["processes"]["alpha_frames"], 1);
    assert_eq!(
        report["processes"]["completed"],
        report["processes"]["total"]
    );
    assert!((32..=128).contains(&report["processes"]["total"].as_u64().unwrap()));
    for field in [
        "errors",
        "failures",
        "skipped",
        "expected_failures",
        "unexpected_successes",
    ] {
        assert_eq!(report[field], 0);
    }
    for source in sources {
        assert_eq!(report["source_hashes"][&source.path], source.sha256);
    }
    let inventory: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/native/coverage.json")).unwrap();
    let expected = inventory["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| {
            row["source_file"] == "tests/test_media_reencode.py"
                && [
                    "TestReencodeGivesUsOurOwnBytes.",
                    "SameImageSurvivesAReEncode.",
                    "AnUnderDeclaredVideoKeepsItsOriginalBytes.",
                    "TestSingleFrameVideoKeepsAContentKey.",
                ]
                .iter()
                .any(|prefix| row["source_case"].as_str().unwrap().starts_with(prefix))
        })
        .map(|row| row["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let actual = report["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case.as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 21);
    assert_eq!(report["excluded_classes"], json!([]));
    let media_cases = report["media_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case.as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let excluded = report["excluded_media_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case.as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(media_cases.len(), 42);
    assert!(excluded.is_empty());
    assert!(media_cases.is_disjoint(&excluded));
    let original_media = inventory["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["source_file"] == "tests/test_media.py")
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        media_cases
            .union(&excluded)
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        original_media
    );
    let repaint_cases = report["repaint_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case.as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let excluded = report["excluded_repaint_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case.as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(repaint_cases.len(), 12);
    assert_eq!(
        excluded,
        std::collections::BTreeSet::from([
            "TheRepaintApiLivesInRepaint.test_the_public_names_live_in_repaint_only"
        ])
    );
    assert!(repaint_cases.is_disjoint(&excluded));
    let original_repaint = inventory["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["source_file"] == "tests/test_lottie_repaint.py")
        .map(|case| case["source_case"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        repaint_cases
            .union(&excluded)
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        original_repaint
    );
    let gate_cases = report["gate_cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case.as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let original_gate = inventory["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["source_file"] == "tests/test_repaintable.py")
        .map(|row| row["source_case"].as_str().unwrap())
        .filter(|case| {
            case.starts_with("TheFlagIsOnTheStickerNotTheSet.")
                || case.starts_with("TheGateAsksBeforeIngesting.")
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(gate_cases.len(), 11);
    assert_eq!(gate_cases, original_gate);
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    assert!(!_native::process_owner::cleanup_pending(&workspace));
    std::fs::remove_dir(workspace).unwrap();
}
