//! Physical replacement recovery keeps copied source bytes until map persistence and intent clear.
use crate::coin_replace_recovery::RecoveryIo;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Files {
    pub root: PathBuf,
    pub map: PathBuf,
    pub state: PathBuf,
    pub images: PathBuf,
    pub intent: PathBuf,
    pub base: String,
}
fn target(path: &Path) -> Result<String, String> {
    crate::coin_provider_stage::physical(path)?;
    let path = path
        .canonicalize()
        .map_err(|_| "cannot resolve replacement target")?;
    let text = path.to_str().ok_or("replacement target is not UTF-8")?;
    Ok(if cfg!(windows) {
        text.to_lowercase()
    } else {
        text.into()
    })
}
struct Disk<'a> {
    files: &'a Files,
    tg: &'a crate::telegram::Telegram,
}
impl Disk<'_> {
    fn staged(&self, intent: &Value) -> Result<PathBuf, String> {
        let stage = PathBuf::from(intent["stage"].as_str().ok_or("replacement has no stage")?);
        crate::coin_provider_stage::physical(&stage)?;
        let incoming = self
            .files
            .images
            .parent()
            .ok_or("replacement images have no parent")?
            .join(".incoming");
        crate::coin_provider_stage::physical(&incoming)?;
        if stage
            .parent()
            .ok_or("replacement stage has no parent")?
            .canonicalize()
            .map_err(|_| "cannot resolve replacement stage parent")?
            != incoming
                .canonicalize()
                .map_err(|_| "cannot resolve replacement incoming directory")?
        {
            return Err("replacement stage is outside its image family".into());
        }
        Ok(stage)
    }
    fn promotion(&self, intent: &Value) -> Result<Vec<(PathBuf, PathBuf)>, String> {
        let stage = self.staged(intent)?;
        let key = intent["key"]
            .as_str()
            .filter(|key| {
                !key.is_empty()
                    && key.bytes().all(|c| {
                        c.is_ascii_lowercase()
                            || c.is_ascii_digit()
                            || matches!(c, b'_' | b'-' | b'.')
                    })
                    && !key.contains("..")
            })
            .ok_or("replacement ticker is not a safe filename")?;
        let images = &self.files.images;
        let raw = images
            .parent()
            .ok_or("replacement images have no parent")?
            .join("png");
        let rows = intent["promote"]
            .as_array()
            .ok_or("replacement has no promotion inventory")?;
        if rows.len() != 2 {
            return Err(
                "replacement promotion inventory must contain original and prepared art".into(),
            );
        }
        let expected = [
            (
                stage.join("png").join(format!("{key}.png")),
                raw.join(format!("{key}.png")),
            ),
            (
                stage.join(format!("{key}.png")),
                images.join(format!("{key}.png")),
            ),
        ];
        let mut result = Vec::new();
        for (row, (source, destination)) in rows.iter().zip(expected) {
            if row.as_array().is_none_or(|row| row.len() != 2)
                || row[0].as_str() != source.to_str()
                || row[1].as_str() != destination.to_str()
            {
                return Err("replacement promotion inventory is outside its recorded art".into());
            }
            crate::coin_provider_stage::physical(&source)?;
            crate::coin_manifest::output_path(&destination)?;
            if !source.is_file() {
                return Err("replacement promotion source is missing".into());
            }
            result.push((source, destination));
        }
        let want = intent["source_dhash"]
            .as_u64()
            .ok_or("replacement has no source identity")?;
        if crate::codec_image::source_dhash(&result[1].0)? != want {
            return Err("replacement staged art no longer matches its recorded identity".into());
        }
        Ok(result)
    }
}
impl RecoveryIo for Disk<'_> {
    fn live(&mut self, name: &str) -> Result<Vec<Value>, String> {
        self.tg
            .read_call(
                "getStickerSet",
                &BTreeMap::from([("name".into(), name.into())]),
            )
            .map_err(|e| e.command_error())?["stickers"]
            .as_array()
            .cloned()
            .ok_or_else(|| "replacement live set has no stickers".into())
    }
    fn proof(&mut self, sticker: &Value, hash: u64) -> Result<bool, String> {
        let bytes = self
            .tg
            .download_bytes(
                sticker["file_id"]
                    .as_str()
                    .ok_or("replacement sticker has no file ID")?,
                5,
            )
            .map_err(|e| e.command_error())?;
        Ok((crate::codec_image::provider_dhash(&bytes)? ^ hash).count_ones() <= 8)
    }
    fn promote(&mut self, intent: &Value) -> Result<(), String> {
        let pairs = self.promotion(intent)?;
        // Read both before any writes; missing/mutated sources retain the intent.
        let bytes = pairs
            .iter()
            .map(|(source, _)| std::fs::read(source).map_err(|_| "cannot read replacement source"))
            .collect::<Result<Vec<_>, _>>()?;
        let want = intent["source_dhash"]
            .as_u64()
            .ok_or("replacement has no source identity")?;
        if crate::codec_image::provider_dhash(&bytes[1])? != want {
            return Err("replacement staged bytes changed before promotion".into());
        }
        for ((_, destination), bytes) in pairs.iter().zip(bytes) {
            crate::atomic::write_bytes(destination, &bytes)
                .map_err(|_| "cannot promote proven replacement art")?;
        }
        Ok(())
    }
    fn repoint(&mut self, old: &str, new: &str) -> Result<(), String> {
        let path = self.files.root.join(".locks/canonical_map.lock");
        crate::coin_manifest::output_path(&path)?;
        let _map = crate::locks::Lease::acquire(&path, &crate::logging::utc())
            .map_err(|_| "replacement canonical map is busy or unavailable")?;
        let mut mapping = crate::coin_provider_recover::object(&self.files.map, false)?;
        if mapping.values().any(|value| !value.is_string()) {
            return Err("replacement map ID compatibility requires text".into());
        }
        let mut changed = false;
        for value in mapping.values_mut() {
            if value.as_str() == Some(old) {
                *value = Value::String(new.into());
                changed = true;
            }
        }
        if changed {
            crate::atomic::write_json(&self.files.map, &Value::Object(mapping))
                .map_err(|_| "cannot repoint replacement map")?;
        }
        Ok(())
    }
    fn clear(&mut self) -> Result<(), String> {
        std::fs::remove_file(&self.files.intent)
            .map_err(|_| "cannot clear proven replacement intent".into())
    }
    fn clean(&mut self, intent: &Value) -> Result<(), String> {
        let stage = self.staged(intent)?;
        // Only the inventoried source files may be removed; foreign contents retain the directory.
        let key = intent["key"]
            .as_str()
            .filter(|key| {
                !key.is_empty()
                    && key.bytes().all(|c| {
                        c.is_ascii_lowercase()
                            || c.is_ascii_digit()
                            || matches!(c, b'_' | b'-' | b'.')
                    })
                    && !key.contains("..")
            })
            .ok_or("replacement ticker is not a safe filename")?;
        for source in [
            stage.join(format!("{key}.png")),
            stage.join("png").join(format!("{key}.png")),
        ] {
            if source.exists() {
                crate::coin_provider_stage::physical(&source)?;
                std::fs::remove_file(source).map_err(|_| "cannot clean replacement source")?;
            }
        }
        if stage.join("png").exists() {
            std::fs::remove_dir(stage.join("png"))
                .map_err(|_| "replacement raw stage is not empty; retained")?;
        }
        std::fs::remove_dir(stage).map_err(|_| "replacement stage is not empty; retained".into())
    }
}
pub fn run(files: &Files, tg: &crate::telegram::Telegram) -> Result<bool, String> {
    if files.base.is_empty() {
        return Err("replacement recovery requires explicit pack family".into());
    }
    for path in [&files.root, &files.map, &files.state, &files.images] {
        crate::coin_provider_stage::physical(path)?;
    }
    crate::coin_manifest::output_path(&files.intent)?;
    let family = crate::locks::pack_family_path(&files.root, &files.base);
    crate::coin_manifest::output_path(&family)?;
    let _family = crate::locks::Lease::acquire(&family, &crate::logging::utc())
        .map_err(|_| "replacement family is busy or unavailable")?;
    if !files.intent.exists() {
        return Ok(true);
    }
    let intent = Value::Object(crate::coin_provider_recover::object(&files.intent, false)?);
    let map = target(&files.map)?;
    let state = target(&files.state)?;
    crate::coin_replace_recovery::recover(Some(&intent), &map, &state, &mut Disk { files, tg })
}
