//! Identity repair is read-only unless explicit apply reaches a verified migration/restore engine.
use crate::{config::Config, logging::RunLog, media_adapter::MediaAdapter};
use std::collections::BTreeMap;
use std::path::PathBuf;
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Usage: numera-emoji identity-repair [report|migrate-video-keys|restore] [--data-dir DIR] [--apply] [--from-backup DB] [--bundle JSON]"
        );
        return Ok(0);
    }
    let (mut command, mut data, mut apply, mut backup, mut bundle) = (
        "report".to_owned(),
        "collection".to_owned(),
        false,
        None,
        None,
    );
    let mut i = 0;
    let mut command_seen = false;
    while i < args.len() {
        match args[i].as_str() {
            "report" | "migrate-video-keys" | "restore" => {
                if command_seen {
                    return Err("only one identity-repair command is accepted".into());
                }
                command = args[i].clone();
                command_seen = true;
            }
            "--apply" => apply = true,
            "--data-dir" | "--from-backup" | "--bundle" => {
                let key = &args[i];
                i += 1;
                let value = args.get(i).ok_or("identity option requires value")?.clone();
                match key.as_str() {
                    "--data-dir" => data = value,
                    "--from-backup" => backup = Some(value),
                    _ => bundle = Some(value),
                }
            }
            value => return Err(format!("unknown identity-repair option: {value}")),
        }
        i += 1;
    }
    let data = config.path(&data);
    if !data.join("catalog.db").is_file() {
        println!("no catalog at {}", data.join("catalog.db").display());
        return Ok(2);
    }
    if command == "restore" {
        if bundle.is_none() {
            println!("restore requires --bundle PATH.rollback.json");
            return Ok(2);
        }
        let manifest = PathBuf::from(bundle.ok_or("restore bundle missing")?);
        let manifest = if manifest.is_absolute() {
            manifest
        } else {
            data.join(manifest)
        };
        return match crate::migration_restore::restore(&data, &config.root, &manifest, apply) {
            Ok(()) => {
                if apply {
                    println!("Restored and verified the complete snapshot.");
                    Ok(0)
                } else {
                    println!("Verified rollback bundle. Re-run with --apply to restore.");
                    Ok(3)
                }
            }
            Err(error) => {
                log.event("ERROR", "identity", &error);
                Ok(4)
            }
        };
    }
    let recovery_backup = backup.map(PathBuf::from).map(|path| {
        if path.is_absolute() {
            path
        } else {
            data.join(path)
        }
    });
    let python = std::env::var_os("PYO3_PYTHON")
        .filter(|_| std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            config.root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    let media = MediaAdapter {
        python,
        root: config.root.clone(),
        scratch: data.join("tmp/identity-codec"),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    if command == "migrate-video-keys" && apply {
        if let Some(backup) = recovery_backup.as_deref()
            && !backup.is_file()
            && !data.join("identity-migration.journal.json").exists()
        {
            println!("no backup at {}", backup.display());
            return Ok(2);
        }
        return match crate::migration_apply::apply(
            config,
            &data,
            &media,
            &runtime,
            recovery_backup.as_deref(),
        ) {
            Ok(result) => {
                println!("{result}");
                Ok(0)
            }
            Err(error) => {
                log.event("ERROR", "identity", &error);
                Ok(4)
            }
        };
    }
    let _ownership = match crate::ownership::Ownership::acquire(
        &data,
        crate::ownership::Mode::Maintenance,
        &crate::logging::iso_utc(),
    ) {
        Ok(owner) => owner,
        Err(error) => {
            log.event("ERROR", "identity", &error);
            return Ok(4);
        }
    };
    let journal = match crate::migration_apply::journal(&data) {
        Ok(journal) => journal,
        Err(error) => {
            log.event("ERROR", "identity", &error);
            return Ok(4);
        }
    };
    let inspected = (|| -> Result<u8, String> {
        let recovered = if command == "migrate-video-keys" {
            if let Some(backup) = recovery_backup.as_deref() {
                if !backup.is_file() {
                    println!("no backup at {}", backup.display());
                    return Ok(2);
                }
                let map = crate::migration_provenance::recover(
                    &data,
                    &config.root,
                    backup,
                    &media,
                    &runtime,
                )?;
                println!(
                    "recovered {} old->new pair(s) from {}",
                    map.len(),
                    backup.file_name().unwrap_or_default().to_string_lossy()
                );
                map
            } else {
                BTreeMap::new()
            }
        } else {
            BTreeMap::new()
        };
        let survey = crate::identity_survey::survey(&data, &config.root, &media, &runtime)?;
        let stale = crate::identity_references::stale(&data, false)?;
        if command == "migrate-video-keys"
            && survey.complete()
            && !survey.pending()
            && recovered.is_empty()
            && journal.is_none()
            && crate::identity_references::stale(&data, true)?.is_empty()
        {
            let issues = crate::identity_invariants::issues(&data, &config.root, &media, &runtime)?;
            if !issues.is_empty() {
                println!("INCOMPLETE: {}", issues.join("; "));
                return Ok(4);
            }
            println!("Nothing to do: identities, hashes and required references are consistent.");
            if !stale.is_empty() {
                println!("Obsolete plan/skipped-history entries are retained unchanged.");
            }
            return Ok(0);
        }
        println!(
            "catalog: {}\n\nVIDEO IDENTITY: {} row(s) checked\n  keys that would change : {}\n  hash-only corrections  : {}\n  already correct        : {}\n  media missing on disk  : {}\n  undecodable            : {}\n  collisions             : {}",
            data.join("catalog.db").display(),
            survey.checked,
            survey.changed.len(),
            survey.hashes.len(),
            survey.unchanged,
            survey.missing.len(),
            survey.undecodable.len(),
            survey.collisions.len()
        );
        for (old, new, name) in survey.changed.iter().take(10) {
            println!("    {old} -> {new}   ({name})");
        }
        if survey.changed.len() > 10 {
            println!("    ... and {} more", survey.changed.len() - 10);
        }
        for line in &survey.missing {
            println!("    MISSING {line}");
        }
        for line in &survey.undecodable {
            println!("    UNDECODABLE {line}");
        }
        for (new, old) in &survey.collisions {
            println!("    COLLISION {new} <- {}", old.join(", "));
        }
        println!("\n  state/plan files naming unknown keys: {}", stale.len());
        for (name, keys) in &stale {
            println!(
                "    {name}: {} stale key(s), e.g. {}",
                keys.len(),
                keys.first().ok_or("empty stale key evidence")?
            );
        }
        let db = crate::identity_survey::read(&data)?;
        let mut statement=db.prepare("SELECT i.content_key,i.format,i.phash,(SELECT COUNT(*) FROM seen_files s WHERE s.content_key=i.content_key) FROM items i WHERE i.phash IS NOT NULL").map_err(|e|e.to_string())?;
        let rows = statement
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)? as u64,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .map(|r| r.map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        let suspects = rows
            .iter()
            .filter(|r| r.3 > 0)
            .filter_map(|row| {
                let count = rows
                    .iter()
                    .filter(|r| r.0 != row.0 && r.1 == row.1 && (r.2 ^ row.2).count_ones() <= 2)
                    .count();
                (count > 0).then_some((row.0.clone(), count))
            })
            .collect::<BTreeMap<_, _>>();
        println!("\nSUSPECT MAPPINGS: {} (heuristic only)", suspects.len());
        println!(
            "  Rows with a recorded Telegram id AND a look-alike of the same\n  format. The old recovery rule compared grayscale structure alone,\n  so these are the rows it COULD have confused. This is NOT a\n  validation of historical id mappings, and zero here would not\n  prove none was ever wrong: the handle a past download was\n  attributed by is not stored, so it cannot be re-checked."
        );
        for (key, count) in suspects.iter().take(10) {
            println!("    {key}  ({count} look-alike(s))");
        }
        if suspects.len() > 10 {
            println!("    ... and {} more", suspects.len() - 10);
        }
        drop(statement);
        drop(db);
        log.event("INFO", "identity", "read-only survey complete");
        if !survey.complete() || !survey.collisions.is_empty() {
            return Ok(4);
        }
        if survey.pending()
            || !recovered.is_empty()
            || journal.is_some()
            || !crate::identity_references::stale(&data, true)?.is_empty()
        {
            println!("Pending: numera-emoji identity-repair migrate-video-keys --apply");
            return Ok(3);
        }
        let issues = crate::identity_invariants::issues(&data, &config.root, &media, &runtime)?;
        if !issues.is_empty() {
            println!("INCOMPLETE: {}", issues.join("; "));
            return Ok(4);
        }
        println!("\nClean: identities, media and all required references agree.");
        if !stale.is_empty() {
            println!(
                "Obsolete frozen-plan/skipped-history entries are retained; they are not live references."
            );
        }
        Ok(0)
    })();
    match inspected {
        Ok(code) => Ok(code),
        Err(error) => {
            log.event("ERROR", "identity", &error);
            Ok(4)
        }
    }
}
