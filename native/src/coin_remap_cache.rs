//! Per-ID signature cache is reconciled against complete live manifests, never set position.
use base64::{Engine, engine::general_purpose::STANDARD};
use pyo3::prelude::*;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::time::Duration;
pub trait CacheIo {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String>;
    fn signature(&mut self, sticker: &Value) -> Result<Vec<u8>, String>;
    fn save(&mut self, cache: &Value) -> Result<(), String>;
    fn wait(&mut self, duration: Duration);
}
pub fn load(raw: &Value) -> Result<Value, String> {
    let raw = raw.as_object().ok_or("remap cache must be an object")?;
    let object = |key: &str| {
        raw.get(key).map_or(Ok(Map::new()), |value| {
            value
                .as_object()
                .cloned()
                .ok_or("remap cache fields must be objects")
        })
    };
    let mut sigs = object("sigs")?;
    let mut sets = object("sets")?;
    let errors = object("errors")?;
    let mut stale = Vec::new();
    for (cid, value) in &sigs {
        let bytes = STANDARD
            .decode(
                value
                    .as_str()
                    .ok_or("remap cache signature must be base64 text")?,
            )
            .map_err(|_| "invalid remap signature base64")?;
        if bytes.len() != 1024 {
            stale.push(cid.clone());
        }
    }
    if !stale.is_empty() {
        for cid in stale {
            sigs.shift_remove(&cid);
        }
        sets.clear();
    }
    Ok(json!({"sets":sets,"sigs":sigs,"errors":errors}))
}
pub fn manifest(stickers: &[Value]) -> Result<String, String> {
    let pairs = stickers
        .iter()
        .map(|sticker| {
            Ok(vec![
                sticker["custom_emoji_id"]
                    .as_str()
                    .ok_or("remap live custom emoji ID must be text")?
                    .to_owned(),
                sticker["file_unique_id"]
                    .as_str()
                    .ok_or("remap live file unique ID must be text")?
                    .to_owned(),
            ])
        })
        .collect::<Result<Vec<_>, String>>()?;
    // External JSON formatting preserves the source's spaced ASCII digest bytes.
    Python::initialize();
    let encoded = Python::attach(|py| {
        py.import("json")?
            .call_method1("dumps", (pairs,))?
            .extract::<String>()
    })
    .map_err(|_| "cannot encode remap manifest digest")?;
    Ok(Sha256::digest(encoded.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()[..32]
        .into())
}
pub fn download<I: CacheIo>(
    sets: &[Value],
    cache: &mut Value,
    io: &mut I,
) -> Result<Vec<String>, String> {
    for key in ["sets", "sigs", "errors"] {
        if !cache[key].is_object() {
            return Err("invalid remap cache fields".into());
        }
    }
    let mut live = BTreeSet::new();
    let mut incomplete = Vec::new();
    for set in sets {
        let name = set["name"]
            .as_str()
            .filter(|name| {
                !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
            })
            .ok_or("unsafe remap set name")?;
        let stickers = io.list(name)?;
        let digest = manifest(&stickers)?;
        let cids = stickers
            .iter()
            .map(|sticker| {
                sticker["custom_emoji_id"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or("missing remap live ID")
            })
            .collect::<Result<Vec<_>, _>>()?;
        live.extend(cids.iter().cloned());
        if cache["sets"][name].as_str() == Some(&digest) {
            continue;
        }
        for (cid, sticker) in cids.iter().zip(&stickers) {
            if cache["sigs"].get(cid).is_some() {
                continue;
            }
            match io.signature(sticker) {
                Ok(bytes) if bytes.len() == 1024 => {
                    cache["sigs"][cid] = json!(STANDARD.encode(bytes));
                    cache["errors"]
                        .as_object_mut()
                        .ok_or("invalid remap error cache")?
                        .shift_remove(cid);
                }
                Ok(_) => {
                    cache["errors"][cid] = json!("invalid remap signature length");
                }
                Err(error) => {
                    cache["errors"][cid] = json!(error);
                }
            }
            io.wait(Duration::from_millis(20));
        }
        if cids.iter().any(|cid| cache["sigs"].get(cid).is_none()) {
            incomplete.push(name.into());
        } else {
            cache["sets"][name] = json!(digest);
        }
        io.save(cache)?;
    }
    let stale = cache["sigs"]
        .as_object()
        .ok_or("invalid remap cache")?
        .keys()
        .filter(|cid| !live.contains(*cid))
        .cloned()
        .collect::<Vec<_>>();
    if !stale.is_empty() {
        for cid in stale {
            cache["sigs"]
                .as_object_mut()
                .ok_or("invalid remap signatures")?
                .shift_remove(&cid);
            cache["errors"]
                .as_object_mut()
                .ok_or("invalid remap errors")?
                .shift_remove(&cid);
        }
        io.save(cache)?;
    }
    Ok(incomplete)
}
