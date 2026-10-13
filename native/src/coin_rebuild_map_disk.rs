//! A locked rebuild snapshot spans plan, live content, current aliases and map replacement.
use crate::{coin_rebuild_map::MapIo, telegram::Telegram};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub struct Files {
    pub root: PathBuf,
    pub directory: PathBuf,
    pub images: PathBuf,
    pub base: String,
}
struct Disk<'a> {
    files: &'a Files,
    tg: &'a Telegram,
}
impl MapIo for Disk<'_> {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String> {
        let result = self
            .tg
            .read_call(
                "getStickerSet",
                &BTreeMap::from([("name".into(), name.into())]),
            )
            .map_err(|e| e.command_error())?;
        result["stickers"]
            .as_array()
            .cloned()
            .ok_or_else(|| "rebuild live set has no stickers".into())
    }
    fn identity(
        &mut self,
        live: &[(String, String)],
        order: &[String],
    ) -> Result<Map<String, Value>, String> {
        let mut sources = Vec::new();
        for name in order {
            if name.is_empty()
                || name.contains("..")
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
            {
                return Err("rebuild source name is not a safe filename".into());
            }
            let path = self.files.images.join(format!("{name}.png"));
            crate::coin_provider_stage::physical(&path).map_err(|_| {
                format!(
                    "the source image for {} is gone, so no live sticker can be proven to be it",
                    crate::python_repr::quoted(name)
                )
            })?;
            sources.push((name.clone(), crate::codec_image::source_dhash(&path)?));
        }
        // Values are deferred: every live file is decoded inside the existing one-to-one proof.
        let ids = live
            .iter()
            .map(|(cid, _)| (cid.clone(), 0))
            .collect::<Vec<_>>();
        let mut position = 0;
        crate::coin_map_identity::resolve(
            &sources,
            &ids,
            |_| true,
            |_, _| {
                let file_id = &live
                    .get(position)
                    .ok_or("rebuild proof requested an unavailable live file")?
                    .1;
                position += 1;
                let bytes = self
                    .tg
                    .download_bytes(file_id, 5)
                    .map_err(|e| e.command_error())?;
                crate::codec_image::provider_dhash(&bytes)
            },
        )
    }
    fn current(&mut self) -> Result<Map<String, Value>, String> {
        crate::coin_provider_recover::object(&self.files.directory.join("ticker_to_id.json"), true)
    }
    fn save(&mut self, candidate: bool, value: &Value) -> Result<(), String> {
        let path = self.files.directory.join(if candidate {
            "ticker_to_id.candidate.json"
        } else {
            "ticker_to_id.json"
        });
        crate::atomic::write_json(&path, value).map_err(|_| "cannot save rebuilt coin map".into())
    }
    fn fill(&mut self, mapping: &Map<String, Value>) -> Result<(), String> {
        crate::coin_provider_cli::refill(&self.files.directory, mapping)
    }
    fn report(&mut self, text: &str) {
        println!("{}", crate::logging::redact(text, &[]));
    }
}
pub fn run(files: &Files, tg: &Telegram) -> Result<(), String> {
    for path in [&files.root, &files.directory, &files.images] {
        crate::coin_provider_stage::physical(path)?;
        if !path.is_dir() {
            return Err("rebuild map paths must be physical directories".into());
        }
    }
    if files.base.is_empty()
        || !files
            .base
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err("rebuild map pack base is invalid".into());
    }
    for name in [
        "rebuild_dedup_plan.json",
        "rebuild_dedup_state.json",
        "shared_logo_groups.json",
        "ticker_to_id.json",
        "ticker_to_id.candidate.json",
        "currency-emoji-inventory.md",
        "currency-emoji-inventory.filled.md",
    ] {
        crate::coin_manifest::output_path(&files.directory.join(name))?;
    }
    let family = crate::locks::pack_family_path(&files.root, &files.base);
    let canonical = files.root.join(".locks/canonical_map.lock");
    crate::coin_manifest::output_path(&family)?;
    crate::coin_manifest::output_path(&canonical)?;
    let _family = crate::locks::Lease::acquire(&family, &crate::logging::utc())
        .map_err(|_| "rebuild pack family is busy or unavailable")?;
    let _map = crate::locks::Lease::acquire(&canonical, &crate::logging::utc())
        .map_err(|_| "rebuild canonical map is busy or unavailable")?;
    let plan = crate::coin_plan_disk::load(&files.directory)?;
    let plan = crate::coin_plan_disk::validate(&plan)?;
    let state = Value::Object(crate::coin_provider_recover::object(
        &files.directory.join("rebuild_dedup_state.json"),
        false,
    )?);
    let problem = crate::coin_state::problem(&state, Some(plan));
    if !problem.is_empty() {
        return Err(format!("rebuild resume state is inconsistent: {problem}"));
    }
    let approved =
        match crate::coin_plan_disk::document(&files.directory.join("shared_logo_groups.json")) {
            Ok(Value::Object(groups)) => groups
                .values()
                .map(|v| {
                    v.as_array()
                        .ok_or("reviewed shared group must be a list")?
                        .iter()
                        .map(|t| {
                            t.as_str()
                                .map(str::to_owned)
                                .ok_or("reviewed shared ticker must be a string")
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        };
    crate::coin_rebuild_map::run(plan, &state, &approved, &mut Disk { files, tg })
}
