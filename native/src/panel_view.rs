//! The panel's pure card model. Membership is never inferred from list position.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

type Result<T> = std::result::Result<T, String>;
fn string<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {k}"))
}
fn first(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .map(str::to_owned)
}
fn format_order(fmt: &str) -> u8 {
    match fmt {
        "static" => 0,
        "video" => 1,
        "animated" => 2,
        _ => 9,
    }
}

pub fn copy_id(label: &str) -> String {
    let Some(raw) = label.strip_prefix("premium-id:") else {
        return String::new();
    };
    let raw = raw.strip_suffix('\n').unwrap_or(raw);
    if !raw.is_empty() && raw.chars().all(|c| c.is_ascii_digit()) {
        raw.into()
    } else {
        String::new()
    }
}
pub fn similarity(items: &[Value]) -> Result<Vec<Value>> {
    let mut formats = Vec::new();
    for item in items {
        let fmt = string(item, "fmt")?;
        if !formats.contains(&fmt) {
            formats.push(fmt)
        }
    }
    formats.sort_by_key(|f| format_order(f));
    let mut out = Vec::new();
    for fmt in formats {
        let group = items
            .iter()
            .filter(|i| i.get("fmt").and_then(Value::as_str) == Some(fmt))
            .collect::<Vec<_>>();
        let mut hashes = Vec::new();
        let mut hashed = Vec::new();
        let mut plain = Vec::new();
        for item in group {
            match item.get("phash") {
                None | Some(Value::Null) => plain.push(item.clone()),
                Some(h) => {
                    hashes.push(h.as_u64().ok_or("invalid perceptual hash")?);
                    hashed.push(item);
                }
            }
        }
        for i in crate::greedy(&hashes) {
            out.push(hashed[i].clone())
        }
        out.extend(plain);
    }
    Ok(out)
}

pub fn build(request: Value) -> Result<Value> {
    let items = request
        .get("items")
        .and_then(Value::as_array)
        .ok_or("missing items")?;
    let published = request
        .get("published")
        .and_then(Value::as_array)
        .ok_or("missing published")?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or("invalid published key"))
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    let where_sets = request
        .get("published_sets")
        .and_then(Value::as_object)
        .ok_or("missing published_sets")?;
    let keep = request
        .get("keep_sets")
        .and_then(Value::as_object)
        .ok_or("missing keep_sets")?;
    let show = request
        .get("show_published")
        .and_then(Value::as_bool)
        .ok_or("missing show_published")?;
    let branded = request
        .get("branded")
        .and_then(Value::as_bool)
        .ok_or("missing branded")?;
    let logo_path = request
        .get("logo_path")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut view = Vec::new();
    let mut paths = BTreeMap::new();
    let mut hidden = 0;
    if branded && keep.is_empty() {
        view.push(json!({"key":"__brand_logo__","fmt":"static","label":"Brand logo (auto-added on publish)","emoji":"","included":true,"isLogo":true}));
        paths.insert("__brand_logo__".to_owned(), logo_path.to_owned());
    }
    let mut seen = BTreeSet::new();
    for item in items {
        let key = string(item, "content_key")?;
        let set = where_sets.get(key).and_then(Value::as_str);
        let pack = if !show {
            set.and_then(|s| keep.get(s))
        } else {
            None
        };
        if !show && published.contains(key) && pack.is_none() {
            hidden += 1;
            continue;
        }
        let label = first(item, "keywords")
            .or_else(|| first(item, "emojis"))
            .unwrap_or_else(|| key.chars().skip(2).take(8).collect());
        let mut card = json!({"key":key,"fmt":string(item,"fmt")?,"label":label,"copyId":copy_id(&label),
            "emoji":first(item,"emojis").unwrap_or_default(),"included":item.get("included").and_then(Value::as_bool).ok_or("invalid inclusion")?});
        if let Some(n) = pack {
            let n = n.as_u64().filter(|n| *n > 0).ok_or("invalid pack index")?;
            card["pack"] = json!(n);
            if branded && !keep.is_empty() && seen.insert(n) {
                let logo_key = format!("__logo_pack_{n}__");
                view.push(json!({"key":logo_key,"fmt":"static","label":format!("Brand logo (live in pack {n})"),"emoji":"","included":true,"isLogo":true,"pack":n}));
                paths.insert(logo_key, logo_path.to_owned());
            }
        }
        paths.insert(key.to_owned(), string(item, "file_path")?.to_owned());
        view.push(card);
    }
    Ok(json!({"view":view,"by_key":paths,"hidden":hidden}))
}
