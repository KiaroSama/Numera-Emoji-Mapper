use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::Duration;
#[derive(Clone, Debug)]
pub struct Request {
    pub verb: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
impl Request {
    pub fn method(&self) -> &str {
        self.path.rsplit('/').next().unwrap()
    }
    pub fn form(&self) -> BTreeMap<String, String> {
        reqwest::Url::parse(&format!(
            "http://fixture/?{}",
            String::from_utf8_lossy(&self.body)
        ))
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
    }
    pub fn field(&self, name: &str) -> Vec<u8> {
        let content = self
            .headers
            .get("content-type")
            .map(String::as_str)
            .unwrap_or("");
        if let Some(boundary) = content.split("boundary=").nth(1) {
            let separator = format!("--{}", boundary.trim_matches('"'));
            let needle = format!("; name=\"{name}\"");
            let mut start = 0;
            let mut fields = Vec::new();
            while let Some(at) = find(&self.body[start..], separator.as_bytes()) {
                let at = start + at + separator.len();
                let Some(next) = find(&self.body[at..], separator.as_bytes()) else {
                    break;
                };
                let part = &self.body[at..at + next];
                if let Some(split) = find(part, b"\r\n\r\n")
                    && String::from_utf8_lossy(&part[..split]).contains(&needle)
                {
                    fields.push(part[split + 4..part.len() - 2].to_vec());
                }
                start = at;
            }
            assert_eq!(
                fields.len(),
                1,
                "multipart request must carry exactly one {name}"
            );
            return fields.pop().unwrap();
        }
        self.form()
            .get(name)
            .unwrap_or_else(|| panic!("form field missing {name}"))
            .as_bytes()
            .to_vec()
    }
}
fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}
pub enum Reply {
    Bytes(u16, Vec<u8>),
    Disconnect,
}
impl Reply {
    pub fn json(v: Value) -> Self {
        Self::Bytes(200, serde_json::to_vec(&v).unwrap())
    }
    pub fn ok(v: Value) -> Self {
        Self::json(serde_json::json!({"ok":true,"result":v}))
    }
}
pub struct Server {
    pub base: String,
    pub port: u16,
    pub observed: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Server {
    pub fn new(mut handler: impl FnMut(&Request) -> Reply + Send + 'static) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(AtomicBool::new(false));
        let observed = Arc::new(Mutex::new(Vec::new()));
        let signal = stop.clone();
        let calls = observed.clone();
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                if signal.load(Ordering::SeqCst) {
                    break;
                }
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    if let Some(i) = find(&bytes, b"\r\n\r\n") {
                        break i;
                    }
                    let mut block = [0; 4096];
                    let n = stream.read(&mut block).unwrap();
                    assert!(n > 0, "incomplete request headers");
                    bytes.extend_from_slice(&block[..n]);
                    assert!(bytes.len() < 1024 * 1024);
                };
                let header = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                let mut lines = header.lines();
                let first = lines.next().unwrap().split_whitespace().collect::<Vec<_>>();
                let headers = lines
                    .filter_map(|l| l.split_once(':'))
                    .map(|(k, v)| (k.to_lowercase(), v.trim().to_owned()))
                    .collect::<BTreeMap<_, _>>();
                let size = headers
                    .get("content-length")
                    .map_or(0, |s| s.parse::<usize>().unwrap());
                assert!(size < 32 * 1024 * 1024);
                while bytes.len() < header_end + 4 + size {
                    let mut block = [0; 8192];
                    let n = stream.read(&mut block).unwrap();
                    assert!(n > 0, "incomplete request body");
                    bytes.extend_from_slice(&block[..n]);
                }
                let req = Request {
                    verb: first[0].into(),
                    path: first[1].into(),
                    headers,
                    body: bytes[header_end + 4..header_end + 4 + size].to_vec(),
                };
                calls.lock().unwrap().push(req.clone());
                match handler(&req) {
                    Reply::Disconnect => {
                        let _ = stream.shutdown(std::net::Shutdown::Both);
                    }
                    Reply::Bytes(status, body) => {
                        let header = format!(
                            "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        stream.write_all(header.as_bytes()).unwrap();
                        stream.write_all(&body).unwrap();
                    }
                }
            }
        });
        Self {
            base: format!("http://127.0.0.1:{port}"),
            port,
            observed,
            stop,
            thread: Some(thread),
        }
    }
    pub fn methods(&self) -> Vec<String> {
        self.observed
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.method().to_owned())
            .collect()
    }
    pub fn telegram(&self) -> numera_runtime::telegram::Telegram {
        numera_runtime::telegram::Telegram::new("test-only-token".into(), self.base.clone())
            .unwrap()
    }
    pub fn environment(&self) -> Vec<(&'static str, String)> {
        vec![("NUMERA_TEST_API_BASE", self.base.clone())]
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        let result = self.thread.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}
