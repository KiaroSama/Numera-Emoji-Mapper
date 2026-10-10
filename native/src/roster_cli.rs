//! Live roster refresh joins provenance/history and preserves unrequested families.
use crate::{atomic, config::Config, logging::RunLog, telegram::Telegram};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
fn read(path: &Path, default: Value) -> Result<Value, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(default),
        Err(e) => Err(e.to_string()),
    }
}
fn signature(path: &Path) -> Value {
    match path.metadata() {
        Ok(meta) => json!([
            meta.len(),
            meta.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
        ]),
        Err(_) => Value::Null,
    }
}
pub fn fingerprint(config: &Config) -> Result<Value, String> {
    let data = config.root.join("collection");
    let base = config.value("COLLECTION_PACK_BASE");
    let paths = [
        data.join("catalog.db"),
        data.join(format!("publish_{base}.json")),
        config.root.join("coins/rebuild_dedup_state.json"),
        config.root.join("coins/ticker_to_id.json"),
    ];
    let mut out = serde_json::Map::new();
    for path in paths {
        out.insert(
            path.file_name()
                .and_then(|s| s.to_str())
                .ok_or("input filename not UTF-8")?
                .into(),
            signature(&path),
        );
    }
    Ok(json!(out))
}
pub fn stale(config: &Config) -> Result<(bool, String), String> {
    let path = config.root.join("packs/index.json");
    if !path.is_file() {
        return Ok((true, "packs/index.json does not exist yet".into()));
    }
    let index = match read(&path, json!({})) {
        Ok(value) => value,
        Err(error) => return Ok((true, format!("packs/index.json is unreadable ({error})"))),
    };
    let now = fingerprint(config)?;
    let changed = now
        .as_object()
        .ok_or("invalid input fingerprint")?
        .iter()
        .filter(|(key, value)| index["inputs"].get(*key) != Some(*value))
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    Ok((
        index.get("inputs") != Some(&now),
        if changed.is_empty() {
            "roster matches its inputs".into()
        } else {
            format!(
                "changed since the roster was written: {}",
                changed.join(", ")
            )
        },
    ))
}
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!(
            "Usage: numera-emoji pack-manifest --refresh|--check [--family all|general|coins]"
        );
        return Ok(0);
    }
    let (mut refresh, mut check, mut family) = (false, false, "all".to_owned());
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--refresh" => refresh = true,
            "--check" => check = true,
            "--family" => {
                i += 1;
                family = args.get(i).cloned().ok_or("--family requires value")?;
            }
            value => return Err(format!("unknown pack-manifest option: {value}")),
        }
        i += 1;
    }
    if refresh == check
        || !["all", "general", "coins"].contains(&family.as_str())
        || check && family != "all"
    {
        return Err("choose --refresh or --check; --check covers all families".into());
    }
    if check {
        let (stale, why) = stale(config)?;
        println!("{}: {why}", if stale { "STALE" } else { "fresh" });
        return Ok(if stale { 3 } else { 0 });
    }
    if family != "coins" {
        config.require("COLLECTION_PACK_BASE")?;
        config.require("GENERAL_BOT_TOKEN")?;
    }
    if family != "general" {
        config.require("TELEGRAM_BOT_TOKEN")?;
    }
    let data = config.root.join("collection");
    let out = config.root.join("packs");
    let old = match read(&out.join("index.json"), json!({})) {
        Ok(value) => value,
        Err(error) => {
            log.event(
                "WARNING",
                "roster",
                &format!("cannot read previous derived index: {error}"),
            );
            json!({})
        }
    };
    let mut previous = Vec::new();
    if out.is_dir() {
        for entry in std::fs::read_dir(&out).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json")
                && path.file_name().and_then(|s| s.to_str()) != Some("index.json")
            {
                match read(&path, json!({})) {
                    Ok(doc) => previous.push(doc),
                    Err(error) => log.event(
                        "WARNING",
                        "roster",
                        &format!("cannot read prior roster: {error}"),
                    ),
                }
            }
        }
    }
    let history = crate::roster_provenance::history(&previous);
    let coin_map = match read(&config.root.join("coins/ticker_to_id.json"), json!({})) {
        Ok(value) => crate::roster_provenance::coins(&value),
        Err(error) => {
            log.event(
                "WARNING",
                "roster",
                &format!("cannot read coin provenance: {error}"),
            );
            BTreeMap::new()
        }
    };
    let api = if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
        std::env::var("NUMERA_TEST_API_BASE").unwrap_or_else(|_| "https://api.telegram.org".into())
    } else {
        "https://api.telegram.org".into()
    };
    let mut jobs = Vec::new();
    if family != "coins" {
        let base = config.require("COLLECTION_PACK_BASE")?;
        let state = read(&data.join(format!("publish_{base}.json")), json!({}))?;
        for set in state["sets"].as_array().into_iter().flatten() {
            jobs.push(("general", set.clone(), "GENERAL_BOT_TOKEN"));
        }
    }
    if family != "general" {
        let state = read(
            &config.root.join("coins/rebuild_dedup_state.json"),
            json!({}),
        )?;
        for set in state["sets"].as_array().into_iter().flatten() {
            jobs.push(("coins", set.clone(), "TELEGRAM_BOT_TOKEN"));
        }
    }
    jobs.sort_by_key(|(family, set, _)| (*family, set["index"].as_u64()));
    if jobs.is_empty() {
        return Err(format!("no published sets found for family {family}"));
    }
    let mut fetched = Vec::new();
    let mut live_ids = BTreeSet::new();
    for (family, set, key) in jobs {
        let tg =
            Telegram::new(config.require(key)?.into(), api.clone()).map_err(|e| e.to_string())?;
        let name = set["name"].as_str().ok_or("recorded pack has no name")?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err("unsafe recorded pack name".into());
        }
        // Nothing is written until every requested set is known. A partial roster
        // would falsely imply the failed pack had disappeared.
        let pack = tg
            .read_call(
                "getStickerSet",
                &BTreeMap::from([("name".into(), name.into())]),
            )
            .map_err(|e| format!("{name}: cannot read the live set: {e}"))?;
        for sticker in pack["stickers"]
            .as_array()
            .ok_or("live pack has no stickers")?
        {
            if let Some(id) = sticker["custom_emoji_id"].as_str() {
                live_ids.insert(id.to_owned());
            }
        }
        fetched.push((family, set, pack));
    }
    let general = json!(crate::roster_provenance::general(
        &data.join("catalog.db"),
        &live_ids
    )?);
    let coin_map = json!(coin_map);
    let history = json!(history);
    let captured = chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string();
    let documents = fetched
        .into_iter()
        .map(|(family, set, pack)| {
            crate::roster::pack(
                &set,
                &pack,
                family,
                if family == "coins" {
                    &coin_map
                } else {
                    &general
                },
                &history,
                &captured,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    log.event(
        "INFO",
        "roster",
        &format!("Fetched {} complete live rosters", documents.len()),
    );
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let general_art = crate::roster_provenance::art(&data.join("catalog.db"), &data, &config.root)?;
    let python = std::env::var_os("PYO3_PYTHON")
        .filter(|_| std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            config.root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    let media = crate::media_adapter::MediaAdapter {
        python,
        root: config.root.clone(),
        scratch: out.join(".codec"),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    for doc in &documents {
        let name = doc["set_name"].as_str().ok_or("invalid roster name")?;
        atomic::write_json(&out.join(format!("{name}.json")), doc).map_err(|e| e.to_string())?;
        let body = crate::roster_markdown::pack(doc)?;
        std::fs::write(out.join(format!("{name}.md")), body.as_bytes())
            .map_err(|e| e.to_string())?;
        let mut art = if doc["family"] == "general" {
            general_art.clone()
        } else {
            BTreeMap::new()
        };
        for row in doc["emoji"].as_array().into_iter().flatten() {
            let Some(id) = row["custom_emoji_id"].as_str() else {
                continue;
            };
            let source = if row["role"] == "brand-logo" {
                config.logo_path(false)?.filter(|p| p.is_file())
            } else if doc["family"] == "coins" && !config.value("COIN_EMOJI_DIR").is_empty() {
                row["name"]
                    .as_str()
                    .map(|name| name.split(',').next().unwrap_or("").trim())
                    .filter(|name| !name.is_empty() && !name.contains(['/', '\\']))
                    .map(|name| {
                        config
                            .path(config.value("COIN_EMOJI_DIR"))
                            .join(format!("{name}.png"))
                    })
                    .filter(|p| p.is_file())
            } else {
                None
            };
            if let Some(source) = source {
                art.insert(id.into(), source);
            }
        }
        let html = runtime.block_on(crate::pack_gallery::render(
            doc,
            &media,
            &art,
            &out.join(".thumbs"),
        ))?;
        std::fs::write(out.join(format!("{name}.html")), html.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    let index =
        crate::roster_index::build(&documents, &family, &old, &fingerprint(config)?, &captured)?;
    atomic::write_json(&out.join("index.json"), &index).map_err(|e| e.to_string())?;
    std::fs::write(
        out.join("README.md"),
        crate::roster_markdown::index(&index)?.as_bytes(),
    )
    .map_err(|e| e.to_string())?;
    let thumbs = out.join(".thumbs");
    let cached = if thumbs.is_dir() {
        std::fs::read_dir(thumbs)
            .map_err(|e| e.to_string())?
            .count()
    } else {
        0
    };
    println!(
        "packs/: {} packs, {} emoji, {} source-id mappings, {} retired ids, {cached} cached thumbnails",
        index["pack_count"],
        index["emoji_count"],
        index["by_source_id"]
            .as_object()
            .ok_or("invalid source lookup")?
            .len(),
        index["id_changes"]
    );
    Ok(0)
}
