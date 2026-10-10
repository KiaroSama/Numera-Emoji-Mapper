//! Native requested-ID collection; a tint invalidates the source-file shortcut before dedup.
use crate::{
    catalog::Catalog, collector::Collector, config::Config, logging::RunLog,
    media_adapter::MediaAdapter, telegram::Telegram,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
fn take(args: &[String], i: &mut usize) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or("option requires a value".into())
}
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!(
            "Usage: numera-emoji fetch-emoji-ids [--ids-file FILE] [--id ID] [--token-env KEY] [--data-dir DIR] [--phash-threshold -1|0..16] [--repaintable ask|skip|keep] [--tint #RRGGBB]"
        );
        return Ok(0);
    }
    let (mut files, mut inline, mut token_key, mut data, mut threshold, mut repaintable, mut tint) = (
        vec![],
        vec![],
        "GENERAL_BOT_TOKEN".to_owned(),
        "collection".to_owned(),
        -1i32,
        "ask".to_owned(),
        None::<String>,
    );
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--ids-file" => files.push(take(args, &mut i)?),
            "--id" => inline.push(take(args, &mut i)?),
            "--token-env" => token_key = take(args, &mut i)?,
            "--data-dir" => data = take(args, &mut i)?,
            "--phash-threshold" => {
                threshold = take(args, &mut i)?
                    .parse()
                    .map_err(|_| "invalid phash threshold")?
            }
            "--repaintable" => repaintable = take(args, &mut i)?,
            "--tint" => tint = Some(take(args, &mut i)?),
            value => return Err(format!("unknown fetch-emoji-ids option: {value}")),
        }
        i += 1;
    }
    if threshold != -1 && !(0..=16).contains(&threshold) {
        return Err("phash threshold must be -1 or 0..16".into());
    }
    if !["ask", "skip", "keep"].contains(&repaintable.as_str()) {
        return Err("invalid repaintable choice".into());
    }
    if tint.as_ref().is_some_and(|s| {
        s.len() != 7 || !s.starts_with('#') || !s[1..].bytes().all(|b| b.is_ascii_hexdigit())
    }) {
        return Err("tint must be #RRGGBB".into());
    }
    let ids = crate::emoji_ids::collect(&files, &inline)?;
    if ids.is_empty() {
        return Err("no valid custom-emoji IDs found".into());
    }
    let api = if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
        std::env::var("NUMERA_TEST_API_BASE").unwrap_or_else(|_| "https://api.telegram.org".into())
    } else {
        "https://api.telegram.org".into()
    };
    let tg = Telegram::new(config.require(&token_key)?.into(), api).map_err(|e| e.to_string())?;
    tg.read_call("getMe", &BTreeMap::new())
        .map_err(|e| e.to_string())?;
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
    let media = MediaAdapter {
        python,
        root: config.root.clone(),
        scratch: data.join("tmp/native-codec"),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let mut cat = Catalog::open(
        &data.join("catalog.db"),
        &crate::logging::utc(),
        &config.root,
    )?;
    let mut collector = Collector {
        cat: &mut cat,
        media: &media,
        data,
        threshold,
    };
    let mut resolved = serde_json::Map::new();
    for batch in ids.chunks(200) {
        let response = tg
            .get_custom_emoji_stickers(batch)
            .map_err(|e| e.to_string())?;
        for sticker in response
            .as_array()
            .ok_or("custom emoji response is not a list")?
        {
            if let Some(id) = sticker["custom_emoji_id"].as_str() {
                resolved.insert(id.into(), sticker.clone());
            }
        }
    }
    let missing = ids.iter().filter(|id| !resolved.contains_key(*id)).count();
    let (mut new, mut dedup, mut failed, mut repaint_skipped, mut repainted) = (0, 0, 0, 0, 0);
    let mut keep = repaintable == "keep" || tint.is_some();
    if !keep && repaintable == "ask" && resolved.values().any(|s| s["needs_repainting"] == true) {
        use std::io::{IsTerminal, Write};
        if std::io::stdin().is_terminal() {
            eprint!("Repaintable emoji use their stored colours here. Ingest anyway? [y/N] ");
            std::io::stderr().flush().map_err(|e| e.to_string())?;
            let mut answer = String::new();
            keep = std::io::stdin().read_line(&mut answer).is_ok()
                && ["y", "yes"].contains(&answer.trim().to_lowercase().as_str());
        }
    }
    for (id, sticker) in resolved {
        let repaint = sticker["needs_repainting"] == true;
        if repaint && !keep {
            repaint_skipped += 1;
            continue;
        }
        let recolour = repaint && tint.is_some();
        let fuid = sticker["file_unique_id"].as_str().unwrap_or("");
        let known = if recolour {
            Value::Null
        } else {
            collector
                .cat
                .dispatch(json!({"operation":"seen","file_unique_id":fuid}))?
        };
        let emojis = sticker["emoji"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| vec![s])
            .unwrap_or_default();
        let mut labels = json!({"emojis":emojis,"keywords":[format!("premium-id:{id}")],"sources":["bot-inventory"],"file_unique_id":fuid});
        if let Some(key) = known.as_str() {
            labels["content_key"] = json!(key);
            collector.cat.merge_labels(&labels)?;
            dedup += 1;
            continue;
        }
        if recolour {
            let tint = tint.as_ref().ok_or("missing tint")?.to_uppercase();
            labels["keywords"]
                .as_array_mut()
                .ok_or("invalid labels")?
                .push(json!(format!("tint:{tint}")));
            labels["tint"] = json!(tint);
        }
        match collector.download_ingest(&tg, &runtime, &sticker, labels) {
            Ok(true) => {
                new += 1;
                if recolour {
                    repainted += 1
                }
            }
            Ok(false) => {
                dedup += 1;
                if recolour {
                    repainted += 1
                }
            }
            Err(error) => {
                failed += 1;
                log.event("WARNING", "collector", &error)
            }
        }
    }
    let mut extra = String::new();
    if repaint_skipped > 0 {
        extra.push_str(&format!(" repaintable_skipped={repaint_skipped}"));
    }
    if repainted > 0 {
        extra.push_str(&format!(" repainted={repainted}"));
    }
    println!(
        "Done. unique_ids={} new={new} dedup={dedup} failed={failed} missing={missing}{extra}",
        ids.len()
    );
    crate::collector::report_stats(collector.cat)?;
    Ok(if failed + missing == 0 {
        0
    } else if new + dedup > 0 {
        3
    } else {
        4
    })
}
