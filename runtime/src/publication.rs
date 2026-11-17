//! Native publishing effects, proven by media identity before durable attribution.
use crate::{
    catalog::Catalog,
    logging,
    media_adapter::{MediaAdapter, MediaCodec},
    telegram::{ApiError, SetState, Telegram},
};
use serde_json::{Value, json};

use std::path::{Path, PathBuf};
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str, String> {
    v.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {k}"))
}
fn list(v: &Value, k: &str) -> Result<Vec<String>, String> {
    v.get(k)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing {k}"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("invalid {k}"))
        })
        .collect()
}

pub(crate) struct Failure {
    pub code: u8,
    pub message: String,
}
impl Failure {
    pub(crate) fn unknown(message: impl Into<String>) -> Self {
        Self {
            code: 3,
            message: message.into(),
        }
    }
}
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self { code: 4, message }
    }
}
impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
impl From<ApiError> for Failure {
    fn from(error: ApiError) -> Self {
        let code = match &error {
            ApiError::Transport(_) | ApiError::Ambiguous(_) | ApiError::FloodWait { .. } => 3,
            ApiError::Rejected(_) | ApiError::Unproven(_) | ApiError::Invalid(_) => 4,
        };
        Self {
            code,
            message: error.command_error(),
        }
    }
}

