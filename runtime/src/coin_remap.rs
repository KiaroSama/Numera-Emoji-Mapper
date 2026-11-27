//! Content-only remap joins live signatures, local artwork and guarded atomic replacement.
use crate::coin_remap_cache::CacheIo;
use crate::coin_remap_policy::Options;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct Files {
    pub root: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
    pub images: PathBuf,
    pub output: PathBuf,
    pub candidates: PathBuf,
    pub base: String,
}

fn durable_path_identity(path: &Path) -> Result<PathBuf, String> {
    crate::coin_manifest::output_path(path)?;
    let resolved = crate::tooling_environment::canonical_existing_prefix(path)
        .map_err(|_| "cannot resolve remap durable path")?;
    #[cfg(windows)]
    let resolved = PathBuf::from(
        resolved
            .to_str()
            .ok_or("remap durable path is not UTF-8")?
            .to_lowercase(),
    );
    Ok(resolved)
}

fn distinct_durable_paths(root: &Path, paths: &[&Path]) -> Result<(), String> {
    let locks = durable_path_identity(&root.join(".locks"))?;
    let mut protected = std::collections::BTreeSet::new();
    match std::fs::read_dir(&locks) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|_| "cannot inspect remap lock storage")?;
                let path = entry.path();
                crate::coin_provider_stage::physical(&path)?;
                if entry
                    .file_type()
                    .map_err(|_| "cannot inspect remap lock entry")?
                    .is_file()
                    && let Some(identity) = crate::locks::existing_file_identity(&path)
                        .map_err(|_| "cannot identify remap lock storage")?
                {
                    protected.insert(identity);
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("cannot inspect remap lock storage".into()),
    }
    let mut destinations = std::collections::BTreeSet::new();
    for path in paths {
        let resolved = durable_path_identity(path)?;
        if resolved.starts_with(&locks)
            || crate::locks::existing_file_identity(&resolved)
                .map_err(|_| "cannot identify remap durable file")?
                .is_some_and(|identity| protected.contains(&identity))
        {
            return Err("remap durable paths must not use lock storage".into());
        }
        if !destinations.insert(resolved) {
            return Err("remap durable paths and backup must be distinct".into());
        }
    }
    Ok(())
}

struct Disk<'a> {
    tg: &'a crate::telegram::Telegram,
    path: &'a Path,
    log: &'a crate::logging::RunLog,
}
impl CacheIo for Disk<'_> {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String> {
        self.tg
            .read_call(
                "getStickerSet",
                &std::collections::BTreeMap::from([("name".into(), name.into())]),
            )
            .map_err(|e| e.command_error())?["stickers"]
            .as_array()
            .cloned()
            .ok_or_else(|| "live remap set has no stickers".into())
    }
    fn signature(&mut self, sticker: &Value) -> Result<Vec<u8>, String> {
        let data = self
            .tg
            .download_bytes(
                sticker["file_id"]
                    .as_str()
                    .ok_or("remap sticker has no file ID")?,
                5,
            )
            .map_err(|e| e.command_error())?;
        crate::codec_image::remap_signature(&data)
    }
    fn save(&mut self, cache: &Value) -> Result<(), String> {
        crate::atomic::write_json(self.path, cache)
            .map_err(|_| "cannot checkpoint remap signatures")?;
        self.log
            .event("INFO", "coins", "remap signatures checkpointed");
        Ok(())
    }
    fn wait(&mut self, duration: std::time::Duration) {
        std::thread::sleep(duration);
    }
}
fn local(
    directory: &Path,
    log: &crate::logging::RunLog,
) -> Result<(Vec<String>, Vec<Vec<f32>>), String> {
    let mut paths = std::fs::read_dir(directory)
        .map_err(|_| "cannot enumerate remap source logos")?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|_| "cannot inspect remap source entry")
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    let mut tickers = Vec::new();
    let mut signatures = Vec::new();
    for path in paths
        .into_iter()
        .filter(|path| path.extension().is_some_and(|value| value == "png"))
    {
        crate::coin_provider_stage::physical(&path)?;
        let ticker = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("remap source name is not UTF-8")?
            .to_lowercase();
        let data = std::fs::read(&path).map_err(|_| "cannot read remap source logo")?;
        match crate::codec_image::remap_signature(&data) {
            Ok(bytes) => {
                tickers.push(ticker);
                signatures.push(bytes.into_iter().map(f32::from).collect());
            }
            Err(_) => log.event("WARNING", "coins", "unreadable local remap source omitted"),
        }
    }
    Ok((tickers, signatures))
}

