//! Archive commands require explicit check/sync/export selection and configured ownership.
use crate::{config::Config, logging::RunLog, telegram::Telegram};
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Usage: numera-emoji pack-archive --check|--sync|--export PACK --zip PATH");
        return Ok(0);
    }
    let (mut check, mut sync, mut export, mut zip) = (false, false, None, None);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--check" => check = true,
            "--sync" => sync = true,
            "--export" | "--zip" => {
                let key = &args[i];
                i += 1;
                let value = args.get(i).ok_or("archive option requires value")?.clone();
                if key == "--export" {
                    export = Some(value);
                } else {
                    zip = Some(value);
                }
            }
            value => return Err(format!("unknown archive option: {value}")),
        }
        i += 1;
    }
    if u8::from(check) + u8::from(sync) + u8::from(export.is_some()) != 1
        || export.is_some() != zip.is_some()
    {
        return Err(
            "give exactly one of --check, --sync or --export; --export and --zip go together"
                .into(),
        );
    }
    if let Err(error) = config.require("COLLECTION_PACK_BASE") {
        log.event("ERROR", "archive", &error);
        return Ok(1);
    }
    if let (Some(pack), Some(zip)) = (export, zip) {
        return crate::pack_export::run(config, &pack, std::path::Path::new(&zip));
    }
    if let Err(error) = config.require("EMOJI_ARCHIVE_DIR") {
        log.event("ERROR", "archive", &error);
        return Ok(1);
    }
    if check {
        let (stale, why) = crate::archive_check::check(config)?;
        println!(
            "{}",
            if stale {
                format!("STALE: {}", why.join("; "))
            } else {
                "fresh: the archive matches the packs".into()
            }
        );
        return Ok(if stale { 3 } else { 0 });
    }
    let api = if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
        std::env::var("NUMERA_TEST_API_BASE").unwrap_or("https://api.telegram.org".into())
    } else {
        "https://api.telegram.org".into()
    };
    let result = (|| {
        let tg = Telegram::new(config.require("GENERAL_BOT_TOKEN")?.into(), api)
            .map_err(|e| e.to_string())?;
        crate::archive_sync::run(config, &tg, log)
    })();
    match result {
        Ok(code) => Ok(code),
        Err(error) => {
            log.event("ERROR", "archive", &error);
            Ok(1)
        }
    }
}
