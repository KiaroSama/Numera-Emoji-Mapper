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
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};
pub const TOKEN: &str = "test-token-value";
pub const UTC: &str = "2026-01-01T00:00:00Z";
static NEXT: AtomicU64 = AtomicU64::new(0);
pub fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
pub struct Data {
    pub root: PathBuf,
    pub db: PathBuf,
    pub keys: Vec<String>,
}
impl Data {
    pub fn new(n: usize) -> Self {
        let base = project().join("logs/tmp/panel-views");
        std::fs::create_dir_all(&base).unwrap();
        let root = loop {
            let path = base.join(format!(
                "case-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => break path.canonicalize().unwrap(),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("{e}"),
            }
        };
        std::fs::create_dir(root.join("assets")).unwrap();
        for name in runtime::asset_version::PANEL_SCRIPTS
            .iter()
            .copied()
            .chain(["panel.html", "logo-128.png"])
        {
            std::fs::copy(
                project().join("assets").join(name),
                root.join("assets").join(name),
            )
            .unwrap();
        }
        let db = root.join("catalog.db");
        let mut cat = Catalog::open(&db, UTC, &root).unwrap();
        let mut keys = Vec::new();
        for i in 0..n {
            let file = root.join(format!("media/static/i{i}.png"));
            png(&file, [10, 20, 30, 255], 40);
            let key = key(i);
            cat.insert(&json!({"content_key":key,"fmt":"static","file_path":file,"emojis":["😀"],"keywords":[format!("item{i}")]}),UTC).unwrap();
            keys.push(key);
        }
        cat.order(&keys).unwrap();
        cat.dispatch(json!({"operation":"set_meta","key":"order_seeded","value":"1"}))
            .unwrap();
        drop(cat);
        Self { root, db, keys }
    }
    pub fn cat(&self) -> Catalog {
        Catalog::open(&self.db, UTC, &self.root).unwrap()
    }
    pub fn publish(&self, i: usize, set: Option<&str>) {
        self.cat().uploaded(&json!({"content_key":self.keys[i],"custom_emoji_id":format!("cid{i}"),"base":"pk","set_name":set}),UTC).unwrap();
    }
    pub fn request(&self, show: bool, keep: Value, branded: bool, logo: Option<&Path>) -> Value {
        let mut cat = self.cat();
        json!({"items":cat.all(None).unwrap(),"published":cat.dispatch(json!({"operation":"published_keys"})).unwrap(),
   "published_sets":cat.dispatch(json!({"operation":"published_sets"})).unwrap(),"keep_sets":keep,"show_published":show,"branded":branded,"logo_path":logo})
    }
    pub fn view(&self, show: bool, keep: Value, branded: bool, logo: Option<&Path>) -> Value {
        runtime::panel_view::build(self.request(show, keep, branded, logo)).unwrap()
    }
    pub fn pack_fixture() -> Self {
        let data = Self::new(6);
        for i in [0, 1] {
            data.publish(i, Some("pk1_by_bot"));
        }
        for i in [2, 3] {
            data.publish(i, Some("pk2_by_bot"));
        }
        png(&data.root.join("brand.png"), [10, 20, 30, 255], 40);
        std::fs::write(data.root.join("publish_pk.json"),json!({"sets":[{"name":"pk1_by_bot","index":1,"live":3,"logo":true},{"name":"pk2_by_bot","index":2,"live":3,"logo":true}]}).to_string()).unwrap();
        data
    }
    pub fn published_fixture() -> Self {
        let data = Self::new(5);
        for i in [0, 1] {
            data.publish(i, Some("pk1_by_bot"));
        }
        data.publish(2, Some("pk2_by_bot"));
        data
    }
    pub fn keep(&self, which: &[u64]) -> Value {
        runtime::panel_view::packs_named(&self.root, which).unwrap()
    }
    pub fn panel(&self, options: Value) -> Panel {
        let log = Arc::new(RunLog::open(&self.root, "panel-views", vec![]));
        let request = self.request(false, json!({}), false, None);
        let view = runtime::panel_view::build(request).unwrap();
        Panel {
            root: self.root.clone(),
            db: self.db.clone(),
            token: TOKEN.into(),
            options,
            branding: None,
            detected_bot: Mutex::new(String::new()),
            view: Mutex::new(view),
            session: runtime::panel_session::identity(&self.db, false, &[]).unwrap(),
            last_refreshed: std::sync::Mutex::new(Some(chrono::Utc::now())),
            utc: || UTC.into(),
            log,
            events: Mutex::new(Events::default()),
            previews: Previews::new(
                NativeMediaAdapter {
                    executable: binary(),
                    root: project(),
                    scratch: self.root.join("codec"),
                },
                &self.root,
                2,
            ),
        }
    }
    pub fn excluded(&self) -> std::collections::BTreeSet<String> {
        let db = rusqlite::Connection::open_with_flags(
            &self.db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        db.prepare("SELECT content_key FROM items WHERE included=0")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }
}
impl Drop for Data {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
pub fn key(i: usize) -> String {
    format!("s:item{i:030}")
}
pub fn png(path: &Path, color: [u8; 4], size: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbaImage::from_pixel(size, size, image::Rgba(color))
        .save(path)
        .unwrap();
}
pub fn binary() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_numera-emoji") {
        return PathBuf::from(path);
    }
    let name = if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    };
    let current = std::env::current_exe().unwrap();
    // Library unit tests do not receive CARGO_BIN_EXE. Locate the built sibling
    // application in the current Cargo profile, including explicit target dirs.
    if let Some(profile) = current.ancestors().find(|path| {
        matches!(
            path.file_name().and_then(|x| x.to_str()),
            Some("debug" | "release")
        )
    }) {
        return profile.join(name);
    }
    panic!("cannot locate the standalone application beside the test executable");
}

