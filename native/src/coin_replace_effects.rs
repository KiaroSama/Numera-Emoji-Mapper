//! One native replacement uses immutable upload bytes, durable intent and verified retry transport.
use crate::coin_replace_disk::{Disk, Files};
use crate::coin_replace_publish::{Mutation, PublishIo};
use crate::coin_replace_recovery::RecoveryIo;
use crate::telegram::{ApiError, SetState, Telegram};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) struct Effect<'a> {
    disk: Disk<'a>,
    user: u64,
    old_file: String,
    keywords: Vec<String>,
}
impl RecoveryIo for Effect<'_> {
    fn live(&mut self, name: &str) -> Result<Vec<Value>, String> {
        self.disk.live(name)
    }
    fn proof(&mut self, sticker: &Value, hash: u64) -> Result<bool, String> {
        self.disk.proof(sticker, hash)
    }
    fn promote(&mut self, intent: &Value) -> Result<(), String> {
        self.disk.promote(intent)
    }
    fn repoint(&mut self, old: &str, new: &str) -> Result<(), String> {
        self.disk.repoint(old, new)
    }
    fn clear(&mut self) -> Result<(), String> {
        self.disk.clear()
    }
    fn clean(&mut self, intent: &Value) -> Result<(), String> {
        self.disk.clean(intent)
    }
}
impl PublishIo for Effect<'_> {
    fn save_intent(&mut self, intent: &Value) -> Result<(), String> {
        if self.disk.files.intent.exists() {
            return Err("unresolved replacement intent refuses another mutation".into());
        }
        crate::atomic::write_json(&self.disk.files.intent, intent)
            .map_err(|_| "cannot save replacement intent".into())
    }
    fn mutate(&mut self, intent: &Value) -> Mutation {
        match mutation(self, intent) {
            Ok(_) => Mutation::Answered,
            Err(ApiError::Rejected(_)) => Mutation::Rejected,
            Err(_) => Mutation::Unknown,
        }
    }
}
fn mutation(effect: &Effect<'_>, intent: &Value) -> crate::telegram::Result<Value> {
    let name = intent["set_name"]
        .as_str()
        .ok_or_else(|| ApiError::Invalid("replacement set name missing".into()))?;
    let old = intent["old_cid"]
        .as_str()
        .ok_or_else(|| ApiError::Invalid("replacement old CID missing".into()))?;
    let source = intent["source"]
        .as_str()
        .ok_or_else(|| ApiError::Invalid("replacement source missing".into()))?;
    let want = intent["source_dhash"]
        .as_u64()
        .ok_or_else(|| ApiError::Invalid("replacement identity missing".into()))?;
    let file = crate::telegram::upload(Path::new(source), "file0")?;
    if crate::codec_image::provider_dhash(&file.bytes).ok() != Some(want) {
        return Err(ApiError::Invalid(
            "replacement upload bytes changed from recorded identity".into(),
        ));
    }
    let input = crate::telegram::input_sticker("static", &["🪙".into()], &effect.keywords)?;
    let data = BTreeMap::from([
        ("user_id".into(), effect.user.to_string()),
        ("name".into(), name.into()),
        ("old_sticker".into(), effect.old_file.clone()),
        ("sticker".into(), input.to_string()),
    ]);
    let check = || -> crate::telegram::Result<Option<bool>> {
        match effect.disk.tg.probe(name) {
            (SetState::Exists, Some(set)) => {
                let Some(stickers) = set["stickers"].as_array() else {
                    return Ok(None);
                };
                let Some(ids) = stickers
                    .iter()
                    .map(|sticker| sticker["custom_emoji_id"].as_str())
                    .collect::<Option<Vec<_>>>()
                else {
                    return Ok(None);
                };
                Ok(Some(!ids.contains(&old)))
            }
            _ => Ok(None),
        }
    };
    effect
        .disk
        .tg
        .call("replaceStickerInSet", &data, &[file], 5, Some(check))
}

pub fn replace(
    files: &Files,
    tg: &Telegram,
    ticker: &str,
    coin_name: &str,
    bytes: &[u8],
    user: u64,
) -> Result<bool, String> {
    if user == 0 || files.base.is_empty() {
        return Err("replacement owner and family must be configured".into());
    }
    if ticker.is_empty()
        || ticker.trim_matches(['.', '_', '-']).is_empty()
        || ticker.contains("..")
        || !ticker.bytes().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'.' | b'_' | b'-')
        })
    {
        return Err("replacement ticker is not a safe filename".into());
    }
    for path in [&files.root, &files.map, &files.state, &files.images] {
        crate::coin_provider_stage::physical(path)?;
    }
    crate::coin_manifest::output_path(&files.intent)?;
    let family = crate::locks::pack_family_path(&files.root, &files.base);
    crate::coin_manifest::output_path(&family)?;
    let _family = crate::locks::Lease::acquire(&family, &crate::logging::utc())
        .map_err(|_| "replacement family is busy or unavailable")?;
    replace_locked(files, tg, ticker, coin_name, bytes, user)
}

