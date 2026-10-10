//! Shared native announcement route; milestone state is committed only after a successful send.
use crate::{
    config::Config,
    telegram::{Result as ApiResult, Telegram},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

pub fn packs(
    config: &Config,
    tg: &Telegram,
    owner: u64,
    packs: &[Value],
    bot: &str,
    note: &str,
    style: &str,
) -> Result<String, String> {
    let url = config.value("WORKER_PUBLISH_URL");
    let secret = config.value("WORKER_PUBLISH_SECRET");
    if !url.is_empty() && !secret.is_empty() {
        let parsed = reqwest::Url::parse(url).map_err(|_| "invalid worker publish URL")?;
        if !["http", "https"].contains(&parsed.scheme())
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err("unsupported worker publish URL".into());
        }
        let hermetic = std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1");
        if hermetic && !["127.0.0.1", "::1", "[::1]"].contains(&parsed.host_str().unwrap_or("")) {
            return Err("native tests may connect only to numeric loopback hosts".into());
        }
        let mut builder = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never());
        if hermetic {
            builder = builder.no_proxy();
        }
        let client = builder
            .build()
            .map_err(|_| "cannot initialize worker announcement client")?;
        let mut body = json!({"bot":bot,"packs":packs,"style":style});
        if !note.is_empty() {
            body["note"] = json!(note);
        }
        let response = client
            .post(parsed)
            .bearer_auth(secret)
            .json(&body)
            .send()
            .map_err(|_| "worker announcement transport failed; outcome unknown")?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(format!(
                "worker announce failed (HTTP {})",
                response.status().as_u16()
            ));
        }
        return Ok("the worker".into());
    }
    let destination = if config.value("PACK_LINKS_CHAT_ID").is_empty() {
        owner.to_string()
    } else {
        let raw = config.value("PACK_LINKS_CHAT_ID");
        if !raw.starts_with('@') {
            raw.parse::<i64>()
                .map_err(|_| "invalid PACK_LINKS_CHAT_ID")?;
        }
        raw.to_owned()
    };
    let send = |text: String| -> Result<(), String> {
        let data = BTreeMap::from([
            ("chat_id".into(), destination.clone()),
            ("text".into(), text),
            ("disable_web_page_preview".into(), "true".into()),
        ]);
        tg.call(
            "sendMessage",
            &data,
            &[],
            2,
            None::<fn() -> ApiResult<Option<bool>>>,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    };
    if style == "list" {
        let mut lines = if note.is_empty() {
            vec![]
        } else {
            vec![note.to_owned(), String::new()]
        };
        for pack in packs {
            let name = pack["name"]
                .as_str()
                .ok_or("announcement pack has no name")?;
            let title = pack["title"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(name);
            lines.push(format!("{title}. https://t.me/addemoji/{name}"));
        }
        send(lines.join("\n"))?;
    } else {
        if !note.is_empty() {
            send(note.into())?;
        }
        for pack in packs {
            let name = pack["name"]
                .as_str()
                .ok_or("announcement pack has no name")?;
            let title = pack["title"]
                .as_str()
                .filter(|s| !s.is_empty())
                .unwrap_or(name);
            send(format!("✅ {title}\nhttps://t.me/addemoji/{name}"))?;
        }
    }
    Ok(destination)
}
