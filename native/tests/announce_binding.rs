//! Original announcement HTTP assertions through the canonical registered Python binding.
//! Retained-wrapper None return and api_base normalization remain separate acceptance gaps.
use _native::{_native as canonical_native, logging::RunLog, tooling_environment, tooling_process};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

const CASE: &str = "test_direct_cards_list_worker_refusal_and_nondefault_timeout";
const ROLE: &str = "NUMERA_ANNOUNCE_BINDING_ROLE";
const BASE: &str = "NUMERA_ANNOUNCE_BINDING_BASE";
const TOKEN: &str = "test-only-token";
const SECRET: &str = "fixture-secret";
const LIMIT: usize = 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

fn packs() -> Value {
    json!([
        {"name":"fixture1_by_YourEmojiBot","title":"Fixture 1","count":1},
        {"name":"fixture2_by_YourEmojiBot","title":"","count":2}
    ])
}

fn shared_publish_body() -> Value {
    use sha2::{Digest, Sha256};
    let bytes = include_bytes!("../../tests/fixtures/contracts/publish_request.json");
    assert_eq!(
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        "dbc24993190ed27e80c3ed55c22b311e3ba40d8775106603ae9b28087f8e4ae3"
    );
    serde_json::from_slice(bytes).unwrap()
}

fn child(role: &str) -> PyResult<()> {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    for key in [
        "TELEGRAM_BOT_TOKEN",
        "GENERAL_BOT_TOKEN",
        "COIN_BOT_TOKEN",
        "PACK_OWNER_USER_ID",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        assert!(std::env::var_os(key).is_none(), "inherited {key}");
    }
    let base = std::env::var(BASE).unwrap();
    let url = reqwest::Url::parse(&base).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    assert!(url.username().is_empty() && url.password().is_none());
    assert!(url.query().is_none() && url.fragment().is_none());
    let root = std::env::current_dir().unwrap();
    println!("announcement role started: {role}");
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = PyModule::new(py, "_native_announcement_test")?;
        canonical_native(&module)?;
        let binding = module.getattr("announce_packs_json")?;
        assert!(binding.is_callable());
        let invoke = |note: &str, style: &str, worker_only: bool, timeout: u8| {
            let request = json!({"owner":if worker_only {0} else {111111111},
                "packs":packs(),"bot":"coin","note":note,"style":style,
                "worker_only":worker_only,"timeout":timeout});
            binding.call1((
                root.to_str().unwrap(),
                if worker_only { "" } else { TOKEN },
                &base,
                request.to_string(),
            ))
        };
        match role {
            "cards" => {
                assert_eq!(std::env::var("PACK_LINKS_CHAT_ID").as_deref(), Ok(""));
                assert_eq!(std::env::var("WORKER_PUBLISH_URL").as_deref(), Ok(""));
                assert_eq!(std::env::var("WORKER_PUBLISH_SECRET").as_deref(), Ok(""));
                assert_eq!(
                    invoke("Note", "cards", false, 30)?.extract::<String>()?,
                    "111111111"
                );
            }
            "list" => {
                assert_eq!(
                    std::env::var("PACK_LINKS_CHAT_ID").as_deref(),
                    Ok("@fixturechannel")
                );
                assert_eq!(std::env::var("WORKER_PUBLISH_URL").as_deref(), Ok(""));
                assert_eq!(std::env::var("WORKER_PUBLISH_SECRET").as_deref(), Ok(""));
                assert_eq!(
                    invoke("Note", "list", false, 30)?.extract::<String>()?,
                    "@fixturechannel"
                );
            }
            "numeric" => {
                assert_eq!(
                    std::env::var("PACK_LINKS_CHAT_ID").as_deref(),
                    Ok("\u{1c}-۰۱۰۰_۱۱۱_۱۱۱_۱۱۱۱\u{1f}")
                );
                assert_eq!(std::env::var("WORKER_PUBLISH_URL").as_deref(), Ok(""));
                assert_eq!(std::env::var("WORKER_PUBLISH_SECRET").as_deref(), Ok(""));
                assert_eq!(
                    invoke("Note", "list", false, 30)?.extract::<String>()?,
                    "-1001111111111"
                );
            }
            "worker" => {
                assert_eq!(
                    std::env::var("WORKER_PUBLISH_URL").unwrap(),
                    format!("{base}/publish")
                );
                assert_eq!(
                    std::env::var("WORKER_PUBLISH_SECRET").as_deref(),
                    Ok(SECRET)
                );
                let mut shared = shared_publish_body();
                shared["owner"] = json!(0);
                shared["worker_only"] = json!(true);
                shared["timeout"] = json!(30);
                assert_eq!(
                    binding
                        .call1((root.to_str().unwrap(), "", &base, shared.to_string()))?
                        .extract::<String>()?,
                    "the worker"
                );
                assert_eq!(
                    invoke("Note", "list", false, 30)?.extract::<String>()?,
                    "the worker"
                );
                // The Python wrapper discards this result. This call checks the
                // actual worker-only route/timeout argument, never its None wrapper return.
                assert_eq!(
                    invoke("", "cards", true, 2)?.extract::<String>()?,
                    "the worker"
                );
                for message in ["HTTP 503", "outcome unknown"] {
                    let error = invoke("", "cards", false, 30).unwrap_err();
                    assert!(
                        error.get_type(py).is(&py.get_type::<PyRuntimeError>()),
                        "{error}"
                    );
                    assert!(
                        error.value(py).str()?.to_str()?.contains(message),
                        "{error}"
                    );
                }
            }
            _ => panic!("unexpected announcement role"),
        }
        Ok(())
    })?;
    println!("announcement role completed: {role}");
    Ok(())
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, String> {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("Missing project root")?;
        let parent = tooling_environment::confined_directory(project, "logs/tmp", true)?;
        loop {
            let path = parent.join(format!(
                "announcement binding {} {}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
    }
    fn clean(&self) -> Result<(), String> {
        tooling_environment::reject_links(&self.0, "Linked announcement scratch")?;
        let tooling = self.0.join("logs/tmp/tooling");
        tooling_environment::reject_links(&tooling, "Linked announcement tooling scratch")?;
        match std::fs::read_dir(tooling) {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    return Err("Announcement ownership unresolved; scratch retained".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        match std::fs::remove_dir_all(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Announcement scratch cleanup failed: {error}")),
        }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        // Only the verified-success path may delete diagnostics. In particular,
        // an assertion unwind must not turn a finished child into erased evidence.
        if std::fs::symlink_metadata(&self.0).is_ok() {
            eprintln!("Announcement scratch retained: {}", self.0.display());
        }
    }
}

fn children(root: &Path, base: &str) -> Result<(), String> {
    let started = Instant::now();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let log = RunLog::open(
        root,
        "announce-binding-test",
        vec![TOKEN.into(), SECRET.into()],
    );
    if log.path.is_none() {
        return Err("Announcement test logging could not initialize".into());
    }
    let outcome = (|| {
        let mut original = BTreeMap::new();
        for key in [
            "PATH",
            "SystemRoot",
            "SYSTEMROOT",
            "WINDIR",
            "LD_LIBRARY_PATH",
            "DYLD_LIBRARY_PATH",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "PYTHONHOME",
        ] {
            if let Ok(value) = std::env::var(key) {
                original.insert(key.into(), value);
            }
        }
        original.extend([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("PYTHONNOUSERSITE".into(), "1".into()),
            ("PYTHONSAFEPATH".into(), "1".into()),
            (BASE.into(), base.into()),
        ]);
        let original = tooling_environment::environment(root, &original, false)?;
        for role in ["cards", "list", "numeric", "worker"] {
            let remaining = Duration::from_secs(35)
                .checked_sub(started.elapsed())
                .filter(|budget| !budget.is_zero())
                .ok_or("Announcement parent work deadline exceeded")?;
            let mut environment = original.clone();
            environment.extend([
                (ROLE.into(), role.into()),
                (
                    "PACK_LINKS_CHAT_ID".into(),
                    match role {
                        "list" => "@fixturechannel",
                        "numeric" => "\u{1c}-۰۱۰۰_۱۱۱_۱۱۱_۱۱۱۱\u{1f}",
                        _ => "",
                    }
                    .into(),
                ),
                (
                    "WORKER_PUBLISH_URL".into(),
                    if role == "worker" {
                        format!("{base}/publish")
                    } else {
                        String::new()
                    },
                ),
                (
                    "WORKER_PUBLISH_SECRET".into(),
                    if role == "worker" { SECRET } else { "" }.into(),
                ),
            ]);
            let argv = [
                std::env::current_exe()
                    .map_err(|e| e.to_string())?
                    .into_os_string(),
                OsString::from("--exact"),
                CASE.into(),
                "--nocapture".into(),
                "--test-threads=1".into(),
            ];
            let wall = remaining.min(Duration::from_secs(12));
            let actual = runtime.block_on(tooling_process::capture(
                root,
                &argv,
                &environment,
                wall,
                wall.min(Duration::from_secs(5)),
                &log,
            ))?;
            if actual.code != 0
                || !actual
                    .stdout
                    .contains(&format!("announcement role started: {role}"))
                || !actual
                    .stdout
                    .contains(&format!("announcement role completed: {role}"))
            {
                return Err(format!(
                    "Announcement role {role} failed (exit={}):\n{}\n{}",
                    actual.code, actual.stdout, actual.stderr
                ));
            }
        }
        Ok(())
    })();
    log.finish(if outcome.is_ok() { 0 } else { 1 });
    outcome
}

#[derive(Debug)]
struct Request {
    path: String,
    content_type: String,
    authorization: Option<String>,
    body: Vec<u8>,
}
fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() >= 16 * 1024 || Instant::now() >= deadline {
            return Err("Announcement headers exceeded their size/time bound".into());
        }
        stream
            .set_read_timeout(Some(deadline.saturating_duration_since(Instant::now())))
            .map_err(|e| e.to_string())?;
        let mut byte = [0];
        stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
        bytes.push(byte[0]);
    }
    let header = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    let mut lines = header.split("\r\n");
    let mut line = lines
        .next()
        .ok_or("Missing request line")?
        .split_whitespace();
    if line.next() != Some("POST") {
        return Err("Announcement fixture expected POST".into());
    }
    let path = line.next().ok_or("Missing announcement path")?.into();
    if line.next() != Some("HTTP/1.1") || line.next().is_some() {
        return Err("Malformed announcement request line".into());
    }
    let mut length = None;
    let mut content_type = String::new();
    let mut authorization = None;
    for line in lines.filter(|line| !line.is_empty()) {
        let (key, value) = line
            .split_once(':')
            .ok_or("Malformed announcement header")?;
        let value = value.trim();
        if key.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err("Duplicate announcement content length".into());
            }
            length = Some(value.parse::<usize>().map_err(|e| e.to_string())?);
        } else if key.eq_ignore_ascii_case("content-type") {
            content_type = value.into();
        } else if key.eq_ignore_ascii_case("authorization") {
            authorization = Some(value.into());
        } else if key.eq_ignore_ascii_case("transfer-encoding") {
            return Err("Unexpected announcement transfer encoding".into());
        }
    }
    let size = length
        .filter(|size| *size <= LIMIT - bytes.len())
        .ok_or("Announcement request exceeds1MiB or has no content length")?;
    let mut body = vec![0; size];
    let mut offset = 0;
    while offset < size {
        if Instant::now() >= deadline {
            return Err("Announcement body deadline exceeded".into());
        }
        stream
            .set_read_timeout(Some(deadline.saturating_duration_since(Instant::now())))
            .map_err(|e| e.to_string())?;
        let count = stream
            .read(&mut body[offset..])
            .map_err(|e| e.to_string())?;
        if count == 0 {
            return Err("Truncated announcement request".into());
        }
        offset += count;
    }
    Ok(Request {
        path,
        content_type,
        authorization,
        body,
    })
}

