//! Read-only coin audit uses shared transport, image identity policy and atomic checkpoints.
use crate::{coin_audit::AuditIo, config::Config, logging::RunLog, telegram::Telegram};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

pub struct Disk<'a> {
    pub tg: &'a Telegram,
    pub cache: &'a Path,
    pub output: &'a Path,
    pub log: &'a RunLog,
}
impl AuditIo for Disk<'_> {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String> {
        self.tg
            .read_call(
                "getStickerSet",
                &BTreeMap::from([("name".into(), name.into())]),
            )
            .map_err(|e| e.command_error())?["stickers"]
            .as_array()
            .cloned()
            .ok_or_else(|| "live coin audit set has no stickers".into())
    }
    fn analyze(&mut self, sticker: &Value) -> Result<(String, bool), String> {
        let bytes = self
            .tg
            .download_bytes(
                sticker["file_id"]
                    .as_str()
                    .ok_or("audit sticker has no file ID")?,
                4,
            )
            .map_err(|e| e.command_error())?;
        crate::codec_image::audit_image(&bytes)
    }
    fn save(&mut self, cache: &Map<String, Value>) -> Result<(), String> {
        crate::atomic::write_json(self.cache, &Value::Object(cache.clone()))
            .map_err(|_| "cannot checkpoint coin audit")?;
        self.log
            .event("INFO", "coins", &format!("audit_cached={}", cache.len()));
        Ok(())
    }
    fn report(&mut self, text: &str) -> Result<(), String> {
        crate::atomic::write_bytes(self.output, self.tg.safe(text).as_bytes())
            .map_err(|_| "cannot save coin audit report")?;
        println!("\n{}", self.tg.safe(text));
        Ok(())
    }
    fn wait(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    let mut directory = config.root.join("coins");
    let mut help = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => help = true,
            "--coin-dir" => {
                i += 1;
                directory = config.path(args.get(i).ok_or("--coin-dir requires a path")?);
            }
            value => return Err(format!("unknown coin-check-packs option: {value}")),
        }
        i += 1;
    }
    if help {
        println!("Usage: numera-emoji coin-check-packs [--coin-dir DIR]");
        return Ok(0);
    }
    let token = config.require("TELEGRAM_BOT_TOKEN")?;
    let base = config.require("COIN_PACK_BASE")?;
    crate::coin_provider_stage::physical(&directory)?;
    let state_path = directory.join("rebuild_dedup_state.json");
    let cache_path = directory.join("pack_audit4.json");
    let output = directory.join("pack_audit4_report.txt");
    crate::coin_provider_stage::physical(&state_path)?;
    crate::coin_manifest::output_path(&cache_path)?;
    crate::coin_manifest::output_path(&output)?;
    let family = crate::locks::pack_family_path(&config.root, base);
    crate::coin_manifest::output_path(&family)?;
    let _family = crate::locks::Lease::acquire(&family, &crate::logging::utc())
        .map_err(|_| "coin audit family is busy or unavailable")?;
    let state = crate::coin_provider_recover::object(&state_path, false)?;
    let sets = state["sets"]
        .as_array()
        .ok_or("coin audit state has no sets")?;
    let mut cache = crate::coin_provider_recover::object(&cache_path, true)?;
    let api = if config.value("NUMERA_EMOJI_MAPPER_NO_DOTENV") == "1" {
        config.require("NUMERA_TEST_API_BASE")?
    } else {
        "https://api.telegram.org"
    };
    let tg = Telegram::new(token.into(), api.into()).map_err(|e| e.command_error())?;
    crate::coin_audit::run(
        sets,
        &mut cache,
        &mut Disk {
            tg: &tg,
            cache: &cache_path,
            output: &output,
            log,
        },
    )
}
