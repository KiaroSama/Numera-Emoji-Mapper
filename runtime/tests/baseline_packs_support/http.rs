use numera_runtime::telegram::Telegram;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
pub const TOKEN: &str = "1234567890:AAxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
#[derive(Clone, Debug)]
pub struct Request {
    pub method: String,
    pub form: BTreeMap<String, String>,
    pub files: BTreeMap<String, Vec<u8>>,
}
pub struct Response(pub Vec<u8>);
pub fn payload(v: Value) -> Response {
    Response(v.to_string().into_bytes())
}
pub fn ok(v: Value) -> Response {
    payload(json!({"ok":true,"result":v}))
}
pub struct Bot {
    pub base: String,
    pub requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Bot {
    pub fn new(mut answer: impl FnMut(&Request) -> Response + Send + 'static) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let requests = Arc::new(Mutex::new(vec![]));
        let seen = requests.clone();
        let thread = std::thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                let mut stream = match listener.accept() {
                    Ok((s, _)) => s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
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
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0, "incomplete fixture HTTP request");
                    bytes.extend_from_slice(&chunk[..n]);
                    assert!(bytes.len() < 8 * 1024 * 1024);
                    if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let h = String::from_utf8(bytes[..end].to_vec()).unwrap();
                        let len = h
                            .lines()
                            .find_map(|l| {
                                l.split_once(':')
                                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                    .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + len {
                            break (h, bytes[end + 4..end + 4 + len].to_vec());
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
                let method = path
                    .strip_prefix(&format!("/bot{TOKEN}/"))
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        format!(
                            "GET {}",
                            path.strip_prefix(&format!("/file/bot{TOKEN}/"))
                                .expect("fixture token and route")
                        )
                    });
                let (form, files) = parse(&headers, &body);
                let req = Request {
                    method,
                    form,
                    files,
                };
                seen.lock().unwrap().push(req.clone());
                let body = answer(&req).0;
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                let _ = stream.write_all(&body);
            }
        });
        Self {
            base,
            requests,
            stop,
            thread: Some(thread),
        }
    }
    pub fn telegram(&self) -> Telegram {
        Telegram::new(TOKEN.into(), self.base.clone()).unwrap()
    }
    pub fn methods(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.method.clone())
            .collect()
    }
}
impl Drop for Bot {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take()
            && let Err(error) = t.join()
        {
            if std::thread::panicking() {
                eprintln!("fixture HTTP worker also panicked during test unwinding");
            } else {
                std::panic::resume_unwind(error);
            }
        }
    }
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
