//! Loopback-only scripted forms, uploads and downloads; no live Telegram access.
use axum::{
    Form, Json, Router,
    extract::{DefaultBodyLimit, FromRequest, Path, Request as HttpRequest, State},
    http::{HeaderMap, StatusCode},
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[path = "bot_multipart.rs"]
mod bot_multipart;

pub const TOKEN: &str = "test-only-token";
#[derive(Clone, Debug)]
pub struct Request {
    #[allow(dead_code)] // Some scripted cohorts validate requests in the handler instead.
    pub method: String,
    #[allow(dead_code)]
    pub form: BTreeMap<String, String>,
    #[allow(dead_code)] // Replacement readback retains the actual transmitted bytes.
    pub files: BTreeMap<String, Vec<u8>>,
    #[allow(dead_code)] // Only the ticker command cohort inspects the complete durable snapshot.
    pub observation: Option<Value>,
}
pub struct Reply {
    pub method: String,
    pub fields: BTreeMap<String, String>,
    pub payload: Value,
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub observation: Option<(std::path::PathBuf, String, Value)>,
    pub files: BTreeMap<String, Vec<u8>>,
    pub upload_sha256: BTreeMap<String, String>,
    pub download: Option<Vec<u8>>,
    pub download_upload: Option<String>,
    pub held: Option<(
        tokio::sync::oneshot::Sender<()>,
        tokio::sync::oneshot::Receiver<()>,
    )>,
}
impl Reply {
    // Shared by suites that do not need an interrupted mutation.
    #[allow(dead_code)]
    pub fn hold(
        mut self,
    ) -> (
        Self,
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (ready, reached) = tokio::sync::oneshot::channel();
        let (release, done) = tokio::sync::oneshot::channel();
        self.held = Some((ready, done));
        (self, reached, release)
    }
    #[allow(dead_code)] // Shared helper; not every importing cohort constructs replies.
    pub fn new(method: &str, fields: &[(&str, &str)], payload: Value) -> Self {
        assert!(matches!(
            method,
            "getMe"
                | "setMyCommands"
                | "getUpdates"
                | "getCustomEmojiStickers"
                | "sendMessage"
                | "getStickerSet"
                | "setStickerPositionInSet"
                | "deleteStickerFromSet"
                | "deleteStickerSet"
                | "createNewStickerSet"
                | "addStickerToSet"
                | "replaceStickerInSet"
                | "uploadStickerFile"
                | "getFile"
                | "POST /publish"
                | "GET /provider/fixture"
                | "GET /v1/search"
                | "GET /coin/alpha-coin/logo.png"
                | "GET /api/v3/coins/markets"
        ));
        Self {
            method: method.into(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            payload,
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            observation: None,
            files: BTreeMap::new(),
            upload_sha256: BTreeMap::new(),
            download: None,
            download_upload: None,
            held: None,
        }
    }
}
struct Script {
    received: usize,
    replies: VecDeque<Reply>,
    requests: Vec<Request>,
    error: Option<String>,
}
async fn received(
    State(script): State<Arc<Mutex<Script>>>,
    request: HttpRequest,
    next: Next,
) -> Response {
    script.lock().unwrap().received += 1;
    next.run(request).await
}
async fn answer(
    State(script): State<Arc<Mutex<Script>>>,
    Path((token, method)): Path<(String, String)>,
    request: HttpRequest,
) -> Response {
    let content_type = request
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let (form, files) = if content_type.starts_with("multipart/form-data;") {
        let bytes = match axum::body::Bytes::from_request(request, &()).await {
            Ok(value) => value,
            Err(error) => return error.into_response(),
        };
        match bot_multipart::parse(&content_type, &bytes) {
            Ok(value) => value,
            Err(_) => {
                script.lock().unwrap().error = Some("fixture multipart request refused".into());
                return StatusCode::BAD_REQUEST.into_response();
            }
        }
    } else {
        match Form::<BTreeMap<String, String>>::from_request(request, &()).await {
            Ok(Form(value)) => (value, BTreeMap::new()),
            Err(error) => return error.into_response(),
        }
    };
    scripted(&script, method, form, files, token == format!("bot{TOKEN}")).await
}
async fn publish(
    State(script): State<Arc<Mutex<Script>>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let form = BTreeMap::from([("json".into(), body.to_string())]);
    scripted(
        &script,
        "POST /publish".into(),
        form,
        BTreeMap::new(),
        headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            == Some("Bearer test-only-worker-secret"),
    )
    .await
}
async fn provider(State(script): State<Arc<Mutex<Script>>>, request: HttpRequest) -> Response {
    let mut form = ["user-agent", "accept", "x-cmc_pro_api_key"]
        .into_iter()
        .filter_map(|name| {
            request
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(|value| (name.to_owned(), value.to_owned()))
        })
        .collect::<BTreeMap<_, _>>();
    form.insert("query".into(), request.uri().query().unwrap_or("").into());
    scripted(
        &script,
        format!("GET {}", request.uri().path()),
        form,
        BTreeMap::new(),
        true,
    )
    .await
}
async fn scripted(
    script: &Arc<Mutex<Script>>,
    method: String,
    form: BTreeMap<String, String>,
    files: BTreeMap<String, Vec<u8>>,
    authorized: bool,
) -> Response {
    let (status, headers, payload, held, raw) = {
        let mut script = script.lock().expect("bot fixture mutex poisoned");
        let mut held = None;
        let mut raw = None;
        let mut status = StatusCode::OK;
        let mut headers = HeaderMap::new();
        let next = script.replies.pop_front();
        script.requests.push(Request {
            method: method.clone(),
            form: form.clone(),
            files: files.clone(),
            observation: None,
        });
        let payload = if let Some(reply) = next.filter(|reply| {
            let uploads_match = if reply.upload_sha256.is_empty() {
                reply.files == files
            } else {
                use sha2::{Digest, Sha256};
                reply.files.is_empty()
                    && files.len() == reply.upload_sha256.len()
                    && reply.upload_sha256.iter().all(|(name, expected)| {
                        files.get(name).is_some_and(|bytes| {
                            let actual: String = Sha256::digest(bytes)
                                .iter()
                                .map(|byte| format!("{byte:02x}"))
                                .collect();
                            actual == *expected
                        })
                    })
            };
            authorized
                && ((reply.download.is_none() && reply.download_upload.is_none())
                    || method.starts_with("GET "))
                && uploads_match
                && reply.method == method
                && reply
                    .fields
                    .iter()
                    .all(|(key, value)| form.get(key) == Some(value))
        }) {
            if let Some((path, pointer, expected)) = &reply.observation {
                let actual = (|| {
                    _native::tooling_environment::reject_links(path, "linked bot observation")
                        .ok()?;
                    use std::io::Read;
                    let file = std::fs::File::open(path).ok()?;
                    let metadata = file.metadata().ok()?;
                    if !metadata.is_file() || metadata.len() > 16384 {
                        return None;
                    }
                    let mut bytes = Vec::new();
                    file.take(16385).read_to_end(&mut bytes).ok()?;
                    if bytes.len() > 16384 {
                        return None;
                    }
                    serde_json::from_slice::<Value>(&bytes).ok()
                })();
                if actual.as_ref().and_then(|value| value.pointer(pointer)) != Some(expected) {
                    script.error = Some("durable pre-request observation mismatch".into());
                    return (
                    StatusCode::OK,
                    Json(
                        serde_json::json!({"ok":false,"description":"scripted durable observation mismatch"}),
                    ),
                ).into_response();
                }
                script
                    .requests
                    .last_mut()
                    .expect("observed request missing")
                    .observation = actual;
            }
            held = reply.held;
            status = reply.status;
            headers = reply.headers;
            raw = if let Some(field) = reply.download_upload {
                if reply.download.is_some() {
                    script.error = Some("fixture download has two byte sources".into());
                    return StatusCode::BAD_REQUEST.into_response();
                }
                let bytes = script
                    .requests
                    .iter()
                    .rev()
                    .find_map(|request| request.files.get(&field))
                    .cloned();
                if bytes.is_none() {
                    script.error = Some("fixture download has no observed upload".into());
                    return StatusCode::BAD_REQUEST.into_response();
                }
                bytes
            } else {
                reply.download
            };
            reply.payload
        } else {
            script.error = Some(format!(
                "unexpected synthetic bot request at step{} method={method}",
                script.requests.len()
            ));
            serde_json::json!({"ok":false,"description":"Conflict: scripted bot request mismatch"})
        };
        (status, headers, payload, held, raw)
    };
    // Do not hold the script mutex across readiness/cancellation or shutdown.
    if let Some((ready, done)) = held {
        let _ = ready.send(());
        if !matches!(
            tokio::time::timeout(Duration::from_secs(10), done).await,
            Ok(Ok(()))
        ) {
            script.lock().unwrap().error = Some("held bot response exceeded10s".into());
            return StatusCode::REQUEST_TIMEOUT.into_response();
        }
    }
    if let Some(bytes) = raw {
        (status, headers, bytes).into_response()
    } else {
        (status, headers, Json(payload)).into_response()
    }
}
async fn download(
    State(script): State<Arc<Mutex<Script>>>,
    Path((token, path)): Path<(String, String)>,
) -> Response {
    {
        let mut state = script.lock().unwrap();
        if state
            .replies
            .front()
            .is_none_or(|reply| reply.download.is_none() && reply.download_upload.is_none())
        {
            state.error = Some("fixture download requires explicit raw bytes".into());
        }
    }
    scripted(
        &script,
        format!("GET {path}"),
        BTreeMap::new(),
        BTreeMap::new(),
        token == format!("bot{TOKEN}"),
    )
    .await
}
async fn unexpected(State(script): State<Arc<Mutex<Script>>>, request: HttpRequest) -> StatusCode {
    let mut script = script.lock().unwrap();
    script.requests.push(Request {
        method: format!("{} {}", request.method(), request.uri().path()),
        form: BTreeMap::new(),
        files: BTreeMap::new(),
        observation: None,
    });
    script.error = Some("unexpected synthetic HTTP route or method".into());
    StatusCode::BAD_REQUEST
}

pub struct Server {
    pub base: String,
    address: std::net::SocketAddr,
    script: Arc<Mutex<Script>>,
    runtime: Option<tokio::runtime::Runtime>,
    task: Option<tokio::task::JoinHandle<Result<(), String>>>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    threads: Arc<AtomicUsize>,
}
impl Server {
    pub fn new(replies: Vec<Reply>) -> Self {
        let script = Arc::new(Mutex::new(Script {
            received: 0,
            replies: replies.into(),
            requests: Vec::new(),
            error: None,
        }));
        let threads = Arc::new(AtomicUsize::new(0));
        let started = threads.clone();
        let stopped = threads.clone();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .max_blocking_threads(1)
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
        let app = Router::new()
            .route("/publish", post(publish))
            .route("/provider/fixture", get(provider))
            .route("/v1/search", get(provider))
            .route("/coin/alpha-coin/logo.png", get(provider))
            .route("/api/v3/coins/markets", get(provider))
            .route("/{token}/{method}", post(answer))
            .route("/file/{token}/{path}", get(download))
            .fallback(unexpected)
            .method_not_allowed_fallback(unexpected)
            .layer(DefaultBodyLimit::max(65536))
            .layer(from_fn_with_state(script.clone(), received))
            .with_state(script.clone());
        let (stop, done) = tokio::sync::oneshot::channel();
        let task = runtime.spawn(async move {
            let serving = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = done.await;
            });
            tokio::time::timeout(Duration::from_secs(30), serving)
                .await
                .map_err(|_| "bot fixture exceeded30s".to_owned())?
                .map_err(|error| error.to_string())
        });
        Self {
            base: format!("http://{address}"),
            address,
            script,
            runtime: Some(runtime),
            task: Some(task),
            stop: Some(stop),
            threads,
        }
    }
    fn stop(&mut self) -> Result<(), String> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let Some(runtime) = self.runtime.take() else {
            return Ok(());
        };
        let result = if let Some(mut task) = self.task.take() {
            match runtime
                .block_on(async { tokio::time::timeout(Duration::from_secs(3), &mut task).await })
            {
                Ok(Ok(result)) => result,
                outcome => {
                    task.abort();
                    let _ = runtime.block_on(async {
                        tokio::time::timeout(Duration::from_secs(1), task).await
                    });
                    Err(format!("bot fixture shutdown failed: {outcome:?}"))
                }
            }
        } else {
            Err("bot fixture task missing".into())
        };
        runtime.shutdown_timeout(Duration::from_secs(1));
        result?;
        if self.threads.load(Ordering::SeqCst) != 0 {
            return Err("bot fixture runtime threads remain".into());
        }
        match std::net::TcpStream::connect_timeout(&self.address, Duration::from_millis(250)) {
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => Ok(()),
            _ => Err("bot fixture listener shutdown unverified".into()),
        }
    }
    pub fn finish(&mut self) -> Result<Vec<Request>, String> {
        self.stop()?;
        let script = self.script.lock().map_err(|_| "bot script poisoned")?;
        if script.received != script.requests.len() {
            return Err("synthetic HTTP request rejected before observation".into());
        }
        if let Some(error) = &script.error {
            return Err(error.clone());
        }
        if !script.replies.is_empty() {
            return Err(format!(
                "{} scripted bot replies were not reached",
                script.replies.len()
            ));
        }
        Ok(script.requests.clone())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            eprintln!("ERROR: {error}");
        }
    }
}