pub fn persist(
    root: &Path,
    output: &Path,
    candidates: &Path,
    decision: &crate::coin_remap_policy::Decision,
) -> Result<(), String> {
    persist_with_lease(root, output, candidates, decision, |path| {
        crate::locks::Lease::acquire(path, &crate::logging::utc())
    })
}

fn persist_with_lease(
    root: &Path,
    output: &Path,
    candidates: &Path,
    decision: &crate::coin_remap_policy::Decision,
    acquire: impl FnOnce(&Path) -> std::io::Result<crate::locks::Lease>,
) -> Result<(), String> {
    let Some(destination) = decision.destination else {
        return Ok(());
    };
    let backup = output.with_extension("prebroken.json");
    distinct_durable_paths(root, &[output, candidates, &backup])?;
    crate::coin_provider_recover::object(&backup, true)?;
    let target = if destination == "candidate" {
        candidates
    } else {
        output
    };
    crate::coin_manifest::output_path(target)?;
    if destination == "candidate" {
        return crate::atomic::write_json(target, &Value::Object(decision.mapping.clone()))
            .map_err(|_| "cannot save reviewable remap candidates".into());
    }
    let lock = crate::locks::canonical_map_path(root);
    crate::coin_manifest::output_path(&lock)?;
    let _map = acquire(&lock).map_err(|_| "canonical map is busy or unavailable")?;
    // Validate/read the current map only under its lock; backup bytes describe what we replace.
    if std::fs::symlink_metadata(output).is_ok() {
        crate::coin_provider_stage::physical(output)?;
        crate::coin_provider_recover::object(output, false)?;
        crate::coin_manifest::output_path(&backup)?;
        crate::coin_provider_recover::object(&backup, true)?;
        if !backup.exists() {
            let bytes = std::fs::read(output).map_err(|_| "cannot read map for remap backup")?;
            // Exclusive sibling publication refuses a competing backup, never overwrites one.
            let staged = crate::codec_video_encode::Staged::new(&backup, "json")?;
            let result = crate::atomic::write_bytes(&staged.0, &bytes)
                .map_err(|_| "cannot stage remap backup".to_owned())
                .and_then(|_| staged.publish(&backup).map(|_| ()));
            staged.clean()?;
            result?;
        }
    }
    crate::atomic::write_json(output, &Value::Object(decision.mapping.clone()))
        .map_err(|_| "cannot replace canonical remap map".into())
}

pub fn run(
    files: &Files,
    tg: &crate::telegram::Telegram,
    options: Options,
    log: &crate::logging::RunLog,
) -> Result<u8, String> {
    run_with_nearest(files, tg, options, log, crate::coin_remap_math::nearest)
}
fn run_with_nearest(
    files: &Files,
    tg: &crate::telegram::Telegram,
    options: Options,
    log: &crate::logging::RunLog,
    nearest: impl FnOnce(&[Vec<f32>], &[Vec<f32>]) -> Result<crate::coin_remap_policy::Nearest, String>,
) -> Result<u8, String> {
    run_with_effects(files, tg, options, log, nearest, |path| {
        crate::locks::Lease::acquire(path, &crate::logging::utc())
    })
}

