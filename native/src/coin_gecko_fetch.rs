//! Resumable CoinGecko logos: verified cache only, private download staging, one keyword writer.
use serde_json::{Map, Value, json};
use std::path::Path;
use std::time::Duration;

pub enum Payload {
    Json(Value),
    Bytes(Vec<u8>),
}
pub fn safe_ticker(symbol: &str) -> String {
    symbol
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
        .collect::<String>()
        .trim_matches(['.', '_', '-'])
        .into()
}
fn publish_bytes(bytes: &[u8], path: &Path) -> Result<bool, String> {
    let marker = path.with_file_name(format!(".logo-marker-{}", std::process::id()));
    let staged = crate::codec_video_encode::Staged::new(&marker, "png")?;
    let result = (|| {
        std::fs::write(&staged.0, bytes).map_err(|_| "cannot stage coin logo")?;
        if !crate::codec_image::valid_logo(&staged.0) {
            return Ok(false);
        }
        crate::atomic::write_bytes(path, bytes).map_err(|_| "cannot persist verified coin logo")?;
        Ok(true)
    })();
    staged.clean()?;
    result
}
pub fn fetch_with<F, W>(
    directory: &Path,
    pages: u32,
    delay: Duration,
    mut get: F,
    mut wait: W,
) -> Result<u8, String>
where
    F: FnMut(&str, bool, u32) -> Result<Payload, String>,
    W: FnMut(Duration) -> Result<(), String>,
{
    if pages == 0 {
        return Err("pages must be at least1".into());
    }
    crate::coin_provider_stage::physical(directory)?;
    let png = directory.join("logos/png");
    let ancestor = png
        .ancestors()
        .find(|p| p.exists())
        .ok_or("coin logo parent unavailable")?;
    crate::coin_provider_stage::physical(ancestor)?;
    std::fs::create_dir_all(&png).map_err(|_| "cannot create coin logo directory")?;
    crate::coin_provider_stage::physical(&png)?;
    let svg = directory.join("logos/svg");
    let mut svg_tickers = std::collections::BTreeSet::new();
    if svg.is_dir() {
        crate::coin_provider_stage::physical(&svg)?;
        for entry in std::fs::read_dir(&svg).map_err(|_| "cannot read SVG logos")? {
            let path = entry.map_err(|_| "cannot read SVG logo")?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("svg") {
                crate::coin_provider_stage::physical(&path)?;
                svg_tickers.insert(
                    path.file_stem()
                        .and_then(|e| e.to_str())
                        .ok_or("SVG filename must be UTF-8")?
                        .to_lowercase(),
                );
            }
        }
    }
    let mut rows = Map::new();
    let mut complete = true;
    for page in 1..=pages {
        let url = format!(
            "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page={page}&sparkline=false"
        );
        let payload = match get(&url, false, 6) {
            Ok(Payload::Json(value)) => value,
            _ => {
                complete = false;
                break;
            }
        };
        let Some(coins) = payload.as_array().filter(|v| !v.is_empty()) else {
            break;
        };
        for coin in coins {
            let ticker = safe_ticker(coin["symbol"].as_str().unwrap_or(""));
            if ticker.is_empty() || rows.contains_key(&ticker) {
                continue;
            }
            let name = coin["name"].as_str().unwrap_or("").trim();
            if svg_tickers.contains(&ticker) {
                rows.insert(ticker.clone(),json!({"ticker":ticker,"name":name,"format":"svg","file":format!("logos/svg/{ticker}.svg")}));
                continue;
            }
            let path = png.join(format!("{ticker}.png"));
            if path.exists() {
                crate::coin_provider_stage::physical(&path)?;
                if !crate::codec_image::valid_logo(&path) {
                    std::fs::remove_file(&path).map_err(|_| "cannot remove corrupt cached logo")?;
                }
            }
            let resumed = path.exists();
            if !resumed {
                let Some(url) = coin["image"].as_str().filter(|url| url.starts_with("http")) else {
                    continue;
                };
                let Ok(Payload::Bytes(bytes)) = get(url, true, 3) else {
                    continue;
                };
                if !publish_bytes(&bytes, &path)? {
                    continue;
                }
            }
            rows.insert(ticker.clone(),json!({"ticker":ticker,"name":name,"format":"png","file":format!("logos/png/{ticker}.png"),"path":path}));
            if !resumed {
                wait(Duration::from_millis(50))?;
            }
        }
        wait(delay)?;
    }
    for ticker in svg_tickers {
        if !rows.contains_key(&ticker) {
            rows.insert(ticker.clone(),json!({"ticker":ticker,"name":"","format":"svg","file":format!("logos/svg/{ticker}.svg")}));
        }
    }
    let mut files = std::fs::read_dir(&png)
        .map_err(|_| "cannot read cached logos")?
        .map(|e| {
            e.map(|e| e.path())
                .map_err(|_| "cannot read cached logo entry")
        })
        .collect::<Result<Vec<_>, _>>()?;
    files.sort();
    for path in files {
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        crate::coin_provider_stage::physical(&path)?;
        let ticker = path
            .file_stem()
            .and_then(|e| e.to_str())
            .ok_or("cached logo filename must be UTF-8")?
            .to_lowercase();
        if rows.contains_key(&ticker) || !crate::codec_image::valid_logo(&path) {
            continue;
        }
        rows.insert(ticker.clone(),json!({"ticker":ticker,"name":"","format":"png","file":format!("logos/png/{ticker}.png"),"path":path}));
    }
    crate::coin_keywords::write(&rows, &directory.join("keywords.csv"))?;
    Ok(if complete { 0 } else { 3 })
}
