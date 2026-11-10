//! Native requested-ID collection; a tint invalidates the source-file shortcut before dedup.
use crate::{
    catalog::Catalog, collector::Collector, config::Config, logging::RunLog,
    media_adapter::MediaCodec, media_native_adapter::NativeMediaAdapter, telegram::Telegram,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn take(args: &[String], i: &mut usize) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or("option requires a value".into())
}
fn validate_repaint(repaintable: &str, tint: Option<&str>) -> Result<Option<String>, String> {
    if !crate::repaint_gate::MODES.contains(&repaintable) {
        return Err("invalid repaintable choice".into());
    }
    tint.map(|text| {
        let [r, g, b] = crate::codec_tint::parse_tint(text)?;
        Ok(format!("#{r:02X}{g:02X}{b:02X}"))
    })
    .transpose()
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
    let tint = validate_repaint(&repaintable, tint.as_deref())?;
    let ids = crate::emoji_ids::collect_with_report(&files, &inline, |message| {
        log.event("INFO", "fetch_emoji_ids", message);
    })?;
    if ids.is_empty() {
        return Err("no valid custom-emoji IDs found".into());
    }
    let api = if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
        std::env::var("NUMERA_TEST_API_BASE").unwrap_or_else(|_| "https://api.telegram.org".into())
    } else {
        "https://api.telegram.org".into()
    };
    let tg = Telegram::new(config.require(&token_key)?.into(), api)
        .map_err(|e| e.to_string())?
        .with_log(log);
    tg.read_call("getMe", &BTreeMap::new())
        .map_err(|e| e.to_string())?;
    let data = config.path(&data);
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let data = data.canonicalize().map_err(|e| e.to_string())?;
    let media = NativeMediaAdapter::for_project(&config.root, data.join("tmp/native-codec"))?;
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
    let counts = fetch_ids(
        &mut collector,
        &tg,
        &runtime,
        &ids,
        &repaintable,
        tint.as_deref(),
        log,
    )?;
    let mut extra = String::new();
    if counts["repaintable"].as_u64().unwrap_or(0) > 0 {
        extra.push_str(&format!(" repaintable_skipped={}", counts["repaintable"]));
    }
    if counts["repainted"].as_u64().unwrap_or(0) > 0 {
        extra.push_str(&format!(" repainted={}", counts["repainted"]));
    }
    println!(
        "Done. unique_ids={} new={} dedup={} failed={} missing={}{extra}",
        ids.len(),
        counts["new"],
        counts["dedup"],
        counts["failed"],
        counts["missing"]
    );
    crate::collector::report_stats(collector.cat)?;
    Ok(crate::collector::ingest_exit_code(
        u64::from(counts["new"] != 0 || counts["dedup"] != 0),
        u64::from(counts["failed"] != 0 || counts["missing"] != 0),
    ))
}

pub fn fetch_ids<M: MediaCodec>(
    collector: &mut Collector<'_, M>,
    tg: &Telegram,
    runtime: &tokio::runtime::Runtime,
    ids: &[String],
    repaintable: &str,
    tint: Option<&str>,
    log: &RunLog,
) -> Result<Value, String> {
    let tint = validate_repaint(repaintable, tint)?;
    let tint = tint.as_deref();
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
    let repainted_ids = resolved
        .iter()
        .filter(|(_, sticker)| crate::repaint_gate::is_repaintable(sticker))
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    let keep = tint.is_some() || crate::repaint_gate::run(&repainted_ids, repaintable, Some(log))?;
    for (id, sticker) in resolved {
        let repaint = crate::repaint_gate::is_repaintable(&sticker);
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
            let tint = tint.ok_or("missing tint")?;
            labels["keywords"]
                .as_array_mut()
                .ok_or("invalid labels")?
                .push(json!(format!("tint:{tint}")));
            labels["tint"] = json!(tint);
        }
        match collector.download_ingest(tg, runtime, &sticker, labels) {
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
    Ok(
        json!({"new":new,"dedup":dedup,"failed":failed,"missing":missing,
        "repaintable":repaint_skipped,"repainted":repainted}),
    )
}
