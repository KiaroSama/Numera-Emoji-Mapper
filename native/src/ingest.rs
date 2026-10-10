//! Dedup decisions operate on the authoritative catalog; hashes only nominate rivals.
use crate::catalog::Catalog;
use serde_json::{Value, json};
use std::path::Path;

type Result<T> = std::result::Result<T, String>;
fn value<'a>(item: &'a Value, name: &str) -> Result<&'a str> {
    item.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {name}"))
}

pub async fn identify<F, C>(
    cat: &Catalog,
    incoming: &Value,
    threshold: i32,
    mut compare: F,
    mut collision: C,
) -> Result<(String, bool)>
where
    F: AsyncFnMut(&Path, &Path, &str) -> Result<Option<bool>>,
    C: AsyncFnMut(&Path, &str) -> Result<String>,
{
    if threshold != -1 && !(0..=16).contains(&threshold) {
        return Err("phash threshold must be -1 or0..16".into());
    }
    let key = value(incoming, "content_key")?;
    let fmt = value(incoming, "fmt")?;
    let path = Path::new(value(incoming, "file_path")?);
    let phash = incoming.get("phash").and_then(Value::as_u64);
    let (prefix, digest) = key.split_once(':').ok_or("invalid content key")?;
    let lookup = format!(
        "{prefix}:{}",
        digest.rsplit(':').next().ok_or("invalid digest")?
    );
    let mut matches = Vec::new();
    let mut unknown = false;
    for item in cat.all(Some(fmt))? {
        let rival = value(&item, "content_key")?;
        let candidate = if rival.matches(':').count() == 2 {
            format!(
                "{prefix}:{}",
                rival.rsplit(':').next().ok_or("invalid rival")?
            )
        } else {
            rival.into()
        };
        let near = threshold >= 0
            && phash
                .zip(item.get("phash").and_then(Value::as_u64))
                .is_some_and(|(a, b)| (a ^ b).count_ones() <= threshold as u32);
        if rival != key && candidate != lookup && !near {
            continue;
        }
        match compare(Path::new(value(&item, "file_path")?), path, fmt).await? {
            Some(true) => matches.push(rival.to_owned()),
            None => unknown = true,
            Some(false) => {}
        }
    }
    if unknown || matches.len() > 1 {
        return Err(
            "catalog identity is undecidable or ambiguous; incoming media was retained".into(),
        );
    }
    if let Some(key) = matches.pop() {
        return Ok((key, false));
    }
    if cat.get(key)?.is_some() {
        let fresh = collision(path, &lookup).await?;
        if cat.get(&fresh)?.is_some() {
            return Err(
                "collision identity is already occupied; incoming media was retained".into(),
            );
        }
        return Ok((fresh, true));
    }
    Ok((key.to_owned(), true))
}

pub async fn add_verified<F, C>(
    cat: &mut Catalog,
    mut item: Value,
    utc: &str,
    threshold: i32,
    compare: F,
    collision: C,
) -> Result<(String, bool)>
where
    F: AsyncFnMut(&Path, &Path, &str) -> Result<Option<bool>>,
    C: AsyncFnMut(&Path, &str) -> Result<String>,
{
    let path = std::path::PathBuf::from(value(&item, "file_path")?);
    let (key, new) = identify(cat, &item, threshold, compare, collision).await?;
    item["content_key"] = json!(key);
    if new {
        if !cat.insert(&item, utc)? {
            return Err("verified insertion identity is already occupied".into());
        }
    } else {
        cat.merge_labels(&item)?;
    }
    if let Some(fuid) = item.get("file_unique_id").and_then(Value::as_str) {
        cat.record_seen(fuid, &key)?;
    }
    if !new {
        let owner = cat.get(&key)?.ok_or("canonical item vanished")?;
        let losing = path.canonicalize().map_err(|e| e.to_string())?;
        let kept = Path::new(value(&owner, "file_path")?)
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if losing != kept && losing.is_file() {
            let another = cat
                .all(None)?
                .iter()
                .filter(|v| v["content_key"] != key)
                .any(|v| {
                    v["file_path"]
                        .as_str()
                        .and_then(|s| Path::new(s).canonicalize().ok())
                        .is_some_and(|p| p == losing)
                });
            if !another {
                std::fs::remove_file(losing).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok((key, new))
}
