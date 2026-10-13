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
