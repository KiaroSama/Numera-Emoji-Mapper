//! Original operator settings through injected configuration and actual dry-run commands.
use _native::config::Config;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

fn config(root: &Path, pairs: &[(&str, &str)]) -> Config {
    let mut values = BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
    values.extend(
        pairs
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into())),
    );
    Config::load_with_environment(root.to_path_buf(), &values).unwrap()
}

#[test]
fn original_operator_settings_preserve_unknown_empty_logo_paths_and_keyword_defaults() {
    let root = Root::new("operator settings café");
    let unset = config(&root.0, &[("COIN_PACK_TITLE", "t")]);
    let error = unset.require("COIN_PACK_BASE").unwrap_err();
    assert!(error.contains("COIN_PACK_BASE"));
    assert!(!error.contains("COIN_PACK_TITLE"));
    assert!(
        unset
            .brand_logo(false, None)
            .unwrap_err()
            .contains("BRAND_LOGO_BOTS")
    );
    assert!(unset.logo_bots(false).unwrap().is_empty());
    assert_eq!(unset.brand_logo(true, None).unwrap(), (vec![], None));
    assert_eq!(unset.logo_keywords(), vec!["logo"]);
    let empty = config(&root.0, &[("BRAND_LOGO_BOTS", "")]);
    assert_eq!(empty.brand_logo(false, None).unwrap(), (vec![], None));
    let listed = config(&root.0, &[("BRAND_LOGO_BOTS", "YourEmojiBot")]);
    assert!(
        listed
            .brand_logo(false, None)
            .unwrap_err()
            .contains("BRAND_LOGO_PATH")
    );
    let missing = config(
        &root.0,
        &[
            ("BRAND_LOGO_BOTS", "YourEmojiBot"),
            ("BRAND_LOGO_PATH", "private/does-not-exist.png"),
        ],
    );
    assert!(missing.brand_logo(false, None).is_err());
    let logo = root.0.join("logo.png");
    std::fs::write(&logo, b"png").unwrap();
    let configured = config(
        &root.0,
        &[
            ("BRAND_LOGO_BOTS", "@YourEmojiBot, other_bot"),
            ("BRAND_LOGO_PATH", "logo.png"),
            ("BRAND_LOGO_KEYWORDS", "yourbrand, logo"),
        ],
    );
    assert_eq!(
        configured.logo_bots(true).unwrap(),
        vec!["other_bot", "youremojibot"]
    );
    assert_eq!(
        configured.brand_logo(false, None).unwrap(),
        (
            vec!["other_bot".into(), "youremojibot".into()],
            Some(logo.clone())
        )
    );
    assert_eq!(configured.logo_keywords(), vec!["yourbrand", "logo"]);
    assert_eq!(
        config(&root.0, &[("BRAND_LOGO_PATH", "private/brand-logo.png")])
            .logo_path(true)
            .unwrap(),
        Some(root.0.join("private/brand-logo.png"))
    );
    let whitespace = config(
        &root.0,
        &[
            (
                "BRAND_LOGO_BOTS",
                "\u{1c}@@YourEmojiBot, @YOURemojiBOT\u{1f}other_bot\u{1e}",
            ),
            (
                "BRAND_LOGO_KEYWORDS",
                "\u{1c}yourbrand\u{1f}, ,\u{1e}logo\u{1c}",
            ),
            ("COIN_PACK_BASE", "\u{1c} \u{1f}"),
        ],
    );
    assert!(whitespace.require("COIN_PACK_BASE").is_err());
    assert_eq!(
        whitespace.logo_bots(true).unwrap(),
        vec!["other_bot", "youremojibot"]
    );
    assert_eq!(whitespace.logo_keywords(), vec!["yourbrand", "logo"]);
    assert_eq!(
        config(&root.0, &[("BRAND_LOGO_KEYWORDS", ", \u{1c},")]).logo_keywords(),
        vec!["logo"]
    );
    let replacement = root.0.join("override.png");
    std::fs::write(&replacement, b"override").unwrap();
    assert_eq!(
        configured
            .brand_logo(false, Some("override.png"))
            .unwrap()
            .1,
        Some(replacement)
    );
    assert_eq!(
        missing
            .brand_logo(true, Some("missing-override.png"))
            .unwrap(),
        (vec![], None)
    );
    assert_eq!(std::fs::read(&logo).unwrap(), b"png");
    root.clean().unwrap();
}

