//! Original view consequences at the public catalog/model/router seams; no browser or codec fake.
use _native::{
    catalog::Catalog,
    config::Config,
    logging::RunLog,
    media_adapter::MediaAdapter,
    panel_events::Events,
    panel_http::{self, Panel},
    panel_preview::Previews,
    panel_view,
};
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

fn key(index: usize) -> String {
    format!("s:item{index:030}")
}
fn add(cat: &mut Catalog, root: &Path, index: usize, label: &str) {
    cat.insert(
        &json!({"content_key":key(index),"fmt":"static",
        "file_path":root.join(format!("media/static/i{index}.png")),
        "emojis":["😀"],"keywords":[label]}),
        "fixture",
    )
    .unwrap();
}
fn published(cat: &mut Catalog, index: usize, set: Option<&str>) {
    cat.uploaded(
        &json!({"content_key":key(index),"custom_emoji_id":format!("cid{index}"),
        "base":"pk","set_name":set}),
        "fixture",
    )
    .unwrap();
}
fn view(cat: &mut Catalog, keep: Value, show: bool, logo: Option<&Path>) -> Value {
    let items = cat.all(None).unwrap();
    let published = cat.dispatch(json!({"operation":"published_keys"})).unwrap();
    let sets = cat.dispatch(json!({"operation":"published_sets"})).unwrap();
    panel_view::build(
        json!({"items":items,"published":published,"published_sets":sets,
        "keep_sets":keep,"show_published":show,"branded":logo.is_some(),"logo_path":logo}),
    )
    .unwrap()
}
fn keys(result: &Value) -> Vec<&str> {
    result["view"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|card| card["isLogo"] != true)
        .map(|card| card["key"].as_str().unwrap())
        .collect()
}
fn page(client: &reqwest::blocking::Client, url: &str, deadline: Instant) -> Vec<Value> {
    assert!(
        Instant::now() < deadline,
        "original view cohort exceeded its wall budget"
    );
    let response = client.get(url).send().unwrap();
    assert_eq!(response.status(), 200);
    let html = response.text().unwrap();
    let script = "23a5e175b383";
    let icon = "4636a2cbdf7a";
    assert!(!html.contains("__ASSET_VER__"));
    assert!(!html.contains("__ICON_VER__"));
    assert!(html.contains("const PER_SET = 200;"));
    assert!(html.contains(&format!("/static/panel-grid.js?v={script}")));
    assert!(html.contains(&format!("/static/logo-128.png?v={icon}")));
    let marker = "<script id=\"items-data\" type=\"application/json\">";
    let start = html.find(marker).unwrap() + marker.len();
    let end = start + html[start..].find("</script>").unwrap();
    serde_json::from_str(&html[start..end]).unwrap()
}

