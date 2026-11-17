use super::runtime;
use runtime::{
    catalog::Catalog,
    logging::RunLog,
    media_native_adapter::NativeMediaAdapter,
    panel_events::Events,
    panel_http::{self, Panel},
    panel_preview::Previews,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use std::time::Duration;

pub const TOKEN: &str = "test-token-value";
pub const UTC: &str = "2026-01-01T00:00:00Z";
static NEXT: AtomicU64 = AtomicU64::new(0);
pub fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
pub struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new() -> Self {
        let parent = project().join("logs/tmp/baseline-panel");
        std::fs::create_dir_all(&parent).unwrap();
        loop {
            let path = parent.join(format!(
                "case-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path.canonicalize().unwrap()),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("create owned panel fixture: {e}"),
            }
        }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
pub fn assets(root: &Path) {
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
            project().join("assets").join(name),
            root.join("assets").join(name),
        )
        .unwrap();
    }
}
pub fn script() -> String {
    [
        "grid", "motion", "save", "drag", "actions", "holding", "draft",
    ]
    .map(|name| std::fs::read_to_string(project().join(format!("assets/panel-{name}.js"))).unwrap())
    .join("\n")
}
pub fn block<'a>(src: &'a str, start: &str, end: &str) -> &'a str {
    let begin = src.find(start).unwrap();
    let end = begin + src[begin..].find(end).unwrap();
    &src[begin..end]
}
pub struct Fixture {
    pub scratch: Scratch,
    pub db: PathBuf,
    pub plan_path: PathBuf,
    pub keys: Vec<String>,
    pub url: String,
    pub port: u16,
    pub client: reqwest::blocking::Client,
    runtime: Option<tokio::runtime::Runtime>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
    threads: Arc<AtomicUsize>,
}
impl Fixture {
    pub fn guard() -> Self {
        Self::new(2, false)
    }
    pub fn intent() -> Self {
        Self::new(5, true)
    }
    pub fn new(count: usize, published: bool) -> Self {
        let scratch = Scratch::new();
        let root = &scratch.0;
        assets(root);
        let db = root.join("catalog.db");
        let plan_path = root.join("pack_plan.json");
        let mut keys = Vec::new();
        {
            let mut cat = Catalog::open(&db, UTC, root).unwrap();
            for i in 0..count {
                let image = if published {
                    image::RgbaImage::from_pixel(100, 100, image::Rgba([40 * i as u8, 40, 80, 255]))
                } else {
                    image::RgbaImage::from_pixel(40, 40, image::Rgba([10, 20, 30, 255]))
                };
                let file = root.join(format!("{i}.png"));
                image.save(&file).unwrap();
                let key = if published {
                    runtime::native_raster::content_key(&image).unwrap()
                } else {
                    format!("s:item{i:030}")
                };
                let mut item = json!({"content_key":key,"fmt":"static","file_path":file});
                if !published {
                    item["emojis"] = json!(["😀"]);
                    item["keywords"] = json!([format!("item{i}")]);
                }
                cat.insert(&item, UTC).unwrap();
                if published && i < 4 {
                    cat.uploaded(
                        &json!({"content_key":key,"custom_emoji_id":format!("CID{i}"),
                        "base":"fixture","set_name":if i<2 {"one"} else {"two"}}),
                        UTC,
                    )
                    .unwrap();
                }
                keys.push(key);
            }
            cat.order(&keys).unwrap();
            cat.dispatch(json!({"operation":"set_meta","key":"order_seeded","value":"1"}))
                .unwrap();
        }
        let log = Arc::new(RunLog::open(root, "baseline-panel", vec![]));
        let options = json!({"keep_sets":if published {json!({"one":1,"two":2})} else {json!({})},
            "show_published":false,"branded":false,"no_open":true,"preview_fps":15});
        let view = Self::fresh_view(&db, root, &options);
        let panel = Arc::new(Panel {
            root: root.clone(),
            db: db.clone(),
            token: TOKEN.into(),
            options,
            branding: None,
            detected_bot: Mutex::new(String::new()),
            view: Mutex::new(view),
            session: runtime::panel_session::identity(&db, false, &[]).unwrap(),
            last_refreshed: std::sync::Mutex::new(Some(chrono::Utc::now())),
            utc: || UTC.into(),
            log: log.clone(),
            events: Mutex::new(Events::default()),
            previews: Previews::new(
                NativeMediaAdapter {
                    executable: "must-not-run-panel-fixture-codec".into(),
                    root: root.clone(),
                    scratch: root.join("codec"),
                },
                root,
                1,
            ),
        });
        let threads = Arc::new(AtomicUsize::new(0));
        let started = threads.clone();
        let stopped = threads.clone();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(4)
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
        let port = listener.local_addr().unwrap().port();
        let (stop, done) = tokio::sync::oneshot::channel();
        let router = panel_http::router(panel.clone());
        let task = runtime.spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = done.await;
                })
                .await
                .unwrap();
        });
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        Self {
            scratch,
            db,
            plan_path,
            keys,
            url: format!("http://127.0.0.1:{port}"),
            port,
            client,
            runtime: Some(runtime),
            stop: Some(stop),
            task: Some(task),
            threads,
        }
    }
    fn fresh_view(db: &Path, root: &Path, options: &Value) -> Value {
        let mut cat = Catalog::open(db, UTC, root).unwrap();
        let mut request = options.clone();
        request["items"] = json!(cat.all(None).unwrap());
        request["published"] = cat.dispatch(json!({"operation":"published_keys"})).unwrap();
        request["published_sets"] = cat.dispatch(json!({"operation":"published_sets"})).unwrap();
        runtime::panel_view::build(request).unwrap()
    }
    pub fn root(&self) -> &Path {
        &self.scratch.0
    }
    pub fn post(&self, path: &str, body: Value) -> (u16, Value) {
        self.raw_post(
            path,
            body.to_string(),
            Some(TOKEN),
            "application/json",
            None,
        )
    }
    pub fn raw_post(
        &self,
        path: &str,
        body: String,
        token: Option<&str>,
        ctype: &str,
        origin: Option<&str>,
    ) -> (u16, Value) {
        let mut request = self
            .client
            .post(format!("{}{path}", self.url))
            .header("Content-Type", ctype)
            .body(body);
        if let Some(token) = token {
            request = request.header("X-Panel-Token", token);
        }
        if let Some(origin) = origin {
            request = request.header("Origin", origin);
        }
        let response = request.send().unwrap();
        let code = response.status().as_u16();
        let bytes = response.bytes().unwrap();
        (
            code,
            if bytes.is_empty() {
                json!({})
            } else {
                serde_json::from_slice(&bytes).unwrap()
            },
        )
    }
    pub fn get(&self, path: &str, host: Option<&str>) -> (u16, Vec<u8>) {
        let mut request = self.client.get(format!("{}{path}", self.url));
        if let Some(host) = host {
            request = request.header("Host", host);
        }
        let response = request.send().unwrap();
        (
            response.status().as_u16(),
            response.bytes().unwrap().to_vec(),
        )
    }
    pub fn page(&self) -> Vec<Value> {
        let (status, body) = self.get("/", None);
        assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
        let html = String::from_utf8(body).unwrap();
        let marker = "<script id=\"items-data\" type=\"application/json\">";
        let start = html.find(marker).unwrap() + marker.len();
        let end = start + html[start..].find("</script>").unwrap();
        serde_json::from_str(&html[start..end]).unwrap()
    }
    pub fn save(&self) -> (u16, Value) {
        self.save_with(json!([[self.keys[0], 2]]), json!(self.keys), json!([]))
    }
    pub fn save_with(&self, packs: Value, known: Value, excluded: Value) -> (u16, Value) {
        let mut body = json!({"known":known,"excluded":excluded});
        if !packs.is_null() {
            body["packs"] = packs;
        }
        self.post("/api/save", body)
    }
    pub fn plan(&self) -> Value {
        serde_json::from_slice(&std::fs::read(&self.plan_path).unwrap()).unwrap()
    }
    pub fn cat(&self) -> Catalog {
        Catalog::open(&self.db, UTC, self.root()).unwrap()
    }
    pub fn included(&self, key: &str) -> bool {
        self.cat().get(key).unwrap().unwrap()["included"]
            .as_bool()
            .unwrap()
    }
    pub fn update(&self, sql: &str, key: &str) {
        let _owner = self.cat();
        let db = rusqlite::Connection::open(&self.db).unwrap();
        db.execute(sql, [key]).unwrap();
    }
    pub fn signature(&self) -> String {
        runtime::migration_signature::signature(&self.db, &[]).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.stop.take().unwrap().send(());
        let runtime = self.runtime.take().unwrap();
        let task = self.task.take().unwrap();
        runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap();
        });
        runtime.shutdown_timeout(Duration::from_secs(2));
        assert_eq!(
            self.threads.load(Ordering::SeqCst),
            0,
            "panel server thread leaked"
        );
    }
}
// Shared original _refuse_stale_scope helper; each concrete test retains its own input.
pub fn refuse_stale_scope(change: &str, refresh: bool) {
    let f = Fixture::intent();
    assert_eq!(f.save().0, 200);
    let old_page = f.page();
    let subject = f.keys.last().unwrap();
    match change {
        "rekey" => f.update(
            "UPDATE items SET content_key='s:new-identity' WHERE content_key=?",
            subject,
        ),
        "delete" => f.update("DELETE FROM items WHERE content_key=?", subject),
        "published" => {
            f.cat()
                .uploaded(
                    &json!({"content_key":subject,"custom_emoji_id":"NEW-CID",
            "base":"fixture","set_name":"hidden-finished-pack"}),
                    UTC,
                )
                .unwrap();
        }
        _ => panic!("unknown fixture change"),
    }
    if refresh {
        assert!(f.page().iter().all(|card| card["key"] != *subject));
    }
    let before_db = f.signature();
    let before_plan = std::fs::read(&f.plan_path).unwrap();
    let old_known = old_page
        .iter()
        .map(|card| card["key"].clone())
        .collect::<Vec<_>>();
    let (status, response) = f.save_with(json!([]), json!(old_known), json!([subject, f.keys[1]]));
    assert_eq!(status, 409, "{response}");
    assert!(
        response["error"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("reload")
    );
    assert_eq!(f.signature(), before_db);
    assert_eq!(std::fs::read(&f.plan_path).unwrap(), before_plan);
    let fresh = f
        .page()
        .iter()
        .map(|card| card["key"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        f.save_with(json!([]), json!(fresh), json!([f.keys[1]])).0,
        200
    );
    assert!(!f.included(&f.keys[1]));
}
pub mod bounded_codec;
pub struct Child(pub std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
pub fn command(executable: &Path) -> std::process::Command {
    let mut command = std::process::Command::new(executable);
    command.env_clear();
    for key in [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "TMPDIR",
        "LANG",
    ] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1");
    command
}
pub fn wait_output(command: &mut std::process::Command, budget: Duration) -> std::process::Output {
    use std::process::Stdio;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = Child(command.spawn().unwrap());
    let deadline = std::time::Instant::now() + budget;
    loop {
        if child.0.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "owned fixture process exceeded its deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut out = Vec::new();
    let mut err = Vec::new();
    use std::io::Read;
    child
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut out)
        .unwrap();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_end(&mut err)
        .unwrap();
    std::process::Output {
        status: child.0.wait().unwrap(),
        stdout: out,
        stderr: err,
    }
}
pub fn copied_binary(root: &Path) -> PathBuf {
    let file = root.join(if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    });
    let source = option_env!("CARGO_BIN_EXE_numera-emoji")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            project()
                .join("runtime/target/debug")
                .join(if cfg!(windows) {
                    "numera-emoji.exe"
                } else {
                    "numera-emoji"
                })
        });
    std::fs::copy(source, &file).unwrap();
    file
}
pub struct HangingBot {
    pub port: u16,
    pub seen: std::sync::mpsc::Receiver<()>,
    cancel: std::sync::mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl HangingBot {
    pub fn new() -> Self {
        use std::net::{TcpListener, TcpStream};
        use std::sync::mpsc;
        let stub = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = stub.local_addr().unwrap().port();
        stub.set_nonblocking(true).unwrap();
        let (cancel, done) = mpsc::channel();
        let (arrived, seen) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let mut connection: Option<TcpStream> = None;
            while std::time::Instant::now() < deadline {
                if done.try_recv().is_ok() {
                    return;
                }
                if connection.is_none()
                    && let Ok((socket, _)) = stub.accept()
                {
                    connection = Some(socket);
                    let _ = arrived.send(());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        Self {
            port,
            seen,
            cancel,
            thread: Some(thread),
        }
    }
}
impl Drop for HangingBot {
    fn drop(&mut self) {
        let _ = self.cancel.send(());
        self.thread.take().unwrap().join().unwrap();
    }
}
