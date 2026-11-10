//! Reciprocal native writer entrypoints exclude migration while the existing child owns them.
use super::{barrier, held, project, retained, runtime};
use crate::{catalog::Catalog, config::Config, logging::RunLog, media_adapter::MediaAdapter};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) fn run(data: &Path, action: &str) {
    let root = data.parent().unwrap();
    let config = Config::load_with_environment(
        root.into(),
        &BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("BRAND_LOGO_BOTS".into(), String::new()),
            ("GENERAL_BOT_TOKEN".into(), "test-only-token".into()),
            ("COLLECTION_PACK_BASE".into(), "newfamily".into()),
            ("EMOJI_ARCHIVE_DIR".into(), "archive".into()),
        ]),
    )
    .unwrap();
    let log = Arc::new(RunLog::open(root, "held-writer", Vec::new()));
    match action {
        "hold-catalog" => {
            let _catalog = Catalog::open(&data.join("catalog.db"), "fixture", root).unwrap();
            barrier("writer-catalog");
        }
        "hold-publisher" => {
            let args = [
                "--base",
                "newfamily",
                "--title",
                "Fixture",
                "--dry-run",
                "--no-brand-logo",
                "--data-dir",
                data.to_str().unwrap(),
            ]
            .map(str::to_owned);
            crate::publish_cli::run(&config, &args, &log).unwrap();
        }
        "hold-archive" => {
            let tg = crate::telegram::Telegram::new(
                "test-only-token".into(),
                "http://127.0.0.1:9".into(),
            )
            .unwrap();
            crate::archive_sync::run(&config, &tg, &log).unwrap();
        }
        "hold-ingest" => {
            let args = [
                data.join("incoming.png").to_str().unwrap().to_owned(),
                "--data-dir".into(),
                data.to_str().unwrap().into(),
            ];
            crate::local_cli::run_with_codec(&config, &args, &log, |scratch| {
                Ok(MediaAdapter {
                    python: "must-not-decode-before-writer-barrier".into(),
                    root: root.into(),
                    scratch,
                })
            })
            .unwrap();
        }
        "hold-panel" => {
            let runtime = runtime();
            let panel = crate::panel_http::Panel {
                root: root.into(),
                db: data.join("catalog.db"),
                token: "writer-fixture".into(),
                options: json!({"show_published":false,"keep_sets":{},"branded":false}),
                branding: None,
                detected_bot: Mutex::new(String::new()),
                view: Mutex::new(json!({"view":[],"by_key":{},"hidden":0})),
                session: json!({}),
                utc: || "fixture".into(),
                log: log.clone(),
                events: Mutex::new(crate::panel_events::Events::default()),
                previews: crate::panel_preview::Previews::new(
                    crate::media_native_adapter::NativeMediaAdapter {
                        executable: "must-not-decode-held-panel".into(),
                        root: root.into(),
                        scratch: data.join("panel-codec"),
                    },
                    data,
                    1,
                ),
            };
            runtime.block_on(async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let server = tokio::spawn(async move {
                    axum::serve(listener, crate::panel_http::router(Arc::new(panel))).await
                });
                let client = reqwest::Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .retry(reqwest::retry::never())
                    .timeout(Duration::from_secs(25))
                    .build()
                    .unwrap();
                let result = client
                    .post(format!("http://{address}/api/save"))
                    .header("x-panel-token", "writer-fixture")
                    .json(&json!({"known":[],"excluded":[]}))
                    .send()
                    .await;
                server.abort();
                let _ = server.await;
                panic!("held panel returned without owner termination: {result:?}");
            });
        }
        other => panic!("unknown writer action: {other}"),
    }
    panic!("writer entrypoint returned without reaching its owned barrier: {action}");
}

pub(super) fn check(parent: &Path) {
    std::fs::create_dir(parent).unwrap();
    let source = project().join("tests/fixtures/native/static-codec/opaque.png");
    assert_eq!(
        crate::migration_files::digest(&source).unwrap(),
        "186ee151bdd1301c0e6c82e4e18550d248d4be36b8a0ae4c37ec6b91d44622f4"
    );
    let bytes = std::fs::read(source).unwrap();
    let runtime = runtime();
    for kind in ["catalog", "publisher", "archive", "ingest", "panel"] {
        let root = parent.join(kind);
        let data = root.join("collection");
        std::fs::create_dir_all(&data).unwrap();
        drop(Catalog::open(&data.join("catalog.db"), "fixture", &root).unwrap());
        let incoming = data.join("incoming.png");
        std::fs::write(&incoming, &bytes).unwrap();
        let config = Config::load_with_environment(
            root.clone(),
            &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
        )
        .unwrap();
        let media = MediaAdapter {
            python: "must-not-survey-an-owned-catalog".into(),
            root: root.clone(),
            scratch: data.join("refused-codec"),
        };
        let before = retained(&data);
        let mut child = held(&data, &format!("hold-{kind}"), &format!("writer-{kind}"));
        assert_eq!(retained(&data), before);
        let error =
            crate::migration_apply::apply(&config, &data, &media, &runtime, None).unwrap_err();
        assert!(
            error.contains("catalog ownership refused"),
            "{kind}: {error}"
        );
        assert_eq!(retained(&data), before);
        assert_eq!(std::fs::read(&incoming).unwrap(), bytes);
        assert!(!media.scratch.exists());
        assert!(!data.join("identity-migration.journal.json").exists());
        child.finish().unwrap();
        assert!(!data.join("worker").exists());
        drop(Catalog::open(&data.join("catalog.db"), "after child", &root).unwrap());
        drop(crate::migration_apply::locks(&data, &root).unwrap());
        assert_eq!(retained(&data), before);
        assert_eq!(std::fs::read(&incoming).unwrap(), bytes);
    }
    runtime.shutdown_timeout(Duration::from_secs(1));
}
