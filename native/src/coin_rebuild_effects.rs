//! Rebuild effects reuse canonical leases, native recovery and the shared legacy upload proof.
use crate::{
    coin_rebuild_delete::DeleteIo,
    coin_rebuild_map_disk::Files,
    coin_rebuild_recovery::RecoveryIo,
    coin_rebuild_recovery_disk::Disk,
    coin_rebuild_walk::{BuildIo, Mutation, Options},
    config::Config,
    telegram::{ApiError, SetState, Telegram},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;

struct Owner<'a> {
    disk: Disk<'a>,
    config: &'a Config,
    options: &'a Options,
}
impl RecoveryIo for Owner<'_> {
    fn source_exists(&mut self, key: &str) -> bool {
        self.disk.source_exists(key)
    }
    fn probe(&mut self, name: &str) -> Result<(SetState, Vec<Value>), String> {
        self.disk.probe(name)
    }
    fn compare(&mut self, sticker: &Value, key: &str) -> Result<Option<bool>, String> {
        self.disk.compare(sticker, key)
    }
    fn source_hash(&mut self, key: &str) -> Result<u64, String> {
        self.disk.source_hash(key)
    }
    fn live_hash(&mut self, sticker: &Value) -> Result<u64, String> {
        self.disk.live_hash(sticker)
    }
    fn save(&mut self, state: &Value) -> Result<(), String> {
        self.disk.save(state)
    }
    fn report(&mut self, text: &str) {
        self.disk.report(text);
    }
}
impl DeleteIo for Owner<'_> {
    fn delete(&mut self, name: &str) -> Result<(), String> {
        self.disk
            .tg
            .call(
                "deleteStickerSet",
                &BTreeMap::from([("name".into(), name.into())]),
                &[],
                5,
                None::<fn() -> crate::telegram::Result<Option<bool>>>,
            )
            .map(|_| ())
            .map_err(|e| e.command_error())
    }
    fn probe(&mut self, name: &str) -> SetState {
        self.disk.tg.probe(name).0
    }
    fn save(&mut self, state: &Value) -> Result<(), String> {
        self.disk.save(state)
    }
    fn report(&mut self, text: &str) {
        self.disk.report(text);
    }
    fn wait(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}
