use super::{NEXT, Root, app_name, dev_name, executor, machine};
use numera_runtime::{self as runtime, tooling_sandbox};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

pub struct FailureServer {
    pub base: String,
    pub lookups: Arc<Mutex<Vec<String>>>,
    pub connections: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl FailureServer {
    pub fn new() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let lookups = Arc::new(Mutex::new(vec![]));
        let seen = lookups.clone();
        let connections = Arc::new(AtomicU64::new(0));
        let accepted = connections.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        accepted.fetch_add(1, Ordering::SeqCst);
                        stream
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        let mut bytes = Vec::new();
                        let length = loop {
                            let mut buf = [0; 4096];
                            let n = stream.read(&mut buf).unwrap();
                            assert!(n > 0);
                            bytes.extend_from_slice(&buf[..n]);
                            assert!(bytes.len() < 65536);
                            if let Some(at) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                                let header = String::from_utf8_lossy(&bytes[..at]);
                                let n = header
                                    .lines()
                                    .find_map(|line| {
                                        line.to_ascii_lowercase()
                                            .strip_prefix("content-length:")
                                            .map(|n| n.trim().parse::<usize>().unwrap())
                                    })
                                    .unwrap_or(0);
                                if bytes.len() >= at + 4 + n {
                                    break (at, n);
                                }
                            }
                        };
                        let body =
                            String::from_utf8_lossy(&bytes[length.0 + 4..length.0 + 4 + length.1]);
                        let name = body
                            .split('&')
                            .find_map(|part| part.strip_prefix("name="))
                            .unwrap_or("");
                        seen.lock().unwrap().push(
                            percent_encoding::percent_decode_str(name)
                                .decode_utf8_lossy()
                                .into_owned(),
                        );
                        let body =
                            r#"{"ok":false,"description":"Bad Request: STICKERSET_INVALID"}"#;
                        write!(stream,"HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if stopped.load(Ordering::SeqCst) {
                            break;
                        }
                        assert!(Instant::now() < deadline);
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("{error}"),
                }
            }
        });
        Self {
            base,
            lookups,
            connections,
            stop,
            thread: Some(thread),
        }
    }
    pub fn env(&self) -> Vec<(&str, String)> {
        vec![
            ("GENERAL_BOT_TOKEN", "unit-test-token".into()),
            ("NUMERA_TEST_API_BASE", self.base.clone()),
        ]
    }
}
impl Drop for FailureServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            if !std::thread::panicking() {
                assert!(result.is_ok(), "loopback fixture failed");
            }
        }
    }
}

