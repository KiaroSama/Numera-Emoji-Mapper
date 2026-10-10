//! Offline status cannot migrate a catalog or mutate the user's packs.
use crate::{config::Config, logging::RunLog};
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Usage: numera-emoji status");
        return Ok(0);
    }
    if !args.is_empty() {
        return Err("status takes no options".into());
    }
    let (mut stale, why) = crate::roster_cli::stale(config)?;
    if stale {
        println!("STALE roster: {why}\n      fix: numera-emoji pack-manifest --refresh");
    } else {
        println!("ok    roster: {why}");
    }
    if config.value("EMOJI_ARCHIVE_DIR").is_empty() {
        println!("skip  archive: EMOJI_ARCHIVE_DIR is not set, so it was not checked");
    } else {
        let (archive_stale, reasons) = crate::archive_check::check(config)?;
        if archive_stale {
            stale = true;
            println!(
                "STALE archive: {}\n      fix: numera-emoji pack-archive --sync",
                reasons.join("; ")
            );
        } else {
            println!("ok    archive: every full pack is archived");
        }
    }
    let base = config.value("COLLECTION_PACK_BASE");
    let pending = if base.is_empty() {
        None
    } else {
        crate::catalog_read::pending(&config.root.join("collection/catalog.db"), base).ok()
    };
    match pending {
        None => println!("skip  catalog: no readable catalog or COLLECTION_PACK_BASE unset"),
        Some(0) => println!("ok    catalog: every included emoji is published to {base}"),
        Some(count) => println!(
            "info  catalog: {count} included emoji not yet published to {base}\n      publish: numera-emoji build-collection --base {base} --title ... --dry-run"
        ),
    }
    log.event(
        "INFO",
        "status",
        if stale {
            "offline state is stale"
        } else {
            "offline state is current"
        },
    );
    Ok(if stale { 3 } else { 0 })
}
