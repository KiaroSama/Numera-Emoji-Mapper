//! Bot API transport with explicit retries. Unknown mutation outcomes never become absence.
use reqwest::blocking::{Client, multipart};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Read;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum ApiError {
    Rejected(String),
    Transport(String),
    Ambiguous(String),
    FloodWait { method: String, seconds: u64 },
    Invalid(String),
}
impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(s) | Self::Transport(s) | Self::Ambiguous(s) | Self::Invalid(s) => {
                write!(f, "{s}")
            }
            Self::FloodWait { method, seconds } => {
                write!(f, "Telegram asks to wait {seconds}s before {method}")
            }
        }
    }
}
impl std::error::Error for ApiError {}
impl ApiError {
    pub fn command_error(self) -> String {
        match self {
            Self::FloodWait { method, seconds } => format!("FLOOD_WAIT {method} {seconds}"),
            error => error.to_string(),
        }
    }
}
pub type Result<T> = std::result::Result<T, ApiError>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetState {
    Exists,
    Missing,
    Unknown,
}
#[derive(Clone)]
pub struct Upload {
    pub field: String,
    pub filename: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

pub struct Telegram {
    token: String,
    base: String,
    client: Client,
    pub max_flood_wait: Option<u64>,
}
impl Telegram {
    pub fn new(token: String, base: String) -> Result<Self> {
        if token.is_empty() {
            return Err(ApiError::Invalid("bot token is unset".into()));
        }
        let url = reqwest::Url::parse(&base)
            .map_err(|_| ApiError::Invalid("invalid Telegram API base".into()))?;
        if !["http", "https"].contains(&url.scheme())
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(ApiError::Invalid("unsupported Telegram API base".into()));
        }
        if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
            let host = url.host_str().unwrap_or("");
            if !["127.0.0.1", "::1", "[::1]"].contains(&host) {
                return Err(ApiError::Invalid(
                    "native tests may connect only to numeric loopback hosts".into(),
                ));
            }
        }
        let mut builder = Client::builder();
        if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
            builder = builder.no_proxy();
        }
        let client = builder
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .map_err(|_| ApiError::Transport("cannot initialize Telegram HTTP client".into()))?;
        Ok(Self {
            token,
            base: base.trim_end_matches('/').into(),
            client,
            max_flood_wait: None,
        })
    }
    pub fn safe(&self, text: &str) -> String {
        crate::logging::redact(text, std::slice::from_ref(&self.token))
    }
    fn post(
        &self,
        method: &str,
        data: &BTreeMap<String, String>,
        uploads: &[Upload],
        timeout: Duration,
    ) -> Result<Value> {
        if method.is_empty() || !method.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(ApiError::Invalid("invalid Bot API method".into()));
        }
        let url = format!("{}/bot{}/{}", self.base, self.token, method);
        let mut request = self.client.post(url).timeout(timeout);
        if uploads.is_empty() {
            request = request.form(data)
        } else {
            let mut form = multipart::Form::new();
            for (k, v) in data {
                form = form.text(k.clone(), v.clone());
            }
            for u in uploads {
                let part = multipart::Part::bytes(u.bytes.clone())
                    .file_name(u.filename.clone())
                    .mime_str(&u.mime)
                    .map_err(|_| ApiError::Invalid("invalid upload MIME".into()))?;
                form = form.part(u.field.clone(), part);
            }
            request = request.multipart(form);
        }
        let response = request
            .send()
            .map_err(|e| ApiError::Transport(self.safe(&e.to_string())))?;
        let status = response.status();
        let mut bytes = Vec::new();
        response
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| ApiError::Transport(self.safe(&e.to_string())))?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(ApiError::Transport("Bot API reply exceeds8MiB".into()));
        }
        let payload: Value = serde_json::from_slice(&bytes)
            .map_err(|_| ApiError::Transport(format!("non-JSON response (HTTP {status})")))?;
        if !payload.is_object() {
            return Err(ApiError::Transport(format!(
                "non-object JSON response (HTTP {status})"
            )));
        }
        Ok(payload)
    }
    pub fn call<F>(
        &self,
        method: &str,
        data: &BTreeMap<String, String>,
        uploads: &[Upload],
        retries: u32,
        mut applied: Option<F>,
    ) -> Result<Value>
    where
        F: FnMut() -> Result<Option<bool>>,
    {
        if retries == 0 || retries > 20 {
            return Err(ApiError::Invalid("retries must be1..20".into()));
        }
        let deadline = Instant::now() + Duration::from_secs(300);
        for attempt in 1..=retries {
            match self.post(method, data, uploads, Duration::from_secs(60)) {
                Ok(payload) => {
                    if payload.get("ok") == Some(&Value::Bool(true)) {
                        return payload.get("result").cloned().ok_or_else(|| {
                            ApiError::Transport("Bot API success has no result".into())
                        });
                    }
                    let desc = self.safe(
                        payload
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                    );
                    let lower = desc.to_lowercase();
                    if lower.contains("retry after") {
                        let wait = payload
                            .pointer("/parameters/retry_after")
                            .and_then(Value::as_u64)
                            .unwrap_or(5);
                        if self.max_flood_wait.is_some_and(|limit| wait > limit) {
                            return Err(ApiError::FloodWait {
                                method: method.into(),
                                seconds: wait,
                            });
                        }
                        let delay = wait
                            .checked_add(1)
                            .ok_or_else(|| ApiError::Invalid("invalid flood wait".into()))?;
                        std::thread::sleep(Duration::from_secs(delay));
                        continue;
                    }
                    if lower.contains("stickerset_invalid")
                        && method == "createNewStickerSet"
                        && attempt < retries
                        && Instant::now() < deadline
                    {
                        std::thread::sleep(
                            Duration::from_secs(u64::from(30 * attempt).min(90))
                                .min(deadline.saturating_duration_since(Instant::now())),
                        );
                        continue;
                    }
                    return Err(ApiError::Rejected(format!("{method} failed: {desc}")));
                }
                Err(ApiError::Transport(reason)) => {
                    if let Some(check) = applied.as_mut() {
                        std::thread::sleep(Duration::from_secs(2));
                        match check()? {
                            Some(true) => return Ok(json!({"verified_applied":true})),
                            None => {
                                return Err(ApiError::Ambiguous(format!(
                                    "{method}: network failure and live state unknown ({reason})"
                                )));
                            }
                            Some(false) => {}
                        }
                    } else if ["addStickerToSet", "createNewStickerSet"].contains(&method) {
                        return Err(ApiError::Ambiguous(format!(
                            "{method}: cannot retry without verified live evidence ({reason})"
                        )));
                    }
                    if attempt < retries {
                        std::thread::sleep(Duration::from_secs(u64::from(3 * attempt).min(20)));
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Err(ApiError::Transport(format!(
            "{method} failed after {retries} attempts"
        )))
    }
    pub fn read_call(&self, method: &str, data: &BTreeMap<String, String>) -> Result<Value> {
        self.call(method, data, &[], 5, None::<fn() -> Result<Option<bool>>>)
    }
    pub fn probe(&self, name: &str) -> (SetState, Option<Value>) {
        let data = BTreeMap::from([("name".into(), name.into())]);
        match self.post("getStickerSet", &data, &[], Duration::from_secs(30)) {
            Ok(p) if p.get("ok") == Some(&Value::Bool(true)) => {
                match p.get("result").filter(|v| v.is_object()) {
                    Some(v) => (SetState::Exists, Some(v.clone())),
                    None => (SetState::Unknown, None),
                }
            }
            Ok(p)
                if p.get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_lowercase()
                    .contains("stickerset_invalid") =>
            {
                (SetState::Missing, None)
            }
            _ => (SetState::Unknown, None),
        }
    }
    pub fn get_custom_emoji_stickers(&self, ids: &[String]) -> Result<Value> {
        if ids.is_empty() {
            return Ok(json!([]));
        }
        if ids.len() > 200 {
            return Err(ApiError::Invalid(
                "getCustomEmojiStickers accepts at most200 IDs per call".into(),
            ));
        }
        self.read_call(
            "getCustomEmojiStickers",
            &BTreeMap::from([("custom_emoji_ids".into(), json!(ids).to_string())]),
        )
    }
    pub fn download_bytes(&self, file_id: &str, retries: u32) -> Result<Vec<u8>> {
        if retries == 0 || retries > 20 {
            return Err(ApiError::Invalid("retries must be 1..20".into()));
        }
        let info = self.read_call(
            "getFile",
            &BTreeMap::from([("file_id".into(), file_id.into())]),
        )?;
        let path = info
            .get("file_path")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::Transport("getFile has no file_path".into()))?;
        if path.starts_with('/') || path.split('/').any(|s| s == "..") {
            return Err(ApiError::Invalid("unsafe Telegram file path".into()));
        }
        let url = format!("{}/file/bot{}/{}", self.base, self.token, path);
        for attempt in 1..=retries {
            let result = (|| {
                let response = self
                    .client
                    .get(&url)
                    .send()
                    .and_then(reqwest::blocking::Response::error_for_status)
                    .map_err(|e| ApiError::Transport(self.safe(&e.to_string())))?;
                let mut data = Vec::new();
                response
                    .take(21 * 1024 * 1024 + 1)
                    .read_to_end(&mut data)
                    .map_err(|e| ApiError::Transport(self.safe(&e.to_string())))?;
                if data.len() > 21 * 1024 * 1024 {
                    return Err(ApiError::Invalid("Telegram download exceeds21MiB".into()));
                }
                Ok(data)
            })();
            match result {
                Ok(v) => return Ok(v),
                Err(ApiError::Invalid(e)) => return Err(ApiError::Invalid(e)),
                Err(_) if attempt < retries => {
                    std::thread::sleep(Duration::from_secs(u64::from(3 * attempt).min(15)))
                }
                Err(e) => return Err(e),
            }
        }
        Err(ApiError::Transport("download exhausted retries".into()))
    }
}

pub fn usable_fuids(stickers: &[Value]) -> Option<BTreeSet<String>> {
    let mut set = BTreeSet::new();
    for sticker in stickers {
        let id = sticker.get("file_unique_id")?.as_str()?.to_owned();
        if id.is_empty() || !set.insert(id) {
            return None;
        }
    }
    Some(set)
}
pub fn trim_keywords(keywords: &[String]) -> Vec<String> {
    let (mut out, mut total) = (Vec::new(), 0);
    for raw in keywords {
        let key = raw.trim().chars().take(48).collect::<String>();
        let len = key.chars().count();
        if len == 0 {
            continue;
        }
        if !out.is_empty() && total + len + 1 > 60 {
            break;
        }
        out.push(key);
        total += len + 1;
        if out.len() >= 20 {
            break;
        }
    }
    out
}
pub fn input_sticker(fmt: &str, emojis: &[String], keywords: &[String]) -> Result<Value> {
    if !["static", "video", "animated"].contains(&fmt) {
        return Err(ApiError::Invalid("invalid sticker format".into()));
    }
    let mut emojis = emojis
        .iter()
        .filter(|s| !s.is_empty())
        .take(20)
        .cloned()
        .collect::<Vec<_>>();
    if emojis.is_empty() {
        emojis.push("🪙".into());
    }
    Ok(
        json!({"sticker":"attach://file0","format":fmt,"emoji_list":emojis,"keywords":trim_keywords(keywords)}),
    )
}
pub fn upload(path: &Path, field: &str) -> Result<Upload> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "tgs" => "application/gzip",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    };
    let metadata = std::fs::metadata(path).map_err(|e| ApiError::Invalid(e.to_string()))?;
    if !metadata.is_file() || metadata.len() > 21 * 1024 * 1024 {
        return Err(ApiError::Invalid("unsupported upload file".into()));
    }
    Ok(Upload {
        field: field.into(),
        filename: path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ApiError::Invalid("non-UTF8 filename".into()))?
            .into(),
        mime: mime.into(),
        bytes: std::fs::read(path).map_err(|e| ApiError::Invalid(e.to_string()))?,
    })
}
