//! Real panel startup remains responsive while its acknowledged bot lookup has no answer.
use _native::{
    catalog::Catalog,
    logging::RunLog,
    media_adapter::MediaAdapter,
    panel_events::Events,
    panel_http::{self, Panel},
    panel_preview::Previews,
    panel_session,
};
use serde_json::json;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

#[allow(dead_code)]
#[path = "support/read_only_api.rs"]
mod read_only_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[test]
fn loopback_bot_lookup_accepts_only_verified_username_and_cancels_without_waiting() {
    use read_only_api::{Fixture, Reply, TOKEN};
    let root = tooling_fixture::Root::new("panel unanswered bot lookup");
    let db = root.0.join("catalog.db");
    let mut catalog = Catalog::open(&db, "fixture", &root.0).unwrap();
    catalog
        .dispatch(json!({"operation":"set_meta","key":"order_seeded","value":"1"}))
        .unwrap();
    drop(catalog);
    let before = _native::migration_signature::signature(&db, &[]).unwrap();
    let (held, reached) = Reply::api("getMe", "", "", json!({"username":"YourEmojiBot"})).hold();
    let mut service = Fixture::new(vec![
        Reply::api("getMe", "", "", json!({"username":"YourEmojiBot"})),
        Reply::envelope("getMe", "", "", json!([])),
        held,
    ]);
    let threads = Arc::new(AtomicUsize::new(0));
    let started = threads.clone();
    let stopped = threads.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(2)
        .on_thread_start(move || {
            started.fetch_add(1, Ordering::SeqCst);
        })
        .on_thread_stop(move || {
            stopped.fetch_sub(1, Ordering::SeqCst);
        })
        .enable_all()
        .build()
        .unwrap();
    assert_eq!(
        runtime.block_on(panel_session::detect_bot(TOKEN, &service.base)),
        Some("YourEmojiBot".into())
    );
    assert_eq!(
        runtime.block_on(panel_session::detect_bot(TOKEN, &service.base)),
        None
    );
    let log = Arc::new(RunLog::open(&root.0, "panel-startup", vec![]));
    let identity = panel_session::identity(&db, false, &[]).unwrap();
    let panel = Panel {
        root: root.0.clone(),
        db: db.clone(),
        token: "startup-fixture".into(),
        options: json!({"no_open":true,"keep_sets":{},"show_published":false,"branded":false}),
        branding: Some((TOKEN.into(), service.base.clone(), vec![])),
        detected_bot: Mutex::new(String::new()),
        view: Mutex::new(json!({"view":[],"by_key":{},"hidden":0})),
        session: identity.clone(),
        utc: || "fixture".into(),
        log: log.clone(),
        events: Mutex::new(Events::default()),
        previews: Previews::new(
            MediaAdapter {
                python: "must-not-run-startup-codec".into(),
                root: root.0.clone(),
                scratch: root.0.join("codec"),
            },
            &root.0,
            1,
        ),
    };
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, done) = tokio::sync::oneshot::channel();
    let server = runtime.spawn(panel_http::serve_listener(panel, listener, async {
        let _ = done.await;
    }));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), reached)
                .await
                .expect("bot lookup never reached the held response")
                .unwrap();
            let started = Instant::now();
            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .timeout(Duration::from_secs(1))
                .build()
                .unwrap();
            let ping = client
                .get(format!("http://{address}/api/ping"))
                .send()
                .await
                .unwrap();
            assert_eq!(ping.status(), 200);
            assert_eq!(
                ping.json::<serde_json::Value>().await.unwrap(),
                json!({"ok":true})
            );
            let session = client
                .get(format!("http://{address}/api/session"))
                .send()
                .await
                .unwrap();
            assert_eq!(session.status(), 200);
            assert_eq!(session.json::<serde_json::Value>().await.unwrap(), identity);
            assert!(started.elapsed() < Duration::from_secs(2));
            assert!(
                !server.is_finished(),
                "unanswered lookup must not stop the panel"
            );
        });
    }));
    let _ = stop.send(());
    let shutdown =
        runtime.block_on(async { tokio::time::timeout(Duration::from_secs(2), server).await });
    runtime.shutdown_timeout(Duration::from_secs(1));
    let observations = service.finish();
    log.finish(if result.is_ok() { 0 } else { 1 });
    drop(log);
    assert_eq!(threads.load(Ordering::SeqCst), 0);
    shutdown
        .expect("panel startup shutdown timed out")
        .unwrap()
        .unwrap();
    assert_eq!(observations.unwrap(), vec![format!("/bot{TOKEN}/getMe"); 3]);
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    assert_eq!(
        _native::migration_signature::signature(&db, &[]).unwrap(),
        before
    );
    assert!(!root.0.join("codec").exists());
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    root.clean().unwrap();
}