fn run_with_effects(
    files: &Files,
    tg: &crate::telegram::Telegram,
    mut options: Options,
    log: &crate::logging::RunLog,
    nearest: impl FnOnce(&[Vec<f32>], &[Vec<f32>]) -> Result<crate::coin_remap_policy::Nearest, String>,
    acquire: impl FnOnce(&Path) -> std::io::Result<crate::locks::Lease>,
) -> Result<u8, String> {
    for path in [&files.root, &files.state, &files.images] {
        crate::coin_provider_stage::physical(path)?;
    }
    if !files.images.is_dir() {
        return Err("remap image source must be a directory".into());
    }
    let backup = files.output.with_extension("prebroken.json");
    distinct_durable_paths(
        &files.root,
        &[
            &files.state,
            &files.cache,
            &files.output,
            &files.candidates,
            &backup,
        ],
    )?;
    crate::coin_provider_recover::object(&backup, true)?;
    let family = crate::locks::pack_family_path(&files.root, &files.base);
    crate::coin_manifest::output_path(&family)?;
    let _family = match crate::locks::Lease::acquire(&family, &crate::logging::utc()) {
        Ok(lease) => lease,
        Err(_) => {
            log.event("ERROR", "coins", "remap pack family is busy or unavailable");
            return Ok(4);
        }
    };
    let state = crate::coin_provider_recover::object(&files.state, false)?;
    let mut sets = state
        .get("sets")
        .and_then(Value::as_array)
        .ok_or("remap state has no sets")?
        .clone();
    if sets.iter().any(|set| set["index"].as_i64().is_none()) {
        return Err("remap set index must be an integer".into());
    }
    sets.sort_by_key(|set| set["index"].as_i64());
    let raw = Value::Object(crate::coin_provider_recover::object(&files.cache, true)?);
    let mut cache = crate::coin_remap_cache::load(&raw)?;
    let (incomplete, stats) = crate::coin_remap_cache::reconcile_inventory(
        &mut cache,
        sets.iter().map(|set| set["name"].as_str()),
        &mut Disk {
            tg,
            path: &files.cache,
            log,
        },
    )?;
    log.event(
        "INFO",
        "coins",
        &format!(
            "remap_cache retired_sigs={} retired_errors={} retained_errors={}",
            stats.retired_sigs, stats.retired_errors, stats.retained_errors
        ),
    );
    let (tickers, local) = local(&files.images, log)?;
    let sigs = cache["sigs"]
        .as_object()
        .ok_or("invalid remap signatures")?;
    if tickers.is_empty() || sigs.is_empty() {
        return Ok(4);
    }
    let cids = sigs.keys().cloned().collect::<Vec<_>>();
    let live = sigs
        .values()
        .map(|value| {
            let bytes = STANDARD
                .decode(value.as_str().ok_or("invalid remap signature")?)
                .map_err(|_| "invalid remap signature base64")?;
            if bytes.len() != 1024 {
                return Err("invalid remap signature length");
            }
            Ok(bytes.into_iter().map(f32::from).collect::<Vec<_>>())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let nearest = nearest(&local, &live)?;
    let matches = crate::coin_remap_policy::matches(&tickers, &cids, nearest)?;
    options.incomplete_sets = incomplete.len();
    options.failed_images = stats.retained_errors;
    let decision = crate::coin_remap_policy::decide(&matches, options)?;
    log.event(
        "INFO",
        "coins",
        &format!(
            "remap_matched={} omitted_far={} ambiguous={} incomplete={}",
            decision.mapping.len(),
            decision.far.len(),
            decision.ambiguous.len(),
            incomplete.len()
        ),
    );
    persist_with_lease(
        &files.root,
        &files.output,
        &files.candidates,
        &decision,
        acquire,
    )?;
    println!(
        "remap matched={} far={} ambiguous={} apply={} exit={}",
        decision.mapping.len(),
        decision.far.len(),
        decision.ambiguous.len(),
        options.apply,
        decision.code
    );
    Ok(decision.code)
}

#[cfg(test)]
#[path = "../tests/baseline_coin_providers_support/remap_units.rs"]
mod baseline_remap_tests;

#[cfg(test)]
#[path = "../tests/baseline_coin_state_support/remap_backup_units.rs"]
mod baseline_coin_state_remap_tests;
