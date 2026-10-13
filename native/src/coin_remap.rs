//! Content-only remap joins live signatures, local artwork and guarded atomic replacement.
use crate::coin_remap_cache::CacheIo;
use crate::coin_remap_policy::{Match, Options};
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
    let Some(destination) = decision.destination else {
        return Ok(());
    };
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
    let lock = root.join(".locks/canonical_map.lock");
    crate::coin_manifest::output_path(&lock)?;
    let _map = crate::locks::Lease::acquire(&lock, &crate::logging::utc())
        .map_err(|_| "canonical map is busy or unavailable")?;
    // Validate/read the current map only under its lock; backup bytes describe what we replace.
    if std::fs::symlink_metadata(output).is_ok() {
        crate::coin_provider_stage::physical(output)?;
        crate::coin_provider_recover::object(output, false)?;
        let backup = output.with_extension("prebroken.json");
        crate::coin_manifest::output_path(&backup)?;
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
    mut options: Options,
    log: &crate::logging::RunLog,
) -> Result<u8, String> {
    for path in [&files.root, &files.state, &files.images] {
        crate::coin_provider_stage::physical(path)?;
    }
    if !files.images.is_dir() {
        return Err("remap image source must be a directory".into());
    }
    for path in [&files.cache, &files.output, &files.candidates] {
        crate::coin_manifest::output_path(path)?;
    }
    if [&files.cache, &files.output, &files.candidates]
        .iter()
        .enumerate()
        .any(|(i, path)| [&files.cache, &files.output, &files.candidates][..i].contains(path))
        || [&files.cache, &files.output, &files.candidates].contains(&&files.state)
    {
        return Err("remap durable paths must be distinct".into());
    }
    let family = crate::locks::pack_family_path(&files.root, &files.base);
    crate::coin_manifest::output_path(&family)?;
    let _family = crate::locks::Lease::acquire(&family, &crate::logging::utc())
        .map_err(|_| "remap pack family is busy or unavailable")?;
    let state = crate::coin_provider_recover::object(&files.state, false)?;
    let mut sets = state["sets"]
        .as_array()
        .ok_or("remap state has no sets")?
        .clone();
    if sets.iter().any(|set| set["index"].as_i64().is_none()) {
        return Err("remap set index must be an integer".into());
    }
    sets.sort_by_key(|set| set["index"].as_i64());
    let raw = Value::Object(crate::coin_provider_recover::object(&files.cache, true)?);
    let mut cache = crate::coin_remap_cache::load(&raw)?;
    let incomplete = crate::coin_remap_cache::download(
        &sets,
        &mut cache,
        &mut Disk {
            tg,
            path: &files.cache,
            log,
        },
    )?;
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
    let nearest = crate::coin_remap_math::nearest(&local, &live)?;
    if [
        nearest.indices.len(),
        nearest.squared.len(),
        nearest.second.len(),
    ]
    .iter()
    .any(|len| *len != tickers.len())
    {
        return Err("remap parallel match arrays disagree".into());
    }
    let matches = tickers
        .into_iter()
        .enumerate()
        .map(|(i, ticker)| {
            let cid = cids
                .get(nearest.indices[i])
                .ok_or("remap nearest index is outside candidates")?
                .clone();
            let distance = nearest.squared[i].sqrt();
            Ok(Match {
                ticker,
                cid,
                distance,
                margin: nearest.second[i].sqrt() - distance,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    options.incomplete_sets = incomplete.len();
    options.failed_images = cache["errors"]
        .as_object()
        .ok_or("invalid remap error cache")?
        .len();
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
    persist(&files.root, &files.output, &files.candidates, &decision)?;
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
