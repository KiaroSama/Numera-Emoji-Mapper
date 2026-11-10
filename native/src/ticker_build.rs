//! Native legacy PNG builder; ticker resume and announcement contracts stay distinct from catalog plans.
use crate::{
    atomic, config::Config, logging::RunLog, media_native_adapter::NativeMediaAdapter,
    telegram::Telegram, ticker_state, upload_effect::UploadEffect,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let codecs = self.0.join("codecs");
        let quiet = !crate::process_owner::cleanup_pending(&codecs)
            // Quiescence alone does not make retained request/response evidence disposable.
            && std::fs::read_dir(&codecs)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or_else(|e| e.kind() == std::io::ErrorKind::NotFound);
        if quiet {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
fn take(args: &[String], i: &mut usize) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or("option requires a value".into())
}
fn notify<M: crate::media_adapter::MediaCodec>(
    publisher: &UploadEffect<'_, M>,
    config: &Config,
    state: &mut Value,
    path: &std::path::Path,
    owner: u64,
    token_key: &str,
    final_run: bool,
) -> Result<(), String> {
    let cap = state["per_set"].as_u64().ok_or("missing pack cap")?;
    let mut sets = state["sets"].as_array().ok_or("invalid sets")?.clone();
    sets.sort_by_key(|s| s["index"].as_u64());
    let last = sets
        .iter()
        .filter_map(|s| s["index"].as_u64())
        .max()
        .unwrap_or(0);
    for set in sets {
        let name = set["name"].as_str().ok_or("set has no name")?;
        if !final_run
            && set["index"].as_u64() == Some(last)
            && set["count"].as_u64().unwrap_or(0) < cap
        {
            continue;
        }
        if state["sent"]
            .as_array()
            .ok_or("invalid sent")?
            .iter()
            .any(|s| s == name)
        {
            continue;
        }
        match crate::announce::packs(
            config,
            publisher.tg,
            owner,
            std::slice::from_ref(&set),
            if token_key == "TELEGRAM_BOT_TOKEN" {
                "coin"
            } else {
                "general"
            },
            "",
            "cards",
        ) {
            Ok(destination) => {
                let destination = if destination == "the worker" {
                    "the worker"
                } else {
                    "[set]"
                };
                println!("  sent link for {name} to {destination}");
                state["sent"]
                    .as_array_mut()
                    .ok_or("invalid sent")?
                    .push(json!(name));
                atomic::write_json(path, state).map_err(|e| e.to_string())?;
            }
            Err(error) => println!(
                "  notify failed for {name}: {}",
                crate::logging::redact(&error, &config.secret_values())
            ),
        }
    }
    Ok(())
}
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!(
            "Usage: numera-emoji build-pack --base NAME --title TITLE [--source-dir DIR] [--token-env KEY] [--user-id N] [--emoji EMOJI] [--per-set N] [--start N] [--limit N] [--keywords FILE|auto] [--state FILE] [--dry-run]"
        );
        return Ok(0);
    }
    let (
        mut base,
        mut title,
        mut source,
        mut token_key,
        mut emoji,
        mut cap,
        mut limit,
        mut start,
        mut keyword_path,
        mut state_file,
        mut dry_run,
        mut owner,
    ) = (
        String::new(),
        String::new(),
        config.root.join("coins/logos/emoji"),
        "TELEGRAM_BOT_TOKEN".to_owned(),
        "🪙".to_owned(),
        200u64,
        0usize,
        0usize,
        "auto".to_owned(),
        None::<PathBuf>,
        false,
        config.integer("PACK_OWNER_USER_ID", 0, 0, i64::MAX) as u64,
    );
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--base" => base = take(args, &mut i)?,
            "--title" => title = take(args, &mut i)?,
            "--source-dir" => source = take(args, &mut i)?.into(),
            "--token-env" => token_key = take(args, &mut i)?,
            "--emoji" => emoji = take(args, &mut i)?,
            "--keywords" => keyword_path = take(args, &mut i)?,
            "--state" => state_file = Some(take(args, &mut i)?.into()),
            "--per-set" => cap = take(args, &mut i)?.parse().map_err(|_| "invalid cap")?,
            "--limit" => limit = take(args, &mut i)?.parse().map_err(|_| "invalid limit")?,
            "--start" => start = take(args, &mut i)?.parse().map_err(|_| "invalid start")?,
            "--user-id" => owner = take(args, &mut i)?.parse().map_err(|_| "invalid user ID")?,
            "--dry-run" => dry_run = true,
            value => return Err(format!("unknown build-pack option: {value}")),
        }
        i += 1;
    }
    crate::collection_names::valid_base(&base)?;
    if title.is_empty() || !(1..=200).contains(&cap) {
        return Err("title and cap1..200 required".into());
    }
    if !source.is_dir() {
        return Err("source dir not found".into());
    }
    let mut files = std::fs::read_dir(&source)
        .map_err(|e| e.to_string())?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    files.retain(|p| {
        p.is_file()
            && p.extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("png"))
    });
    files.sort_by_key(|p| p.file_name().map(|s| s.to_string_lossy().to_lowercase()));
    let files = files
        .into_iter()
        .skip(start)
        .take(if limit == 0 { usize::MAX } else { limit })
        .map(|p| p.canonicalize().map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if files.is_empty() {
        return Ok(2);
    }
    config.require(&token_key)?;
    if owner == 0 {
        return Err("set PACK_OWNER_USER_ID or --user-id".into());
    }
    let keywords = if keyword_path == "auto" {
        let coin = config.root.join("coins/logos/emoji");
        let configured = config.value("COIN_EMOJI_DIR");
        if source.canonicalize().ok() == coin.canonicalize().ok()
            || !configured.is_empty()
                && source.canonicalize().ok() == PathBuf::from(configured).canonicalize().ok()
        {
            crate::ticker_keywords::load(&config.root.join("coins/keywords.csv"))?
        } else {
            BTreeMap::new()
        }
    } else {
        crate::ticker_keywords::load(std::path::Path::new(&keyword_path))?
    };
    let path = state_file.unwrap_or_else(|| config.root.join(format!("state_{base}.json")));
    if dry_run {
        println!(
            "DRY RUN: token-env={token_key}  owner_user_id=[set]  source={}  images={}  keywords={}",
            source.display(),
            files.len(),
            keywords.len()
        );
        println!(
            "DRY RUN: {} images -> {} set(s) of up to {cap}, named {base}1_by_<bot> ...  state={}",
            files.len(),
            (files.len() as u64).div_ceil(cap),
            path.file_name()
                .ok_or("state has no filename")?
                .to_string_lossy()
        );
        return Ok(0);
    }
    let api = if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
        std::env::var("NUMERA_TEST_API_BASE").unwrap_or_else(|_| "https://api.telegram.org".into())
    } else {
        "https://api.telegram.org".into()
    };
    let tg = Telegram::new(config.require(&token_key)?.into(), api)
        .map_err(|e| e.to_string())?
        .with_log(log);
    let bot = tg
        .read_call("getMe", &BTreeMap::new())
        .map_err(|e| e.to_string())?["username"]
        .as_str()
        .ok_or("getMe has no username")?
        .to_owned();
    let branded = config
        .logo_bots(true)?
        .iter()
        .any(|name| name == &bot.to_lowercase());
    if branded && cap < 2 {
        return Err("branded packs require capacity for logo plus an emoji".into());
    }
    let logo_source = if branded {
        config.logo_path(true)?.filter(|p| p.is_file())
    } else {
        None
    };
    if branded && logo_source.is_none() {
        return Err("required brand logo is missing; no publication attempted".into());
    }
    crate::collection_names::name_length(&base, &bot, "")?;
    let _family = crate::locks::Lease::acquire(
        &config.root.join(".locks").join(format!("pack_{base}.lock")),
        &crate::logging::utc(),
    )
    .map_err(|e| e.to_string())?;
    let loaded = (|| -> Result<Value, String> {
        let mut state = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ticker_state::initial(&base, cap),
            Err(e) => return Err(e.to_string()),
        };
        if !state.is_object() {
            return Err("state is not an object; nothing was published".into());
        }
        if !state["per_set"].is_null() && state["per_set"] != cap {
            return Err("resume cap differs from this run".into());
        }
        state["per_set"] = json!(cap);
        for field in ["done", "sets", "sent"] {
            if state.get(field).is_none() {
                state[field] = json!([]);
            }
        }
        ticker_state::validate(&state, &base, cap)?;
        Ok(state)
    })();
    let mut state = match loaded {
        Ok(state) => state,
        Err(error) => {
            log.event("ERROR", "build-pack", &error);
            return Ok(4);
        }
    };
    let scratch = config
        .root
        .join("logs")
        .join(format!("ticker-codec-{}", std::process::id()));
    std::fs::create_dir(&scratch).map_err(|e| e.to_string())?;
    let _workspace = Scratch(scratch.clone());
    let media = NativeMediaAdapter::for_project(&config.root, scratch.join("codecs"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let logo = if let Some(source) = &logo_source {
        Some(runtime.block_on(crate::brand_logo::prepare(&media, source, &scratch, config))?)
    } else {
        None
    };
    let publisher = UploadEffect {
        tg: &tg,
        media: &media,
        data: &scratch,
        runtime: &runtime,
    };
    let recovery = crate::ticker_recovery::recover_checked(
        &publisher,
        &mut state,
        &files,
        &path,
        &title,
        &base,
        logo.as_ref(),
    )
    .and_then(|()| crate::ticker_recovery::verify_counts_checked(&publisher, &state));
    if let Err(failure) = recovery {
        log.event("ERROR", "build-pack", &failure.message);
        return Ok(failure.code);
    }
    notify(
        &publisher, config, &mut state, &path, owner, &token_key, false,
    )?;
    let mut done = state["done"]
        .as_array()
        .ok_or("invalid done")?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let pending = files
        .iter()
        .filter(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| !done.contains(&s.to_lowercase()))
        })
        .count();
    println!(
        "Bot: @{bot}  owner_user_id=[set]  images={}  already_done={}  pending={pending}",
        files.len(),
        done.len()
    );
    let initial_sets = state["sets"].as_array().ok_or("invalid sets")?.len();
    let mut uploaded = 0;
    let mut failed = 0;
    for source in files {
        let key = source
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("source stem not UTF-8")?
            .to_lowercase();
        if done.contains(&key) {
            continue;
        }
        if source.metadata().map_err(|e| e.to_string())?.len() == 0
            || runtime.block_on(
                media.call(json!({"operation":"usable","source":source,"format":"static"})),
            ) != Ok(json!(true))
        {
            failed += 1;
            continue;
        }
        let active = state["sets"]
            .as_array()
            .ok_or("invalid sets")?
            .last()
            .filter(|s| s["count"].as_u64().unwrap_or(cap) < cap)
            .cloned();
        let mut active = active;
        if active.is_none()
            && let Some(logo) = &logo
        {
            let index = state["sets"].as_array().ok_or("invalid sets")?.len() as u64 + 1;
            let name = format!("{base}{index}_by_{bot}");
            let set_title = format!("{title} {index}");
            state["in_flight"] = ticker_state::intent("__brand_logo__", &name, index, 0);
            state["in_flight"]["title"] = json!(set_title);
            atomic::write_json(&path, &state).map_err(|e| e.to_string())?;
            publisher
                .mutate(logo, &name, Some(&set_title), owner, false)
                .map_err(|e| e.command_error())?;
            let set = json!({"name":name,"title":set_title,"index":index,"count":1});
            state["sets"]
                .as_array_mut()
                .ok_or("invalid sets")?
                .push(set.clone());
            state["in_flight"] = Value::Null;
            atomic::write_json(&path, &state).map_err(|e| e.to_string())?;
            active = Some(set);
        }
        let creating = active.is_none();
        let index = active
            .as_ref()
            .and_then(|s| s["index"].as_u64())
            .unwrap_or(state["sets"].as_array().ok_or("invalid sets")?.len() as u64 + 1);
        let name = active
            .as_ref()
            .and_then(|s| s["name"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{base}{index}_by_{bot}"));
        let before = active
            .as_ref()
            .and_then(|s| s["count"].as_u64())
            .unwrap_or(0);
        let set_title = format!("{title} {index}");
        state["in_flight"] = ticker_state::intent(&key, &name, index, before);
        if creating {
            state["in_flight"]["title"] = json!(set_title);
        }
        atomic::write_json(&path, &state).map_err(|e| e.to_string())?;
        let labels = keywords
            .get(&key)
            .map(String::as_str)
            .unwrap_or(&key)
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        let labels = if labels.is_empty() {
            vec![key.as_str()]
        } else {
            labels
        };
        let item = json!({"file_path":source,"fmt":"static","emojis":[emoji],"keywords":labels});
        // A retried ticker add may follow a foreign arrival. The legacy proof
        // measures the live size; collection publication requires a unique tail.
        let mutation = if creating {
            publisher
                .mutate(&item, &name, Some(&set_title), owner, false)
                .map(|result| result["live"].clone())
        } else {
            crate::telegram_mutation::mutate(
                &tg,
                &json!({"name":name,"source":source,"format":"static","create":false,
                    "user_id":owner,"emojis":[emoji],"keywords":labels,"expected_before":before}),
                |_, sticker, source, fmt| {
                    publisher
                        .same(sticker, std::path::Path::new(source), fmt)
                        .map_err(crate::telegram::ApiError::Ambiguous)
                },
            )
            .map(|live| {
                if live.is_null() {
                    json!(before + 1)
                } else {
                    live
                }
            })
        };
        let result = match mutation {
            Ok(result) => result,
            Err(crate::telegram::ApiError::Ambiguous(error)) if creating => {
                // Only an unanswered create gets this last settle. A name and
                // one sticker alone never prove that this run owns the pack.
                let (known, pack) = tg.probe(&name);
                let stickers = pack.as_ref().and_then(|pack| pack["stickers"].as_array());
                if known == crate::telegram::SetState::Exists
                    && stickers.is_some_and(|stickers| {
                        stickers.len() == 1
                            && publisher.same(&stickers[0], &source, "static").ok()
                                == Some(Some(true))
                    })
                {
                    log.event(
                        "WARNING",
                        "build-pack",
                        "adopted content-proven ambiguous create",
                    );
                    json!(1)
                } else {
                    log.event("ERROR", "build-pack", &error);
                    return Ok(3);
                }
            }
            Err(crate::telegram::ApiError::Rejected(error)) => {
                log.event("WARNING", "build-pack", &error);
                state["in_flight"] = Value::Null;
                atomic::write_json(&path, &state).map_err(|e| e.to_string())?;
                failed += 1;
                continue;
            }
            Err(error) => {
                log.event("ERROR", "build-pack", &error.to_string());
                return Ok(3);
            }
        };
        if creating {
            println!("[set {index}] created {name}");
            state["sets"]
                .as_array_mut()
                .ok_or("invalid sets")?
                .push(json!({"name":name,"title":set_title,"index":index,"count":result}));
        } else {
            let last = state["sets"]
                .as_array_mut()
                .ok_or("invalid sets")?
                .last_mut()
                .ok_or("missing active set")?;
            last["count"] = result.clone();
        }
        done.insert(key);
        state["done"] = json!(done);
        state["in_flight"] = Value::Null;
        atomic::write_json(&path, &state).map_err(|e| e.to_string())?;
        uploaded += 1;
        notify(
            &publisher, config, &mut state, &path, owner, &token_key, false,
        )?;
    }
    notify(
        &publisher, config, &mut state, &path, owner, &token_key, true,
    )?;
    atomic::write_json(&path, &state).map_err(|e| e.to_string())?;
    let sets = state["sets"].as_array().ok_or("invalid sets")?;
    println!(
        "\nDONE. {} total emojis across {} set(s). New sets this run: {}. Uploaded: {uploaded}. Failed/skipped: {failed}.",
        done.len(),
        sets.len(),
        sets.len().saturating_sub(initial_sets)
    );
    for set in sets {
        println!(
            "  https://t.me/addemoji/{}  ({})",
            set["name"].as_str().ok_or("set has no name")?,
            set["count"]
        );
    }
    Ok(if failed == 0 {
        0
    } else if uploaded > 0 {
        3
    } else {
        4
    })
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    use std::path::Path;

    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).expect("clean owned synthetic cleanup fixtures");
        }
    }

    #[test]
    fn cleanup_pending_guard_retains_uncertain_codec_bytes() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let root = parent.join(format!("ticker cleanup guard {}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let fixture = Fixture(root);
        for (case, relative, shape) in [
            ("legacy", "codecs/run/codec-cleanup-pending.json", "file"),
            (
                "native",
                "codecs/run/owner/codec-cleanup-pending.json",
                "file",
            ),
            (
                "marker-directory",
                "codecs/run/codec-cleanup-pending.json",
                "directory",
            ),
            ("owner-file", "codecs/run/owner", "file"),
            ("container-file", "codecs", "file"),
            ("entry-file", "codecs/run", "file"),
            ("settled-request", "codecs/run/request.json", "file"),
            ("settled-response", "codecs/run/response.json", "file"),
            ("empty-envelope", "codecs/run/owner", "directory"),
            ("quiet", "codecs", "directory"),
            ("absent", "", "none"),
            #[cfg(unix)]
            (
                "marker-link",
                "codecs/run/codec-cleanup-pending.json",
                "link",
            ),
            #[cfg(unix)]
            ("owner-link", "codecs/run/owner", "link"),
            #[cfg(unix)]
            ("container-link", "codecs", "link"),
        ] {
            let workspace = fixture.0.join(case);
            std::fs::create_dir(&workspace).unwrap();
            let sentinel = workspace.join("keep.bin");
            let bytes = b"not JSON; unresolved synthetic bytes";
            std::fs::write(&sentinel, bytes).unwrap();
            let target = workspace.join(relative);
            if shape != "none" {
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                match shape {
                    "file" => std::fs::write(&target, bytes).unwrap(),
                    "directory" => std::fs::create_dir(&target).unwrap(),
                    #[cfg(unix)]
                    "link" => std::os::unix::fs::symlink("missing", &target).unwrap(),
                    _ => unreachable!(),
                }
            }
            drop(Scratch(workspace.clone()));
            if matches!(case, "quiet" | "absent") {
                assert!(!workspace.exists(), "ordinary cleanup did not run: {case}");
            } else {
                assert_eq!(std::fs::read(&sentinel).unwrap(), bytes, "{case}");
                match shape {
                    "file" => assert_eq!(std::fs::read(&target).unwrap(), bytes, "{case}"),
                    "directory" => assert!(std::fs::symlink_metadata(&target).unwrap().is_dir()),
                    #[cfg(unix)]
                    "link" => {
                        assert_eq!(std::fs::read_link(&target).unwrap(), Path::new("missing"))
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}
