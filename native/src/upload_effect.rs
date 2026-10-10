//! Upload/readback effects shared by both builders without imposing catalog storage.
use crate::{
    media_adapter::MediaAdapter,
    telegram::{ApiError, SetState, Telegram, input_sticker, upload, usable_fuids},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key].as_str().ok_or_else(|| format!("missing {key}"))
}
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub struct UploadEffect<'a> {
    pub tg: &'a Telegram,
    pub media: &'a MediaAdapter,
    pub data: &'a Path,
    pub runtime: &'a tokio::runtime::Runtime,
}
impl UploadEffect<'_> {
    pub fn live(&self, name: &str) -> Result<Vec<Value>, String> {
        match self.tg.probe(name) {
            (SetState::Exists, Some(pack)) => pack["stickers"]
                .as_array()
                .cloned()
                .ok_or("live set has no stickers".into()),
            (SetState::Missing, _) => Err("recorded set is missing; refusing recreation".into()),
            _ => Err("live state unknown; refusing mutation".into()),
        }
    }
    pub fn same(&self, sticker: &Value, source: &Path, fmt: &str) -> Result<Option<bool>, String> {
        let bytes = self
            .tg
            .download_bytes(text(sticker, "file_id")?, 5)
            .map_err(|e| e.to_string())?;
        let directory = self.data.join("tmp");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let (mut file, path) = loop {
            let path = directory.join(format!(
                "verify-{}-{}.dl",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => break (file, path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        };
        let scratch = Scratch(path);
        use std::io::Write;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        let result = self.runtime.block_on(self.media.call(
            json!({"operation":"same_image","source":scratch.0,"other":source,"format":fmt}),
        ))?;
        if result.is_null() {
            Ok(None)
        } else {
            result
                .as_bool()
                .map(Some)
                .ok_or("invalid media comparison result".into())
        }
    }
    pub fn mutate(
        &self,
        item: &Value,
        name: &str,
        title: Option<&str>,
        user: u64,
        repaint: bool,
    ) -> Result<Value, ApiError> {
        let path = PathBuf::from(text(item, "file_path").map_err(ApiError::Invalid)?);
        let fmt = text(item, "fmt").map_err(ApiError::Invalid)?;
        let labels = |key: &str| -> Result<Vec<String>, ApiError> {
            item[key]
                .as_array()
                .ok_or_else(|| ApiError::Invalid(format!("missing {key}")))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| ApiError::Invalid(format!("invalid {key}")))
                })
                .collect()
        };
        let input = input_sticker(fmt, &labels("emojis")?, &labels("keywords")?)?;
        let file = upload(&path, "file0")?;
        let before = if title.is_some() {
            vec![]
        } else {
            self.live(name).map_err(ApiError::Ambiguous)?
        };
        let identities = usable_fuids(&before);
        let mut data = BTreeMap::from([
            ("user_id".into(), user.to_string()),
            ("name".into(), name.into()),
        ]);
        let method = if let Some(title) = title {
            data.insert("title".into(), title.into());
            data.insert("sticker_type".into(), "custom_emoji".into());
            data.insert("stickers".into(), json!([input]).to_string());
            if repaint {
                data.insert("needs_repainting".into(), "true".into());
            }
            "createNewStickerSet"
        } else {
            data.insert("sticker".into(), input.to_string());
            "addStickerToSet"
        };
        let applied = || -> crate::telegram::Result<Option<bool>> {
            let (state, pack) = self.tg.probe(name);
            if state == SetState::Unknown {
                return Ok(None);
            }
            if state == SetState::Missing {
                return Ok(Some(false));
            }
            let Some(pack) = pack else { return Ok(None) };
            let Some(live) = pack["stickers"].as_array() else {
                return Ok(None);
            };
            if title.is_some() {
                if live.len() != 1 {
                    return Ok(None);
                }
                return Ok(self.same(&live[0], &path, fmt).unwrap_or(None));
            }
            let (Some(old), Some(now)) = (&identities, usable_fuids(live)) else {
                return Ok(None);
            };
            if old == &now {
                return Ok(Some(false));
            }
            if !old.is_subset(&now) {
                return Ok(None);
            }
            let new = live
                .iter()
                .filter(|s| {
                    s["file_unique_id"]
                        .as_str()
                        .is_some_and(|id| !old.contains(id))
                })
                .collect::<Vec<_>>();
            if new.len() != 1 {
                return Ok(None);
            }
            Ok(self.same(new[0], &path, fmt).unwrap_or(None))
        };
        self.tg.call(method, &data, &[file], 5, Some(applied))?;
        let after = self.live(name).map_err(ApiError::Ambiguous)?;
        let old = before
            .iter()
            .map(crate::reconcile::identity)
            .collect::<BTreeSet<_>>();
        let new = after
            .iter()
            .filter(|s| {
                let id = crate::reconcile::identity(s);
                (!id.0.is_empty() || !id.1.is_empty()) && !old.contains(&id)
            })
            .collect::<Vec<_>>();
        if new.len() != 1 {
            return Err(ApiError::Ambiguous(
                "cannot identify exactly one new sticker".into(),
            ));
        }
        if self.same(new[0], &path, fmt).unwrap_or(None) != Some(true) {
            return Err(ApiError::Ambiguous(
                "new live sticker content is not proven ours".into(),
            ));
        }
        Ok(json!({"sticker":new[0],"live":after.len()}))
    }
}
