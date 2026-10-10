//! Separate initial/full milestones; failed sends never become recorded announcements.
use crate::{atomic, publication::Publisher};
use serde_json::{Value, json};
use std::path::Path;

pub fn notify(
    publisher: &Publisher<'_>,
    owner: u64,
    state: &mut Value,
    path: &Path,
    set: &Value,
    full: bool,
) -> Result<(), String> {
    let config = publisher
        .config
        .ok_or("publication announcement configuration is unavailable")?;
    let name = set["name"].as_str().ok_or("announcement set has no name")?;
    let title = set["title"]
        .as_str()
        .ok_or("announcement set has no recorded title")?;
    let field = if full { "sent_full" } else { "sent" };
    let sent = state[field]
        .as_array()
        .ok_or("invalid announcement state")?;
    if sent.iter().any(|v| v == name) {
        return Ok(());
    }
    match crate::announce::packs(
        config,
        publisher.tg,
        owner,
        &[json!({"name":name,"title":title})],
        "general",
        "",
        "cards",
    ) {
        Ok(_) => {
            state[field]
                .as_array_mut()
                .ok_or("invalid announcement state")?
                .push(json!(name));
            atomic::write_json(path, state).map_err(|e| e.to_string())?;
        }
        Err(error) => eprintln!(
            "WARNING: {}",
            crate::logging::redact(&format!("notify failed: {error}"), &config.secret_values())
        ),
    }
    Ok(())
}
