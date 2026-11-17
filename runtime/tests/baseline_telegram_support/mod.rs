//! Owned finite numeric-loopback fault transcripts. No interpreter or live endpoint.
use numera_runtime::{
    logging::RunLog,
    telegram::{ApiError, Telegram},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
pub const TOKEN: &str = "1234567890:AAxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
pub const A: &str = "5893098741073715471";
pub const B: &str = "5893098741073715472";
pub const CHANNEL: i64 = -1001234567890;
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Root(pub PathBuf);
impl Root {
    pub fn new() -> Self {
        loop {
            let p = std::env::temp_dir().join(format!(
                "numera-baseline-telegram-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&p) {
                Ok(()) => return Self(p),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("{e}"),
            }
        }
    }
    pub fn log(&self) -> RunLog {
        RunLog::open(&self.0, "baseline-telegram", vec![TOKEN.into()])
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("owned fixture cleanup");
    }
}
#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub form: BTreeMap<String, String>,
    pub files: BTreeMap<String, Vec<u8>>,
    pub observation: Option<Value>,
}
pub enum Body {
    Json(Value),
    Bytes(Vec<u8>),
    Disconnect,
}
pub struct Reply {
    pub method: String,
    pub body: Body,
    pub observe: Option<PathBuf>,
}
pub fn reply(method: &str, body: Value) -> Reply {
    Reply {
        method: method.into(),
        body: Body::Json(body),
        observe: None,
    }
}
pub fn fault(method: &str) -> Reply {
    Reply {
        method: method.into(),
        body: Body::Disconnect,
        observe: None,
    }
}
pub fn ok(result: Value) -> Value {
    json!({"ok":true,"result":result})
}
pub fn reject(desc: &str) -> Value {
    json!({"ok":false,"description":desc})
}
pub fn listing(ids: &[&str]) -> Reply {
    reply(
        "getStickerSet",
        ok(
            json!({"stickers":ids.iter().map(|id|json!({"file_unique_id":id,"file_id":id})).collect::<Vec<_>>()}),
        ),
    )
}
pub fn listed(stickers: Value) -> Reply {
    reply("getStickerSet", ok(json!({"stickers":stickers})))
}
pub fn missing() -> Reply {
    reply("getStickerSet", reject("STICKERSET_INVALID"))
}
fn split<'a>(bytes: &'a [u8], separator: &[u8]) -> Vec<&'a [u8]> {
    let mut rest = bytes;
    let mut out = vec![];
    while let Some(n) = rest.windows(separator.len()).position(|b| b == separator) {
        out.push(&rest[..n]);
        rest = &rest[n + separator.len()..];
    }
    out.push(rest);
    out
}
fn parse(headers: &str, body: &[u8]) -> (BTreeMap<String, String>, BTreeMap<String, Vec<u8>>) {
    let mut fields = BTreeMap::new();
    let mut files = BTreeMap::new();
    let kind = headers
        .lines()
        .find_map(|l| {
            l.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.trim())
        })
        .unwrap_or("");
    if kind.starts_with("multipart/form-data;") {
        let boundary = kind
            .split(';')
            .find_map(|x| x.trim().strip_prefix("boundary="))
            .unwrap()
            .trim_matches('"');
        let sep = format!("--{boundary}");
        for part in split(body, sep.as_bytes()).into_iter().skip(1) {
            if part.starts_with(b"--") {
                break;
            }
            let part = part.strip_prefix(b"\r\n").unwrap();
            let end = part.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
            let h = std::str::from_utf8(&part[..end]).unwrap();
            let value = part[end + 4..].strip_suffix(b"\r\n").unwrap();
            let disposition = h
                .lines()
                .find(|l| l.to_ascii_lowercase().starts_with("content-disposition:"))
                .unwrap();
            let name = disposition
                .split(';')
                .find_map(|x| x.trim().strip_prefix("name=\""))
                .unwrap()
                .strip_suffix('"')
                .unwrap()
                .to_owned();
            assert!(!fields.contains_key(&name) && !files.contains_key(&name));
            if disposition.contains("filename=") {
                files.insert(name, value.to_vec());
            } else {
                fields.insert(name, String::from_utf8(value.to_vec()).unwrap());
            }
        }
    } else {
        let url = reqwest::Url::parse(&format!(
            "http://127.0.0.1/?{}",
            std::str::from_utf8(body).unwrap()
        ))
        .unwrap();
        fields.extend(
            url.query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned())),
        );
    }
    (fields, files)
}
pub struct Server {
    pub base: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<Vec<Request>, String>>>,
}
impl Server {
    pub fn new(replies: Vec<Reply>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut seen = vec![];
            let mut script = std::collections::VecDeque::from(replies);
            let deadline = Instant::now() + Duration::from_secs(35);
            loop {
                let mut stream = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if stopping.load(Ordering::SeqCst) {
                            break;
                        }
                        if Instant::now() > deadline {
                            return Err("loopback transcript exceeded deadline".into());
                        }
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => return Err(e.to_string()),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = vec![];
                let (headers, body) = loop {
                    let mut chunk = [0; 8192];
                    let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
                    if n == 0 {
                        return Err("incomplete request".into());
                    }
                    bytes.extend_from_slice(&chunk[..n]);
                    if bytes.len() > 1024 * 1024 {
                        return Err("oversized fixture request".into());
                    }
                    if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let headers =
                            String::from_utf8(bytes[..end].to_vec()).map_err(|e| e.to_string())?;
                        let len = headers
                            .lines()
                            .find_map(|l| {
                                l.split_once(':')
                                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                    .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break (headers, bytes[end + 4..end + 4 + len].to_vec());
                        }
                    }
                };
                let path = headers
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let prefix = format!("/bot{TOKEN}/");
                let method = if let Some(name) = path.strip_prefix(&prefix) {
                    name.to_owned()
                } else {
                    format!(
                        "GET {}",
                        path.strip_prefix(&format!("/file/bot{TOKEN}/"))
                            .ok_or("unexpected fixture token/route")?
                    )
                };
                let (form, files) = parse(&headers, &body);
                seen.push(Request {
                    method: method.clone(),
                    form,
                    files,
                    observation: None,
                });
                let next = script
                    .pop_front()
                    .ok_or_else(|| format!("extra method {method}"))?;
                if next.method != method {
                    return Err(format!("expected{} got{method}", next.method));
                }
                if let Some(path) = next.observe {
                    seen.last_mut().unwrap().observation = Some(
                        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())?,
                    );
                }
                match next.body {
                    Body::Disconnect => drop(stream),
                    Body::Bytes(body) => {
                        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).map_err(|e|e.to_string())?;
                        stream.write_all(&body).map_err(|e| e.to_string())?;
                    }
                    Body::Json(payload) => {
                        let body = payload.to_string();
                        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n{body}",body.len()).as_bytes()).map_err(|e|e.to_string())?;
                    }
                }
            }
            if !script.is_empty() {
                return Err(format!("{} unconsumed transcript requests", script.len()));
            }
            Ok(seen)
        });
        Self {
            base,
            stop,
            thread: Some(thread),
        }
    }
    pub fn telegram(&self) -> Telegram {
        Telegram::new(TOKEN.into(), self.base.clone()).unwrap()
    }
    pub fn finish(mut self) -> Vec<Request> {
        self.stop.store(true, Ordering::SeqCst);
        self.thread
            .take()
            .unwrap()
            .join()
            .expect("fixture thread panic")
            .expect("complete fault transcript")
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
pub fn call(
    tg: &Telegram,
    method: &str,
    retries: u32,
    waits: &mut Vec<Duration>,
) -> Result<Value, ApiError> {
    tg.call_with_wait(
        method,
        &BTreeMap::new(),
        &[],
        retries,
        None::<fn() -> Result<Option<bool>, ApiError>>,
        |d| waits.push(d),
    )
}
pub fn message(uid: i64, text: &str, kind: &str) -> Value {
    json!({"message":{"message_id":1,"text":text,"from":{"id":uid},"chat":{"id":uid,"type":kind}}})
}
pub fn post() -> Value {
    json!({"channel_post":{"message_id":1,"chat":{"id":CHANNEL,"type":"channel","title":"Somebody's channel"},"entities":[{"type":"custom_emoji","custom_emoji_id":"111111111"}]}})
}
pub fn entities(ids: &[&str]) -> Value {
    json!({"text":"xx","entities":ids.iter().enumerate().map(|(i,id)|json!({"type":"custom_emoji","offset":i,"length":2,"custom_emoji_id":id})).collect::<Vec<_>>()})
}
pub fn payload(ids: &[String], labels: Value, rich: bool) -> Value {
    numera_runtime::bot_payloads::payloads(ids, &labels, rich).unwrap()
}
pub fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| (*s).into()).collect()
}
pub fn series(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| (10_u64.pow(18) + i as u64).to_string())
        .collect()
}
pub fn copied(payloads: &Value) -> Vec<String> {
    payloads
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| {
            p[1]["inline_keyboard"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|row| row[0]["copy_text"]["text"].as_str())
                .flat_map(|s| s.trim_end_matches('\n').split('\n').map(str::to_owned))
        })
        .collect()
}

