//! Finite reopen commands use the same live panel listener without rebuilding its catalog or codec.
use _native::{
    catalog::Catalog, logging::RunLog, panel_session, tooling_environment, tooling_process,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub type SessionReply = Arc<Mutex<Option<(u16, Vec<u8>)>>>;

pub async fn override_session(
    axum::extract::State(reply): axum::extract::State<SessionReply>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if request.uri().path() == "/api/session" {
        let response = reply.lock().unwrap().clone();
        if let Some((status, bytes)) = response {
            use axum::response::IntoResponse;
            return (axum::http::StatusCode::from_u16(status).unwrap(), bytes).into_response();
        }
    }
    next.run(request).await
}

pub fn check(
    runtime: &tokio::runtime::Runtime,
    port: u16,
    db: &Path,
    root: &Path,
    reply: &SessionReply,
    log: &RunLog,
    deadline: Instant,
) {
    let sample = concat!(
        "Active Connections\n\n",
        "  Proto  Local Address      Foreign Address    State       PID\n",
        "  TCP    127.0.0.1:9450     0.0.0.0:0          LISTENING   4321\n",
        "  TCP    127.0.0.1:9451     0.0.0.0:0          LISTENING   9999\n",
        "  TCP    10.0.0.5:59450     1.2.3.4:443        ESTABLISHED 1111\n",
    );
    assert_eq!(
        panel_session::listener_pid(sample, 9450).as_deref(),
        Some("4321")
    );
    assert_eq!(
        panel_session::listener_pid(sample, 9451).as_deref(),
        Some("9999")
    );
    for unmatched in [443, 1234] {
        assert_eq!(panel_session::listener_pid(sample, unmatched), None);
    }
    assert_eq!(panel_session::listener_pid("garbage\n", 9450), None);
    assert_eq!(
        panel_session::listener_pid("TCP 127.0.0.1:9450 * LISTENING invalid", 9450),
        None
    );
    #[cfg(windows)]
    assert_eq!(
        runtime
            .block_on(panel_session::port_holder(root, port))
            .unwrap(),
        Some(std::process::id().to_string()),
        "the real Windows listener must be attributed without stopping it",
    );
    let identity = panel_session::identity(db, false, &[1, 2]).unwrap();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let command = root.join("session-command");
    std::fs::create_dir(&command).unwrap();
    std::fs::create_dir(command.join("assets")).unwrap();
    std::fs::copy(
        project.join("assets/panel.html"),
        command.join("assets/panel.html"),
    )
    .unwrap();
    std::fs::copy(
        project.join("pyproject.toml"),
        command.join("pyproject.toml"),
    )
    .unwrap();
    let executable = command.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &executable).unwrap();
    let inherited = std::env::vars().collect::<BTreeMap<_, _>>();
    let mut environment = _native::tooling_test_runner::scrubbed_environment(&inherited);
    environment.insert(
        "PYO3_PYTHON".into(),
        command.join("must-not-open-codec").to_str().unwrap().into(),
    );
    let environment = tooling_environment::environment(&command, &environment, false).unwrap();
    let catalog_bytes = std::fs::read(db).unwrap();
    let signature = _native::migration_signature::signature(db, &[]).unwrap();
    let launch = |data: &Path, extra: &[&str]| {
        assert!(Instant::now() < deadline);
        let mut argv = vec![
            executable.clone().into_os_string(),
            OsString::from("panel"),
            "--data-dir".into(),
            data.into(),
            "--port".into(),
            port.to_string().into(),
            "--with-pack".into(),
            "1".into(),
            "--with-pack".into(),
            "2".into(),
            "--no-open".into(),
        ];
        argv.extend(extra.iter().map(|value| OsString::from(*value)));
        let result = runtime
            .block_on(tooling_process::capture(
                &command,
                &argv,
                &environment,
                Duration::from_secs(5),
                Duration::from_secs(3),
                log,
            ))
            .unwrap();
        assert_eq!(std::fs::read(db).unwrap(), catalog_bytes);
        assert_eq!(
            _native::migration_signature::signature(db, &[]).unwrap(),
            signature
        );
        assert!(!command.join("must-not-open-codec").exists());
        assert!(!db.with_file_name("pack_plan.json").exists());
        assert!(
            std::fs::read_dir(command.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
        assert!(!_native::process_owner::cleanup_pending(&command));
        result
    };
    assert_eq!(panel_session::probe(port).as_ref(), Some(&identity));
    let current = launch(db.parent().unwrap(), &[]);
    assert_eq!(current.code, 0, "{}", current.stderr);
    assert!(current.stdout.contains("Panel already running:"));
    for extra in [vec!["--all"], vec!["--with-pack", "3"]] {
        let changed = launch(db.parent().unwrap(), &extra);
        assert_eq!(changed.code, 2);
        assert!(changed.stderr.contains("another collection or view"));
    }
    let other = root.join("other-session");
    std::fs::create_dir(&other).unwrap();
    drop(Catalog::open(&other.join("catalog.db"), "fixture", root).unwrap());
    let changed = launch(&other, &[]);
    assert_eq!(changed.code, 2);
    assert!(changed.stderr.contains("another collection or view"));

    for body in [
        b"invalid JSON".to_vec(),
        json!({"application":"other"}).to_string().into_bytes(),
        json!({"application":"numera-emoji-mapper-panel","legacy":true})
            .to_string()
            .into_bytes(),
    ] {
        *reply.lock().unwrap() = Some((200, body));
        assert!(panel_session::probe(port).is_none());
        let refused = launch(db.parent().unwrap(), &[]);
        assert_eq!(refused.code, 2);
        assert!(!refused.stdout.contains("Panel already running:"));
    }
    *reply.lock().unwrap() = Some((200, vec![b'x'; 4 * 1024 * 1024 + 1]));
    assert!(panel_session::probe(port).is_none());
    *reply.lock().unwrap() = Some((404, Vec::new()));
    assert_eq!(
        panel_session::probe(port),
        Some(json!({"application":"numera-emoji-mapper-panel","legacy":true}))
    );
    let legacy = launch(db.parent().unwrap(), &[]);
    assert_eq!(legacy.code, 0, "{}", legacy.stderr);
    assert!(legacy.stdout.contains("Panel already running:"));
    assert!(legacy.stderr.contains("older panel"));
    let page = root.join("assets/panel.html");
    let original_page = std::fs::read(&page).unwrap();
    std::fs::write(&page, b"Other app").unwrap();
    assert!(panel_session::probe(port).is_none());
    let foreign = launch(db.parent().unwrap(), &[]);
    assert_eq!(foreign.code, 2);
    assert!(!foreign.stdout.contains("Panel already running:"));
    std::fs::write(&page, &original_page).unwrap();
    *reply.lock().unwrap() = None;
    assert_eq!(panel_session::probe(port).as_ref(), Some(&identity));
    assert_eq!(std::fs::read(db).unwrap(), catalog_bytes);
}
