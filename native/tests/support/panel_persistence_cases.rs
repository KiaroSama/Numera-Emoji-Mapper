//! Original scoped-save and intent consequences through the real router and committed catalog.
use _native::{
    catalog::Catalog,
    logging::RunLog,
    media_adapter::MediaAdapter,
    panel_events::Events,
    panel_http::{self, Panel},
    panel_preview::Previews,
};
use axum::serve::ListenerExt;
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

pub(super) fn key(index: usize) -> String {
    format!("s:item{index:030}")
}
pub(super) fn call(
    client: &reqwest::blocking::Client,
    url: &str,
    route: &str,
    body: Option<&Value>,
    deadline: Instant,
) -> (u16, String) {
    assert!(
        Instant::now() < deadline,
        "panel persistence exceeded wall budget"
    );
    let request = if let Some(body) = body {
        client
            .post(format!("{url}{route}"))
            .header("x-panel-token", "persistence-fixture")
            .json(body)
    } else {
        client.get(format!("{url}{route}"))
    };
    let response = request.send().unwrap();
    (response.status().as_u16(), response.text().unwrap())
}
pub(super) fn post(
    client: &reqwest::blocking::Client,
    url: &str,
    body: Value,
    deadline: Instant,
) -> (u16, Value) {
    let (code, text) = call(client, url, "api/save", Some(&body), deadline);
    (code, serde_json::from_str(&text).unwrap())
}
pub(super) fn page(client: &reqwest::blocking::Client, url: &str, deadline: Instant) -> Vec<Value> {
    let (code, text) = call(client, url, "", None, deadline);
    assert_eq!(code, 200, "{text}");
    let start = text
        .find("<script id=\"items-data\" type=\"application/json\">")
        .unwrap()
        + "<script id=\"items-data\" type=\"application/json\">".len();
    let end = start + text[start..].find("</script>").unwrap();
    serde_json::from_str(&text[start..end]).unwrap()
}
pub(super) fn excluded(db: &Path) -> BTreeSet<String> {
    let con = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    con.busy_timeout(Duration::from_secs(1)).unwrap();
    let mut query = con
        .prepare("SELECT content_key FROM items WHERE included=0")
        .unwrap();
    let rows = query.query_map([], |row| row.get(0)).unwrap();
    rows.collect::<rusqlite::Result<_>>().unwrap()
}
fn plan(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
pub(super) fn snapshot(db: &Path, project: &Path) -> (Vec<Value>, Value, Value, String) {
    let mut cat = Catalog::open(db, "observe", project).unwrap();
    (
        cat.all(None).unwrap(),
        cat.dispatch(json!({"operation":"published_keys"})).unwrap(),
        cat.dispatch(json!({"operation":"published_sets"})).unwrap(),
        _native::migration_signature::signature(db, &[]).unwrap(),
    )
}
fn scope(client: &reqwest::blocking::Client, url: &str, db: &Path, deadline: Instant) {
    let known = (0..3).map(key).collect::<Vec<_>>();
    let (code, body) = post(
        client,
        url,
        json!({"known":known,"excluded":[key(1)]}),
        deadline,
    );
    assert_eq!(code, 200);
    assert_eq!(body["excluded"], 1);
    assert_eq!(excluded(db), BTreeSet::from([key(1)]));
    assert_eq!(
        post(client, url, json!({"known":known,"excluded":[]}), deadline).0,
        200
    );
    assert!(excluded(db).is_empty());
    assert_eq!(
        post(
            client,
            url,
            json!({"known":known,"excluded":[key(2)]}),
            deadline
        )
        .0,
        200
    );
    assert_eq!(
        post(
            client,
            url,
            json!({"known":[key(0),key(1)],"excluded":[]}),
            deadline
        )
        .0,
        200
    );
    assert_eq!(excluded(db), BTreeSet::from([key(2)]));
    assert_eq!(
        post(
            client,
            url,
            json!({"known":known,"excluded":[key(0),key(2)]}),
            deadline
        )
        .0,
        200
    );
    assert_eq!(
        post(
            client,
            url,
            json!({"known":[key(0),key(1)],"excluded":[key(0)]}),
            deadline
        )
        .0,
        200
    );
    assert_eq!(excluded(db), BTreeSet::from([key(0), key(2)]));
    let before = excluded(db);
    let (code, body) = post(client, url, json!({"excluded":[]}), deadline);
    assert_eq!(code, 409);
    assert!(body["error"].as_str().unwrap().contains("reload"));
    assert_eq!(excluded(db), before);
    for bad in [json!("everything"), json!(7), json!([1, 2]), json!({"a":1})] {
        assert_eq!(
            post(client, url, json!({"known":bad,"excluded":[]}), deadline).0,
            409
        );
        assert_eq!(excluded(db), before);
    }
    assert_eq!(
        post(client, url, json!({"known":known,"excluded":[]}), deadline).0,
        200
    );
    let (code, body) = post(
        client,
        url,
        json!({"known":[key(0),"s:vanished","nonsense"],"excluded":[key(0),"s:vanished"]}),
        deadline,
    );
    assert_eq!(code, 409);
    assert!(body["error"].as_str().unwrap().contains("reload"));
    assert!(excluded(db).is_empty());
    let (code, body) = post(
        client,
        url,
        json!({"known":[],"excluded":[],"wat":1}),
        deadline,
    );
    assert_eq!(code, 400);
    assert!(body["error"].as_str().unwrap().contains("unknown keys"));
}
#[cfg(windows)]
fn write_refusal(client: &reqwest::blocking::Client, url: &str, db: &Path, deadline: Instant) {
    use std::os::windows::fs::OpenOptionsExt;
    let path = db.with_file_name("pack_plan.json");
    let before = std::fs::read(&path).unwrap();
    let body = json!({"known":(0..5).map(key).collect::<Vec<_>>(),
        "excluded":[key(1)],"packs":[[key(0),2]]});
    let occupied = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1 | 2)
        .open(&path)
        .unwrap();
    let (status, response) = post(client, url, body.clone(), deadline);
    assert_eq!(status, 503, "{response}");
    assert_ne!(response.get("ok"), Some(&Value::Bool(true)));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(excluded(db), BTreeSet::from([key(1)]));
    drop(occupied);
    assert_eq!(post(client, url, body, deadline).0, 200);
    let saved = plan(&path);
    assert_eq!(saved["targets"], json!([[key(0), 2]]));
    assert_eq!(saved["excluded"], json!([key(1)]));
}

