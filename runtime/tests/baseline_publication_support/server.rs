use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
#[derive(Clone, Debug)]
pub struct Request {
    pub path: String,
    pub form: BTreeMap<String, String>,
    pub files: BTreeMap<String, (String, Vec<u8>)>,
    pub authorization: String,
    pub raw: Vec<u8>,
}
pub fn sticker(fuid: &str, cid: &str) -> Value {
    json!({"file_unique_id":fuid,"custom_emoji_id":cid,"file_id":format!("f-{fuid}")})
}
#[derive(Default)]
pub struct Model {
    pub username: Option<String>,
    pub sets: BTreeMap<String, Vec<Value>>,
    pub bodies: BTreeMap<String, Vec<u8>>,
    pub fuids: BTreeMap<String, String>,
    pub unknown: BTreeSet<String>,
    pub undownloadable: BTreeSet<String>,
    pub requests: Vec<Request>,
    pub uploaded: Vec<String>,
    pub adds: Vec<String>,
    pub downloaded: Vec<String>,
    pub checked: Vec<String>,
    pub messages: Vec<Value>,
    pub worker: Vec<Value>,
    pub prefix: String,
    pub fail_after: Option<usize>,
    pub reject_adds: usize,
    pub transport_adds: usize,
    pub mutation_mode: String,
    pub final_edit: String,
    pub final_armed: bool,
    pub edited: bool,
    pub refuse: BTreeSet<String>,
    pub unknown_upload: BTreeSet<String>,
    pub worker_failure: bool,
    pub send_failure: bool,
    pub occupied: bool,
}
impl Model {
    pub fn ordinary() -> Self {
        Self {
            prefix: "UP-".into(),
            ..Self::default()
        }
    }
    fn foreign(fuid: &str) -> Vec<u8> {
        let seed = fuid.as_bytes().iter().map(|b| u32::from(*b)).sum::<u32>() % 200;
        Self::pixels([seed as u8, 255 - seed as u8, 90, 255])
    }
    fn pixels(color: [u8; 4]) -> Vec<u8> {
        let mut im = image::RgbaImage::new(100, 100);
        for y in 20..80 {
            for x in 20..80 {
                im.put_pixel(x, y, image::Rgba(color));
            }
        }
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(im)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }
    fn answer(&mut self, r: Request) -> Option<(u16, Vec<u8>)> {
        let path = r.path.clone();
        self.requests.push(r.clone());
        if path.starts_with("/file/") {
            let fuid = path.rsplit('/').next().unwrap().to_string();
            if self.undownloadable.contains(&fuid) {
                return Some((404, b"no file".to_vec()));
            }
            let body = self
                .bodies
                .get(&fuid)
                .cloned()
                .unwrap_or_else(|| Self::foreign(&fuid));
            if self.uploaded.len() >= 2 && fuid.ends_with("item1") {
                self.final_armed = true;
            }
            return Some((200, body));
        }
        if path == "/worker" {
            self.worker.push(serde_json::from_slice(&r.raw).unwrap());
            return Some((if self.worker_failure { 503 } else { 200 }, b"{}".to_vec()));
        }
        let method = path.rsplit('/').next().unwrap();
        let ok = |value: Value| {
            Some((
                200,
                json!({"ok":true,"result":value}).to_string().into_bytes(),
            ))
        };
        let no = |why: &str| {
            Some((
                200,
                json!({"ok":false,"description":why})
                    .to_string()
                    .into_bytes(),
            ))
        };
        match method {
            "getMe" => ok(json!({"username":self.username.as_deref().unwrap_or("YourEmojiBot")})),
            "getStickerSet" => {
                let name = &r.form["name"];
                if self.unknown.contains(name) {
                    return Some((200, b"[]".to_vec()));
                }
                if self.final_armed
                    && !self.edited
                    && !self.final_edit.is_empty()
                    && let Some(set) = self.sets.get_mut(name)
                {
                    match self.final_edit.as_str() {
                        "reverse" => set.reverse(),
                        "foreign" => set[0] = sticker("NEVER-SEEN", "foreign-cid"),
                        _ => {}
                    }
                    self.edited = true;
                }
                match self.sets.get(name) {
                    Some(set) => ok(json!({"stickers":set})),
                    None => no("STICKERSET_INVALID"),
                }
            }
            "getFile" => {
                let fid = &r.form["file_id"];
                let fuid = fid.strip_prefix("f-").unwrap_or(fid);
                self.downloaded.push(fuid.to_string());
                if self.undownloadable.contains(fuid) {
                    return no("file unavailable");
                }
                ok(json!({"file_path":fuid}))
            }
            "sendMessage" => {
                if self.send_failure {
                    return no("message refused");
                }
                self.messages.push(json!(r.form));
                ok(json!(true))
            }
            "uploadStickerFile" => {
                let (filename, _) = r.files.values().next().unwrap();
                self.checked.push(filename.clone());
                if self.unknown_upload.contains(filename) {
                    return None;
                }
                if self.refuse.contains(filename) {
                    return no("Bad Request: wrong file type");
                }
                ok(json!({"file_id":"checked"}))
            }
            "createNewStickerSet" | "addStickerToSet" => {
                let name = r.form["name"].clone();
                let creating = method == "createNewStickerSet";
                if creating && (self.occupied || self.sets.contains_key(&name)) {
                    return no("sticker set name is already occupied");
                }
                let (filename, bytes) = r.files.values().next().unwrap();
                let stem = std::path::Path::new(filename)
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string();
                if !creating {
                    self.adds.push(stem.clone());
                    if self.reject_adds > 0 {
                        self.reject_adds -= 1;
                        return no("Bad Request: wrong file type");
                    }
                    if self.transport_adds > 0 {
                        self.transport_adds -= 1;
                        return None;
                    }
                    if self.mutation_mode == "silent" {
                        return ok(json!(true));
                    }
                    if self.mutation_mode == "foreign_only" {
                        self.mutation_mode.clear();
                        self.sets
                            .get_mut(&name)
                            .unwrap()
                            .push(sticker("FU-foreign-1", "foreign-cid"));
                        self.bodies
                            .insert("FU-foreign-1".into(), Self::pixels([10, 200, 40, 255]));
                        return None;
                    }
                }
                if self.fail_after.is_some_and(|n| self.uploaded.len() >= n) {
                    return no("BAD_REQUEST: STICKER_PNG_DIMENSIONS");
                }
                if creating {
                    self.sets.insert(name.clone(), Vec::new());
                }
                let set = self.sets.get_mut(&name).unwrap();
                let fuid = self
                    .fuids
                    .get(&stem)
                    .cloned()
                    .unwrap_or_else(|| format!("{}{stem}", self.prefix));
                let cid = format!("{name}-{}", set.len());
                set.push(sticker(&fuid, &cid));
                self.bodies.insert(fuid, bytes.clone());
                self.uploaded.push(stem);
                if !creating && self.mutation_mode == "race" {
                    self.sets
                        .get_mut(&name)
                        .unwrap()
                        .push(sticker("SOMEONE-ELSE", "other-cid"));
                }
                if !creating && self.mutation_mode == "after_apply" {
                    self.mutation_mode.clear();
                    return None;
                }
                ok(json!(true))
            }
            _ => panic!("unexpected publication request {}", r.path),
        }
    }
}
fn find(bytes: &[u8], needle: &[u8]) -> Option<usize> {
    bytes.windows(needle.len()).position(|w| w == needle)
}
fn request(s: &mut TcpStream) -> Request {
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut bytes = Vec::new();
    let mut block = [0; 8192];
    let end = loop {
        assert!(Instant::now() < deadline);
        let n = s.read(&mut block).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&block[..n]);
        assert!(bytes.len() < 16 * 1024 * 1024);
        if let Some(end) = find(&bytes, b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..end]).unwrap().to_string();
    let path = headers
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    let header = |key: &str| {
        headers.lines().find_map(|line| {
            line.split_once(':')
                .filter(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v.trim().to_string())
        })
    };
    let length = header("content-length")
        .map(|v| v.parse::<usize>().unwrap())
        .unwrap_or(0);
    assert!(length < 16 * 1024 * 1024);
    while bytes.len() < end + length {
        assert!(Instant::now() < deadline);
        let n = s.read(&mut block).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&block[..n]);
    }
    let raw = bytes[end..end + length].to_vec();
    let mut form = BTreeMap::new();
    let mut files = BTreeMap::new();
    if let Some(kind) = header("content-type") {
        if let Some(boundary) = kind.split("boundary=").nth(1) {
            let marker = format!("--{}", boundary.trim_matches('"')).into_bytes();
            let mut at = 0;
            while let Some(start) = find(&raw[at..], &marker) {
                let start = at + start + marker.len();
                if raw.get(start..start + 2) == Some(b"--") {
                    break;
                }
                let start = start + 2;
                let split = start + find(&raw[start..], b"\r\n\r\n").unwrap();
                let part = std::str::from_utf8(&raw[start..split]).unwrap();
                let textstart = split + 4;
                let next = textstart + find(&raw[textstart..], &marker).unwrap();
                let value = raw[textstart..next - 2].to_vec();
                let attr = |key: &str| {
                    part.split(&format!("{key}=\""))
                        .nth(1)
                        .map(|v| v.split('"').next().unwrap().to_string())
                };
                let name = attr("name").unwrap();
                if let Some(filename) = attr("filename") {
                    files.insert(name, (filename, value));
                } else {
                    form.insert(name, String::from_utf8(value).unwrap());
                }
                at = next;
            }
        } else if kind.starts_with("application/x-www-form-urlencoded") {
            form = reqwest::Url::parse(&format!(
                "http://127.0.0.1/?{}",
                std::str::from_utf8(&raw).unwrap()
            ))
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect();
        }
    }
    Request {
        path,
        form,
        files,
        authorization: header("authorization").unwrap_or_default(),
        raw,
    }
}
pub struct Bot {
    pub base: String,
    pub model: Arc<Mutex<Model>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Bot {
    pub fn new(model: Model) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let model = Arc::new(Mutex::new(model));
        let stop = Arc::new(AtomicBool::new(false));
        let state = model.clone();
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let r = request(&mut stream);
                        if let Some((status, body)) = state.lock().unwrap().answer(r) {
                            write!(stream,"HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                            stream.write_all(&body).unwrap();
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        });
        Self {
            base,
            model,
            stop,
            thread: Some(thread),
        }
    }
    pub fn telegram(&self) -> super::production::telegram::Telegram {
        super::telegram(&self.base)
    }
    pub fn edit<T>(&self, f: impl FnOnce(&mut Model) -> T) -> T {
        f(&mut self.model.lock().unwrap())
    }
}
impl Drop for Bot {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            if !std::thread::panicking() {
                thread.join().unwrap();
            } else {
                let _ = thread.join();
            }
        }
    }
}
