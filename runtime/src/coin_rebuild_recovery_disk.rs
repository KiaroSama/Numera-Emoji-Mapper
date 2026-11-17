//! Locked recovery downloads proof only; it never starts another upload or deletes a pack.
use crate::{
    coin_rebuild_map_disk::Files,
    coin_rebuild_recovery::RecoveryIo,
    telegram::{SetState, Telegram},
};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(crate) struct Disk<'a> {
    pub(crate) files: &'a Files,
    pub(crate) tg: &'a Telegram,
}
impl Disk<'_> {
    pub(crate) fn source(&self, key: &str) -> Result<PathBuf, String> {
        if key.is_empty()
            || key.contains("..")
            || !key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
        {
            return Err("rebuild recovery source name is unsafe".into());
        }
        let path = self.files.images.join(format!("{key}.png"));
        crate::coin_provider_stage::physical(&path)?;
        if !path.is_file() {
            return Err("rebuild recovery source is unavailable".into());
        }
        Ok(path)
    }
    fn bytes(&self, sticker: &Value) -> Result<Vec<u8>, String> {
        let id = sticker["file_id"]
            .as_str()
            .ok_or("rebuild recovery live file ID is unavailable")?;
        self.tg.download_bytes(id, 5).map_err(|e| e.command_error())
    }
}
impl RecoveryIo for Disk<'_> {
    fn source_exists(&mut self, key: &str) -> bool {
        self.source(key).is_ok()
    }
    fn probe(&mut self, name: &str) -> Result<(SetState, Vec<Value>), String> {
        let (status, pack) = self.tg.probe(name);
        match status {
            SetState::Exists => match pack.and_then(|p| p["stickers"].as_array().cloned()) {
                Some(stickers) => Ok((status, stickers)),
                None => Ok((SetState::Unknown, Vec::new())),
            },
            _ => Ok((status, Vec::new())),
        }
    }
    fn compare(&mut self, sticker: &Value, key: &str) -> Result<Option<bool>, String> {
        let bytes = match self.bytes(sticker) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(None),
        };
        let scratch = self.files.root.join("logs/tmp");
        crate::coin_manifest::output_path(&scratch)?;
        let output = scratch.join(format!(
            "rebuild-proof-{}-{}.png",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let staged = crate::codec_video_encode::Staged::new(&output, "png")?;
        let result = (|| {
            crate::atomic::write_bytes(&staged.0, &bytes)
                .map_err(|_| "cannot stage rebuild recovery proof")?;
            Ok(crate::codec_image::same_image(
                &self.source(key)?,
                &staged.0,
            ))
        })();
        staged.clean()?;
        result
    }
    fn source_hash(&mut self, key: &str) -> Result<u64, String> {
        crate::codec_image::source_dhash(&self.source(key)?)
    }
    fn live_hash(&mut self, sticker: &Value) -> Result<u64, String> {
        crate::codec_image::provider_dhash(&self.bytes(sticker)?)
    }
    fn save(&mut self, state: &Value) -> Result<(), String> {
        crate::atomic::write_json(
            &self.files.directory.join("rebuild_dedup_state.json"),
            state,
        )
        .map_err(|_| "cannot persist rebuild recovery".into())
    }
    fn report(&mut self, text: &str) {
        println!("{}", crate::logging::redact(text, &[]));
    }
}
pub fn run(files: &Files, title: &str, tg: &Telegram) -> Result<(Value, u64), String> {
    for path in [&files.root, &files.directory, &files.images] {
        crate::coin_provider_stage::physical(path)?;
        if !path.is_dir() {
            return Err("rebuild recovery paths must be physical directories".into());
        }
    }
    if files.base.is_empty()
        || !files
            .base
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err("rebuild recovery requires a valid pack base".into());
    }
    crate::coin_manifest::output_path(&files.directory.join("rebuild_dedup_state.json"))?;
    let family = crate::locks::pack_family_path(&files.root, &files.base);
    crate::coin_manifest::output_path(&family)?;
    let _family = crate::locks::Lease::acquire(&family, &crate::logging::utc())
        .map_err(|_| "rebuild recovery pack family is busy or unavailable")?;
    let plan = crate::coin_plan_disk::load(&files.directory)?;
    let plan = crate::coin_plan_disk::validate(&plan)?;
    let mut state = Value::Object(crate::coin_provider_recover::object(
        &files.directory.join("rebuild_dedup_state.json"),
        false,
    )?);
    let problem = crate::coin_state::problem(&state, Some(plan));
    if !problem.is_empty() {
        return Err(format!("rebuild recovery state is inconsistent: {problem}"));
    }
    let mut io = Disk { files, tg };
    let mut total = 0u64;
    for set in state["sets"]
        .as_array_mut()
        .ok_or("rebuild recovery has no sets")?
    {
        let name = set["name"]
            .as_str()
            .ok_or("rebuild recovery set has no name")?;
        let (status, stickers) = io.probe(name)?;
        if status != SetState::Exists {
            return Err("rebuild recovery live count is unknown; refusing guessed resume".into());
        }
        total = total
            .checked_add(stickers.len() as u64)
            .ok_or("rebuild recovery live total overflow")?;
        set["live"] = serde_json::json!(stickers.len());
    }
    let total = crate::coin_rebuild_recovery::recover(&mut state, total, title, &mut io)?;
    Ok((state, total))
}
