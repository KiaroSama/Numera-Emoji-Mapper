//! Native long-polling bot; cursor advances only after handling or explicit dead-letter.
use crate::{
    atomic, bot_payloads,
    config::Config,
    logging::RunLog,
    telegram::{ApiError, Result as ApiResult, Telegram},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

const DENIED: &str = "This bot is private and your account is not on its access list.\nAsk the owner to add your numeric Telegram user ID if you need access.";
const START: &str = "<b>Numera Emoji Mapper</b> (premium custom-emoji ID extractor)\n\n• Send one or more <b>premium emoji</b> in a row (spaces and newlines do not matter), and I will reply with an expandable list of <i>emoji + ID</i> (tap an ID to copy it) plus a <b>Copy all</b> button.\n• Send or forward a <b>post with premium emoji</b> for the same reply.\n• Send <b>emoji IDs</b> (one per line, comma-separated, or a single ID), and I will show the matching emoji.\n• <b>Add me to a channel or group</b> (as admin), and I will DM you the premium emoji IDs from new posts there.\n\nNote: Telegram only delivers posts sent after a bot joins a channel.";
fn call(tg: &Telegram, method: &str, data: &Value, retries: u32) -> ApiResult<Value> {
    let data = data
        .as_object()
        .ok_or_else(|| ApiError::Invalid("bot payload must be object".into()))?
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    tg.call(
        method,
        &data,
        &[],
        retries,
        None::<fn() -> ApiResult<Option<bool>>>,
    )
}
fn labels(tg: &Telegram, ids: &[String]) -> ApiResult<Value> {
    let mut out = serde_json::Map::new();
    for batch in ids.chunks(200) {
        let response = tg.get_custom_emoji_stickers(batch)?;
        for (id, sticker) in crate::telegram::validated_custom_emoji_stickers(&response, batch)? {
            let emoji = match sticker.get("emoji") {
                None => "",
                Some(value) => value.as_str().ok_or_else(|| {
                    ApiError::Invalid("getCustomEmojiStickers returned a non-string emoji".into())
                })?,
            };
            out.insert(id.into(), json!(emoji));
        }
    }
    Ok(Value::Object(out))
}
fn reply(
    tg: &Telegram,
    chat: i64,
    ids: &[String],
    reply_to: Option<i64>,
    header: Option<String>,
    log: &RunLog,
) -> Result<(), String> {
    // Entity IDs are already established; glyph lookup is only display enrichment.
    let labels = labels(tg, ids).unwrap_or_else(|error| {
        log.event(
            "WARNING",
            "bot",
            &format!("optional emoji glyph lookup failed; using fallback glyphs: {error}"),
        );
        json!({})
    });
    let rich = bot_payloads::payloads(ids, &labels, true)?;
    let mut plain = None;
    for (i, row) in rich
        .as_array()
        .ok_or("invalid bot payloads")?
        .iter()
        .enumerate()
    {
        let text = row[0].as_str().ok_or("invalid message body")?;
        let data = |text: &str| {
            let body = if i == 0 {
                header
                    .as_ref()
                    .map(|h| format!("{h}\n\n{text}"))
                    .unwrap_or_else(|| text.into())
            } else {
                text.into()
            };
            let mut out = json!({"chat_id":chat,"text":body,"parse_mode":"HTML","disable_web_page_preview":true,"reply_markup":row[1].to_string()});
            if i == 0
                && let Some(id) = reply_to.filter(|n| *n != 0)
            {
                out["reply_to_message_id"] = json!(id);
            }
            out
        };
        match call(tg, "sendMessage", &data(text), 1) {
            Ok(_) => {}
            Err(ApiError::Rejected(error)) => {
                log.event("WARNING", "bot", &format!("rich reply rejected: {error}"));
                let plain = plain.get_or_insert(bot_payloads::payloads(ids, &labels, false)?);
                call(
                    tg,
                    "sendMessage",
                    &data(plain[i][0].as_str().ok_or("invalid fallback body")?),
                    1,
                )
                .map_err(|e| e.to_string())?;
            }
            Err(error) => {
                log.event(
                    "ERROR",
                    "bot",
                    &format!("reply outcome unresolved; no fallback: {error}"),
                );
                return Ok(());
            }
        }
    }
    Ok(())
}
fn typed_reply(
    tg: &Telegram,
    chat: i64,
    ids: &[String],
    reply_to: Option<i64>,
    log: &RunLog,
) -> Result<(), String> {
    // Absence claims require a successful, well-formed response for every batch.
    let known =
        labels(tg, ids).map_err(|error| format!("typed emoji ID lookup failed: {error}"))?;
    let found = ids
        .iter()
        .filter(|id| known.get(*id).is_some())
        .cloned()
        .collect::<Vec<_>>();
    let missing = ids
        .iter()
        .filter(|id| known.get(*id).is_none())
        .collect::<Vec<_>>();
    let header = if missing.is_empty() {
        None
    } else {
        let shown = missing
            .iter()
            .take(10)
            .map(|id| format!("<code>{}</code>", bot_payloads::escape(id)))
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "⚠️ Telegram does not know {} of these: {shown}{}",
            missing.len(),
            if missing.len() > 10 {
                format!(" (+{} more)", missing.len() - 10)
            } else {
                String::new()
            }
        ))
    };
    if !found.is_empty() {
        return reply(tg, chat, &found, reply_to, header, log);
    }
    let mut data = json!({"chat_id":chat,"text":header.unwrap_or("No usable ids in that message.".into()),"parse_mode":"HTML","disable_web_page_preview":true});
    if let Some(id) = reply_to.filter(|n| *n != 0) {
        data["reply_to_message_id"] = json!(id);
    }
    call(tg, "sendMessage", &data, 1)
        .map(|_| ())
        .map_err(|e| e.to_string())
}
pub fn handle(
    tg: &Telegram,
    owner: i64,
    update: &Value,
    allowed: &BTreeSet<i64>,
    channels: &BTreeSet<i64>,
    log: &RunLog,
) -> Result<(), String> {
    if let Some(message) = update.get("message").filter(|v| !v.is_null()) {
        let chat = message["chat"]["id"]
            .as_i64()
            .ok_or("message has no chat ID")?;
        let private = message["chat"]["type"] == "private";
        if !message["from"]["id"]
            .as_i64()
            .is_some_and(|id| allowed.contains(&id))
        {
            if private {
                let _ = call(tg, "sendMessage", &json!({"chat_id":chat,"text":DENIED}), 5);
            }
            return Ok(());
        }
        let text = message["text"].as_str().unwrap_or("");
        if ["/start", "/help", "/menu"]
            .iter()
            .any(|p| text.starts_with(p))
        {
            return call(
                tg,
                "sendMessage",
                &json!({"chat_id":chat,"text":START,"parse_mode":"HTML"}),
                5,
            )
            .map(|_| ())
            .map_err(|e| e.to_string());
        }
        let typed = bot_payloads::typed(text);
        if !typed.is_empty() {
            return typed_reply(tg, chat, &typed, message["message_id"].as_i64(), log);
        }
        let ids = bot_payloads::extract(message);
        if private {
            return reply(tg, chat, &ids, message["message_id"].as_i64(), None, log);
        }
        if !ids.is_empty() {
            return reply(
                tg,
                owner,
                &ids,
                None,
                Some(format!(
                    "From group <b>{}</b>:",
                    bot_payloads::escape(message["chat"]["title"].as_str().unwrap_or(""))
                )),
                log,
            );
        }
    } else if let Some(post) = update.get("channel_post").filter(|v| !v.is_null()) {
        if !post["chat"]["id"]
            .as_i64()
            .is_some_and(|id| channels.contains(&id))
        {
            return Ok(());
        }
        let ids = bot_payloads::extract(post);
        if !ids.is_empty() {
            reply(
                tg,
                owner,
                &ids,
                None,
                Some(format!(
                    "From channel <b>{}</b>:",
                    bot_payloads::escape(post["chat"]["title"].as_str().unwrap_or(""))
                )),
                log,
            )?;
        }
    }
    Ok(())
}
fn allowlist(raw: &str, channels: bool, log: &RunLog) -> BTreeSet<i64> {
    raw.split([',', ';'])
        .filter_map(|part| {
            let part = part.trim_matches(crate::emoji_ids::whitespace);
            if part.is_empty() {
                return None;
            }
            // Channel syntax is -?\d+, unlike the user's signed/underscored int grammar.
            let channel_digits = part.strip_prefix('-').unwrap_or(part);
            let valid = !channels
                || (!channel_digits.is_empty()
                    && channel_digits.chars().all(crate::emoji_ids::decimal));
            let value = valid
                .then(|| crate::config_integer::decimal(part))
                .flatten()
                .and_then(|value| value.parse().ok());
            if value.is_none() {
                log.event(
                    "WARNING",
                    "bot",
                    if channels {
                        "ignoring malformed or out-of-range BOT_ALLOWED_CHANNEL_IDS entry"
                    } else {
                        "ignoring malformed or out-of-range BOT_ALLOWED_USER_IDS entry"
                    },
                );
            }
            value
        })
        .collect()
}
fn allowed_users(config: &Config, log: &RunLog) -> Result<BTreeSet<i64>, String> {
    let owner = config.integer("PACK_OWNER_USER_ID", 0, 0, i64::MAX);
    if owner <= 0 {
        return Err("PACK_OWNER_USER_ID must be a valid positive numeric ID".into());
    }
    let users = config
        .value("BOT_ALLOWED_USER_IDS")
        .trim_matches(crate::emoji_ids::whitespace);
    let allowed = if users.is_empty() {
        BTreeSet::from([owner])
    } else {
        allowlist(users, false, log)
    };
    if allowed.is_empty() {
        return Err("no authorized users; refusing an open bot".into());
    }
    Ok(allowed)
}
fn load_offset(path: &std::path::Path) -> i64 {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|v| {
            v["offset"]
                .as_i64()
                .or_else(|| v["offset"].as_str()?.parse().ok())
        })
        .unwrap_or(0)
}
fn save_offset(path: &std::path::Path, offset: i64) -> std::io::Result<()> {
    atomic::write_json(path, &json!({"offset":offset}))
}
#[cfg(test)]
#[path = "../tests/baseline_telegram_support/bot_units.rs"]
mod baseline_bot_tests;

pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if !args.is_empty() {
        return Err("emoji-bot accepts no command options".into());
    }
    let token = config.require("GENERAL_BOT_TOKEN")?;
    let owner = config.integer("PACK_OWNER_USER_ID", 0, 0, i64::MAX);
    if owner <= 0 {
        return Err("PACK_OWNER_USER_ID must be a valid positive numeric ID".into());
    }
    let allowed = allowed_users(config, log)?;
    let channels = allowlist(config.value("BOT_ALLOWED_CHANNEL_IDS"), true, log);
    let base = if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
        std::env::var("NUMERA_TEST_API_BASE").unwrap_or_else(|_| "https://api.telegram.org".into())
    } else {
        "https://api.telegram.org".into()
    };
    let tg = Telegram::new(token.into(), base)
        .map_err(|e| e.to_string())?
        .with_log(log);
    tg.read_call("getMe", &BTreeMap::new())
        .map_err(|e| e.to_string())?;
    let _ = call(
        &tg,
        "setMyCommands",
        &json!({"commands":json!([{"command":"start","description":"How to use the bot"},{"command":"help","description":"Show help / menu"}]).to_string()}),
        5,
    );
    let path = config.root.join("state_emoji_bot.json");
    let mut offset = load_offset(&path);
    loop {
        let updates = match call(
            &tg,
            "getUpdates",
            &json!({"offset":offset,"timeout":50,"allowed_updates":json!(["message","channel_post","my_chat_member"]).to_string()}),
            5,
        ) {
            Ok(updates) => updates,
            Err(error) => {
                if error.to_string().to_lowercase().contains("conflict") {
                    log.event(
                        "ERROR",
                        "bot",
                        "getUpdates conflict: another poller or webhook owns this bot",
                    );
                    return Ok(4);
                }
                log.event("WARNING", "bot", &format!("getUpdates failed: {error}"));
                std::thread::sleep(Duration::from_secs(3));
                continue;
            }
        };
        for update in updates
            .as_array()
            .ok_or("getUpdates did not return an update list")?
        {
            if let Err(error) = handle(&tg, owner, update, &allowed, &channels, log) {
                log.event(
                    "ERROR",
                    "bot",
                    &format!("dead-lettering update after handler error: {error}"),
                );
            }
            offset = update["update_id"]
                .as_i64()
                .and_then(|n| n.checked_add(1))
                .ok_or("invalid update ID")?;
            if save_offset(&path, offset).is_err() {
                log.event("WARNING", "bot", "could not persist update offset");
            }
        }
    }
}
