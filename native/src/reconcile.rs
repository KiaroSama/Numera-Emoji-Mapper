//! Identity-backed live reconciliation. Unexamined evidence is never a foreign sticker.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, String>;
fn text<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
fn strings(v: Option<&Value>) -> Result<Vec<String>> {
    v.and_then(Value::as_array)
        .ok_or("keys must be a list")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "invalid key".into())
        })
        .collect()
}

#[derive(Debug, PartialEq)]
pub enum NearMatch {
    Missing,
    Unique(String),
    Ambiguous,
    Undecidable,
}

pub async fn near_catalog<F>(
    items: &[Value],
    fmt: &str,
    probe: Option<u64>,
    lookup: &str,
    mut compare: F,
) -> Result<NearMatch>
where
    F: AsyncFnMut(&Value) -> Result<Option<bool>>,
{
    let Some(probe) = probe else {
        return Ok(if fmt == "animated" {
            NearMatch::Missing
        } else {
            NearMatch::Undecidable
        });
    };
    let mut verified = Vec::new();
    let mut unknown = false;
    for item in items.iter().filter(|item| text(item, "fmt") == fmt) {
        let key = text(item, "content_key");
        let digest_match = fmt == "video"
            && (key == lookup
                || key.matches(':').count() == 2
                    && key.rsplit(':').next() == lookup.split_once(':').map(|(_, v)| v));
        let close = item.get("phash").is_none_or(Value::is_null)
            || item["phash"]
                .as_u64()
                .is_some_and(|hash| (hash ^ probe).count_ones() <= 2)
            || digest_match;
        if !close {
            continue;
        }
        match compare(item).await? {
            Some(true) => verified.push(key.to_owned()),
            None => unknown = true,
            Some(false) => {}
        }
    }
    Ok(if verified.len() > 1 {
        NearMatch::Ambiguous
    } else if unknown {
        NearMatch::Undecidable
    } else if let Some(key) = verified.pop() {
        NearMatch::Unique(key)
    } else {
        NearMatch::Missing
    })
}

pub fn open(set: &Value) -> bool {
    let keys = set
        .get("keys")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let logo = set.get("logo").and_then(Value::as_bool).unwrap_or(false);
    set.get("live").and_then(Value::as_u64) == Some(keys as u64 + u64::from(logo))
}
pub fn identity(sticker: &Value) -> (String, String) {
    (
        text(sticker, "file_unique_id").into(),
        text(sticker, "custom_emoji_id").into(),
    )
}

// The I/O caller resolves content with the exact media adapter and supplies one
// result per sticker. Null is a proven miss; an error never enters this function.
pub fn reconcile(request: &Value) -> Result<Value> {
    let mut set = request.get("set").cloned().ok_or("missing set")?;
    let live = request
        .get("stickers")
        .and_then(Value::as_array)
        .ok_or("missing live stickers")?;
    let resolved = request
        .get("resolved")
        .and_then(Value::as_array)
        .ok_or("missing identity evidence")?;
    if resolved.len() != live.len() {
        return Err("incomplete identity evidence".into());
    }
    let publications = request
        .get("publications")
        .and_then(Value::as_object)
        .ok_or("missing publication identities")?;
    let seen = request
        .get("seen")
        .and_then(Value::as_object)
        .ok_or("missing known file identities")?;
    let mut keys = strings(set.get("keys"))?;
    let offset = usize::from(set.get("logo").and_then(Value::as_bool).unwrap_or(false));
    if live.len() < offset + keys.len() {
        return Err("live set is smaller than its recorded manifest".into());
    }
    for (i, key) in keys.iter().enumerate() {
        let sticker = &live[i + offset];
        let (fuid, cid) = identity(sticker);
        let recorded = publications.get(key).and_then(Value::as_str);
        let known = seen.get(&fuid).and_then(Value::as_str);
        if !cid.is_empty() && recorded == Some(&cid)
            || known == Some(key)
            || resolved[i + offset].as_str() == Some(key)
        {
            continue;
        }
        return Err(format!(
            "live position {} no longer proves recorded key {key}",
            i + offset
        ));
    }
    let start = offset + keys.len();
    let mut records = Vec::new();
    for (i, value) in resolved.iter().enumerate().skip(start) {
        if value.is_null() {
            if resolved[i + 1..].iter().any(|v| v.as_str().is_some()) {
                return Err("foreign sticker precedes an unrecorded catalog item; refusing positional attribution".into());
            }
            break;
        }
        let key = value.as_str().ok_or("unresolvable live identity")?;
        if keys.iter().any(|k| k == key) {
            return Err(format!("live set holds {key} twice"));
        }
        keys.push(key.into());
        records.push(json!({"content_key":key,"custom_emoji_id":live[i].get("custom_emoji_id").cloned().unwrap_or(Value::Null),
            "file_unique_id":live[i].get("file_unique_id").cloned().unwrap_or(Value::Null)}));
    }
    set["keys"] = json!(keys);
    set["live"] = json!(live.len());
    Ok(json!({"set":set,"publications":records}))
}

pub fn confirm(request: &Value) -> Result<Value> {
    let before = request
        .get("before")
        .and_then(Value::as_array)
        .ok_or("missing before identities")?;
    let after = request
        .get("after")
        .and_then(Value::as_array)
        .ok_or("missing after stickers")?;
    let old = before.iter().map(identity).collect::<BTreeSet<_>>();
    let new = after
        .iter()
        .filter(|s| {
            let id = identity(s);
            (!id.0.is_empty() || !id.1.is_empty()) && !old.contains(&id)
        })
        .collect::<Vec<_>>();
    if new.len() != 1 {
        return Err(format!(
            "{} new identities appeared; expected exactly one",
            new.len()
        ));
    }
    if request.get("same_image").and_then(Value::as_bool) != Some(true) {
        return Err("new sticker content cannot be proven ours".into());
    }
    let key = text(request, "content_key");
    if key.is_empty() {
        return Err("missing content key".into());
    }
    if let Some(owner) = request.get("known_owner").and_then(Value::as_str)
        && owner != key
    {
        return Err("new sticker already belongs to another key".into());
    }
    Ok(new[0].clone())
}

pub fn pending(
    items: &[Value],
    plan: &[String],
    published: &BTreeSet<String>,
    skipped: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let map = items
        .iter()
        .map(|v| (text(v, "content_key"), v))
        .collect::<BTreeMap<_, _>>();
    let pos = items
        .iter()
        .enumerate()
        .map(|(i, v)| (text(v, "content_key"), i))
        .collect::<BTreeMap<_, _>>();
    let mut keys = plan
        .iter()
        .filter(|k| {
            map.get(k.as_str())
                .is_some_and(|v| v.get("included").and_then(Value::as_bool) == Some(true))
                && !published.contains(*k)
                && !skipped.contains(*k)
        })
        .cloned()
        .collect::<Vec<_>>();
    keys.sort_by_key(|k| pos.get(k.as_str()).copied().unwrap_or(pos.len()));
    Ok(keys)
}
