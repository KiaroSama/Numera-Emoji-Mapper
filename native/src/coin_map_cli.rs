//! Additive coin map commands share canonical ownership, existing alias policies and atomic writes.
use crate::{config::Config, logging::RunLog};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn universal(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}
fn rows(path: &Path) -> Result<Vec<Value>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(path)
        .map_err(|_| "cannot read coin keyword CSV")?;
    let headers = reader
        .headers()
        .map_err(|_| "invalid coin keyword headers")?
        .clone();
    if !headers.iter().any(|h| h == "ticker") {
        return Err("coin keyword CSV has no ticker header".into());
    }
    reader
        .records()
        .map(|record| {
            let record = record.map_err(|_| "invalid coin keyword row")?;
            Ok(Value::Object(
                headers
                    .iter()
                    .enumerate()
                    .map(|(i, key)| {
                        (
                            key.into(),
                            record.get(i).map(|v| json!(v)).unwrap_or(Value::Null),
                        )
                    })
                    .collect(),
            ))
        })
        .collect()
}
fn tickers(text: &str) -> BTreeSet<String> {
    let lower = text.to_lowercase();
    let mut rest = lower.as_str();
    let mut out = BTreeSet::new();
    while let Some(index) = rest.find("ticker:") {
        rest = &rest[index + 7..];
        let value = rest
            .trim_start_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
        let end = value
            .find(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
            .unwrap_or(value.len());
        if end > 0 {
            out.insert(value[..end].into());
        }
        rest = &value[end..];
    }
    out
}

pub fn run(config: &Config, command: &str, args: &[String], log: &RunLog) -> Result<u8, String> {
    let mut directory = config.root.join("coins");
    let mut help = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => help = true,
            "--coin-dir" => {
                index += 1;
                directory = config.path(args.get(index).ok_or("--coin-dir requires a path")?);
            }
            value => return Err(format!("unknown {command} option: {value}")),
        }
        index += 1;
    }
    if help {
        println!("Usage: numera-emoji {command} [--coin-dir DIR]");
        return Ok(0);
    }
    crate::coin_provider_stage::physical(&directory)?;
    let inventory = universal(
        &std::fs::read_to_string(directory.join("currency-emoji-inventory.md"))
            .map_err(|_| "cannot read coin inventory")?,
    );
    let path = directory.join("ticker_to_id.json");
    crate::coin_provider_stage::physical(&path)?;
    let mapping = {
        let _lease = crate::locks::Lease::acquire(
            &config.root.join(".locks/canonical_map.lock"),
            &crate::logging::utc(),
        )
        .map_err(|_| "canonical coin map is busy or unavailable")?;
        let mut mapping = crate::coin_provider_recover::object(&path, false)?;
        if mapping.values().any(|value| !value.is_string()) {
            return Err("coin map IDs must be strings; primitive compatibility unverified".into());
        }
        let have = mapping.keys().cloned().collect::<BTreeSet<_>>();
        let added = if command == "coin-alias-map" {
            let missing = crate::coin_inventory_parse::missing(&inventory, &have);
            let result = crate::coin_aliases::apply_aliases(
                &mapping,
                &rows(&directory.join("keywords.csv"))?,
                &missing,
            )
            .map_err(|e| e.message)?;
            mapping = result["mapping"]
                .as_object()
                .ok_or("invalid native alias result")?
                .clone();
            for message in result["messages"]
                .as_array()
                .ok_or("invalid alias messages")?
            {
                println!("{}", message.as_str().ok_or("invalid alias message")?);
            }
            println!(
                "added {} name-alias mappings ({} left unmapped as ambiguous)",
                result["added"], result["ambiguous"]
            );
            result["added"].as_u64().unwrap_or(0)
        } else {
            let mut added = 0;
            for ticker in tickers(&inventory) {
                if have.contains(&ticker) {
                    continue;
                }
                let base = crate::coin_inventory::base_ticker(&ticker);
                if have.contains(base) {
                    mapping.insert(ticker.clone(), mapping[base].clone());
                    added += 1;
                }
            }
            println!("added {added} chain-variant mappings");
            added
        };
        crate::atomic::write_json(&path, &Value::Object(mapping.clone()))
            .map_err(|_| "cannot persist canonical coin map")?;
        log.event("INFO", "coins", &format!("mapped_added={added}"));
        mapping
    };
    let strings = mapping
        .iter()
        .map(|(key, value)| {
            Ok((
                key.clone(),
                value.as_str().ok_or("coin map ID must be text")?.into(),
            ))
        })
        .collect::<Result<BTreeMap<String, String>, String>>()?;
    let filled = crate::coin_inventory::refill(&strings, &inventory);
    let text = if cfg!(windows) {
        filled.text.replace('\n', "\r\n")
    } else {
        filled.text
    };
    crate::atomic::write_bytes(
        &directory.join("currency-emoji-inventory.filled.md"),
        text.as_bytes(),
    )
    .map_err(|_| "cannot persist filled coin inventory")?;
    println!("inventory filled: {}/{}", filled.filled, filled.total);
    Ok(0)
}