pub struct Live {
    pub owner: runtime::process_owner::OwnedProcess,
    pub url: String,
    pub root: PathBuf,
    pub account: Option<FailureServer>,
    pub directory: PathBuf,
}
impl Live {
    pub fn panel(root: &Root, data: &Path, port: u16) -> Self {
        let directory = root.0.join(format!(
            "owned-panel-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        std::fs::create_dir(directory.join("owner")).unwrap();
        let mut command = std::process::Command::new(root.0.join(app_name()));
        command
            .args([
                "panel",
                "--data-dir",
                data.to_str().unwrap(),
                "--port",
                &port.to_string(),
                "--no-open",
            ])
            .current_dir(&root.0)
            .env_clear()
            .envs(machine())
            .stdin(std::process::Stdio::null())
            .stdout(std::fs::File::create(directory.join("stdout")).unwrap())
            .stderr(std::fs::File::create(directory.join("stderr")).unwrap());
        let owner = runtime::process_owner::OwnedProcess::spawn_session(
            &mut command,
            directory.join("owner"),
        )
        .unwrap();
        Self {
            owner,
            url: format!("http://127.0.0.1:{port}"),
            root: root.0.clone(),
            account: None,
            directory,
        }
    }
    pub fn wrapper(root: &Root, source: &Path, port: u16) -> Self {
        Self::wrapper_budget(root, source, port, 30)
    }
    pub fn wrapper_budget(root: &Root, source: &Path, port: u16, seconds: u64) -> Self {
        let directory = root.0.join(format!(
            "owned-wrapper-{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        std::fs::create_dir(directory.join("owner")).unwrap();
        let account = FailureServer::new();
        let budget = seconds.to_string();
        let mut command = std::process::Command::new(root.0.join(dev_name()));
        command
            .args([
                "panel-sandbox",
                "--source",
                source.to_str().unwrap(),
                "--port",
                &port.to_string(),
                "--timeout-seconds",
                &budget,
            ])
            .current_dir(&root.0)
            .env_clear()
            .envs(machine())
            .env("GENERAL_BOT_TOKEN", "111:live")
            .env("COIN_BOT_TOKEN", "222:live")
            .env("NUMERA_TEST_API_BASE", &account.base)
            .stdin(std::process::Stdio::null())
            .stdout(std::fs::File::create(directory.join("stdout")).unwrap())
            .stderr(std::fs::File::create(directory.join("stderr")).unwrap());
        let owner = runtime::process_owner::OwnedProcess::spawn_session(
            &mut command,
            directory.join("owner"),
        )
        .unwrap();
        Self {
            owner,
            url: format!("http://127.0.0.1:{port}"),
            root: root.0.clone(),
            account: Some(account),
            directory,
        }
    }
    pub fn graceful(mut self) -> i32 {
        let controls = std::fs::read_dir(self.root.join("logs/tmp/tooling"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.join("sandbox-controller.json").is_file())
            .collect::<Vec<_>>();
        assert_eq!(controls.len(), 1);
        std::fs::write(controls[0].join("stop"), b"stop").unwrap();
        let status = executor()
            .block_on(
                self.owner
                    .wait(Duration::from_secs(15), Duration::from_secs(15)),
            )
            .unwrap();
        self.owner.finish().unwrap();
        status.code().unwrap_or(1)
    }
    pub fn finish_code(&mut self) -> i32 {
        let status = executor()
            .block_on(
                self.owner
                    .wait(Duration::from_secs(15), Duration::from_secs(15)),
            )
            .unwrap();
        self.owner.finish().unwrap();
        status.code().unwrap_or(1)
    }
    pub fn diagnostics(&self) -> String {
        ["stdout", "stderr"]
            .iter()
            .map(|name| std::fs::read_to_string(self.directory.join(name)).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn ready_for_catalog(&self, deadline: Instant) {
        // A live HTTP listener is not an initial catalog-refresh barrier.
        // Wait inside the same readiness budget for the actual child to finish
        // its startup writer phase before this separate parent writer opens.
        loop {
            let listening = get(&format!("{}/api/ping", self.url)).map(|v| v.0) == Some(200);
            let catalog_ready = listening
                && std::fs::read_dir(self.root.join("logs"))
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .filter(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.starts_with("numera-emoji_"))
                    })
                    .any(|path| {
                        let marker = b"[preview] warm-up rendered=";
                        std::fs::read(path)
                            .unwrap()
                            .windows(marker.len())
                            .any(|window| window == marker)
                    });
            if listening && catalog_ready {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "panel did not complete its initial catalog phase"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    pub fn ready(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while get(&format!("{}/api/ping", self.url)).map(|v| v.0) != Some(200) {
            assert!(Instant::now() < deadline, "panel did not become ready");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    pub fn get(&self, path: &str) -> (u16, Vec<u8>) {
        get(&format!("{}{path}", self.url)).unwrap()
    }
}

pub struct Legacy {
    pub port: u16,
    pub seen: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Legacy {
    pub fn new() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let requests = seen.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let ended = stop.clone();
        let thread = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(35);
            while !ended.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        let mut bytes = vec![];
                        while !bytes.windows(4).any(|x| x == b"\r\n\r\n") {
                            let mut block = [0; 4096];
                            let n = stream.read(&mut block).unwrap();
                            assert!(n > 0);
                            bytes.extend_from_slice(&block[..n]);
                            assert!(bytes.len() < 65536);
                        }
                        let header = String::from_utf8_lossy(&bytes);
                        let path = header
                            .lines()
                            .next()
                            .unwrap()
                            .split_whitespace()
                            .nth(1)
                            .unwrap()
                            .to_owned();
                        requests.lock().unwrap().push(path.clone());
                        let (code, body) = match path.as_str() {
                            "/api/session" => (404, "missing"),
                            "/api/ping" => (200, r#"{"ok":true}"#),
                            _ => (
                                200,
                                "<title>Numera Emoji Mapper — Curate</title><div id=\"items-data\"></div>/static/panel-grid.js<script>const TOKEN = \"fixture\";</script>",
                            ),
                        };
                        write!(stream,"HTTP/1.1 {code} Answer\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline);
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("{error}"),
                }
            }
        });
        Self {
            port,
            seen,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Legacy {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            if !std::thread::panicking() {
                assert!(result.is_ok(), "legacy listener fixture failed");
            }
        }
    }
}
pub fn active_clone(root: &Root) -> PathBuf {
    let parent = root.0.join("logs/tmp");
    let paths = std::fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_str().unwrap().starts_with(tooling_sandbox::PREFIX))
                && path.join("catalog.db").is_file()
        })
        .collect::<Vec<_>>();
    assert_eq!(paths.len(), 1);
    paths[0].clone()
}
impl Drop for Live {
    fn drop(&mut self) {
        if let Err(error) = self.owner.finish() {
            if std::thread::panicking() {
                eprintln!("panel custody unresolved; fixture retained: {error}");
            } else {
                panic!("panel/codec descendants must settle before fixture cleanup: {error}");
            }
        }
    }
}
pub fn get(url: &str) -> Option<(u16, Vec<u8>)> {
    let response = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?
        .get(url)
        .send()
        .ok()?;
    let code = response.status().as_u16();
    Some((code, response.bytes().ok()?.to_vec()))
}
pub fn port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}
