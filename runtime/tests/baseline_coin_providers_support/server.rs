//! Finite loopback fixture shared by independently named original coin/provider tests.
#![allow(dead_code)]
use super::rt::{logging::RunLog, telegram::Telegram};
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
    Uploaded(String),
    Disconnect,
}
pub type Action = Box<dyn FnOnce(&Request) + Send>;
pub struct Reply {
    pub method: String,
    pub body: Body,
    pub observe: Option<PathBuf>,
    pub status: u16,
    pub retry_after: Option<String>,
    pub action: Option<Action>,
}
pub fn reply(method: &str, body: Value) -> Reply {
    Reply {
        method: method.into(),
        body: Body::Json(body),
        observe: None,
        status: 200,
        retry_after: None,
        action: None,
    }
}
pub fn fault(method: &str) -> Reply {
    Reply {
        method: method.into(),
        body: Body::Disconnect,
        observe: None,
        status: 200,
        retry_after: None,
        action: None,
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
                stream.set_nonblocking(false).unwrap();
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
                } else if let Some(name) = path.strip_prefix(&format!("/file/bot{TOKEN}/")) {
                    format!("GET {name}")
                } else if headers.starts_with("GET ") {
                    format!("GET {path}")
                } else {
                    return Err("unexpected fixture token/route".into());
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
                if let Some(action) = next.action {
                    action(seen.last().unwrap());
                }
                let status = next.status;
                let retry = next
                    .retry_after
                    .map(|v| format!("Retry-After: {v}\r\n"))
                    .unwrap_or_default();
                let body = match next.body {
                    Body::Uploaded(field) => Body::Bytes(
                        seen.iter()
                            .rev()
                            .find_map(|r| r.files.get(&field).cloned())
                            .expect("proof download requires prior real upload bytes"),
                    ),
                    body => body,
                };
                match body {
                    Body::Uploaded(_) => unreachable!(),
                    Body::Disconnect => drop(stream),
                    Body::Bytes(body) => {
                        stream.write_all(format!("HTTP/1.1 {status} Fixture\r\n{retry}Content-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).map_err(|e|e.to_string())?;
                        stream.write_all(&body).map_err(|e| e.to_string())?;
                    }
                    Body::Json(payload) => {
                        let body = payload.to_string();
                        stream.write_all(format!("HTTP/1.1 {status} Fixture\r\n{retry}Content-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n{body}",body.len()).as_bytes()).map_err(|e|e.to_string())?;
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