fn branding(root: &Path, project: &Path, logo: &Path, deadline: Instant) {
    let directory = root.join("branding");
    std::fs::create_dir(&directory).unwrap();
    let db = directory.join("catalog.db");
    let mut cat = Catalog::open(&db, "fixture", root).unwrap();
    for i in 0..3 {
        add(&mut cat, root, i, &format!("item{i}"));
    }
    drop(cat);
    let config = Config::load_with_environment(
        root.into(),
        &BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("BRAND_LOGO_BOTS".into(), "YourEmojiBot".into()),
            ("BRAND_LOGO_PATH".into(), logo.to_str().unwrap().into()),
        ]),
    )
    .unwrap();
    let log = Arc::new(RunLog::open(root, "original-panel-view", vec![]));
    let panel = Arc::new(Panel {
        root: root.into(),
        db: db.clone(),
        token: "original-view-fixture".into(),
        options: json!({"keep_sets":{},"show_published":false,"branded":false,
            "logo_path":config.logo_path(false).unwrap(),"no_open":true}),
        branding: Some((
            "must-not-contact-telegram".into(),
            "http://127.0.0.1:9".into(),
            config.logo_bots(false).unwrap(),
        )),
        detected_bot: Mutex::new("YourEmojiBot".into()),
        view: Mutex::new(json!({"view":[],"by_key":{},"hidden":0})),
        session: json!({"application":"numera-emoji-mapper-panel","catalog":"fixture","all":false,"packs":[]}),
        utc: || "fixture".into(),
        log: log.clone(),
        events: Mutex::new(Events::default()),
        previews: Previews::new(
            MediaAdapter {
                python: "must-not-run-view-codec".into(),
                root: project.into(),
                scratch: directory.join("codec"),
            },
            &directory,
            1,
        ),
    });
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
    let (stop, done) = tokio::sync::oneshot::channel::<()>();
    let router = panel_http::router(panel.clone());
    // Exercise refresh through the actual router without unrelated startup preview/Telegram warming.
    let server = runtime.spawn(async move {
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
        let shown = page(&client, &url, deadline);
        let script = client
            .get(format!("{url}static/panel-grid.js?v=23a5e175b383"))
            .send()
            .unwrap();
        assert_eq!(script.status(), 200);
        assert_eq!(script.headers()["content-type"], "application/javascript");
        assert!(
            script.headers()["cache-control"]
                .to_str()
                .unwrap()
                .contains("immutable")
        );
        assert_eq!(
            script.bytes().unwrap().as_ref(),
            std::fs::read(project.join("assets/panel-grid.js")).unwrap()
        );
        assert_eq!(shown.len(), 4);
        assert_eq!(shown[0]["key"], "__brand_logo__");
        assert_eq!(shown[0]["isLogo"], true);
        assert!(shown[1..].iter().all(|card| card["isLogo"] != true));
        assert_eq!(
            panel.view.lock().unwrap()["by_key"]["__brand_logo__"],
            json!(logo)
        );
        for bot in ["YourCoinEmojiBot", ""] {
            *panel.detected_bot.lock().unwrap() = bot.into();
            let cards = page(&client, &url, deadline);
            assert_eq!(cards.len(), 3, "{bot}");
            assert!(cards.iter().all(|card| card["isLogo"] != true));
        }
        *panel.detected_bot.lock().unwrap() = "YourEmojiBot".into();
        let held = root.join("brand-original.png");
        std::fs::rename(logo, &held).unwrap();
        let missing = page(&client, &url, deadline);
        assert_eq!(missing.len(), 3);
        assert!(missing.iter().all(|card| card["isLogo"] != true));
        std::fs::rename(&held, logo).unwrap();
        let cat = Catalog::open(&db, "reopen", root).unwrap();
        assert_eq!(cat.all(None).unwrap().len(), 3);
        assert_eq!(cat.meta("order_seeded").unwrap().as_deref(), Some("1"));
        assert!(!directory.join("codec").exists());
    }));
    let _ = stop.send(());
    let stopped =
        runtime.block_on(async { tokio::time::timeout(Duration::from_secs(5), server).await });
    runtime.shutdown_timeout(Duration::from_secs(1));
    assert_eq!(
        threads.load(Ordering::SeqCst),
        0,
        "view runtime thread leaked"
    );
    stopped
        .expect("view server shutdown timed out")
        .unwrap()
        .unwrap();
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    drop(panel);
    log.finish(if result.is_ok() { 0 } else { 1 });
    drop(log);
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
}

