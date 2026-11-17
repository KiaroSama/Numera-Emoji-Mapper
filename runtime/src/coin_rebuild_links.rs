//! Final coin links use the existing shared announcer and persist only after an answered send.
use crate::{config::Config, telegram::Telegram};
use serde_json::{Value, json};
use std::path::Path;

pub fn notify(
    state: &mut Value,
    name: &str,
    announce: impl FnOnce() -> Result<String, String>,
    save: impl FnOnce(&Value) -> Result<(), String>,
    mut report: impl FnMut(&str),
) -> Result<(), String> {
    if state["sent"]
        .as_array()
        .ok_or("rebuild sent must be a list")?
        .iter()
        .any(|v| v == name)
    {
        return Ok(());
    }
    let result = (|| {
        let destination = announce()?;
        state["sent"]
            .as_array_mut()
            .ok_or("rebuild sent must be a list")?
            .push(json!(name));
        save(state)?;
        report(&format!("  sent link for {name} to {destination}"));
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        report(&format!("  notify failed {name}: {error}"));
    }
    Ok(())
}

pub fn final_links(
    state: &mut Value,
    title: &str,
    announce: impl FnOnce(&[Value], &str) -> Result<String, String>,
    save: impl FnOnce(&Value) -> Result<(), String>,
    mut report: impl FnMut(&str),
) -> Result<(), String> {
    let mut sets = state["sets"]
        .as_array()
        .ok_or("coin links state has no sets")?
        .clone();
    sets.sort_by_key(|set| set["index"].as_u64());
    if sets.is_empty() {
        report("no sets to send.");
        return Ok(());
    }
    let packs = sets
        .iter()
        .map(|set| json!({"name":set["name"],"title":set["index"].to_string()}))
        .collect::<Vec<_>>();
    let destination = announce(&packs, &format!("📦 {title} — all packs:"))?;
    state["final_sent"] = json!(true);
    save(state)?;
    report(&format!(
        "sent final links for {} packs to {destination} (preview off).",
        sets.len()
    ));
    Ok(())
}

pub fn send(
    config: &Config,
    directory: &Path,
    base: &str,
    title: &str,
    owner: u64,
    tg: &Telegram,
) -> Result<(), String> {
    if owner == 0 || title.is_empty() {
        return Err("coin links require configured owner and title".into());
    }
    crate::collection_names::valid_base(base)?;
    crate::coin_provider_stage::physical(directory)?;
    let path = directory.join("rebuild_dedup_state.json");
    crate::coin_manifest::output_path(&path)?;
    let family = crate::locks::pack_family_path(&config.root, base);
    crate::coin_manifest::output_path(&family)?;
    let _lease = crate::locks::Lease::acquire(&family, &crate::logging::utc())
        .map_err(|_| "coin links pack family is busy or unavailable")?;
    let mut state = match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            json!({"sets":[],"sent":[],"deleted_old":false,"final_sent":false,"order":[],"cursor":0,"in_flight":null})
        }
        Err(_) => return Err("cannot inspect coin link state".into()),
        Ok(_) => Value::Object(crate::coin_provider_recover::object(&path, false)?),
    };
    let problem = crate::coin_state::problem(&state, None);
    if !problem.is_empty() {
        return Err(format!(
            "coin links resume state is inconsistent: {problem}"
        ));
    }
    final_links(
        &mut state,
        title,
        |packs, note| {
            crate::announce::packs(config, tg, owner, packs, "coin", note, "list").map(
                |destination| {
                    if destination == "the worker" {
                        destination
                    } else {
                        "[set]".into()
                    }
                },
            )
        },
        |value| {
            crate::atomic::write_json(&path, value)
                .map_err(|_| "cannot save final coin link state".into())
        },
        |text| println!("{}", crate::logging::redact(text, &config.secret_values())),
    )
}
