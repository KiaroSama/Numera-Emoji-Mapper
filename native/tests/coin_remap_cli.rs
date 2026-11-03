//! Actual content-only command reads loopback, resumes signatures and replaces only admitted maps.
use _native::{logging::RunLog, tooling_process};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/coin_remap_commands.rs"]
mod coin_remap_commands;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server};
const SET: &str = "cryptoemoji1_by_YourEmojiBot";

fn document(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn member(cid: &str) -> Value {
    json!({"custom_emoji_id":cid,"file_unique_id":format!("fu-{cid}"),"file_id":cid})
}
fn listing(members: &[Value]) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", SET)],
        json!({"ok":true,"result":{"stickers":members}}),
    )
}
fn download(replies: &mut Vec<Reply>, cid: &str, bytes: &[u8]) {
    let path = format!("{cid}.png");
    replies.push(Reply::new(
        "getFile",
        &[("file_id", cid)],
        json!({"ok":true,"result":{"file_path":path}}),
    ));
    let mut reply = Reply::new("getFile", &[], Value::Null);
    reply.method = format!("GET {path}");
    reply.download = Some(bytes.to_vec());
    replies.push(reply);
}
struct Files {
    directory: PathBuf,
    images: PathBuf,
    state: PathBuf,
    cache: PathBuf,
    output: PathBuf,
    candidate: PathBuf,
}
impl Files {
    fn new(root: &Path, label: &str) -> Self {
        let directory = root.join(label);
        std::fs::create_dir(&directory).unwrap();
        let images = directory.join("images");
        std::fs::create_dir(&images).unwrap();
        let files = Self {
            state: directory.join("state.json"),
            cache: directory.join("cache.json"),
            output: directory.join("map.json"),
            candidate: directory.join("candidate.json"),
            directory,
            images,
        };
        _native::atomic::write_json(&files.state, &json!({"sets":[{"index":1,"name":SET}]}))
            .unwrap();
        files
    }
    fn argv(&self, executable: &Path, extra: &[&str]) -> Vec<std::ffi::OsString> {
        let mut argv = vec![executable.as_os_str().to_owned(), "coin-remap-ids".into()];
        for (option, path) in [
            ("--emoji-dir", &self.images),
            ("--state", &self.state),
            ("--cache", &self.cache),
            ("--out", &self.output),
            ("--candidates", &self.candidate),
        ] {
            argv.push(option.into());
            argv.push(path.as_os_str().to_owned());
        }
        argv.extend(extra.iter().map(|value| std::ffi::OsString::from(*value)));
        argv
    }
}
struct Harness {
    root: PathBuf,
    executable: PathBuf,
    input: PathBuf,
    runtime: tokio::runtime::Runtime,
    log: RunLog,
}
impl Harness {
    fn capture(
        &self,
        files: &Files,
        extra: &[&str],
        server: &Server,
    ) -> _native::tooling_process::Completed {
        let mut environment =
            _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
        environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        environment.insert("TELEGRAM_BOT_TOKEN".into(), bot_api::TOKEN.into());
        environment.insert("COIN_PACK_BASE".into(), "cryptoemoji".into());
        self.runtime
            .block_on(tooling_process::capture_with_input(
                &self.root,
                &files.argv(&self.executable, extra),
                &environment,
                &self.input,
                Duration::from_secs(10),
                Duration::from_secs(5),
                &self.log,
            ))
            .unwrap()
    }
    fn released(&self) {
        for path in [
            _native::locks::pack_family_path(&self.root, "cryptoemoji"),
            self.root.join(".locks/canonical_map.lock"),
        ] {
            drop(_native::locks::Lease::acquire(&path, "remap released").unwrap());
            assert!(path.is_file());
        }
        assert!(!_native::process_owner::cleanup_pending(&self.root));
        assert!(
            std::fs::read_dir(self.root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
    }
}
#[test]
fn missing_numpy_names_the_coin_extra_before_configuration_network_or_data_work() {
    const CHILD: &str = "NUMERA_REMAP_MISSING_NUMPY_CHILD";
    let child = std::env::var(CHILD).as_deref() == Ok("1");
    let fixture =
        (!child).then(|| tooling_fixture::Root::new("missing remap numerical dependency"));
    let working = std::env::current_dir().unwrap();
    let root = fixture.as_ref().map_or(&working, |fixture| &fixture.0);
    let log = RunLog::open(root, "remap-dependency-test", Vec::new());
    assert!(log.path.is_some());
    if child {
        assert_eq!(
            std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
            Ok("1")
        );
        for name in [
            "TELEGRAM_BOT_TOKEN",
            "GENERAL_BOT_TOKEN",
            "COIN_BOT_TOKEN",
            "PACK_OWNER_USER_ID",
        ] {
            assert!(std::env::var_os(name).is_none(), "inherited {name}");
        }
        use pyo3::prelude::*;
        use pyo3::types::PyDict;
        println!("missing NumPy case started");
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let modules = py
                .import("sys")?
                .getattr("modules")?
                .cast_into::<PyDict>()?;
            let previous = modules.get_item("numpy")?;
            modules.set_item("numpy", py.None())?;
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let expected = "coin-remap-ids needs numpy: pip install -r requirements-coins.txt";
                assert_eq!(
                    _native::coin_remap_math::nearest(&[vec![1.0]], &[vec![1.0]])
                        .err()
                        .unwrap(),
                    expected
                );
                let environment = std::collections::BTreeMap::from([(
                    "NUMERA_EMOJI_MAPPER_NO_DOTENV".into(),
                    "1".into(),
                )]);
                let config =
                    _native::config::Config::load_with_environment(root.clone(), &environment)
                        .unwrap();
                let before = std::fs::read_dir(root)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .collect::<std::collections::BTreeSet<_>>();
                let args = vec!["--emoji-dir".into(), "missing-images".into()];
                assert_eq!(
                    _native::coin_remap_cli::run(&config, &args, &log).unwrap_err(),
                    expected
                );
                assert_eq!(
                    _native::coin_remap_cli::run(&config, &["--help".into()], &log).unwrap(),
                    0
                );
                assert!(
                    _native::coin_remap_cli::run(&config, &[], &log)
                        .unwrap_err()
                        .contains("--emoji-dir")
                );
                assert_eq!(
                    std::fs::read_dir(root)
                        .unwrap()
                        .map(|entry| entry.unwrap().path())
                        .collect::<std::collections::BTreeSet<_>>(),
                    before
                );
                assert!(!root.join("coins").exists());
                assert!(!root.join(".locks").exists());
                assert!(!root.join("missing-images").exists());
            }));
            match &previous {
                Some(value) => modules.set_item("numpy", value)?,
                None => modules.del_item("numpy")?,
            }
            let current = modules.get_item("numpy")?;
            assert!(match (previous, current) {
                (Some(before), Some(after)) => before.is(&after),
                (None, None) => true,
                _ => false,
            });
            if let Err(panic) = outcome {
                std::panic::resume_unwind(panic);
            }
            Ok(())
        })
        .unwrap();
        println!("missing NumPy case completed");
    } else {
        let mut environment =
            _native::tooling_test_runner::scrubbed_environment(&std::env::vars().collect());
        environment.insert(CHILD.into(), "1".into());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let argv = [
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            "missing_numpy_names_the_coin_extra_before_configuration_network_or_data_work".into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ];
        let result = runtime
            .block_on(tooling_process::capture(
                root,
                &argv,
                &environment,
                Duration::from_secs(10),
                Duration::from_secs(5),
                &log,
            ))
            .unwrap();
        assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
        assert!(result.stdout.contains("missing NumPy case started"));
        assert!(result.stdout.contains("missing NumPy case completed"));
        assert!(!_native::process_owner::cleanup_pending(root));
        assert!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
        runtime.shutdown_timeout(Duration::from_secs(1));
    }
    log.finish(0);
    drop(log);
    if let Some(fixture) = fixture {
        fixture.clean().unwrap();
    }
}

