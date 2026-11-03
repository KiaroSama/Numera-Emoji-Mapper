//! Existing panel discovery compares only a canonical catalog digest, not private paths.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 9450;

pub fn identity(db: &Path, show_all: bool, packs: &[u64]) -> Result<Value, String> {
    let canonical = db.canonicalize().map_err(|e| e.to_string())?;
    let raw = canonical.to_str().ok_or("catalog path is not UTF-8")?;
    #[cfg(windows)]
    let raw = raw
        .strip_prefix("\\\\?\\")
        .unwrap_or(raw)
        .to_lowercase()
        .replace('/', "\\");
    #[cfg(not(windows))]
    let raw = raw.to_owned();
    let digest = Sha256::digest(raw.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let mut packs = packs.to_vec();
    packs.sort_unstable();
    packs.dedup();
    Ok(
        json!({"application":"numera-emoji-mapper-panel","catalog":digest,"all":show_all,"packs":packs}),
    )
}
pub async fn detect_bot(token: &str, base: &str) -> Option<String> {
    let url = reqwest::Url::parse(base).ok()?;
    if !["http", "https"].contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let hermetic = std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1");
    if hermetic && !["127.0.0.1", "::1", "[::1]"].contains(&url.host_str().unwrap_or("")) {
        return None;
    }
    let mut client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never());
    if hermetic {
        client = client.no_proxy();
    }
    let mut response = client
        .build()
        .ok()?
        .post(format!("{}/bot{token}/getMe", base.trim_end_matches('/')))
        .send()
        .await
        .ok()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len().checked_add(chunk.len())? > 8 * 1024 * 1024 {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    let payload: Value = serde_json::from_slice(&bytes).ok()?;
    if payload["ok"] != true {
        return None;
    }
    payload["result"]["username"].as_str().map(str::to_owned)
}

pub async fn sandbox_parent_ended() {
    #[cfg(target_os = "macos")]
    if let (Ok(parent), Ok(birth)) = (
        std::env::var("NUMERA_SANDBOX_PARENT_PID"),
        std::env::var("NUMERA_SANDBOX_PARENT_START"),
    ) {
        if let (Ok(parent), Ok(birth)) = (parent.parse::<u32>(), birth.parse::<u64>()) {
            loop {
                if !matches!(crate::process_owner::process_birth(parent), Ok(Some(current)) if current == birth)
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let parent = std::env::var("NUMERA_SANDBOX_PARENT_PID")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|n| *n > 0);
        let birth = std::env::var("NUMERA_SANDBOX_PARENT_START")
            .ok()
            .and_then(|v| v.parse::<u64>().ok());
        if let (Some(parent), Some(birth)) = (parent, birth) {
            loop {
                let state = std::fs::read_to_string(format!("/proc/{parent}/stat"))
                    .ok()
                    .and_then(|raw| {
                        let fields = raw
                            .rsplit_once(')')?
                            .1
                            .split_whitespace()
                            .collect::<Vec<_>>();
                        Some((
                            fields.first()?.to_string(),
                            fields.get(19)?.parse::<u64>().ok()?,
                        ))
                    });
                if !state.is_some_and(|(state, current)| {
                    current == birth && !["Z", "X", "x"].contains(&state.as_str())
                }) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
    std::future::pending::<()>().await;
}

pub fn listener_pid(output: &str, port: u16) -> Option<String> {
    let suffix = format!(":{port}");
    for line in output.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() >= 5
            && fields[3].eq_ignore_ascii_case("LISTENING")
            && fields[1].ends_with(&suffix)
        {
            return crate::config_integer::decimal(fields[4]);
        }
    }
    None
}

pub async fn port_holder(root: &Path, port: u16) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        let Ok(tool) = crate::codec_process::executable("netstat") else {
            return Ok(None);
        };
        let environment = ["PATH", "SystemRoot", "SYSTEMROOT", "WINDIR", "TEMP", "TMP"]
            .into_iter()
            .filter_map(|key| std::env::var_os(key).map(|value| (key.into(), value)))
            .collect();
        let outcome = crate::codec_process::capture_settled(
            root,
            &tool,
            &["-ano".into(), "-p".into(), "TCP".into()],
            1024 * 1024,
            None,
            &environment,
            crate::codec_process::Budget {
                wall: Duration::from_secs(10),
                idle: Duration::from_secs(3),
                warning: None,
            },
        )
        .await?;
        Ok(outcome.ok().and_then(|capture| {
            // Localized headings are irrelevant; only the numeric listener row is used.
            listener_pid(&String::from_utf8_lossy(&capture.stdout), port)
        }))
    }
    #[cfg(not(windows))]
    {
        let _ = (root, port);
        Ok(None)
    }
}

pub fn probe(port: u16) -> Option<Value> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(1))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .ok()?;
    let get = |path: &str| -> Option<(u16, Vec<u8>)> {
        use std::io::Read;
        let response = client
            .get(format!("http://127.0.0.1:{port}{path}"))
            .send()
            .ok()?;
        let status = response.status().as_u16();
        let mut bytes = Vec::new();
        response
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        (bytes.len() <= 4 * 1024 * 1024).then_some((status, bytes))
    };
    let (status, bytes) = get("/api/session")?;
    if status == 200 {
        let info: Value = serde_json::from_slice(&bytes).ok()?;
        let obj = info.as_object()?;
        return (obj.len() == 4
            && ["application", "catalog", "all", "packs"]
                .iter()
                .all(|k| obj.contains_key(*k))
            && info["application"] == "numera-emoji-mapper-panel")
            .then_some(info);
    }
    // A malformed current service must never be reclassified as a legacy panel.
    if status != 404 {
        return None;
    }
    let (status, bytes) = get("/api/ping")?;
    if status != 200 || bytes.trim_ascii() != br#"{"ok":true}"# {
        return None;
    }
    let (status, bytes) = get("/")?;
    (status == 200
        && [
            "<title>Numera Emoji Mapper — Curate</title>",
            "id=\"items-data\"",
            "/static/panel-grid.js",
            "const TOKEN = ",
        ]
        .iter()
        .all(|marker| {
            bytes
                .windows(marker.len())
                .any(|part| part == marker.as_bytes())
        }))
    .then(|| json!({"application":"numera-emoji-mapper-panel","legacy":true}))
}
