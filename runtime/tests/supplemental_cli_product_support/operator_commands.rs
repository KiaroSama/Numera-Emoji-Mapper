//! Supplemental actual dry-run configuration admission matrix.
use crate::root::Root;
use numera_runtime as _native;
use serde_json::json;
use std::{collections::BTreeMap, time::Duration};
// Native supplemental: native/tests/operator_config.rs::actual_collection_dry_run_refuses_unknown_branding_before_catalog_writes_or_network
#[test]
fn actual_collection_dry_run_refuses_unknown_branding_before_catalog_writes_or_network() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("operator command café");
    std::fs::create_dir(root.0.join("assets")).unwrap();
    std::fs::write(root.0.join("assets/panel.html"), b"isolated fixture").unwrap();
    let executable = root.0.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let environment = crate::test_environment();
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