impl BuildIo for Owner<'_> {
    fn live_count(&mut self, name: &str) -> Result<u64, String> {
        let (status, stickers) = self.disk.probe(name)?;
        match status {
            SetState::Exists => Ok(stickers.len() as u64),
            SetState::Missing => Ok(0),
            SetState::Unknown => Err(format!("live state of {name} is unknown")),
        }
    }
    fn source_status(&mut self, key: &str) -> Result<Option<bool>, String> {
        if key.is_empty()
            || key.contains("..")
            || !key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
        {
            return Err("rebuild source name is unsafe".into());
        }
        let path = self.disk.files.images.join(format!("{key}.png"));
        crate::coin_manifest::output_path(&path)?;
        let metadata = match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("cannot inspect rebuild source".into()),
            Ok(metadata) => metadata,
        };
        if !metadata.is_file() || metadata.len() == 0 {
            return Ok(None);
        }
        crate::coin_provider_stage::physical(&path)?;
        let bytes = std::fs::read(path).map_err(|_| "cannot read rebuild source")?;
        Ok(Some(
            crate::codec_image::audit_image(&bytes).map_or(true, |(_, blank)| blank),
        ))
    }
    fn delete_old(&mut self, state: &mut Value) -> Result<bool, String> {
        let path = self.disk.files.directory.join("rebuild_state.json");
        crate::coin_manifest::output_path(&path)?;
        let names = match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err("cannot inspect old rebuild state".into()),
            Ok(_) => {
                let old = crate::coin_provider_recover::object(&path, false)?;
                let names = old.get("sets").map_or(Ok(Vec::new()), |sets| {
                    sets.as_array()
                        .ok_or("old rebuild sets must be a list")?
                        .iter()
                        .map(|set| {
                            set["name"]
                                .as_str()
                                .map(str::to_owned)
                                .ok_or("old rebuild set has no name")
                        })
                        .collect::<Result<Vec<_>, _>>()
                })?;
                Some(names)
            }
        };
        crate::coin_rebuild_delete::delete(names.as_deref(), state, self)
    }
    fn mutate(&mut self, intent: &Value, keywords: &str) -> Mutation {
        let result = (|| {
            let key = intent["key"]
                .as_str()
                .ok_or_else(|| ApiError::Invalid("rebuild intent has no key".into()))?;
            let path = self.disk.source(key).map_err(ApiError::Invalid)?;
            let creating = intent["operation"] == "create";
            let request = json!({"name":intent["set_name"],"source":path,"format":"static","create":creating,
                "user_id":self.options.owner,"emojis":["🪙"],"keywords":keywords.split(',').collect::<Vec<_>>(),
                "expected_before":intent["expected_before"],"title":format!("{} {}",self.options.title,intent["set_index"])});
            let tg = self.disk.tg;
            crate::telegram_mutation::mutate(tg, &request, |_, sticker, _, _| {
                self.disk.compare(sticker, key).map_err(ApiError::Ambiguous)
            })
        })();
        match result {
            Ok(_) => Mutation::Answered,
            Err(ApiError::Rejected(error) | ApiError::Transport(error)) => {
                Mutation::Rejected(error)
            }
            Err(error) => Mutation::Unknown(error.command_error()),
        }
    }
    fn notify(&mut self, state: &mut Value, name: &str, title: &str) -> Result<(), String> {
        crate::coin_rebuild_links::notify(
            state,
            name,
            || {
                crate::announce::packs(
                    self.config,
                    self.disk.tg,
                    self.options.owner,
                    &[json!({"name":name,"title":title})],
                    "coin",
                    "",
                    "cards",
                )
                .map(|destination| {
                    if destination == "the worker" {
                        destination
                    } else {
                        "[set]".into()
                    }
                })
            },
            |value| {
                crate::atomic::write_json(
                    &self.disk.files.directory.join("rebuild_dedup_state.json"),
                    value,
                )
                .map_err(|_| "cannot persist rebuild recovery".into())
            },
            |text| {
                println!(
                    "{}",
                    crate::logging::redact(text, &self.config.secret_values())
                )
            },
        )
    }
    fn wait(&mut self) {
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn run(config: &Config, files: &Files, options: &Options, tg: &Telegram) -> Result<(), String> {
    for path in [&files.root, &files.directory, &files.images] {
        crate::coin_provider_stage::physical(path)?;
        if !path.is_dir() {
            return Err("rebuild inputs must be physical directories".into());
        }
    }
    if options.owner == 0 || options.base != files.base || options.title.is_empty() {
        return Err("rebuild owner, matching base and title must be configured".into());
    }
    crate::collection_names::name_length(&options.base, &options.bot, "")?;
    let family = crate::locks::pack_family_path(&files.root, &files.base);
    crate::coin_manifest::output_path(&family)?;
    let _family = crate::locks::Lease::acquire(&family, &crate::logging::utc())
        .map_err(|_| "rebuild pack family is busy or unavailable")?;
    let plan_path = files.directory.join("rebuild_dedup_plan.json");
    let plan = match std::fs::symlink_metadata(&plan_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            crate::coin_plan_disk::build(&files.directory, &files.images)?
        }
        Err(_) => return Err("cannot inspect frozen rebuild plan".into()),
        Ok(_) => crate::coin_plan_disk::load(&files.directory)?,
    };
    let plan = crate::coin_plan_disk::validate(&plan)?;
    let state_path = files.directory.join("rebuild_dedup_state.json");
    crate::coin_manifest::output_path(&state_path)?;
    let mut state = match std::fs::symlink_metadata(&state_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            json!({"sets":[],"sent":[],"deleted_old":false,"final_sent":false,"order":[],"cursor":0,"in_flight":null})
        }
        Err(_) => return Err("cannot inspect rebuild resume state".into()),
        Ok(_) => Value::Object(crate::coin_provider_recover::object(&state_path, false)?),
    };
    crate::coin_rebuild_walk::walk(
        plan,
        &mut state,
        options,
        &mut Owner {
            disk: Disk { files, tg },
            config,
            options,
        },
    )
}