#[test]
fn actual_collection_dry_run_refuses_unknown_branding_before_catalog_writes_or_network() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("operator command café");
    std::fs::create_dir(root.0.join("assets")).unwrap();
    std::fs::write(root.0.join("assets/panel.html"), b"isolated fixture").unwrap();
    std::fs::write(root.0.join("pyproject.toml"), b"# isolated fixture\n").unwrap();
    let executable = root.0.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let environment =
        _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
    let environment =
        _native::tooling_environment::environment(&root.0, &environment, false).unwrap();
    let log = _native::logging::RunLog::open(&root.0, "operator-test-owner", vec![]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for (name, bots, logo, disabled, expected) in [
        ("unset", None, None, false, Some("BRAND_LOGO_BOTS")),
        (
            "missing-path",
            Some("YourEmojiBot"),
            None,
            false,
            Some("BRAND_LOGO_PATH"),
        ),
        (
            "missing-file",
            Some("YourEmojiBot"),
            Some("absent.png"),
            false,
            Some("brand logo"),
        ),
        ("explicit-empty", Some(""), None, false, None),
        ("disabled", None, None, true, None),
    ] {
        let data = root.0.join(name);
        let database = data.join("catalog.db");
        drop(_native::catalog::Catalog::open(&database, "fixture", &root.0).unwrap());
        let snapshot = || {
            std::fs::read_dir(&data)
                .unwrap()
                .map(|entry| {
                    let entry = entry.unwrap();
                    (entry.file_name(), std::fs::read(entry.path()).unwrap())
                })
                .collect::<BTreeMap<_, _>>()
        };
        let before = snapshot();
        let mut environment = environment.clone();
        for key in ["BRAND_LOGO_BOTS", "BRAND_LOGO_PATH", "BRAND_LOGO_KEYWORDS"] {
            environment.remove(key);
        }
        if let Some(value) = bots {
            environment.insert("BRAND_LOGO_BOTS".into(), value.into());
        }
        if let Some(value) = logo {
            environment.insert("BRAND_LOGO_PATH".into(), value.into());
        }
        let mut args = vec![
            executable.as_os_str().to_owned(),
            "build-collection".into(),
            "--base".into(),
            "fixture".into(),
            "--title".into(),
            "Fixture".into(),
            "--data-dir".into(),
            data.as_os_str().to_owned(),
            "--formats".into(),
            "static".into(),
            "--dry-run".into(),
        ];
        if disabled {
            args.push("--no-brand-logo".into());
        }
        let completed = runtime
            .block_on(_native::tooling_process::capture(
                &root.0,
                &args,
                &environment,
                Duration::from_secs(5),
                Duration::from_secs(3),
                &log,
            ))
            .unwrap();
        if let Some(key) = expected {
            assert_eq!(completed.code, 2, "{name}: {}", completed.stderr);
            assert!(
                completed.stderr.contains(key),
                "{name}: {}",
                completed.stderr
            );
            assert_eq!(snapshot(), before);
        } else {
            assert_eq!(completed.code, 0, "{name}: {}", completed.stderr);
            assert!(completed.stdout.contains("DRY RUN: nothing uploaded."));
            assert!(completed.stdout.contains("0 emoji"));
            let mut catalog =
                _native::catalog::Catalog::open(&data.join("catalog.db"), "fixture", &root.0)
                    .unwrap();
            assert_eq!(
                catalog
                    .dispatch(json!({"operation":"published_keys"}))
                    .unwrap(),
                json!([])
            );
        }
        assert!(!data.join("publish_fixture.json").exists());
        assert!(!data.join("publish_plan_fixture.json").exists());
        assert!(!data.join("tmp").exists());
    }
    assert!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
    runtime.shutdown_timeout(Duration::from_secs(1));
    drop(log);
    root.clean().unwrap();
}

#[test]
fn original_operator_sources_refuse_before_accessing_runtime_or_workspace() {
    use _native::source_git_inventory::Source;
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    let sources = [
        "emojikit/operator_config.py",
        "tests/test_operator_config.py",
        "emojikit/errors.py",
    ]
    .map(|path| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: fixture["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["source"] == path)
            .unwrap()["sha256"]
            .as_str()
            .unwrap()
            .into(),
        text: "raise AssertionError('must not execute')".into(),
    });
    let absent = PathBuf::from("must-not-be-inspected");
    assert!(
        _native::source_operator_oracle::observe(
            &absent,
            &absent,
            &sources.iter().collect::<Vec<_>>()
        )
        .unwrap_err()
        .contains("source identity drift")
    );
    assert!(_native::source_operator_oracle::source_identity(&[]).is_err());
}