#[test]
fn actual_remap_reuses_live_cache_and_applies_exact_content_with_original_backup() {
    let fixture = tooling_fixture::Root::new("coin remap original commands");
    let root = &fixture.0;
    std::fs::create_dir(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/panel.html"), b"fixture marker").unwrap();
    std::fs::write(root.join("pyproject.toml"), b"# fixture marker\n").unwrap();
    let executable = root.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let input = root.join("command-eof");
    std::fs::write(&input, b"").unwrap();
    let harness = Harness {
        root: root.clone(),
        executable,
        input,
        runtime: tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap(),
        log: RunLog::open(root, "coin-remap-command-owner", Vec::new()),
    };
    let files = Files::new(root, "cache-resume");
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let art =
        std::fs::read(project.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
    std::fs::write(files.images.join("aaa.png"), &art).unwrap();
    let old = b"{\"aaa\":\"wrong\",\"unrelated\":\"backup only\"}\n";
    std::fs::write(&files.output, old).unwrap();
    let mut replies = vec![listing(&[member("111111111")])];
    download(&mut replies, "111111111", &art);
    replies.push(listing(&[member("111111111")]));
    let mut server = Server::new(replies);
    for iteration in 0..2 {
        let result = harness.capture(
            &files,
            if iteration == 0 {
                &[]
            } else {
                &["--max-distance", "1", "--apply"]
            },
            &server,
        );
        assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
        let cache = document(&files.cache);
        assert_eq!(
            STANDARD
                .decode(cache["sigs"]["111111111"].as_str().unwrap())
                .unwrap()
                .len(),
            1024
        );
        assert!(cache["sets"][SET].is_string());
        if iteration == 0 {
            assert_eq!(std::fs::read(&files.output).unwrap(), old);
            assert!(!files.output.with_extension("prebroken.json").exists());
        } else {
            assert_eq!(document(&files.output), json!({"aaa":"111111111"}));
            assert_eq!(
                std::fs::read(files.output.with_extension("prebroken.json")).unwrap(),
                old
            );
        }
        assert!(!files.candidate.exists());
        harness.released();
    }
    assert_eq!(
        server
            .finish()
            .unwrap()
            .iter()
            .map(|request| request.method.as_str())
            .collect::<Vec<_>>(),
        [
            "getStickerSet",
            "getFile",
            "GET 111111111.png",
            "getStickerSet"
        ]
    );
    coin_remap_commands::check(&harness);
    drop(harness);
    fixture.clean().unwrap();
}