pub fn check() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let root = super::tooling_fixture::Root::new("original panel view");
    let deadline = Instant::now() + Duration::from_secs(60);
    std::fs::create_dir_all(root.0.join("media/static")).unwrap();
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
    let logo = root.0.join("brand.png");
    Python::attach(|py| -> PyResult<()> {
        let module = py.import("PIL.Image")?;
        assert_eq!(
            module.getattr("__version__")?.extract::<String>()?,
            "12.3.0"
        );
        for path in (0..6)
            .map(|i| root.0.join(format!("media/static/i{i}.png")))
            .chain([logo.clone()])
        {
            let image = module.call_method1("new", ("RGBA", (40, 40), (10, 20, 30, 255)))?;
            let saved = image.call_method1("save", (&path, "PNG"));
            let closed = image.call_method0("close");
            saved?;
            closed?;
        }
        Ok(())
    })
    .unwrap();
    let original = (0..6)
        .map(|i| root.0.join(format!("media/static/i{i}.png")))
        .chain([logo.clone()])
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect::<Vec<_>>();
    branding(&root.0, project, &logo, deadline);
    let db = root.0.join("catalog.db");
    let mut cat = Catalog::open(&db, "fixture", &root.0).unwrap();
    for i in 0..5 {
        add(&mut cat, &root.0, i, &format!("item{i}"));
    }
    for i in [0, 1] {
        published(&mut cat, i, Some("pk1_by_bot"));
    }
    published(&mut cat, 2, Some("pk2_by_bot"));
    let before = cat.all(None).unwrap();
    let publication_before = cat.dispatch(json!({"operation":"published_keys"})).unwrap();
    let hidden = view(&mut cat, json!({}), false, None);
    assert_eq!(hidden["hidden"], 3);
    assert_eq!(keys(&hidden), [key(3), key(4)]);
    assert!(!root.0.join("publish_pk.json").exists());
    assert_eq!(cat.all(None).unwrap(), before);
    assert_eq!(
        cat.dispatch(json!({"operation":"published_keys"})).unwrap(),
        publication_before
    );
    published(&mut cat, 3, None);
    assert!(!keys(&view(&mut cat, json!({}), false, None)).contains(&key(3).as_str()));
    let all = view(&mut cat, json!({}), true, None);
    assert_eq!(all["hidden"], 0);
    assert_eq!(all["view"].as_array().unwrap().len(), 5);
    drop(cat);
    let mut cat = Catalog::open(&db, "reopen", &root.0).unwrap();
    assert_eq!(cat.all(None).unwrap().len(), 5);
    assert_eq!(
        cat.dispatch(json!({"operation":"custom_id","base":"pk","content_key":key(0)}))
            .unwrap(),
        "cid0"
    );
    add(&mut cat, &root.0, 5, "item5");
    published(&mut cat, 3, Some("pk2_by_bot"));
    let state = json!({"sets":[{"name":"pk1_by_bot","index":1,"live":3,"logo":true},
        {"name":"pk2_by_bot","index":2,"live":3,"logo":true}]});
    let state_path = root.0.join("publish_pk.json");
    _native::atomic::write_json(&state_path, &state).unwrap();
    let state_bytes = std::fs::read(&state_path).unwrap();
    assert_eq!(
        panel_view::packs_named(&root.0, &[2]).unwrap(),
        json!({"pk2_by_bot":2})
    );
    assert_eq!(
        panel_view::packs_named(&root.0, &[1, 2]).unwrap(),
        json!({"pk1_by_bot":1,"pk2_by_bot":2})
    );
    assert_eq!(panel_view::packs_named(&root.0, &[9]).unwrap(), json!({}));
    let keep = panel_view::packs_named(&root.0, &[1, 2]).unwrap();
    let shown = view(&mut cat, keep.clone(), false, None);
    let cards = shown["view"].as_array().unwrap();
    assert_eq!(cards[0]["pack"], 1);
    assert_eq!(cards[2]["pack"], 2);
    assert!(cards[4]["pack"].is_null());
    let branded = view(&mut cat, keep, false, Some(&logo));
    let cards = branded["view"].as_array().unwrap();
    let logos = cards
        .iter()
        .filter(|card| card["isLogo"] == true)
        .collect::<Vec<_>>();
    assert_eq!(
        logos
            .iter()
            .map(|card| card["pack"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    for card in logos {
        assert_ne!(card["key"], "__brand_logo__");
        let key = card["key"].as_str().unwrap();
        assert_eq!(branded["by_key"][key], json!(logo));
        let at = cards.iter().position(|item| item["key"] == key).unwrap();
        let first = cards
            .iter()
            .position(|item| item["pack"] == card["pack"] && item["isLogo"] != true)
            .unwrap();
        assert!(at < first);
    }
    let normal = view(&mut cat, json!({}), false, Some(&logo));
    assert_eq!(normal["view"][0]["key"], "__brand_logo__");
    assert_eq!(
        normal["view"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|card| card["isLogo"] == true)
            .count(),
        1
    );
    let normal = view(&mut cat, json!({}), false, None);
    assert_eq!(normal["hidden"], 4);
    assert_eq!(keys(&normal), [key(4), key(5)]);
    assert!(
        normal["view"]
            .as_array()
            .unwrap()
            .iter()
            .all(|card| card["pack"].is_null())
    );
    let one = view(
        &mut cat,
        panel_view::packs_named(&root.0, &[2]).unwrap(),
        false,
        None,
    );
    assert_eq!(one["hidden"], 2);
    assert_eq!(keys(&one), [key(2), key(3), key(4), key(5)]);
    published(&mut cat, 4, None);
    let unknown = view(
        &mut cat,
        panel_view::packs_named(&root.0, &[2]).unwrap(),
        false,
        None,
    );
    assert!(!keys(&unknown).contains(&key(4).as_str()));
    assert_eq!(cat.all(None).unwrap().len(), 6);
    assert_eq!(std::fs::read(state_path).unwrap(), state_bytes);
    drop(cat);
    let copy_db = root.0.join("copy.db");
    let mut cat = Catalog::open(&copy_db, "fixture", &root.0).unwrap();
    add(&mut cat, &root.0, 0, "premium-id:5406926593698312391");
    add(&mut cat, &root.0, 1, "hand drawn");
    let copies = view(&mut cat, json!({}), false, None);
    assert_eq!(copies["view"][0]["label"], "premium-id:5406926593698312391");
    assert_eq!(copies["view"][0]["copyId"], "5406926593698312391");
    assert_eq!(copies["view"][1]["label"], "hand drawn");
    assert_eq!(copies["view"][1]["copyId"], "");
    drop(cat);
    for (path, bytes) in original {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert!(Instant::now() < deadline);
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    root.clean().unwrap();
}