struct Server {
    base: String,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<Result<Vec<Request>, String>>>,
}
impl Server {
    fn new() -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let base = format!(
            "http://{}",
            listener.local_addr().map_err(|e| e.to_string())?
        );
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let (stop, mut receiver) = oneshot::channel();
        let thread = std::thread::Builder::new().name("announcement-fixture".into()).spawn(move || {
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).map_err(|e| e.to_string())?;
                let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
                let mut observed = Vec::new();
                loop {
                    if tokio::time::Instant::now() >= deadline {
                        return Err("Announcement fixture total deadline exceeded".into());
                    }
                    let (stream, _) = tokio::select! {
                        biased;
                        accepted = tokio::time::timeout_at(deadline, listener.accept()) => {
                            accepted.map_err(|_| "Announcement fixture total deadline exceeded")?
                                .map_err(|e| e.to_string())?
                        }
                        _ = &mut receiver => break,
                    };
                    let mut stream = stream.into_std().map_err(|e| e.to_string())?;
                    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
                    let request = read_request(&mut stream)?;
                    let worker = request.path == "/publish";
                    observed.push(request);
                    let number = observed.len();
                    if number > 64 { return Err("Announcement fixture exceeded64 requests".into()); }
                    // Stay alive after the expected ten: retries/fallbacks are evidence,
                    // not requests hidden by an already stopped fixture.
                    if number >= 10 {
                        stream.shutdown(Shutdown::Both).map_err(|e| e.to_string())?;
                        continue;
                    }
                    let status = if number == 9 {"503 Service Unavailable"} else {"200 OK"};
                    let body = if worker {r#"{"ok":true}"#} else {r#"{"ok":true,"result":{"message_id":1}}"#};
                    stream.set_write_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
                    write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
                        .map_err(|e| e.to_string())?;
                }
                Ok(observed)
            })
        }).map_err(|e| e.to_string())?;
        Ok(Self {
            base,
            stop: Some(stop),
            thread: Some(thread),
        })
    }
    fn finish(&mut self) -> Result<Vec<Request>, String> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        self.thread
            .take()
            .ok_or("Announcement fixture already joined")?
            .join()
            .map_err(|_| "Announcement fixture thread panicked")?
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if self.thread.is_some() {
            if let Err(error) = self.finish() {
                eprintln!("Announcement fixture cleanup: {error}");
            }
        }
    }
}

