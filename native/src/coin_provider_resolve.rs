//! Resumable provider search preserves unknowns outside the durable no-match cache.
use serde_json::{Map, Value, json};

pub enum Search {
    Quota,
    Transient,
    Match {
        id: Option<String>,
        confidence: String,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct Resolved {
    pub quota: bool,
    pub unknown: usize,
}

pub fn resolve_file<F>(
    path: &std::path::Path,
    missing: &[(String, String)],
    retry_unmatched: bool,
    search: F,
) -> Result<(Resolved, Map<String, Value>), String>
where
    F: FnMut(&str, &str) -> Result<Search, String>,
{
    if !path.is_absolute() {
        return Err("provider cache path must be absolute".into());
    }
    let parent = path.parent().ok_or("provider cache has no parent")?;
    std::fs::create_dir_all(parent).map_err(|_| "cannot create provider cache directory")?;
    let _lease =
        crate::locks::Lease::acquire(&path.with_extension("json.lock"), &crate::logging::utc())
            .map_err(|_| "provider cache is busy or unavailable")?;
    let mut cache = match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
            let bytes = std::fs::read(path).map_err(|_| "cannot read provider cache")?;
            serde_json::from_slice::<Value>(&bytes)
                .map_err(|_| "invalid provider cache JSON")?
                .as_object()
                .cloned()
                .ok_or("provider cache must be an object")?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Map::new(),
        _ => return Err("provider cache must be a regular non-symlink file".into()),
    };
    if cache.values().any(|entry| !entry.is_object()) {
        return Err("provider cache entries must be objects".into());
    }
    let result = resolve(missing, &mut cache, retry_unmatched, search, |cache| {
        crate::atomic::write_json(path, &Value::Object(cache.clone()))
            .map_err(|_| "cannot persist provider search cache".into())
    })?;
    Ok((result, cache))
}

pub fn resolve<F, S>(
    missing: &[(String, String)],
    cache: &mut Map<String, Value>,
    retry_unmatched: bool,
    mut search: F,
    mut save: S,
) -> Result<Resolved, String>
where
    F: FnMut(&str, &str) -> Result<Search, String>,
    S: FnMut(&Map<String, Value>) -> Result<(), String>,
{
    let mut result = Resolved {
        quota: false,
        unknown: 0,
    };
    for (name, ticker) in missing {
        if let Some(entry) = cache.get(ticker)
            && !(retry_unmatched && entry["id"].is_null())
        {
            continue;
        }
        match search(name, ticker)? {
            Search::Quota => {
                result.quota = true;
                break;
            }
            Search::Transient => {
                result.unknown += 1;
                continue;
            }
            Search::Match { id, confidence } => {
                let id = id.filter(|value| {
                    !value.is_empty() && matches!(confidence.as_str(), "name-exact" | "symbol")
                });
                cache.insert(
                    ticker.clone(),
                    json!({"id":id,"conf":confidence,"name":name}),
                );
                save(cache)?;
            }
        }
    }
    Ok(result)
}
