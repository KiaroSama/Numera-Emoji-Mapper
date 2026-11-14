//! Native provider effects share transport and recovery; no Python dispatcher or second catalog.
use crate::{
    coin_provider_proof::RecoveryIo,
    coin_provider_publish::{Mutation, PublishIo, publish},
    coin_provider_recover::{Disk, Files, leases, object},
    coin_provider_stage::Stage,
    telegram::{ApiError, SetState, Telegram, input_sticker, upload, usable_fuids},
};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

impl PublishIo for Disk<'_> {
    fn live_count(&mut self, name: &str) -> Result<usize, String> {
        match self.tg.probe(name) {
            (SetState::Exists, Some(value)) => value["stickers"]
                .as_array()
                .map(Vec::len)
                .ok_or_else(|| "provider live set has no stickers".into()),
            _ => Err("provider existing set is missing or unknown".into()),
        }
    }
    fn source(&mut self, ticker: &str) -> Result<(String, u64), String> {
        let path = self.stage.for_intent(&json!({"key":ticker}))?;
        crate::coin_provider_stage::physical(&path)?;
        let hash = crate::codec_image::source_dhash(&path)?;
        Ok((path.to_string_lossy().into_owned(), hash))
    }
    fn mutate(&mut self, intent: &Value, keywords: &str) -> Mutation {
        match mutate(self, intent, keywords) {
            Ok(_) | Err(ApiError::Ambiguous(_)) => Mutation::Answered,
            Err(ApiError::Rejected(_)) => Mutation::Rejected,
            Err(_) => Mutation::Unknown,
        }
    }
    fn pause(&mut self) -> Result<(), String> {
        std::thread::sleep(Duration::from_millis(300));
        Ok(())
    }
}
fn mutate(disk: &mut Disk<'_>, intent: &Value, keywords: &str) -> crate::telegram::Result<Value> {
    let name = intent["set_name"]
        .as_str()
        .ok_or_else(|| ApiError::Invalid("provider intent has no set name".into()))?;
    let source = intent["source_path"]
        .as_str()
        .ok_or_else(|| ApiError::Invalid("provider intent has no source".into()))?;
    let want = intent["source_dhash"]
        .as_u64()
        .ok_or_else(|| ApiError::Invalid("provider intent has no dHash".into()))?;
    let creating = intent["operation"] == "create";
    let input = input_sticker(
        "static",
        &["🪙".into()],
        &keywords
            .split(',')
            .map(|s| s.trim().to_owned())
            .collect::<Vec<_>>(),
    )?;
    let file = upload(std::path::Path::new(source), "file0")?;
    // User identity is supplied by the caller, never an operator default in source.
    let user = intent["user_id"]
        .as_u64()
        .ok_or_else(|| ApiError::Invalid("provider intent has no owner ID".into()))?;
    let mut data = BTreeMap::from([
        ("name".into(), name.into()),
        ("user_id".into(), user.to_string()),
    ]);
    let old = if creating {
        None
    } else {
        match disk.tg.probe(name) {
            (SetState::Exists, Some(set)) => {
                set["stickers"].as_array().and_then(|v| usable_fuids(v))
            }
            _ => None,
        }
    };
    let method = if creating {
        data.insert(
            "title".into(),
            intent["title"].as_str().unwrap_or("").into(),
        );
        data.insert("sticker_type".into(), "custom_emoji".into());
        data.insert("stickers".into(), json!([input]).to_string());
        "createNewStickerSet"
    } else {
        data.insert("sticker".into(), input.to_string());
        "addStickerToSet"
    };
    let check = || -> crate::telegram::Result<Option<bool>> {
        let (state, live) = disk.tg.probe(name);
        if state == SetState::Unknown {
            return Ok(None);
        }
        if state == SetState::Missing {
            return Ok(if creating { Some(false) } else { None });
        }
        let Some(stickers) = live.as_ref().and_then(|v| v["stickers"].as_array()) else {
            return Ok(None);
        };
        let candidates = if creating {
            if stickers.len() != 1 {
                return Ok(None);
            }
            stickers.iter().collect::<Vec<_>>()
        } else {
            let (Some(old), Some(now)) = (&old, usable_fuids(stickers)) else {
                return Ok(None);
            };
            if !old.is_subset(&now) {
                return Ok(None);
            }
            if old == &now {
                return Ok(Some(false));
            }
            let fresh = stickers
                .iter()
                .filter(|s| {
                    s["file_unique_id"]
                        .as_str()
                        .is_some_and(|id| !old.contains(id))
                })
                .collect::<Vec<_>>();
            if fresh.len() != 1 {
                return Ok(None);
            }
            fresh
        };
        let sticker = candidates[0];
        let Some(id) = sticker["file_id"].as_str() else {
            return Ok(None);
        };
        let Ok(bytes) = disk.tg.download_bytes(id, 5) else {
            return Ok(None);
        };
        let Ok(got) = crate::codec_image::provider_dhash(&bytes) else {
            return Ok(None);
        };
        Ok(Some((want ^ got).count_ones() <= 8))
    };
    disk.tg.call(method, &data, &[file], 5, Some(check))
}

pub fn run(
    files: &Files,
    tg: &Telegram,
    stage: &mut Stage,
    tickers: &[String],
    title: &str,
    user: u64,
    keywords: &Map<String, Value>,
) -> Result<(usize, usize, Map<String, Value>), String> {
    let mut state = Value::Null;
    let mut mapping = Map::new();
    let result = (|| {
        if user == 0 || title.is_empty() {
            return Err("provider owner ID and title must be configured".into());
        }
        crate::collection_names::valid_base(&files.base)?;
        let _leases = leases(files)?;
        state = Value::Object(object(&files.state, false)?);
        mapping = object(&files.mapping, true)?;
        let bot = tg
            .read_call("getMe", &BTreeMap::new())
            .map_err(|e| e.command_error())?["username"]
            .as_str()
            .ok_or("provider bot username missing")?
            .to_owned();
        publish(
            &mut state,
            &mut mapping,
            tickers,
            &files.base,
            title,
            &bot,
            &crate::logging::utc(),
            keywords,
            &mut Owner {
                disk: Disk { files, stage, tg },
                user,
            },
        )
    })();
    let keep = state
        .get("provider_in_flight")
        .and_then(|v| v.get("source_path"))
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from);
    stage.clean(keep.as_deref())?;
    result.map(|(added, failed)| (added, failed, mapping))
}
struct Owner<'a> {
    disk: Disk<'a>,
    user: u64,
}
impl RecoveryIo for Owner<'_> {
    fn fallback_hash(&mut self, v: &Value) -> Result<u64, String> {
        self.disk.fallback_hash(v)
    }
    fn landed(&mut self, v: &Value, w: u64) -> Result<Option<String>, String> {
        self.disk.landed(v, w)
    }
    fn save(&mut self, k: &str, v: &Value) -> Result<(), String> {
        self.disk.save(k, v)
    }
    fn promote(&mut self, v: &Value, w: u64) -> Result<bool, String> {
        self.disk.promote(v, w)
    }
}
impl PublishIo for Owner<'_> {
    fn live_count(&mut self, n: &str) -> Result<usize, String> {
        self.disk.live_count(n)
    }
    fn source(&mut self, t: &str) -> Result<(String, u64), String> {
        self.disk.source(t)
    }
    fn mutate(&mut self, v: &Value, k: &str) -> Mutation {
        let mut v = v.clone();
        v["user_id"] = json!(self.user);
        self.disk.mutate(&v, k)
    }
    fn pause(&mut self) -> Result<(), String> {
        self.disk.pause()
    }
}
