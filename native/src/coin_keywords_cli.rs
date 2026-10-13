//! Keyword command orchestration owns file inventory and names; existing codec/CSV policies are reused.
use crate::{config::Config, logging::RunLog};
use serde_json::{Map, Value, json};
use std::path::Path;
use std::time::Duration;

pub fn build(directory: &Path, names: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    crate::coin_provider_stage::physical(directory)?;
    let mut rows = Map::new();
    for format in ["svg", "png"] {
        let folder = directory.join("logos").join(format);
        if !folder.is_dir() {
            continue;
        }
        crate::coin_provider_stage::physical(&folder)?;
        let mut files = std::fs::read_dir(&folder)
            .map_err(|_| "cannot read coin logos")?
            .map(|entry| {
                entry
                    .map(|e| e.path())
                    .map_err(|_| "cannot read coin logo entry")
            })
            .collect::<Result<Vec<_>, _>>()?;
        files.sort();
        for path in files {
            if path.extension().and_then(|v| v.to_str()) != Some(format) {
                continue;
            }
            crate::coin_provider_stage::physical(&path)?;
            let ticker = path
                .file_stem()
                .and_then(|v| v.to_str())
                .ok_or("coin logo filename must be UTF-8")?
                .to_lowercase();
            let row = json!({"ticker":ticker,"name":names.get(&ticker).cloned().unwrap_or(json!("")),"format":format,
                "file":format!("logos/{format}/{ticker}.{format}"),"path":path});
            if format == "svg" || !rows.contains_key(&ticker) {
                rows.insert(ticker, row);
            }
        }
    }
    crate::coin_keywords::write(&rows, &directory.join("keywords.csv"))?;
    Ok(rows)
}

pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
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
            value => return Err(format!("unknown coin-build-keywords option: {value}")),
        }
        index += 1;
    }
    if help {
        println!("Usage: numera-emoji coin-build-keywords [--coin-dir DIR]");
        return Ok(0);
    }
    crate::coin_provider_stage::physical(&directory)?;
    let raw = config.value("COIN_PAGE_DELAY");
    let delay = raw
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map(|v| v.max(0.0))
        .unwrap_or(2.0);
    let delay = Duration::try_from_secs_f64(delay)
        .map_err(|_| "COIN_PAGE_DELAY exceeds native duration range")?;
    let client = crate::coin_http::ProviderHttp::new(Duration::from_secs(60))?;
    let (names, complete) = crate::coin_gecko::names(&client, delay)?;
    let rows = build(&directory, &names)?;
    let count = |format: &str| rows.values().filter(|row| row["format"] == format).count();
    let named = rows
        .values()
        .filter(|row| row["name"].as_str().is_some_and(|s| !s.is_empty()))
        .count();
    println!(
        "keywords.csv written: {} logos ({} svg, {} png), {named} with names.",
        rows.len(),
        count("svg"),
        count("png")
    );
    log.event(
        "INFO",
        "coins",
        &format!("keyword_rows={} complete={complete}", rows.len()),
    );
    if !complete {
        println!(
            "INCOMPLETE: a name lookup failed; earlier names were kept. Re-run to refresh them."
        );
        return Ok(3);
    }
    Ok(0)
}