pub fn scripts(root: &Path) -> String {
    runtime::asset_version::PANEL_SCRIPTS
        .iter()
        .map(|name| {
            std::fs::read_to_string(root.join("assets").join(name))
                .unwrap()
                .replace("\r\n", "\n")
                .replace('\r', "\n")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn keys(view: &Value) -> std::collections::BTreeSet<String> {
    view["view"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["isLogo"] != true)
        .map(|x| x["key"].as_str().unwrap().to_owned())
        .collect()
}

pub fn options() -> Value {
    json!({"keep_sets":{},"show_published":false,"branded":false,"no_open":true,"preview_fps":15})
}
pub struct Server {
    pub data: Data,
    pub panel: Arc<Panel>,
    pub port: u16,
    pub client: reqwest::blocking::Client,
    pub accepted: Arc<AtomicUsize>,
    runtime: Option<tokio::runtime::Runtime>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
    threads: Arc<AtomicUsize>,
}
impl Server {
    pub fn new(n: usize) -> Self {
        Self::with_token(n, TOKEN)
    }
    pub fn with_token(n: usize, token: &str) -> Self {
        let data = Data::new(n);
        let mut panel = data.panel(options());
        panel.token = token.into();
        Self::start(data, panel)
    }
    pub fn start(data: Data, panel: Panel) -> Self {
        let panel = Arc::new(panel);
        let threads = Arc::new(AtomicUsize::new(0));
        let start = threads.clone();
        let end = threads.clone();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(4)
            .on_thread_start(move || {
                start.fetch_add(1, Ordering::SeqCst);
            })
            .on_thread_stop(move || {
                end.fetch_sub(1, Ordering::SeqCst);
            })
            .enable_all()
            .build()
            .unwrap();
        let listener = runtime
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepted = Arc::new(AtomicUsize::new(0));
        let count = accepted.clone();
        use axum::serve::ListenerExt;
        let listener = listener.tap_io(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
        });
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
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        Self {
            data,
            panel,
            port,
            client,
            accepted,
            runtime: Some(runtime),
            stop: Some(stop),
            task: Some(task),
            threads,
        }
    }
    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
    pub fn get(&self, path: &str) -> reqwest::blocking::Response {
        self.client.get(self.url(path)).send().unwrap()
    }
    pub fn page(&self) -> String {
        let r = self.get("/");
        assert_eq!(r.status(), 200);
        r.text().unwrap()
    }
    pub fn post(&self, path: &str, body: Value) -> (u16, Value) {
        let r = self
            .client
            .post(self.url(path))
            .header("X-Panel-Token", self.panel.token.as_str())
            .json(&body)
            .send()
            .unwrap();
        let status = r.status().as_u16();
        (status, r.json().unwrap())
    }
}
impl Drop for Server {
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
            "panel threads leaked"
        );
    }
}
pub struct Connection {
    reader: std::io::BufReader<std::net::TcpStream>,
    port: u16,
    token: String,
}
pub struct Answer {
    pub status: u16,
    pub headers: std::collections::BTreeMap<String, String>,
    pub body: Vec<u8>,
}
impl Connection {
    pub fn new(port: u16, token: &str) -> Self {
        let socket = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        Self {
            reader: std::io::BufReader::new(socket),
            port,
            token: token.into(),
        }
    }
    pub fn request(
        &mut self,
        method: &str,
        path: &str,
        body: &[u8],
        length: Option<&str>,
    ) -> Answer {
        use std::io::{BufRead, Read, Write};
        let socket = self.reader.get_mut();
        write!(
            socket,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n",
            self.port
        )
        .unwrap();
        if method == "POST" {
            write!(
                socket,
                "X-Panel-Token: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
                self.token,
                length
                    .map(str::to_owned)
                    .unwrap_or_else(|| body.len().to_string())
            )
            .unwrap();
        }
        socket.write_all(b"\r\n").unwrap();
        socket.write_all(body).unwrap();
        socket.flush().unwrap();
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        assert!(line.starts_with("HTTP/1.1 "), "{line}");
        let status = line
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let mut headers = std::collections::BTreeMap::new();
        loop {
            line.clear();
            self.reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            let (k, v) = line.split_once(':').unwrap();
            headers.insert(k.to_lowercase(), v.trim().to_owned());
        }
        let mut out = Vec::new();
        if status != 204 {
            if let Some(size) = headers.get("content-length") {
                let n = size.parse::<usize>().unwrap();
                assert!(n < 4 * 1024 * 1024);
                out.resize(n, 0);
                self.reader.read_exact(&mut out).unwrap();
            } else if headers.get("transfer-encoding").map(String::as_str) == Some("chunked") {
                loop {
                    line.clear();
                    self.reader.read_line(&mut line).unwrap();
                    let n =
                        usize::from_str_radix(line.trim().split(';').next().unwrap(), 16).unwrap();
                    if n == 0 {
                        line.clear();
                        self.reader.read_line(&mut line).unwrap();
                        assert_eq!(line, "\r\n");
                        break;
                    }
                    assert!(out.len() + n < 4 * 1024 * 1024);
                    let at = out.len();
                    out.resize(at + n, 0);
                    self.reader.read_exact(&mut out[at..]).unwrap();
                    let mut crlf = [0; 2];
                    self.reader.read_exact(&mut crlf).unwrap();
                    assert_eq!(&crlf, b"\r\n");
                }
            } else {
                panic!("response has no finite body framing");
            }
        }
        Answer {
            status,
            headers,
            body: out,
        }
    }
}
pub struct Child(pub std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
pub fn command(exe: &Path) -> std::process::Command {
    let mut c = std::process::Command::new(exe);
    c.env_clear();
    for key in [
        "NUMERA_RUNTIME_TEST_ADMISSION",
        "PATH",
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "TMPDIR",
        "LANG",
    ] {
        if let Some(v) = std::env::var_os(key) {
            c.env(key, v);
        }
    }
    c.env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1");
    c
}
pub fn output(c: &mut std::process::Command, timeout: Duration) -> std::process::Output {
    use std::io::Read;
    use std::process::Stdio;
    c.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = Child(c.spawn().unwrap());
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if child.0.try_wait().unwrap().is_some() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "child deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_end(&mut stderr)
        .unwrap();
    std::process::Output {
        status: child.0.wait().unwrap(),
        stdout,
        stderr,
    }
}
pub fn obstruct_catalog(data: &Data) {
    for suffix in ["", "-wal", "-shm"] {
        let p = data.root.join(format!("catalog.db{suffix}"));
        if p.exists() {
            std::fs::remove_file(p).unwrap();
        }
    }
    std::fs::create_dir(&data.db).unwrap();
}

pub fn startup_warm_cache_survives_catalog_failure() {
    let data = Data::new(1);
    let mut panel = data.panel(options());
    panel.view = Mutex::new(json!({"items":[]}));
    panel.last_refreshed = Mutex::new(None);
    let log_path = panel.log.path.clone().unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = runtime.spawn(panel_http::serve_listener(panel, listener, async {
        let _ = stopped.await;
    }));
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while !std::fs::read_to_string(&log_path)
        .unwrap()
        .contains("warm-up rendered=")
    {
        assert!(
            std::time::Instant::now() < deadline,
            "startup did not warm the real cache"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    obstruct_catalog(&data);
    let response = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
        .get(format!("http://{address}/"))
        .send()
        .unwrap();
    let status = response.status();
    let page = response.text().unwrap();
    stop.send(()).unwrap();
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    });
    runtime.shutdown_timeout(Duration::from_secs(2));
    assert_eq!(status, 200);
    let raw = page
        .split("const STALE = ")
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let stale: String = serde_json::from_str(raw).unwrap();
    assert_eq!(
        stale.len(),
        5,
        "startup cache must carry its refresh timestamp: {stale}"
    );
    assert!(stale.bytes().enumerate().all(|(i, byte)| if i == 2 {
        byte == b':'
    } else {
        byte.is_ascii_digit()
    }));
    assert!(
        page.contains(&data.keys[0]),
        "startup cache lost its real catalog row"
    );
}