pub struct Publisher<'a, M = MediaAdapter> {
    pub tg: &'a Telegram,
    pub cat: &'a mut Catalog,
    pub media: &'a M,
    pub data: PathBuf,
    pub runtime: &'a tokio::runtime::Runtime,
    pub config: Option<&'a crate::config::Config>,
}
impl<M: MediaCodec> Publisher<'_, M> {
    fn live(&self, name: &str) -> Result<Vec<Value>, Failure> {
        match self.tg.probe(name) {
            (SetState::Exists, Some(value)) => value
                .get("stickers")
                .and_then(Value::as_array)
                .cloned()
                .ok_or("live set has no stickers".into()),
            (SetState::Missing, _) => {
                Err("recorded set no longer exists; refusing to recreate it".into())
            }
            _ => Err(Failure::unknown("live state unknown; refusing mutation")),
        }
    }
    pub fn same(&self, sticker: &Value, source: &Path, fmt: &str) -> Result<Option<bool>, String> {
        self.effects().same(sticker, source, fmt)
    }
    fn effects(&self) -> crate::upload_effect::UploadEffect<'_, M> {
        crate::upload_effect::UploadEffect {
            tg: self.tg,
            media: self.media,
            data: &self.data,
            runtime: self.runtime,
        }
    }
    pub fn resolve(&mut self, sticker: &Value) -> Result<Option<String>, String> {
        let fuid = sticker
            .get("file_unique_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !fuid.is_empty() {
            let known = self
                .cat
                .dispatch(json!({"operation":"seen","file_unique_id":fuid}))?;
            if let Some(k) = known.as_str()
                && self.cat.get(k)?.is_some()
            {
                return Ok(Some(k.into()));
            }
        }
        let fmt = if sticker.get("is_video") == Some(&Value::Bool(true)) {
            "video"
        } else if sticker.get("is_animated") == Some(&Value::Bool(true)) {
            "animated"
        } else {
            "static"
        };
        // A single download is evidence for every nominated candidate, not a
        // fresh network read for each possible owner.
        let bytes = self
            .tg
            .download_bytes(text(sticker, "file_id")?, 5)
            .map_err(|e| e.to_string())?;
        let scratch = crate::publication_download::Download::new(&self.data, "resolve", &bytes)?;
        let media = self.media.with_scratch(scratch.codec());
        let resolved = (|| {
            let fingerprint = self.runtime.block_on(media.call(json!({
                "operation":"fingerprint", "source":scratch.path, "format":fmt
            })))?;
            let key = text(&fingerprint, "content_key")?.to_owned();
            let key = if fmt != "video" && self.cat.get(&key)?.is_some() {
                key
            } else {
                let items = self.cat.all(Some(fmt))?;
                let result = self.runtime.block_on(crate::reconcile::near_catalog(
                    &items,
                    fmt,
                    fingerprint["phash"].as_u64(),
                    &key,
                    async |item: &Value| {
                        let source = PathBuf::from(text(item, "file_path")?);
                        if !source.is_file() {
                            return Ok(None);
                        }
                        let result = media
                            .call(json!({"operation":"same_image", "source":source,
                        "other":scratch.path, "format":fmt}))
                            .await;
                        Ok(result.ok().and_then(|v| v.as_bool()))
                    },
                ))?;
                match result {
                    crate::reconcile::NearMatch::Missing => return Ok(None),
                    crate::reconcile::NearMatch::Unique(key) => key,
                    crate::reconcile::NearMatch::Ambiguous => return Err(
                        "live identity matches more than one catalog item; refusing attribution"
                            .into(),
                    ),
                    crate::reconcile::NearMatch::Undecidable => {
                        return Err(
                            "live identity has an unexamined catalog rival; refusing attribution"
                                .into(),
                        );
                    }
                }
            };
            Ok(Some(key))
        })();
        let key = scratch.finish(resolved)?;
        if let Some(key) = &key
            && !fuid.is_empty()
        {
            self.cat.record_seen(fuid, key)?;
        }
        Ok(key)
    }
    pub fn reconcile(&mut self, set: &mut Value, base: &str) -> Result<(), String> {
        self.reconcile_checked(set, base)
            .map_err(|failure| failure.message)
    }
    pub(crate) fn reconcile_checked(&mut self, set: &mut Value, base: &str) -> Result<(), Failure> {
        let name = text(set, "name")?.to_owned();
        let live = self.live(&name)?;
        let mut keys = list(set, "keys")?;
        let offset = usize::from(set["logo"].as_bool().unwrap_or(false));
        if live.len() < offset + keys.len() {
            return Err("live set is smaller than recorded membership".into());
        }
        for (i, key) in keys.iter().enumerate() {
            let sticker = &live[i + offset];
            let cid = sticker["custom_emoji_id"].as_str().unwrap_or("");
            let known = self
                .cat
                .dispatch(json!({"operation":"custom_id","base":base,"content_key":key}))?;
            if cid.is_empty() || known.as_str() != Some(cid) {
                let position = i + offset;
                let resolved = self.resolve(sticker).map_err(|error| {
                    format!("live position {position} could not be examined ({error}); recorded key {key} remains unproven")
                })?;
                if resolved.as_deref() != Some(key) {
                    return Err(format!(
                        "recorded key {key} no longer holds its live position {position}"
                    )
                    .into());
                }
            }
        }
        let start = offset + keys.len();
        for (i, sticker) in live.iter().enumerate().skip(start) {
            let resolved = self.resolve(sticker).map_err(|error| {
                format!("live position {i} could not be examined ({error}); refusing attribution")
            })?;
            let Some(key) = resolved else {
                for (position, other) in live.iter().enumerate().skip(i + 1) {
                    let resolved = self.resolve(other).map_err(|error| {
                        format!("live position {i} is unrecognized and position {position} behind it could not be examined ({error})")
                    })?;
                    if let Some(key) = resolved {
                        return Err(format!(
                            "foreign sticker at position {i} precedes unrecorded catalog item {key} at position {position}; refusing positional attribution"
                        ).into());
                    }
                }
                break;
            };
            if keys.contains(&key) {
                return Err("live set records an emoji twice".into());
            }
            self.cat.uploaded(&json!({"content_key":key,"custom_emoji_id":sticker.get("custom_emoji_id"),"base":base,"set_name":name}),&logging::utc())?;
            if let Some(fuid) = sticker["file_unique_id"].as_str() {
                self.cat.record_seen(fuid, &key)?;
            }
            keys.push(key);
        }
        // The whole prefix was identity-validated above before assigning any IDs.
        for (i, key) in keys.iter().enumerate() {
            let sticker = &live[i + offset];
            let cid = sticker["custom_emoji_id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("verified live sticker has no custom_emoji_id")?;
            self.cat.uploaded(
                &json!({"content_key":key,"custom_emoji_id":cid,
                "base":base,"set_name":name}),
                &logging::utc(),
            )?;
            if let Some(fuid) = sticker["file_unique_id"].as_str() {
                self.cat.record_seen(fuid, key)?;
            }
        }
        set["keys"] = json!(keys);
        set["live"] = json!(live.len());
        Ok(())
    }
    pub fn mutate(
        &self,
        item: &Value,
        name: &str,
        title: Option<&str>,
        user_id: u64,
        repaint: bool,
    ) -> Result<Value, ApiError> {
        self.effects().mutate(item, name, title, user_id, repaint)
    }
}
