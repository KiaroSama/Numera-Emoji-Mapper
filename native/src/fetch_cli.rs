//! Native whole-pack collection entry point; exact media is the only source-runtime adapter.
use crate::{
    catalog::Catalog, collector::Collector, config::Config, logging::RunLog,
    media_adapter::MediaAdapter, telegram::Telegram,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn option(args: &[String], i: &mut usize) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or("option requires a value".into())
}
fn pack_name(value: &str) -> String {
    let value = value.trim();
    for prefix in [
        "https://t.me/addemoji/",
        "http://t.me/addemoji/",
        "t.me/addemoji/",
        "tg://addemoji?slug=",
    ] {
        if let Some(value) = value.strip_prefix(prefix) {
            return value
                .split('?')
                .next()
                .unwrap_or("")
                .trim_matches('/')
                .into();
        }
    }
    value.into()
}
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Usage: numera-emoji fetch-pack PACKS... [--token-env KEY] [--data-dir DIR] [--phash-threshold -1|0..16] [--limit N] [--repaintable ask|keep|skip]"
        );
        return Ok(0);
    }
    let (mut packs, mut token_env, mut data, mut threshold, mut limit, mut repaintable) = (
        Vec::new(),
        "GENERAL_BOT_TOKEN".to_owned(),
        "collection".to_owned(),
        -1i32,
        0u64,
        "ask".to_owned(),
    );
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--token-env" => token_env = option(args, &mut i)?,
            "--data-dir" => data = option(args, &mut i)?,
            "--phash-threshold" => {
                threshold = option(args, &mut i)?
                    .parse()
                    .map_err(|_| "invalid phash threshold")?
            }
            "--limit" => {
                limit = option(args, &mut i)?
                    .parse()
                    .map_err(|_| "--limit must be 0 or greater")?
            }
            "--repaintable" => repaintable = option(args, &mut i)?,
            value if !value.starts_with('-') => packs.push(value.to_owned()),
            value => return Err(format!("unknown fetch-pack option: {value}")),
        }
        i += 1;
    }
    if packs.is_empty() {
        return Err("at least one pack or custom emoji ID is required".into());
    }
    if threshold != -1 && !(0..=16).contains(&threshold) {
        return Err("phash threshold must be -1 or 0..16".into());
    }
    if !["ask", "keep", "skip"].contains(&repaintable.as_str()) {
        return Err("invalid repaintable choice".into());
    }
    let token = config.require(&token_env)?;
    let base = if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
        std::env::var("NUMERA_TEST_API_BASE").unwrap_or_else(|_| "https://api.telegram.org".into())
    } else {
        "https://api.telegram.org".into()
    };
    let tg = Telegram::new(token.into(), base).map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    let mut names = Vec::new();
    for value in packs {
        let value = value.trim();
        let id = value.strip_prefix("premium-id:").unwrap_or(value);
        if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
            ids.push(id.to_owned());
        } else {
            names.push(pack_name(value));
        }
    }
    let mut resolved = BTreeMap::new();
    for batch in ids.chunks(200) {
        let stickers = tg
            .get_custom_emoji_stickers(batch)
            .map_err(|e| e.to_string())?;
        for sticker in stickers
            .as_array()
            .ok_or("custom emoji response is not a list")?
        {
            if let (Some(id), Some(name)) = (
                sticker["custom_emoji_id"].as_str(),
                sticker["set_name"].as_str(),
            ) {
                resolved.insert(id.to_owned(), name.to_owned());
            }
        }
    }
    let mut missing = 0u64;
    for id in ids {
        if let Some(name) = resolved.get(&id) {
            names.push(name.clone())
        } else {
            missing += 1;
        }
    }
    let mut unique = std::collections::BTreeSet::new();
    names.retain(|name| unique.insert(name.clone()));
    if names.iter().any(|name| {
        name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    }) {
        return Err("invalid pack name".into());
    }
    let data = config.path(&data);
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let data = data.canonicalize().map_err(|e| e.to_string())?;
    let python = std::env::var_os("PYO3_PYTHON")
        .filter(|_| std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            config.root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    let adapter = MediaAdapter {
        python,
        root: config.root.clone(),
        scratch: data.join("tmp/native-codec"),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let mut catalog = Catalog::open(
        &data.join("catalog.db"),
        &crate::logging::utc(),
        &config.root,
    )?;
    let mut collector = Collector {
        cat: &mut catalog,
        media: &adapter,
        data,
        threshold,
    };
    let mut total = json!({"new":0,"dedup":0,"skipped":0,"failed":0,"repaintable":0});
    let mut packs_failed = missing;
    for name in names {
        log.event("INFO", "collector", "fetching requested pack");
        match collector.fetch_pack(&tg, &runtime, &name, limit, &repaintable) {
            Ok(counts) => {
                for field in ["new", "dedup", "skipped", "failed", "repaintable"] {
                    total[field] = json!(
                        total[field].as_u64().unwrap_or(0) + counts[field].as_u64().unwrap_or(0)
                    );
                }
            }
            Err(error) => {
                log.event("WARNING", "collector", &error);
                packs_failed += 1;
            }
        }
    }
    let mut extra = if packs_failed > 0 {
        format!(" packs_failed={packs_failed}")
    } else {
        String::new()
    };
    let repaintable = total["repaintable"].as_u64().unwrap_or(0);
    if repaintable > 0 {
        extra.push_str(&format!(" repaintable_skipped={repaintable}"));
    }
    println!(
        "Done. new={} dedup={} failed={}{extra}",
        total["new"], total["dedup"], total["failed"]
    );
    crate::collector::report_stats(collector.cat)?;
    let succeeded = total["new"].as_u64().unwrap_or(0) + total["dedup"].as_u64().unwrap_or(0);
    Ok(if total["failed"] == 0 && packs_failed == 0 {
        0
    } else if succeeded > 0 {
        3
    } else {
        4
    })
}
