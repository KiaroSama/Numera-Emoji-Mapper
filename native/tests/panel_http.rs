//! Public router smoke covers host, token, schema, persistence and immutable session boundaries.
use _native::{
    catalog::Catalog,
    logging::RunLog,
    media_adapter::MediaAdapter,
    panel_events::Events,
    panel_http::{self, Panel},
    panel_preview::Previews,
};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn loopback_router_validates_mutations_and_persists_curation() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let root = project
        .join("logs")
        .join(format!("panel-http-{}", std::process::id()));
    let data = root.join("collection");
    std::fs::create_dir_all(&data).unwrap();
    let root = root.canonicalize().unwrap();
    let data = root.join("collection");
    let db = data.join("catalog.db");
    std::fs::create_dir(root.join("assets")).unwrap();
    for name in [
        "panel.html",
        "panel-grid.js",
        "panel-motion.js",
        "panel-save.js",
        "panel-drag.js",
        "panel-actions.js",
        "panel-holding.js",
        "panel-draft.js",
        "logo-128.png",
    ] {
        std::fs::copy(
            project.join("assets").join(name),
            root.join("assets").join(name),
        )
        .unwrap();
    }
    let logo = root.join("assets/logo-128.png");
    let item = {
        let mut catalog = Catalog::open(&db, "fixture", &root).unwrap();
        catalog.insert(&json!({"content_key":"s:fixture","fmt":"static","file_path":"./art.png","emojis":["😀"],"keywords":["fixture"]}), "fixture").unwrap();
        catalog.insert(&json!({"content_key":"s:unseen","fmt":"static","file_path":"./unseen.png"}), "fixture").unwrap();
        catalog.inclusion(&["s:unseen".into()]).unwrap();
        catalog.get("s:fixture").unwrap().unwrap()
    };
    let view = json!({"view":[{"key":"s:fixture","label":"fixture","format":"static","included":true,"pack":1}],"by_key":{},"hidden":0});
    let log = Arc::new(RunLog::open(&root, "panel-fixture", vec![]));
    let panel = Panel {
        branding: Some((
            "test-only-token".into(),
            "http://127.0.0.1:9".into(),
            vec!["youremojibot".into()],
        )),
        detected_bot: Mutex::new("YourEmojiBot".into()),
        root: root.clone(),
        db: db.clone(),
        token: "fixture-token".into(),
        options: json!({"items":[item],"published":[],"published_sets":{},"show_published":false,"keep_sets":{},"branded":false,"logo_path":logo}),
        view: Mutex::new(view),
        session: json!({"application":"numera-emoji-mapper-panel","catalog":"fixture","all":false,"packs":[]}),
        utc: || "fixture".into(),
        log,
        events: Mutex::new(Events::default()),
        previews: Previews::new(
            MediaAdapter {
                python: "must-not-run".into(),
                root: root.clone(),
                scratch: data.join("codec"),
            },
            &data,
            1,
        ),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let (stop, done) = tokio::sync::oneshot::channel::<()>();
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = runtime.spawn(async move {
        panel_http::serve_listener(panel, listener, async {
            let _ = done.await;
        })
        .await
        .unwrap();
    });
    let result = std::panic::catch_unwind(|| {
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let url = format!("http://127.0.0.1:{port}");
        let ping = client.get(format!("{url}/api/ping")).send().unwrap();
        assert_eq!(ping.status(), 200);
        assert_eq!(ping.headers()["x-content-type-options"], "nosniff");
        assert_eq!(ping.json::<Value>().unwrap(), json!({"ok":true}));
        let page = client.get(format!("{url}/")).send().unwrap();
        assert_eq!(page.status(), 200);
        assert_eq!(page.headers()["x-frame-options"], "DENY");
        let html = page.text().unwrap();
        assert!(html.contains("fixture-token"));
        assert!(!html.contains("__ITEMS__"));
        assert!(
            html.contains("__brand_logo__"),
            "detected configured bot must show existing brand card"
        );
        let telemetry = client
            .post(format!("{url}/api/client-log"))
            .header("x-panel-token", "fixture-token")
            .json(&json!({"events":[{"event":"ready","source":"panel-grid.js","count":1}]}))
            .send()
            .unwrap();
        assert_eq!(telemetry.status(), 204);
        assert!(
            !telemetry.headers().contains_key("content-length"),
            "204 must have no Content-Length"
        );
        assert_eq!(telemetry.bytes().unwrap().len(), 0);
        assert_eq!(
            client
                .get(format!("{url}/static/panel-grid.js?v=fixture"))
                .send()
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            client
                .get(format!("{url}/static/%2e%2e/catalog.db"))
                .send()
                .unwrap()
                .status(),
            404
        );
        assert_eq!(
            client
                .get(format!("{url}/api/session"))
                .header("Host", "evil.invalid")
                .send()
                .unwrap()
                .status(),
            403
        );
        let denied = client
            .post(format!("{url}/api/order"))
            .json(&json!({"order":["s:fixture"]}))
            .send()
            .unwrap();
        assert_eq!(denied.status(), 403);
        let malformed = client
            .post(format!("{url}/api/save"))
            .header("x-panel-token", "fixture-token")
            .header("Content-Type", "application/json")
            .body("[]")
            .send()
            .unwrap();
        assert_eq!(malformed.status(), 400);
        let forbidden = client
            .post(format!("{url}/api/save"))
            .header("x-panel-token", "fixture-token")
            .header("Origin", "https://evil.invalid")
            .json(&json!({"known":["s:fixture"],"excluded":[]}))
            .send()
            .unwrap();
        assert_eq!(forbidden.status(), 403);
        // Reject from the header alone, avoiding a client/server upload-close race.
        {
            use std::io::{Read, Write};
            let mut socket = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            write!(socket, "POST /api/save HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nx-panel-token: fixture-token\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", 4 * 1024 * 1024 + 1).unwrap();
            let mut bytes = [0; 256];
            let count = socket.read(&mut bytes).unwrap();
            assert!(String::from_utf8_lossy(&bytes[..count]).starts_with("HTTP/1.1 413 "));
        }
        let unknown = client
            .post(format!("{url}/api/save"))
            .header("x-panel-token", "fixture-token")
            .json(&json!({"known":["s:fixture"],"excluded":[],"unrecognized":true}))
            .send()
            .unwrap();
        assert_eq!(unknown.status(), 400);
        let stale = client
            .post(format!("{url}/api/save"))
            .header("x-panel-token", "fixture-token")
            .json(&json!({"excluded":["s:fixture"]}))
            .send()
            .unwrap();
        assert_eq!(stale.status(), 409);
        let stale_scope = client
            .post(format!("{url}/api/save"))
            .header("x-panel-token", "fixture-token")
            .json(&json!({"known":["s:missing"],"excluded":[]}))
            .send()
            .unwrap();
        assert_eq!(stale_scope.status(), 409);
        {
            let catalog = Catalog::open(&db, "fixture", &root).unwrap();
            assert_eq!(catalog.get("s:fixture").unwrap().unwrap()["included"], true);
            assert!(!data.join("pack_plan.json").exists());
        }
        let owner = _native::ownership::Ownership::acquire(
            &data,
            _native::ownership::Mode::Maintenance,
            "fixture",
        )
        .unwrap();
        let busy = client
            .post(format!("{url}/api/save"))
            .header("x-panel-token", "fixture-token")
            .json(&json!({"known":["s:fixture"],"excluded":[]}))
            .send()
            .unwrap();
        assert_eq!(busy.status(), 503);
        drop(owner);
        let save = client
            .post(format!("{url}/api/save"))
            .header("x-panel-token", "fixture-token")
            .json(&json!({"known":["s:fixture"],"excluded":["s:fixture"]}))
            .send()
            .unwrap();
        let status = save.status();
        let body = save.json::<Value>().unwrap();
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["excluded"], 2);
        let catalog = Catalog::open(&db, "fixture", &root).unwrap();
        assert_eq!(
            catalog.get("s:fixture").unwrap().unwrap()["included"],
            false
        );
        assert_eq!(catalog.get("s:unseen").unwrap().unwrap()["included"], false);
    });
    let _ = stop.send(());
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    });
    drop(runtime);
    std::fs::remove_dir_all(root).unwrap();
    result.unwrap();
}