fn assert_requests(requests: &[Request]) {
    assert_eq!(
        requests.len(),
        10,
        "unexpected retry/fallback request(s): {requests:?}"
    );
    let list = "Note\n\nFixture 1. https://t.me/addemoji/fixture1_by_YourEmojiBot\nfixture2_by_YourEmojiBot. https://t.me/addemoji/fixture2_by_YourEmojiBot";
    let texts = [
        "Note",
        "✅ Fixture 1\nhttps://t.me/addemoji/fixture1_by_YourEmojiBot",
        "✅ fixture2_by_YourEmojiBot\nhttps://t.me/addemoji/fixture2_by_YourEmojiBot",
        list,
        list,
    ];
    for (index, request) in requests[..5].iter().enumerate() {
        assert_eq!(request.path, "/bottest-only-token/sendMessage");
        assert_eq!(request.content_type, "application/x-www-form-urlencoded");
        assert!(request.authorization.is_none());
        let mut url = reqwest::Url::parse("http://127.0.0.1/").unwrap();
        url.set_query(Some(std::str::from_utf8(&request.body).unwrap()));
        let mut form: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (key, value) in url.query_pairs() {
            form.entry(key.into_owned())
                .or_default()
                .push(value.into_owned());
        }
        assert_eq!(
            form,
            BTreeMap::from([
                (
                    "chat_id".into(),
                    vec![
                        match index {
                            3 => "@fixturechannel",
                            4 => "-1001111111111",
                            _ => "111111111",
                        }
                        .to_owned()
                    ]
                ),
                ("disable_web_page_preview".into(), vec!["true".into()]),
                ("text".into(), vec![texts[index].into()]),
            ])
        );
    }
    for (index, request) in requests[5..].iter().enumerate() {
        assert_eq!(request.path, "/publish");
        assert_eq!(request.content_type, "application/json");
        assert_eq!(
            request.authorization.as_deref(),
            Some("Bearer fixture-secret")
        );
        let expected = if index == 0 {
            shared_publish_body()
        } else if index == 1 {
            json!({"bot":"coin","packs":packs(),"style":"list","note":"Note"})
        } else {
            json!({"bot":"coin","packs":packs(),"style":"cards"})
        };
        assert_eq!(
            serde_json::from_slice::<Value>(&request.body).unwrap(),
            expected
        );
    }
}

