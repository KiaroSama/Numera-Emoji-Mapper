//! Durable provider recovery reuses native transport, family/map leases and exact-source promotion.
use crate::{
    coin_provider_proof::{RecoveryIo, recover},
    coin_provider_stage::Stage,
    telegram::{SetState, Telegram},
};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub struct Files {
    pub root: PathBuf,
    pub state: PathBuf,
    pub mapping: PathBuf,
    pub published: PathBuf,
    pub base: String,
}
pub(crate) struct Disk<'a> {
    pub(crate) files: &'a Files,
    pub(crate) stage: &'a Stage,
    pub(crate) tg: &'a Telegram,
}
impl RecoveryIo for Disk<'_> {
    fn fallback_hash(&mut self, intent: &Value) -> Result<u64, String> {
        crate::codec_image::source_dhash(&self.stage.for_intent(intent)?)
    }
    fn landed(&mut self, intent: &Value, want: u64) -> Result<Option<String>, String> {
        let name = intent["set_name"]
            .as_str()
            .ok_or("provider intent has no set name")?;
        let (state, live) = self.tg.probe(name);
        let stickers = match state {
            SetState::Exists => live
                .as_ref()
                .and_then(|set| set["stickers"].as_array())
                .ok_or("provider live set has no stickers")?
                .as_slice(),
            _ => &[],
        };
        let before = intent
            .get("expected_before")
            .filter(|v| !v.is_null())
            .map_or(Ok(0), |v| {
                v.as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or("invalid provider expected_before")
            })?;
        crate::coin_provider_proof::landed(state, stickers, before, want, |sticker| {
            let id = sticker["file_id"]
                .as_str()
                .ok_or("live provider sticker has no file ID")?;
            let bytes = self
                .tg
                .download_bytes(id, 5)
                .map_err(|_| "cannot read live provider sticker")?;
            crate::codec_image::provider_dhash(&bytes)
        })
    }
    fn save(&mut self, kind: &str, value: &Value) -> Result<(), String> {
        let path = match kind {
            "map" => &self.files.mapping,
            "state" => &self.files.state,
            _ => return Err("invalid provider persistence kind".into()),
        };
        crate::atomic::write_json(path, value)
            .map_err(|_| "cannot save provider recovery")
            .map_err(str::to_owned)
    }
    fn promote(&mut self, intent: &Value, want: u64) -> Result<bool, String> {
        self.stage.promote(
            &self.stage.for_intent(intent)?,
            intent["key"]
                .as_str()
                .ok_or("provider intent has no ticker")?,
            want,
        )
    }
}
pub(crate) fn object(path: &Path, missing: bool) -> Result<Map<String, Value>, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if missing && e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        _ => return Err("provider durable file is unavailable".into()),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("provider durable file must be regular and nonlinked".into());
    }
    serde_json::from_slice::<Value>(
        &std::fs::read(path).map_err(|_| "cannot read provider durable file")?,
    )
    .map_err(|_| "invalid provider durable JSON")?
    .as_object()
    .cloned()
    .ok_or_else(|| "provider durable document must be an object".into())
}

pub(crate) fn leases(files: &Files) -> Result<(crate::locks::Lease, crate::locks::Lease), String> {
    if files.base.is_empty()
        || [&files.root, &files.state, &files.mapping, &files.published]
            .iter()
            .any(|p| !p.is_absolute())
    {
        return Err("provider recovery requires explicit absolute paths and pack base".into());
    }
    crate::coin_provider_stage::physical(&files.root)?;
    for path in [&files.state, &files.mapping] {
        crate::coin_provider_stage::physical(
            path.parent().ok_or("provider durable file has no parent")?,
        )?;
        if path.exists() {
            crate::coin_provider_stage::physical(path)?;
        }
    }
    let _family = crate::locks::Lease::acquire(
        &crate::locks::pack_family_path(&files.root, &files.base),
        &crate::logging::utc(),
    )
    .map_err(|_| "provider pack family is busy or unavailable")?;
    let _map = crate::locks::Lease::acquire(
        &files.root.join(".locks/canonical_map.lock"),
        &crate::logging::utc(),
    )
    .map_err(|_| "canonical coin map is busy or unavailable")?;
    Ok((_family, _map))
}

pub fn run(files: &Files, tg: &Telegram) -> Result<Option<String>, String> {
    let _leases = leases(files)?;
    // Read after both owning leases, never a snapshot taken before waiting.
    let mut state = Value::Object(object(&files.state, false)?);
    let mut mapping = object(&files.mapping, true)?;
    let mut stage = Stage::new(&files.published)?;
    let result = recover(
        &mut state,
        &mut mapping,
        &mut Disk {
            files,
            stage: &stage,
            tg,
        },
    );
    let keep = state
        .get("provider_in_flight")
        .and_then(|intent| intent.get("source_path"))
        .and_then(Value::as_str)
        .map(PathBuf::from);
    stage.clean(keep.as_deref())?;
    result
}
