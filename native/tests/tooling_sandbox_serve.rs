//! Actual copied native panel lifetime: isolated clone, inherited lease, bounded cancellation and source preservation.
use _native::{
    catalog::Catalog,
    logging::RunLog,
    tooling_launch::SandboxOptions,
    tooling_sandbox::{self, Session},
    tooling_sandbox_serve,
};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

#[test]
fn actual_sandbox_native_child_holds_clone_lease_and_cancellation_stops_listener_before_cleanup() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let configured = std::env::var_os("PYO3_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            project.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    // CI configures a command name, whereas the copied fixture needs a real
    // absolute prerequisite before its CWD changes. Reuse the native PATH resolver.
    let python = if configured.components().count() == 1 && !configured.is_absolute() {
        let lookup = if cfg!(windows)
            && configured
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case("exe"))
        {
            configured.with_extension("")
        } else {
            configured.clone()
        };
        _native::codec_process::executable(
            lookup
                .to_str()
                .expect("configured interpreter must be UTF-8"),
        )
        .expect("configured exact media interpreter must resolve on PATH")
    } else {
        _native::migration_bundle::resolved(&configured)
            .expect("configured interpreter path must resolve")
    };
    assert!(
        python.is_absolute() && python.is_file(),
        "existing exact media interpreter required; no installation is performed"
    );
    let root = Root::new("sandbox native wrapper");
    std::fs::create_dir(root.0.join("assets")).unwrap();
    for entry in std::fs::read_dir(project.join("assets")).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            std::fs::copy(&path, root.0.join("assets").join(path.file_name().unwrap())).unwrap();
        }
    }
    std::fs::copy(
        project.join("pyproject.toml"),
        root.0.join("pyproject.toml"),
    )
    .unwrap();
    std::fs::write(
        root.0.join(".env.example"),
        b"GENERAL_BOT_TOKEN=\nPACK_OWNER_USER_ID=\n",
    )
    .unwrap();
    let runtime_directory = root.0.join("native/runtime");
    std::fs::create_dir_all(&runtime_directory).unwrap();
    std::fs::copy(
        env!("CARGO_BIN_EXE_numera-emoji"),
        runtime_directory.join(if cfg!(windows) {
            "numera-emoji.exe"
        } else {
            "numera-emoji"
        }),
    )
    .unwrap();
    let python_destination = root.0.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    std::fs::create_dir_all(python_destination.parent().unwrap()).unwrap();
    // Included static rows need no warm-up codec. This copied prerequisite is
    // selection evidence, not a valid CPython installation or media decode proof.
    #[cfg(unix)]
    std::os::unix::fs::symlink(&python, &python_destination).unwrap();
    #[cfg(windows)]
    std::fs::copy(&python, &python_destination).unwrap();
    let inherited = std::collections::BTreeMap::from([
        ("PYO3_PYTHON".into(), "python".into()),
        ("KEEP".into(), "fixture".into()),
    ]);
    let launch = _native::tooling_launch::plan(&root.0, &[], &inherited).unwrap();
    assert_eq!(
        Path::new(&launch.environment["PYO3_PYTHON"]),
        python_destination,
        "child must receive the absolute package prerequisite, not the inherited command name"
    );
    assert!(Path::new(&launch.environment["PYO3_PYTHON"]).is_absolute());
    assert_eq!(inherited["PYO3_PYTHON"], "python");
    assert_eq!(inherited["KEEP"], "fixture");
    let source = root.0.join("source");
    std::fs::create_dir(&source).unwrap();
    let mut catalog = Catalog::open(&source.join("catalog.db"), "fixture", &root.0).unwrap();
    let mut originals = Vec::new();
    for (index, letter) in ['a', 'b'].into_iter().enumerate() {
        let path = source.join(format!("{index}.png"));
        let bytes = format!("storage-only sandbox fixture {index}").into_bytes();
        std::fs::write(&path, &bytes).unwrap();
        assert!(
            catalog
                .insert(
                    &serde_json::json!({
                        "content_key":format!("s:{}", letter.to_string().repeat(32)),
                        "fmt":"static","file_path":path,
                    }),
                    "fixture"
                )
                .unwrap()
        );
        originals.push((path, bytes));
    }
    // Seed before the preservation snapshot so background refresh cannot change it.
    catalog
        .dispatch(serde_json::json!({"operation":"set_meta","key":"order_seeded","value":"1"}))
        .unwrap();
    drop(catalog);
    let before = _native::tooling_manifest::digest(&source.join("catalog.db")).unwrap();
    let clone = root.0.join("panel-sandbox-active");
    let session = Session::create(&root.0, &source, &clone).unwrap();
    let lease = tooling_sandbox::lifetime_lock_path(&clone).unwrap();
    let reserved = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = reserved.local_addr().unwrap().port();
    assert_ne!(port, _native::panel_session::DEFAULT_PORT);
    drop(reserved);
    let options = SandboxOptions {
        port: i64::from(port),
        all: false,
        with_pack: vec![],
        bot_username: "YourEmojiBot".into(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = RunLog::open(&root.0, "sandbox-native-test", vec![]);
    runtime.block_on(async {
        let started = Instant::now();
        let mut serving = Box::pin(tooling_sandbox_serve::serve(
            &root.0,
            session,
            &options,
            _native::panel_session::DEFAULT_PORT,
            Duration::from_secs(5),
            Duration::from_secs(5),
            &log,
        ));
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(300))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(4);
        let page = loop {
            tokio::select! {
                result = &mut serving => panic!("sandbox stopped before readiness: {result:?}"),
                ready = async {
                    let session = client.get(format!("http://127.0.0.1:{port}/api/session")).send().await.ok()?;
                    if session.status() != 200 { return None; }
                    let _: serde_json::Value = session.json().await.ok()?;
                    let page = client.get(format!("http://127.0.0.1:{port}/")).send().await.ok()?;
                    if page.status() != 200 { return None; }
                    page.text().await.ok().filter(|text| text.contains("id=\"items-data\""))
                } => {
                    if let Some(page) = ready { break page; }
                }
            }
            assert!(
                Instant::now() < deadline,
                "native sandbox did not become ready"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        assert!(clone.join("catalog.db").is_file());
        assert!(
            !clone.join(".sandbox-owner.json").exists(),
            "child must never start with a reclaimable marker"
        );
        assert!(clone.join(".sandbox-recovery.json").is_file());
        _native::sandbox_lease::validate_marker(&clone).unwrap();
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
        let cloned_db = clone.join("catalog.db");
        let identity = _native::panel_session::identity(&cloned_db, false, &[]).unwrap();
        let snapshot = _native::migration_signature::signature(&cloned_db, &[]).unwrap();
        let environment = _native::tooling_test_runner::scrubbed_environment(
            &std::env::vars().collect::<std::collections::BTreeMap<_, _>>(),
        );
        let environment = _native::tooling_environment::environment(&root.0, &environment, false).unwrap();
        let command = vec![
            launch.argv[0].clone(), "panel".into(), "--data-dir".into(), clone.clone().into_os_string(),
            "--port".into(), port.to_string().into(), "--no-open".into(),
            "--bot-username".into(), "YourEmojiBot".into(),
        ];
        let remaining = Duration::from_secs(5).saturating_sub(started.elapsed()).min(Duration::from_secs(2));
        assert!(remaining > Duration::from_secs(1), "no unbounded second-launch budget");
        let reopening = _native::tooling_process::capture(
            &root.0, &command, &environment, remaining, Duration::from_secs(1), &log,
        );
        tokio::pin!(reopening);
        let reopened = tokio::select! {
            result = &mut serving => panic!("first native panel exited during reopen: {result:?}"),
            result = &mut reopening => result.unwrap(),
        };
        assert_eq!(reopened.code, 0, "{}", reopened.stderr);
        assert!(reopened.stdout.contains("Panel already running:"));
        assert_eq!(client.get(format!("http://127.0.0.1:{port}/api/session")).send().await.unwrap().json::<serde_json::Value>().await.unwrap(), identity);
        assert_eq!(client.get(format!("http://127.0.0.1:{port}/api/ping")).send().await.unwrap().status(), 200);
        assert_eq!(_native::migration_signature::signature(&cloned_db, &[]).unwrap(), snapshot);
        let token = page.split_once("const TOKEN = \"").unwrap().1.split_once('"').unwrap().0;
        let cards = page.split_once("<script id=\"items-data\" type=\"application/json\">").unwrap().1.split_once("</script>").unwrap().0;
        let cards: Vec<serde_json::Value> = serde_json::from_str(cards).unwrap();
        let known = cards.iter().filter(|card| card["isLogo"] != true)
            .map(|card| card["key"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
        assert_eq!(known.len(), 2);
        assert_eq!(token.len(), 48, "page must provide its actual session token");
        assert_eq!(known.iter().cloned().collect::<std::collections::BTreeSet<_>>(),
            ['a', 'b'].map(|letter| format!("s:{}", letter.to_string().repeat(32))).into_iter().collect::<std::collections::BTreeSet<_>>());
        let saving = async {
            client.post(format!("http://127.0.0.1:{port}/api/save"))
                .header("x-panel-token", token)
                .json(&serde_json::json!({"known":known,"excluded":[known[0]],"packs":[]}))
                .send().await.unwrap()
        };
        tokio::pin!(saving);
        let saved = tokio::select! {
            result = &mut serving => panic!("sandbox stopped during save: {result:?}"),
            result = &mut saving => result,
        };
        assert_eq!(saved.status(), 200);
        assert_eq!(saved.json::<serde_json::Value>().await.unwrap()["ok"], true);
        let db = rusqlite::Connection::open_with_flags(&cloned_db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let excluded = {
            let mut query = db.prepare("SELECT content_key FROM items WHERE included=0 ORDER BY content_key").unwrap();
            query.query_map([], |row| row.get::<_, String>(0)).unwrap()
                .collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        assert_eq!(excluded, [known[0].clone()]);
        db.close().unwrap();
        assert_eq!(_native::tooling_manifest::digest(&source.join("catalog.db")).unwrap(), before);
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
        assert!(started.elapsed() < Duration::from_secs(5));
        // Dropping the future models cancellation; its synchronous owner Drop
        // must stop and reap the native panel before clone cleanup returns.
        drop(serving);
    });
    assert!(!clone.exists(), "owned clone must not outlive cancellation");
    assert!(lease.is_file());
    assert!(
        std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(200)
        )
        .is_err()
    );
    assert_eq!(
        _native::tooling_manifest::digest(&source.join("catalog.db")).unwrap(),
        before
    );
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    let parent = std::env::vars().collect::<std::collections::BTreeMap<_, _>>();
    let refused = root.0.join("panel-sandbox-preflight-refused");
    let session = Session::create(&root.0, &source, &refused).unwrap();
    let mut invalid = options;
    invalid.port = i64::from(_native::panel_session::DEFAULT_PORT);
    let error = runtime
        .block_on(tooling_sandbox_serve::serve(
            &root.0,
            session,
            &invalid,
            _native::panel_session::DEFAULT_PORT,
            Duration::from_secs(5),
            Duration::from_secs(3),
            &log,
        ))
        .unwrap_err();
    assert!(error.contains("real panel's port"), "{error}");
    assert!(!refused.exists());
    assert!(
        tooling_sandbox::lifetime_lock_path(&refused)
            .unwrap()
            .is_file()
    );
    assert_eq!(
        _native::tooling_manifest::digest(&source.join("catalog.db")).unwrap(),
        before
    );
    assert!(
        std::env::vars().collect::<std::collections::BTreeMap<_, _>>() == parent,
        "sandbox changed parent environment"
    );
    for (path, bytes) in originals {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    runtime.shutdown_timeout(Duration::from_secs(1));
    log.finish(0);
    drop(log);
    root.clean().unwrap();
}
