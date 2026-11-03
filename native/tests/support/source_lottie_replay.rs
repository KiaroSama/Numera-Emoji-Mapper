//! Fixed loaded and real-file validation replays share one Git corpus and bounded worker.
use super::source_oracle_evidence::refusal;
use _native::source_git_inventory::Source;
use serde_json::{Value, json};
use std::path::Path;

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, sources: &[Source]) {
    let source = sources
        .iter()
        .find(|source| source.path == "emojikit/media_lottie.py")
        .unwrap();
    let errors = sources
        .iter()
        .find(|source| source.path == "emojikit/errors.py")
        .unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/lottie-validation.json"
    ))
    .unwrap();
    let unavailable = root.join("logs/tmp/not-a-lottie-oracle-executable");
    for dependency in [false, true] {
        let original = if dependency { errors } else { source };
        for mutation in ["revision", "path", "hash", "bytes"] {
            let mut changed = Source {
                baseline_revision: original.baseline_revision.clone(),
                path: original.path.clone(),
                sha256: original.sha256.clone(),
                text: original.text.clone(),
            };
            match mutation {
                "revision" => changed.baseline_revision = "0".repeat(40),
                "path" => changed.path = "tests/unverified_lottie.py".into(),
                "hash" => changed.sha256 = "0".repeat(64),
                "bytes" => changed
                    .text
                    .push_str("\nraise AssertionError('must not execute')"),
                _ => unreachable!(),
            }
            let (source, errors) = if dependency {
                (source, &changed)
            } else {
                (&changed, errors)
            };
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::check_lottie(
                        root,
                        &unavailable,
                        source,
                        errors,
                        &fixture,
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
            assert!(
                _native::source_lottie_oracle::check(source, errors, &fixture)
                    .unwrap_err()
                    .contains("source identity drift")
            );
            let (preview, gallery) = _native::source_lottie_render::fixtures().unwrap();
            assert!(
                runtime
                    .block_on(_native::source_oracle_worker::generate_lottie_render(
                        root,
                        &unavailable,
                        &unavailable,
                        source,
                        errors,
                        &preview,
                        &gallery,
                    ))
                    .unwrap_err()
                    .contains("source identity drift")
            );
        }
    }
    let mut input_drift = fixture.clone();
    input_drift["cases"][0]["document"]["fr"] = json!(1);
    input_drift["cases"][0]["expected"] = json!("coordinated input/output must still refuse");
    assert!(
        runtime
            .block_on(_native::source_oracle_worker::check_lottie(
                root,
                &unavailable,
                source,
                errors,
                &input_drift,
            ))
            .unwrap_err()
            .contains("recipe drift")
    );
    let render_preview: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/lottie-preview.json"
    ))
    .unwrap();
    let render_gallery: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/gallery-codec.json"
    ))
    .unwrap();
    _native::source_lottie_render::fixture_identity(&render_preview, &render_gallery).unwrap();
    let mut drift = render_preview.clone();
    drift["cases"][0]["size"] = json!(104);
    drift["cases"][0]["expected"]["size"] = json!([104, 104]);
    let missing_workspace = root.join("logs/tmp/render admission must not create");
    assert!(!missing_workspace.exists());
    assert!(
        runtime
            .block_on(_native::source_oracle_worker::generate_lottie_render(
                root,
                &unavailable,
                &missing_workspace,
                source,
                errors,
                &drift,
                &render_gallery,
            ))
            .unwrap_err()
            .contains("recipe drift")
    );
    assert!(!missing_workspace.exists());
    let mut observation_drift = render_preview.clone();
    observation_drift["cases"][0]["expected"] = json!({"size":[1,1],"frames":[]});
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let render_workspace = parent.join(format!("Lottie source renders {}", std::process::id()));
    std::fs::create_dir(&render_workspace).unwrap();
    let executable = Path::new(env!("CARGO_BIN_EXE_numera-emoji"));
    let rendered = runtime
        .block_on(_native::source_oracle_worker::generate_lottie_render(
            root,
            executable,
            &render_workspace,
            source,
            errors,
            &observation_drift,
            &render_gallery,
        ))
        .unwrap();
    assert_eq!(rendered["cases_checked"], 5);
    assert_eq!(rendered["source_runtime"], "CPython3.11.9");
    assert_eq!(rendered["source_hash"], source.sha256);
    assert_eq!(rendered["dependency_source_hash"], errors.sha256);
    assert_eq!(rendered["supplied_ports"], json!([]));
    assert_eq!(rendered["platform_specific_bytes"], true);
    assert_eq!(
        rendered["preview"]["cases"][0]["expected"]["size"],
        json!([52, 52])
    );
    assert_eq!(rendered["preview"]["cases"].as_array().unwrap().len(), 3);
    for case in rendered["preview"]["cases"].as_array().unwrap() {
        assert!(case["expected"]["bytes"].as_u64().unwrap() > 0);
        assert_eq!(case["expected"]["webp_sha256"].as_str().unwrap().len(), 64);
        assert!(!case["expected"]["frames"].as_array().unwrap().is_empty());
    }
    for (actual, original) in rendered["gallery"]["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(render_gallery["cases"].as_array().unwrap())
    {
        if actual["format"] != "animated" {
            assert_eq!(actual, original);
        }
    }
    assert_eq!(std::fs::read_dir(&render_workspace).unwrap().count(), 0);
    std::fs::write(render_workspace.join("keep"), b"caller data").unwrap();
    assert!(
        _native::source_lottie_render::generate(
            root,
            &render_workspace,
            source,
            errors,
            &render_preview,
            &render_gallery,
        )
        .unwrap_err()
        .contains("empty")
    );
    assert_eq!(
        std::fs::read(render_workspace.join("keep")).unwrap(),
        b"caller data"
    );
    std::fs::remove_file(render_workspace.join("keep")).unwrap();
    std::fs::remove_dir(&render_workspace).unwrap();
    let cli_scratch = parent.join(format!("Lottie render command {}", std::process::id()));
    std::fs::create_dir(&cli_scratch).unwrap();
    let log = _native::logging::RunLog::open(&cli_scratch, "render-command-test", vec![]);
    let mut environment = std::collections::BTreeMap::new();
    for key in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "PYTHONHOME",
    ] {
        if let Ok(value) = std::env::var(key) {
            environment.insert(key.into(), value);
        }
    }
    environment.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
    environment.insert("PYTHONNOUSERSITE".into(), "1".into());
    environment.insert("PYTHONSAFEPATH".into(), "1".into());
    let environment =
        _native::tooling_environment::environment(&cli_scratch, &environment, false).unwrap();
    let occupied = cli_scratch.join("occupied");
    std::fs::create_dir(&occupied).unwrap();
    std::fs::write(occupied.join("keep"), b"caller data").unwrap();
    for (args, code, message) in [
        (vec!["--help".to_owned()], 0, "--output-dir"),
        (vec!["--unknown".to_owned()], 1, "requires --output-dir"),
        (
            vec![
                "--output-dir".to_owned(),
                occupied.to_string_lossy().into_owned(),
            ],
            1,
            "must be new",
        ),
    ] {
        let mut argv = vec![
            executable.as_os_str().to_owned(),
            "source-lottie-render".into(),
        ];
        argv.extend(args.into_iter().map(std::ffi::OsString::from));
        let actual = runtime
            .block_on(_native::tooling_process::capture(
                &cli_scratch,
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
    log.finish(0);
    drop(log);
    assert_eq!(
        std::fs::read_dir(cli_scratch.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    std::fs::remove_dir_all(&cli_scratch).unwrap();
    let report = runtime
        .block_on(_native::source_oracle_worker::check_lottie(
            root, executable, source, errors, &fixture,
        ))
        .unwrap();
    assert_eq!(report["cases_checked"], 84);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["source_hash"], source.sha256);
    assert_eq!(report["dependency_source_hash"], errors.sha256);
    assert_eq!(report["fixture_drift"], false);
    assert_eq!(report["supplied_ports"], json!(["stat", "magic", "load"]));
    assert_eq!(report["file_codec_coverage"], false);
    let files: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/lottie-file.json"
    ))
    .unwrap();
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", true).unwrap();
    let workspace = parent.join(format!("Lottie source files {}", std::process::id()));
    assert!(!workspace.exists());
    let mut drift = files.clone();
    drift["validations"][0]["input_sha256"] = json!("0".repeat(64));
    assert!(
        runtime
            .block_on(_native::source_oracle_worker::check_lottie_files(
                root,
                &unavailable,
                &workspace,
                source,
                errors,
                &drift,
            ))
            .unwrap_err()
            .contains("recipe drift")
    );
    assert!(
        !workspace.exists(),
        "admission must precede scratch creation"
    );
    std::fs::create_dir(&workspace).unwrap();
    let report = runtime
        .block_on(_native::source_oracle_worker::check_lottie_files(
            root, executable, &workspace, source, errors, &files,
        ))
        .unwrap();
    assert_eq!(report["cases_checked"], 7);
    assert_eq!(report["source_runtime"], "CPython3.11.9");
    assert_eq!(report["source_hash"], source.sha256);
    assert_eq!(report["dependency_source_hash"], errors.sha256);
    assert_eq!(report["fixture_drift"], false);
    assert_eq!(report["supplied_ports"], json!([]));
    assert_eq!(
        report["file_codec_coverage"],
        "seven fixed validation observations"
    );
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    let mut drift = files.clone();
    drift["validations"][0]["expected"] = json!(false);
    let evidence = refusal(
        root,
        "lottie-files",
        &json!({"workspace":workspace,"fixture":drift}),
        runtime.block_on(_native::source_oracle_worker::check_lottie_files(
            root, executable, &workspace, source, errors, &drift,
        )),
        "observation drift",
    );
    // All inputs are admitted first; an early mismatch must retain the later
    // decoded input, not erase the evidence just to leave scratch empty.
    let retained = workspace.join(files["validations"][6]["name"].as_str().unwrap());
    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 1);
    assert_eq!(
        _native::tooling_manifest::digest(&retained).unwrap(),
        files["validations"][6]["input_sha256"].as_str().unwrap()
    );
    evidence.clean();
    std::fs::remove_file(retained).unwrap();
    std::fs::write(workspace.join("keep"), b"caller data").unwrap();
    assert!(
        _native::source_lottie_oracle::check_files(root, &workspace, source, errors, &files)
            .unwrap_err()
            .contains("empty")
    );
    assert_eq!(
        std::fs::read(workspace.join("keep")).unwrap(),
        b"caller data"
    );
    std::fs::remove_file(workspace.join("keep")).unwrap();
    std::fs::remove_dir(&workspace).unwrap();
    for mutation in ["message", "kind"] {
        let mut drift = fixture.clone();
        if mutation == "message" {
            drift["cases"][0]["expected"] = json!("target-derived refusal");
        } else {
            drift["cases"][0]["source_error_kind"] = json!("TypeError");
        }
        refusal(
            root,
            "lottie-validation",
            &drift,
            runtime.block_on(_native::source_oracle_worker::check_lottie(
                root, executable, source, errors, &drift,
            )),
            "observation drift",
        )
        .clean();
    }
}
