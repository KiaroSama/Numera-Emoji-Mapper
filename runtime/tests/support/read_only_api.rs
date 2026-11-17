//! Exact scripted Bot API reads and downloads, with bounded I/O and cancellation on unwind.
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const TOKEN: &str = "test-only-token";
pub struct Reply {
    path: String,
    form: BTreeMap<String, String>,
    bytes: Vec<u8>,
    held: Option<tokio::sync::oneshot::Sender<()>>,
}
impl Reply {
    pub fn api(method: &str, key: &str, value: &str, result: Value) -> Self {
        Self::envelope(method, key, value, json!({"ok":true,"result":result}))
    }
    pub fn envelope(method: &str, key: &str, value: &str, payload: Value) -> Self {
        assert!(matches!(
            method,
            "getMe" | "getStickerSet" | "getFile" | "getCustomEmojiStickers"
        ));
        let form = if method == "getMe" {
            assert!(key.is_empty() && value.is_empty());
            BTreeMap::new()
        } else {
            assert!(!key.is_empty());
            BTreeMap::from([(key.into(), value.into())])
        };
        Self {
            path: format!("/bot{TOKEN}/{method}"),
            form,
            bytes: payload.to_string().into_bytes(),
            held: None,
        }
    }
    // This shared fixture is also compiled by suites without a cancellation case.
    #[allow(dead_code)]
    pub fn hold(self) -> (Self, tokio::sync::oneshot::Receiver<()>) {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        (
            Self {
                held: Some(sender),
                ..self
            },
            receiver,
        )
    }
    pub fn file(path: &str, bytes: &[u8]) -> Self {
        Self {
            path: format!("/file/bot{TOKEN}/{path}"),
            form: BTreeMap::new(),
            bytes: bytes.to_vec(),
            held: None,
        }
    }
}

type Observed = (String, BTreeMap<String, String>);
fn request(stream: &mut TcpStream, deadline: Instant) -> Result<Observed, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let end = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("read-only fixture request wall limit exceeded".into());
        }
        stream
            .set_read_timeout(Some(remaining.min(Duration::from_secs(2))))
            .map_err(|e| e.to_string())?;
        let count = stream.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 || bytes.len() + count > 16384 {
            return Err("truncated or excessive read-only request".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..end])
        .map_err(|e| e.to_string())?
        .to_owned();
    let mut line = headers
        .lines()
        .next()
        .ok_or("missing request line")?
        .split_whitespace();
    let method = line.next().ok_or("missing method")?;
    let path = line.next().ok_or("missing path")?.to_owned();
    if !matches!(method, "GET" | "POST") {
        return Err("read-only fixture received an unsupported HTTP method".into());
    }
    let length = headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>())
        })
        .transpose()
        .map_err(|e| e.to_string())?
        .unwrap_or(0);
    if length > 4096 || bytes.len() > end + length {
        return Err("excessive read-only form".into());
    }
    while bytes.len() < end + length {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("read-only fixture body wall limit exceeded".into());
        }
        stream
            .set_read_timeout(Some(remaining.min(Duration::from_secs(2))))
            .map_err(|e| e.to_string())?;
        let count = stream
            .read(&mut buffer[..(end + length - bytes.len()).min(4096)])
            .map_err(|e| e.to_string())?;
        if count == 0 {
            return Err("truncated read-only form".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    let body = std::str::from_utf8(&bytes[end..]).map_err(|e| e.to_string())?;
    let mut url = reqwest::Url::parse("http://127.0.0.1/").map_err(|e| e.to_string())?;
    url.set_query(Some(body));
    let form = url.query_pairs().into_owned().collect();
    if (method == "GET") != path.starts_with("/file/") {
        return Err("read-only fixture used the wrong HTTP method".into());
    }
    Ok((path, form))
}

pub struct Fixture {
    pub base: String,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<Result<Vec<String>, String>>>,
}
impl Fixture {
    #[allow(dead_code)] // Shared fixture: some consumers supply their existing whole-flow deadline.
    pub fn new(script: Vec<Reply>) -> Self {
        Self::start(script, None)
    }
    #[allow(dead_code)] // Shared fixture: ordinary read-only cohorts retain the default30s budget.
    pub fn with_deadline(script: Vec<Reply>, deadline: Instant) -> Self {
        Self::start(script, Some(deadline))
    }
    fn start(script: Vec<Reply>, deadline: Option<Instant>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (stop, receiver) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let deadline = deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(30));
            let mut observed = Vec::new();
            let count = script.len();
            for (index, reply) in script.into_iter().enumerate() {
                let mut stream = loop {
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "read-only fixture script deadline elapsed after {index}/{count} completed responses; next expected path: {}",
                            reply.path
                        ));
                    }
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            // Cancellation wakes the bounded readiness wait on assertion unwind.
                            match receiver.recv_timeout(Duration::from_millis(5)) {
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                                _ => return Err("read-only fixture stopped early".into()),
                            }
                        }
                        Err(error) => return Err(error.to_string()),
                    }
                };
                stream.set_nonblocking(false).map_err(|e| e.to_string())?;
                let (path, form) = request(&mut stream, deadline)?;
                if path != reply.path || form != reply.form {
                    return Err(format!("unexpected read-only API request: {path}"));
                }
                if let Some(held) = reply.held {
                    if index + 1 != count {
                        return Err("held read-only response must end the script".into());
                    }
                    observed.push(path);
                    held.send(())
                        .map_err(|_| "held request observer disappeared")?;
                    return match receiver
                        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => Ok(observed),
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            Err("held read-only fixture deadline elapsed".into())
                        }
                    };
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err("read-only fixture response wall limit exceeded".into());
                }
                stream
                    .set_write_timeout(Some(remaining.min(Duration::from_secs(2))))
                    .map_err(|e| e.to_string())?;
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    reply.bytes.len()
                )
                .map_err(|e| e.to_string())?;
                stream.write_all(&reply.bytes).map_err(|e| e.to_string())?;
                observed.push(path);
            }
            Ok(observed)
        });
        Self {
            base,
            stop: Some(stop),
            thread: Some(thread),
        }
    }
    pub fn finish(&mut self) -> Result<Vec<String>, String> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.thread
            .take()
            .ok_or("read-only fixture already joined")?
            .join()
            .map_err(|_| "read-only fixture thread panicked")?
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.thread.is_some()
            && let Err(error) = self.finish()
        {
            eprintln!("Read-only fixture cleanup: {error}");
        }
    }
}
