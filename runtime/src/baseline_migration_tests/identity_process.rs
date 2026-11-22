use super::identity_support::*;
use crate::{
    config::Config, identity_cli, logging::RunLog, media_adapter::MediaCodec,
    process_owner::OwnedProcess,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
#[derive(Clone)]
struct LateDecoder {
    data: PathBuf,
    media: Fake,
}
impl MediaCodec for LateDecoder {
    fn with_scratch(&self, _: PathBuf) -> Self {
        self.clone()
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        if crate::migration_apply::journal(&self.data)?.is_some_and(|doc| doc["stage"] == "files") {
            Err("decoder lost".into())
        } else {
            self.media.call(request).await
        }
    }
}
pub(super) fn cli(f: &Fixture, media: &impl MediaCodec, args: &[&str]) -> u8 {
    let log = RunLog::open(&f.root.0, "baseline-identity-cli", Vec::new());
    let mut argv = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    argv.extend(["--data-dir".into(), f.data.to_str().unwrap().into()]);
    // Borrowing the supplied codec keeps construction lazy and exercises actual CLI dispatch.
    struct Borrowed<'a, M>(&'a M);
    impl<M: MediaCodec> MediaCodec for Borrowed<'_, M> {
        fn with_scratch(&self, _: PathBuf) -> Self {
            Self(self.0)
        }
        async fn call(&self, request: Value) -> Result<Value, String> {
            self.0.call(request).await
        }
    }
    identity_cli::run_with_codec_factory(&f.config, &argv, &log, |_, _| Ok(Borrowed(media)))
        .unwrap()
}
#[test]
fn identity_worker() {
    let Some(raw) = std::env::var_os("NUMERA_BASELINE_IDENTITY_DATA") else {
        return;
    };
    let data = PathBuf::from(raw);
    let root = data.parent().unwrap();
    let action = std::env::var("NUMERA_BASELINE_IDENTITY_ACTION").unwrap();
    if action.starts_with("hold-") {
        writer(&data, &action);
        return;
    }
    let fake = Fake::load(&root.join("fixture-codec.json"));
    let config = Config::load_with_environment(
        root.into(),
        &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
    )
    .unwrap();
    let log = RunLog::open(root, "baseline-worker", Vec::new());
    let args = [
        "migrate-video-keys".into(),
        "--apply".into(),
        "--data-dir".into(),
        data.to_str().unwrap().into(),
    ];
    let code = if action == "late-decode" {
        identity_cli::run_with_codec_factory(&config, &args, &log, |_, _| {
            Ok(LateDecoder {
                data: data.clone(),
                media: fake,
            })
        })
        .unwrap()
    } else {
        identity_cli::run_with_codec_factory(&config, &args, &log, |_, _| Ok(fake)).unwrap()
    };
    crate::atomic::write_json(&root.join("worker-result.json"), &json!({"code":code})).unwrap();
}
fn spawn(
    f: &Fixture,
    media: &Fake,
    action: &str,
    stage: Option<&str>,
    release: bool,
) -> (OwnedProcess, PathBuf) {
    media.save(&f.root.0.join("fixture-codec.json"));
    let scratch = f.data.join("identity-worker");
    std::fs::create_dir(&scratch).unwrap();
    let ready = scratch.join("ready.json");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "migration_crash_tests::identity_process::identity_worker",
            "--nocapture",
        ])
        .env("NUMERA_BASELINE_IDENTITY_DATA", &f.data)
        .env("NUMERA_BASELINE_IDENTITY_ACTION", action)
        .env("NUMERA_MIGRATION_TEST_READY", &ready)
        .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
        .env("TEMP", &scratch)
        .env("TMP", &scratch)
        .env("TMPDIR", &scratch)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(scratch.join("stderr.txt")).unwrap());
    if let Some(stage) = stage {
        command.env("NUMERA_MIGRATION_TEST_STAGE", stage);
    } else {
        command.env_remove("NUMERA_MIGRATION_TEST_STAGE");
    }
    if release {
        command.env("NUMERA_BASELINE_RELEASE", f.root.0.join("writer-release"));
    } else {
        command.env_remove("NUMERA_BASELINE_RELEASE");
    }
    if action == "refuse-backup" {
        command.env("NUMERA_MIGRATION_TEST_ACTION", "refuse-backup");
    } else {
        command.env_remove("NUMERA_MIGRATION_TEST_ACTION");
    }
    for (name, _) in std::env::vars() {
        if ["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
            .iter()
            .any(|part| name.to_uppercase().contains(part))
        {
            command.env_remove(name);
        }
    }
    (
        OwnedProcess::spawn_session(&mut command, scratch).unwrap(),
        ready,
    )
}
pub(super) fn held(
    f: &Fixture,
    media: &Fake,
    action: &str,
    stage: &str,
    release: bool,
) -> OwnedProcess {
    let (mut child, ready) = spawn(f, media, action, Some(stage), release);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ready.is_file() {
        if Instant::now() >= deadline {
            let errors = std::fs::read_to_string(f.data.join("identity-worker/stderr.txt"))
                .unwrap_or_default();
            child.finish().unwrap();
            panic!("{stage} not reached: {errors}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(read_json(&ready)["stage"], stage);
    child
}
pub(super) fn kill(f: &Fixture, media: &Fake, stage: &str) {
    let mut child = held(f, media, "migrate", stage, false);
    child.finish().unwrap();
    assert!(!f.data.join("identity-worker").exists());
}
pub(super) fn completed(f: &Fixture, media: &Fake, action: &str) -> i32 {
    let (mut child, _) = spawn(f, media, action, None, false);
    let status = runtime()
        .block_on(child.wait(Duration::from_secs(20), Duration::from_secs(20)))
        .unwrap();
    let error = std::fs::read_to_string(f.data.join("identity-worker/stderr.txt")).unwrap();
    child.finish().unwrap();
    assert!(status.success() || action == "exit73", "{error}");
    status.code().unwrap()
}
pub(super) fn result(f: &Fixture) -> u64 {
    read_json(&f.root.0.join("worker-result.json"))["code"]
        .as_u64()
        .unwrap()
}
fn writer(data: &Path, action: &str) {
    use crate::{catalog::Catalog, media_native_adapter::NativeMediaAdapter};
    use std::sync::{Arc, Mutex};
    let root = data.parent().unwrap();
    let config = Config::load_with_environment(
        root.into(),
        &BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("BRAND_LOGO_BOTS".into(), "".into()),
            ("GENERAL_BOT_TOKEN".into(), "test-only-token".into()),
            ("COLLECTION_PACK_BASE".into(), "newfamily".into()),
            ("EMOJI_ARCHIVE_DIR".into(), "archive".into()),
        ]),
    )
    .unwrap();
    let log = Arc::new(RunLog::open(root, "baseline-held-writer", Vec::new()));
    match action {
        "hold-catalog" => {
            let _cat = Catalog::open(&data.join("catalog.db"), UTC, root).unwrap();
            super::barrier("writer-catalog");
        }
        "hold-publisher" => {
            crate::publish_cli::run(
                &config,
                &[
                    "--base",
                    "newfamily",
                    "--title",
                    "Fixture",
                    "--dry-run",
                    "--no-brand-logo",
                    "--data-dir",
                    data.to_str().unwrap(),
                ]
                .map(str::to_owned),
                &log,
            )
            .unwrap();
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
            crate::local_cli::run_with_codec(
                &config,
                &[
                    data.join("incoming.png").to_str().unwrap().to_owned(),
                    "--data-dir".into(),
                    data.to_str().unwrap().into(),
                ],
                &log,
                |scratch| {
                    Ok(NativeMediaAdapter {
                        executable: "must-not-decode-held-writer".into(),
                        root: root.into(),
                        scratch,
                    })
                },
            )
            .unwrap();
        }
        "hold-panel" => {
            let panel = crate::panel_http::Panel {
                root: root.into(),
                db: data.join("catalog.db"),
                token: "writer-fixture".into(),
                options: json!({"show_published":false,"keep_sets":{},"branded":false}),
                branding: None,
                detected_bot: Mutex::new(String::new()),
                view: Mutex::new(json!({"view":[],"by_key":{},"hidden":0})),
                session: json!({}),
                last_refreshed: std::sync::Mutex::new(None),
                utc: || UTC.into(),
                log: log.clone(),
                events: Mutex::new(crate::panel_events::Events::default()),
                previews: crate::panel_preview::Previews::new(
                    NativeMediaAdapter {
                        executable: "must-not-decode-held-panel".into(),
                        root: root.into(),
                        scratch: data.join("panel-codec"),
                    },
                    data,
                    1,
                ),
            };
            runtime().block_on(async {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let server = tokio::spawn(async move {
                    axum::serve(listener, crate::panel_http::router(Arc::new(panel))).await
                });
                let response = reqwest::Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(25))
                    .build()
                    .unwrap()
                    .post(format!("http://{address}/api/save"))
                    .header("x-panel-token", "writer-fixture")
                    .json(&json!({"excluded":[],"known":[]}))
                    .send()
                    .await;
                server.abort();
                panic!("held panel returned: {response:?}");
            });
        }
        _ => panic!("unknown held writer {action}"),
    }
    panic!("writer returned before barrier");
}