pub(crate) fn replace_locked(
    files: &Files,
    tg: &Telegram,
    ticker: &str,
    coin_name: &str,
    bytes: &[u8],
    user: u64,
) -> Result<bool, String> {
    if !crate::coin_replace_disk::recover_locked(files, tg)? {
        return Err("previous replacement remains unresolved".into());
    }
    let map_lock = files.root.join(".locks/canonical_map.lock");
    crate::coin_manifest::output_path(&map_lock)?;
    let _map = crate::locks::Lease::acquire(&map_lock, &crate::logging::utc())
        .map_err(|_| "replacement map is busy or unavailable")?;
    let mapping = crate::coin_provider_recover::object(&files.map, false)?;
    let Some(old) = mapping
        .get(ticker)
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    else {
        return Ok(false);
    };
    let state = crate::coin_provider_recover::object(&files.state, false)?;
    let sets = state["sets"]
        .as_array()
        .ok_or("replacement state has no sets")?;
    let mut location = None;
    for set in sets {
        let name = set["name"]
            .as_str()
            .ok_or("replacement recorded set has no name")?;
        let pack = tg
            .read_call(
                "getStickerSet",
                &BTreeMap::from([("name".into(), name.into())]),
            )
            .map_err(|e| e.command_error())?;
        let stickers = pack["stickers"]
            .as_array()
            .ok_or("replacement live set has no stickers")?;
        let before = stickers
            .iter()
            .map(|sticker| {
                sticker["custom_emoji_id"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or("replacement live ID must be text")
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(position) = before.iter().position(|id| id == old) {
            location = Some((
                name.to_owned(),
                position,
                stickers[position]["file_id"]
                    .as_str()
                    .ok_or("replacement old file ID missing")?
                    .to_owned(),
                before,
            ));
            break;
        }
    }
    let Some((name, position, old_file, before)) = location else {
        return Ok(false);
    };
    // source() owns the filename validator, including an explicit CLI ticker.
    let mut stage = crate::coin_provider_stage::Stage::new(&files.images)?;
    let prepared = stage.source(ticker)?;
    let raw_dir = stage.directory().join("png");
    let raw = raw_dir.join(format!("{ticker}.png"));
    let mut intent = None;
    let result = (|| {
        std::fs::create_dir(&raw_dir)
            .map_err(|_| "cannot create replacement original-art stage")?;
        crate::atomic::write_bytes(&raw, bytes)
            .map_err(|_| "cannot stage replacement original bytes")?;
        // General converter checks blankness after fitting, matching the repair source.
        crate::codec_image::to_static_png(&raw, &prepared)?;
        let hash = crate::codec_image::source_dhash(&prepared)?;
        let value = json!({"key":ticker,"operation":"replace","set_name":name,"set_index":position,"expected_before":before.len(),
            "format":"static","started_utc":crate::logging::utc(),"old_cid":old,"before":before,"source":prepared,"source_dhash":hash,"stage":stage.directory(),
            "promote":[[raw,files.images.parent().ok_or("replacement art has no parent")?.join("png").join(format!("{ticker}.png"))],[prepared,files.images.join(format!("{ticker}.png"))]],
            "map_target":crate::coin_replace_disk::target(&files.map)?,"state_target":crate::coin_replace_disk::target(&files.state)?});
        intent = Some(value.clone());
        crate::coin_replace_publish::replace(
            Some(&value),
            &mut Effect {
                disk: Disk {
                    files,
                    tg,
                    map_locked: true,
                },
                user,
                old_file,
                keywords: vec![ticker.into(), coin_name.to_lowercase()],
            },
        )
    })();
    // Durable pending state, not an in-memory result, decides whether the owned source survives.
    match std::fs::symlink_metadata(&files.intent) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if stage.directory().exists() {
                if let Some(intent) = intent {
                    Disk {
                        files,
                        tg,
                        map_locked: true,
                    }
                    .clean(&intent)?;
                } else {
                    if raw.exists() {
                        std::fs::remove_file(&raw)
                            .map_err(|_| "cannot clean replacement original stage")?;
                    }
                    if raw_dir.exists() {
                        std::fs::remove_dir(&raw_dir)
                            .map_err(|_| "cannot clean replacement original directory")?;
                    }
                    stage.clean(None)?;
                }
            }
        }
        Err(_) => return Err("replacement intent availability unknown; source retained".into()),
    }
    result
}