pub struct Mutation {
    pub result: Result<Value, ApiError>,
    pub requests: Vec<Request>,
    pub compared: Vec<String>,
    pub waits: Vec<Duration>,
}
pub fn mutation(
    creating: bool,
    before: Option<Value>,
    after: Option<Reply>,
    readback: Option<Reply>,
    matches: BTreeMap<String, Option<bool>>,
    retry_rejected: bool,
    repaint: bool,
) -> Mutation {
    mutation_with_retry(
        creating,
        before,
        after,
        readback,
        matches,
        retry_rejected.then(|| reject("fixture retry rejected")),
        repaint,
    )
}
pub fn mutation_with_retry(
    creating: bool,
    before: Option<Value>,
    after: Option<Reply>,
    readback: Option<Reply>,
    matches: BTreeMap<String, Option<bool>>,
    retry_response: Option<Value>,
    repaint: bool,
) -> Mutation {
    let root = Root::new();
    let source = root.0.join("ours.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 30, 30, 255]))
        .save(&source)
        .unwrap();
    let bytes = std::fs::read(&source).unwrap();
    let unsettled = after.is_some();
    let retried = retry_response.is_some();
    let mut script = vec![];
    if let Some(before) = &before {
        script.push(listed(before.clone()));
    }
    let method = if creating {
        "createNewStickerSet"
    } else {
        "addStickerToSet"
    };
    script.push(if after.is_some() {
        fault(method)
    } else {
        reply(method, ok(json!(true)))
    });
    if let Some(after) = after {
        script.push(after);
    }
    if let Some(response) = retry_response {
        script.push(reply(method, response));
    }
    if let Some(readback) = readback {
        script.push(readback);
    }
    let server = Server::new(script);
    let mut compared = vec![];
    let mut waits = vec![];
    let request = json!({"name":"s","source":source,"format":"static","create":creating,"title":"T","user_id":1,"emojis":["😀"],"keywords":["kw"],"expected_before":before.as_ref().map(|v|v.as_array().unwrap().len()),"needs_repainting":repaint});
    let result = numera_runtime::telegram_mutation::mutate_with_wait(
        &server.telegram(),
        &request,
        |_, member, path, fmt| {
            assert_eq!(std::path::Path::new(path), source);
            assert_eq!(fmt, "static");
            let id = member["file_unique_id"].as_str().unwrap().to_owned();
            compared.push(id.clone());
            Ok(matches.get(&id).copied().unwrap_or(None))
        },
        |d| waits.push(d),
    );
    let requests = server.finish();
    let expected_waits = if !unsettled {
        vec![]
    } else if retried {
        vec![Duration::from_secs(2), Duration::from_secs(3)]
    } else {
        vec![Duration::from_secs(2)]
    };
    assert_eq!(waits, expected_waits, "settle/proof/retry ordering");
    for probe in requests.iter().filter(|r| r.method == "getStickerSet") {
        assert_eq!(probe.form["name"], "s");
    }
    for r in requests.iter().filter(|r| r.method == method) {
        assert_eq!(r.form["user_id"], "1");
        assert_eq!(r.form["name"], "s");
        assert_eq!(r.files.get("file0"), Some(&bytes));
        let key = if creating { "stickers" } else { "sticker" };
        let input: Value = serde_json::from_str(&r.form[key]).unwrap();
        let input = if creating { &input[0] } else { &input };
        assert_eq!(
            input,
            &json!({"sticker":"attach://file0","format":"static","emoji_list":["😀"],"keywords":["kw"]})
        );
    }
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    Mutation {
        result,
        requests,
        compared,
        waits,
    }
}
pub fn members(ids: &[&str]) -> Value {
    json!(
        ids.iter()
            .map(|id| json!({"file_unique_id":id,"file_id":id}))
            .collect::<Vec<_>>()
    )
}
pub fn matches(values: &[(&str, Option<bool>)]) -> BTreeMap<String, Option<bool>> {
    values.iter().map(|(id, v)| ((*id).into(), *v)).collect()
}
pub fn mutation_count(m: &Mutation) -> usize {
    m.requests
        .iter()
        .filter(|r| r.method == "addStickerToSet" || r.method == "createNewStickerSet")
        .count()
}

pub fn child_driver() -> bool {
    let Some(root) = std::env::var_os("NUMERA_BASELINE_TELEGRAM_CHILD_ROOT") else {
        return false;
    };
    let root = PathBuf::from(root);
    let mode = std::env::var("NUMERA_BASELINE_TELEGRAM_CHILD_MODE").unwrap();
    let values = std::env::vars().collect();
    let config =
        numera_runtime::config::Config::load_with_environment(root.clone(), &values).unwrap();
    let log = RunLog::open(&root, "baseline-child", vec![TOKEN.into()]);
    if mode == "redaction" {
        let tg = Telegram::new(TOKEN.into(), std::env::var("NUMERA_TEST_API_BASE").unwrap())
            .unwrap()
            .with_log(&log);
        let result = call(&tg, "getMe", 2, &mut vec![]);
        assert!(result.is_err());
        assert!(!format!("{result:?}").contains(TOKEN));
        return true;
    }
    let result = if mode == "bot" {
        numera_runtime::emoji_bot::run(&config, &[], &log)
    } else {
        let args: Vec<String> =
            serde_json::from_str(&std::env::var("NUMERA_BASELINE_TELEGRAM_CHILD_ARGS").unwrap())
                .unwrap();
        numera_runtime::ticker_build::run(&config, &args, &log)
    };
    let expected = if mode == "bot" { 4 } else { 0 };
    assert_eq!(result.unwrap(), expected);
    true
}
pub fn run_child(test: &str, root: &Root, base: &str, mode: &str, args: &[String]) {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .current_dir(&root.0)
        .env_clear();
    for key in [
        "NUMERA_RUNTIME_TEST_ADMISSION",
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
    ] {
        if let Some(v) = std::env::var_os(key) {
            command.env(key, v);
        }
    }
    command
        .env("NUMERA_BASELINE_TELEGRAM_CHILD_ROOT", &root.0)
        .env("NUMERA_BASELINE_TELEGRAM_CHILD_MODE", mode)
        .env(
            "NUMERA_BASELINE_TELEGRAM_CHILD_ARGS",
            json!(args).to_string(),
        )
        .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
        .env("NUMERA_TEST_API_BASE", base)
        .env("GENERAL_BOT_TOKEN", TOKEN)
        .env("FAKE_TOKEN", TOKEN)
        .env("PACK_OWNER_USER_ID", "424242")
        .env("BOT_ALLOWED_USER_IDS", "")
        .env("BOT_ALLOWED_CHANNEL_IDS", "")
        .env("BRAND_LOGO_BOTS", "");
    let mut argv = vec![command.get_program().to_owned()];
    argv.extend(command.get_args().map(std::ffi::OsStr::to_owned));
    let env = command
        .get_envs()
        .filter_map(|(k, v)| {
            v.map(|v| {
                (
                    k.to_string_lossy().into_owned(),
                    v.to_string_lossy().into_owned(),
                )
            })
        })
        .collect();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = root.log();
    let result = runtime
        .block_on(numera_runtime::tooling_process::capture(
            &root.0,
            &argv,
            &env,
            Duration::from_secs(30),
            Duration::from_secs(12),
            &log,
        ))
        .unwrap();
    assert_eq!(result.code, 0, "{}\n{}", result.stdout, result.stderr);
    for output in [&result.stdout, &result.stderr] {
        assert!(!output.contains(TOKEN));
    }
    assert!(!numera_runtime::process_owner::cleanup_pending(&root.0));
}
pub fn download(script: &mut Vec<Reply>, id: &str, bytes: &[u8]) {
    script.push(reply(
        "getFile",
        ok(json!({"file_path":format!("{id}.png")})),
    ));
    script.push(Reply {
        method: format!("GET {id}.png"),
        body: Body::Bytes(bytes.into()),
        observe: None,
    });
}