#[test]
fn original_destination_resolves_owner_channel_username_and_decimal_grammar() {
    let root = PathBuf::from("not-read-announcement-configuration");
    let owner = 111111111;
    for (raw, expected) in [
        (None, "111111111"),
        (Some(""), "111111111"),
        (Some("   "), "111111111"),
        (Some("-1001111111111"), "-1001111111111"),
        (Some("@packlinks"), "@packlinks"),
        (Some("  -1001111111111  "), "-1001111111111"),
        (Some("\u{1c}-۰۱۰۰_۱۱۱_۱۱۱_۱۱۱۱\u{1f}"), "-1001111111111"),
        (Some("+۰۰۴۲"), "42"),
        (Some("-۰"), "0"),
        (Some("\u{1c}@packlinks\u{1f}"), "@packlinks"),
        (Some("9223372036854775808"), "9223372036854775808"),
        (Some("-9223372036854775809"), "-9223372036854775809"),
    ] {
        let mut environment =
            BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
        if let Some(raw) = raw {
            environment.insert("PACK_LINKS_CHAT_ID".into(), raw.into());
        }
        let before = environment.clone();
        let config =
            _native::config::Config::load_with_environment(root.clone(), &environment).unwrap();
        assert_eq!(
            _native::announce::links_chat_id(&config, owner).unwrap(),
            expected
        );
        assert_eq!(environment, before);
    }
    let limit = "0".repeat(4300);
    let over_limit = "0".repeat(4301);
    for (raw, expected) in [
        (limit.as_str(), Some("0")),
        (over_limit.as_str(), None),
        ("_1", None),
        ("1_", None),
        ("1__0", None),
        ("0x10", None),
        ("²", None),
        ("1\0", None),
        ("+", None),
        ("- 1", None),
    ] {
        let environment = BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("PACK_LINKS_CHAT_ID".into(), raw.into()),
        ]);
        let config =
            _native::config::Config::load_with_environment(root.clone(), &environment).unwrap();
        let actual = _native::announce::links_chat_id(&config, owner);
        if let Some(expected) = expected {
            assert_eq!(actual.unwrap(), expected);
        } else {
            assert_eq!(actual.unwrap_err(), "invalid PACK_LINKS_CHAT_ID");
        }
    }
}

#[test]
fn test_direct_cards_list_worker_refusal_and_nondefault_timeout() {
    if let Ok(role) = std::env::var(ROLE) {
        child(&role).unwrap();
        return;
    }
    let inventory: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    assert_eq!(
        inventory["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let source = inventory["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["source"] == "tests/test_native_announce_compat.py")
        .unwrap();
    assert_eq!(
        source["sha256"],
        "623cf48bd11476186c5386b829e7751f066516e8c69e7679d94dd75a4db17706"
    );
    let started = Instant::now();
    let scratch = Scratch::new().unwrap();
    let mut server = Server::new().unwrap();
    let outcome = children(&scratch.0, &server.base);
    let requests = server.finish();
    let requests = requests.unwrap();
    assert_requests(&requests);
    assert!(outcome.is_ok(), "{}", outcome.unwrap_err());
    // A 35s command budget plus the existing owner's bounded cleanup and
    // the independently bounded server join must never become an open-ended test.
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "announcement parent total bound exceeded"
    );
    scratch.clean().unwrap();
}