fn intent(
    client: &reqwest::blocking::Client,
    url: &str,
    db: &Path,
    root: &Path,
    deadline: Instant,
) {
    let path = db.with_file_name("pack_plan.json");
    let known = (0..5).map(key).collect::<Vec<_>>();
    let save = |targets: Value, scope: Value, excluded: Value| json!({"known":scope,"excluded":excluded,"packs":targets});
    assert_eq!(
        post(
            client,
            url,
            save(json!([[key(0), 2]]), json!(known), json!([])),
            deadline
        )
        .0,
        200
    );
    for _ in 0..2 {
        let cards = page(client, url, deadline);
        let item = cards.iter().find(|card| card["key"] == key(0)).unwrap();
        assert_eq!(item["pack"], 2);
        let pairs = cards
            .iter()
            .filter(|card| card.get("pack").is_some())
            .map(|card| json!([card["key"], card["pack"]]))
            .collect::<Vec<_>>();
        assert_eq!(
            post(
                client,
                url,
                save(json!(pairs), json!(known), json!([])),
                deadline
            )
            .0,
            200
        );
        assert_eq!(
            plan(&path)["moves"],
            json!([{"key":key(0),"label":item["label"],"from_pack":1,"to_pack":2}])
        );
    }
    // Reset intent only inside this owned fixture, retaining all media and publication identities.
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        post(
            client,
            url,
            save(json!([[key(4), 2]]), json!(known), json!([])),
            deadline
        )
        .0,
        200
    );
    let cards = page(client, url, deadline);
    assert_eq!(
        cards.iter().find(|card| card["key"] == key(4)).unwrap()["pack"],
        2
    );
    assert_eq!(plan(&path)["moves"], json!([]));
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        post(
            client,
            url,
            save(json!([[key(0), 2]]), json!(known), json!([key(2)])),
            deadline
        )
        .0,
        200
    );
    let before = plan(&path);
    assert_eq!(
        post(
            client,
            url,
            save(json!([[key(1), 1]]), json!([key(1)]), json!([])),
            deadline
        )
        .0,
        200
    );
    let after = plan(&path);
    assert_eq!(after["moves"], before["moves"]);
    assert_eq!(after["held"], before["held"]);
    assert_eq!(_native::intent::targets(&after).unwrap()[&key(0)], 2);
    assert!(
        after["excluded"]
            .as_array()
            .unwrap()
            .contains(&json!(key(2)))
    );
    assert_eq!(
        post(
            client,
            url,
            json!({"known":known,"excluded":[key(1)]}),
            deadline
        )
        .0,
        200
    );
    assert_eq!(_native::intent::targets(&plan(&path)).unwrap()[&key(0)], 2);
    assert!(
        plan(&path)["excluded"]
            .as_array()
            .unwrap()
            .contains(&json!(key(1)))
    );
    assert_eq!(
        post(
            client,
            url,
            save(json!([[key(0), 2]]), json!(known), json!([])),
            deadline
        )
        .0,
        200
    );
    let before = std::fs::read(&path).unwrap();
    for targets in [
        json!([[key(0), -1]]),
        json!([[key(0), 0]]),
        json!([[key(0), true]]),
        json!([[key(0), 1.5]]),
        json!([[key(0), 1_u64 << 54]]),
        json!([[key(0), 1], [key(0), 2]]),
        json!([["s:unknown", 1]]),
    ] {
        assert_eq!(
            post(
                client,
                url,
                save(targets, json!(known), json!([key(1)])),
                deadline
            )
            .0,
            400
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(excluded(db).is_empty());
    }
    assert_eq!(
        post(
            client,
            url,
            save(json!([[key(0), 2]]), json!([key(1)]), json!([])),
            deadline
        )
        .0,
        400
    );
    assert_eq!(
        post(
            client,
            url,
            save(json!([]), json!([key(0)]), json!([key(1)])),
            deadline
        )
        .0,
        400
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(excluded(db).is_empty());
    let unchanged = snapshot(db, root);
    for text in [
        "null",
        "{broken",
        "{\"version\":99}",
        "{\"version\":1,\"moves\":[],\"held\":[],\"targets\":[[\"x\",-1]]}",
    ] {
        _native::atomic::write_bytes(&path, text.as_bytes()).unwrap();
        assert_eq!(
            post(
                client,
                url,
                save(json!([[key(0), 2]]), json!(known), json!([key(1)])),
                deadline
            )
            .0,
            409
        );
        assert_eq!(call(client, url, "", None, deadline).0, 409);
        assert_eq!(std::fs::read(&path).unwrap(), text.as_bytes());
        assert_eq!(snapshot(db, root), unchanged);
    }
    let old = json!({"version":1,"held":[],"moves":[{"key":key(0),"label":"first","from_pack":1,"to_pack":2}]});
    _native::atomic::write_json(&path, &old).unwrap();
    assert_eq!(page(client, url, deadline)[0]["pack"], 2);
    assert_eq!(
        post(
            client,
            url,
            save(json!([[key(1), 1]]), json!([key(1)]), json!([])),
            deadline
        )
        .0,
        200
    );
    assert_eq!(plan(&path)["moves"], old["moves"]);
    assert_eq!(
        post(
            client,
            url,
            save(json!([[key(0), 2]]), json!(known), json!([key(1)])),
            deadline
        )
        .0,
        200
    );
    assert!(
        plan(&path)["excluded"]
            .as_array()
            .unwrap()
            .contains(&json!(key(1)))
    );
    assert_eq!(_native::intent::targets(&plan(&path)).unwrap()[&key(0)], 2);
    let mut cat = Catalog::open(db, "move current membership", root).unwrap();
    cat.uploaded(&json!({"content_key":key(0),"base":"fixture","set_name":"two","custom_emoji_id":"MOVED-CID"}),"fixture").unwrap();
    drop(cat);
    assert_eq!(
        post(
            client,
            url,
            save(json!([[key(0), 1]]), json!(known), json!([])),
            deadline
        )
        .0,
        200
    );
    assert_eq!(
        plan(&path)["moves"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (
                row["from_pack"].as_u64().unwrap(),
                row["to_pack"].as_u64().unwrap()
            ))
            .collect::<Vec<_>>(),
        [(2, 1)]
    );
}

pub fn check() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let root = super::tooling_fixture::Root::new("original panel persistence");
    let deadline = Instant::now() + Duration::from_secs(60);
    let db = root.0.join("catalog.db");
    std::fs::create_dir(root.0.join("assets")).unwrap();
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
            root.0.join("assets").join(name),
        )
        .unwrap();
    }
    Python::initialize();
    let mut cat = Catalog::open(&db, "fixture", &root.0).unwrap();
    let mut originals = Vec::new();
    Python::attach(|py| -> PyResult<()> {
        let image = py.import("PIL.Image")?;
        assert_eq!(image.getattr("__version__")?.extract::<String>()?, "12.3.0");
        for i in 0..5 {
            let path = root.0.join(format!("{i}.png"));
            let value = image.call_method1("new", ("RGBA", (100, 100), (40 * i, 40, 80, 255)))?;
            let saved = value.call_method1("save", (&path, "PNG"));
            let closed = value.call_method0("close");
            saved?;
            closed?;
            originals.push((path.clone(), std::fs::read(&path)?));
            cat.insert(
                &json!({"content_key":key(i),"fmt":"static","file_path":path}),
                "fixture",
            )
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
        }
        Ok(())
    })
    .unwrap();
    cat.order(&(0..5).map(key).collect::<Vec<_>>()).unwrap();
    cat.dispatch(json!({"operation":"set_meta","key":"order_seeded","value":"1"}))
        .unwrap();
    drop(cat);
    let log = Arc::new(RunLog::open(&root.0, "panel-persistence", vec![]));
    let identity = _native::panel_session::identity(&db, false, &[1, 2]).unwrap();
    let panel = Panel {
        root: root.0.clone(),
        db: db.clone(),
        token: "persistence-fixture".into(),
        options: json!({"keep_sets":{"one":1,"two":2},"show_published":false,"branded":false,"no_open":true}),
        branding: None,
        detected_bot: Mutex::new(String::new()),
        view: Mutex::new(json!({"view":[],"by_key":{},"hidden":0})),
        session: identity.clone(),
        utc: || "fixture".into(),
        log: log.clone(),
        events: Mutex::new(Events::default()),
        previews: Previews::new(
            MediaAdapter {
                python: std::env::var_os("PYO3_PYTHON")
                    .expect("CI must select the retained preview interpreter")
                    .into(),
                root: project.into(),
                scratch: root.0.join("codec"),
            },
            &root.0,
            1,
        ),
    };
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
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let address = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let connections = accepted.clone();
    let listener = listener.tap_io(move |_| {
        connections.fetch_add(1, Ordering::SeqCst);
    });
    let (stop, done) = tokio::sync::oneshot::channel::<()>();
    let session_reply: super::panel_session_cases::SessionReply = Arc::new(Mutex::new(None));
    let arrivals: super::panel_concurrency_cases::Arrivals = Arc::new(Mutex::new(None));
    let panel = Arc::new(panel);
    let router = panel_http::router(panel.clone())
        .layer(axum::middleware::from_fn_with_state(
            session_reply.clone(),
            super::panel_session_cases::override_session,
        ))
        .layer(axum::middleware::from_fn_with_state(
            arrivals.clone(),
            super::panel_concurrency_cases::acknowledge,
        ));
    let task = runtime.spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = done.await;
            })
            .await
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let url = format!("http://{address}/");
        super::panel_logging_cases::check(
            &client,
            &url,
            log.path.as_deref().unwrap(),
            &root.0.join("assets"),
            deadline,
        );
        super::panel_connection_cases::check(address.port(), &accepted, &db, deadline);
        super::panel_session_cases::check(
            &runtime,
            address.port(),
            &db,
            &root.0,
            &session_reply,
            &log,
            deadline,
        );
        super::panel_request_cases::check(&client, &url, &db, &root.0, deadline);
        super::panel_concurrency_cases::check(
            &client, &url, &db, &root.0, &panel, &arrivals, deadline,
        );
        scope(&client, &url, &db, deadline);
        super::panel_stale_cases::order(&client, &url, &db, &root.0, deadline);
        let mut cat = Catalog::open(&db, "fixture", &root.0).unwrap();
        for i in 0..4 {
            cat.uploaded(&json!({"content_key":key(i),"base":"fixture","set_name":if i<2{"one"}else{"two"},"custom_emoji_id":format!("CID{i}")}),"fixture").unwrap();
        }
        drop(cat);
        intent(&client, &url, &db, &root.0, deadline);
        #[cfg(windows)]
        write_refusal(&client, &url, &db, deadline);
        super::panel_stale_cases::saves(&client, &url, &db, &root.0, deadline);
        super::panel_migration_cases::check(&client, &url, &db, &root.0, project, deadline);
        assert_eq!(snapshot(&db, &root.0).0.len(), 5);
        for (path, bytes) in &originals {
            assert_eq!(std::fs::read(path).unwrap(), *bytes);
        }
        assert!(!root.0.join("codec").exists());
        super::panel_preview_contracts::check(&client, &url, &panel, &root.0, deadline);
    }));
    let _ = stop.send(());
    let stopped =
        runtime.block_on(async { tokio::time::timeout(Duration::from_secs(5), task).await });
    runtime.shutdown_timeout(Duration::from_secs(1));
    assert_eq!(threads.load(Ordering::SeqCst), 0);
    stopped
        .expect("persistence server shutdown timed out")
        .unwrap()
        .unwrap();
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    drop(panel);
    log.finish(if result.is_ok() { 0 } else { 1 });
    drop(log);
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    assert!(Instant::now() < deadline);
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    root.clean().unwrap();
}
