//! Frozen rebuild plans are loaded once; corrupt plans are never silently regenerated.
use serde_json::{Map, Value, json};
use std::path::Path;

pub fn validate(plan: &Value) -> Result<&[Value], String> {
    let entries = plan
        .as_array()
        .filter(|p| !p.is_empty())
        .ok_or("frozen rebuild plan must be a nonempty list")?;
    for group in entries {
        if group["rep"].as_str().is_none_or(str::is_empty)
            || group["kw"].as_str().is_none()
            || group["tickers"]
                .as_array()
                .is_none_or(|t| t.is_empty() || t.iter().any(|v| !v.is_string()))
        {
            return Err("frozen rebuild plan entries are missing rep/tickers/kw".into());
        }
    }
    Ok(entries)
}
pub(crate) fn document(path: &Path) -> Result<Value, String> {
    crate::coin_provider_stage::physical(path)?;
    serde_json::from_slice(&std::fs::read(path).map_err(|_| "cannot read frozen rebuild plan")?)
        .map_err(|_| "invalid frozen rebuild plan JSON".into())
}
pub fn load(directory: &Path) -> Result<Value, String> {
    let path = directory.join("rebuild_dedup_plan.json");
    let plan = document(&path)?;
    validate(&plan)?;
    Ok(plan)
}
pub fn build(directory: &Path, images: &Path) -> Result<Value, String> {
    for path in [directory, images] {
        crate::coin_provider_stage::physical(path)?;
        if !path.is_dir() {
            return Err("rebuild plan inputs must be physical directories".into());
        }
    }
    let plan_path = directory.join("rebuild_dedup_plan.json");
    let shared_path = directory.join("shared_logo_groups.json");
    for path in [&plan_path, &shared_path] {
        crate::coin_manifest::output_path(path)?;
    }
    match std::fs::symlink_metadata(&plan_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => return Err("frozen rebuild plan already exists; refusing regeneration".into()),
        Err(_) => return Err("cannot determine whether frozen rebuild plan exists".into()),
    }
    let mut paths = std::fs::read_dir(images)
        .map_err(|_| "cannot list rebuild images")?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|_| "cannot list rebuild images")?;
    paths.sort();
    let keywords = crate::ticker_keywords::load(&directory.join("keywords.csv"))?
        .into_iter()
        .map(|(k, v)| (k, json!(v)))
        .collect::<Map<_, _>>();
    let inventory = std::fs::read_to_string(directory.join("currency-emoji-inventory.md"))
        .map_err(|_| "cannot read rebuild inventory")?;
    let inventory = crate::coin_map_cli::tickers(&inventory)
        .into_iter()
        .collect::<Vec<_>>();
    let mut files = Vec::new();
    for path in paths {
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        crate::coin_provider_stage::physical(&path)?;
        let metadata = std::fs::metadata(&path).map_err(|_| "cannot inspect rebuild image")?;
        if !metadata.is_file() || metadata.len() == 0 {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("rebuild filename is not UTF-8")?;
        let ticker = path
            .file_stem()
            .and_then(|n| n.to_str())
            .ok_or("rebuild ticker is not UTF-8")?
            .to_lowercase();
        let bytes = std::fs::read(&path).map_err(|_| "cannot read rebuild image")?;
        match crate::codec_image::audit_image(&bytes) {
            Ok((hash, _)) => files.push(json!({"filename":filename,"ticker":ticker,"hash":hash})),
            Err(error) => println!(
                "  hash failed {filename}: {}",
                crate::logging::redact(&error, &[])
            ),
        }
    }
    let plan = crate::coin_plan::group(&files, &inventory, &keywords)?;
    let mut shared = Map::new();
    let mut extra = 0;
    for group in plan
        .as_array()
        .ok_or("rebuild grouping did not return a list")?
    {
        let tickers = group["tickers"]
            .as_array()
            .ok_or("rebuild group has no tickers")?;
        extra += tickers.len().saturating_sub(1);
        if tickers.len() > 1 {
            shared.insert(
                group["rep"]
                    .as_str()
                    .ok_or("rebuild group has no representative")?
                    .into(),
                group["tickers"].clone(),
            );
        }
    }
    crate::atomic::write_json(&plan_path, &plan)
        .map_err(|_| "cannot persist frozen rebuild plan")?;
    crate::atomic::write_json(&shared_path, &Value::Object(shared.clone()))
        .map_err(|_| "cannot persist shared logo report")?;
    println!(
        "plan: {} unique images | shared-logo groups: {} | coins collapsing onto a shared image: {extra}",
        plan.as_array().map_or(0, Vec::len),
        shared.len()
    );
    Ok(plan)
}
