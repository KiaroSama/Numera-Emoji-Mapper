//! Native collection orchestration; the only Python calls are codec/identity operations.
use crate::{catalog::Catalog, logging, media_adapter::MediaAdapter, telegram::Telegram};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Download(PathBuf);
impl Drop for Download {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str, String> {
    v.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {k}"))
}
fn fmt(sticker: &Value) -> &'static str {
    if sticker.get("is_video") == Some(&Value::Bool(true)) {
        "video"
    } else if sticker.get("is_animated") == Some(&Value::Bool(true)) {
        "animated"
    } else {
        "static"
    }
}

pub fn report_stats(catalog: &Catalog) -> Result<(), String> {
    let stats = catalog.stats()?;
    let formats = stats
        .as_object()
        .ok_or("invalid catalog statistics")?
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    for format in formats {
        println!(
            "  catalog {format}: {} total ({} pending upload)",
            stats[&format]["total"], stats[&format]["pending"]
        );
    }
    Ok(())
}

pub struct Collector<'a> {
    pub cat: &'a mut Catalog,
    pub media: &'a MediaAdapter,
    pub data: PathBuf,
    pub threshold: i32,
}
impl Collector<'_> {
    pub async fn ingest(
        &mut self,
        source: &Path,
        labels: Value,
        reencode: bool,
    ) -> Result<bool, String> {
        if self.threshold != -1 && !(0..=16).contains(&self.threshold) {
            return Err("invalid phash threshold".into());
        }
        let fingerprint = self
            .media
            .call(json!({"operation":"prepare","source":source,"reencode":reencode}))
            .await?;
        let key = text(&fingerprint, "content_key")?.to_owned();
        let format = text(&fingerprint, "fmt")?.to_owned();
        let ext = text(&fingerprint, "extension")?;
        let destination = self
            .data
            .join("media")
            .join(&format)
            .join(format!("{}{ext}", key.replace(':', "_")));
        std::fs::create_dir_all(destination.parent().ok_or("invalid destination")?)
            .map_err(|e| e.to_string())?;
        let destination =
            crate::media_store::store(self.media, source, &destination, &format, &key, &labels)
                .await?;
        let mut item = labels;
        item["content_key"] = json!(key);
        item["fmt"] = json!(format);
        item["file_path"] = json!(destination);
        item["phash"] = fingerprint["phash"].clone();
        let media = self.media;
        let (_, new) = crate::ingest::add_verified(
            self.cat,
            item,
            &logging::utc(),
            self.threshold,
            async |a: &Path, b: &Path, format: &str| {
                Ok(media.compare(a, b, format).await.unwrap_or(None))
            },
            async |source: &Path, key: &str| {
                let result = media
                    .call(json!({"operation":"collision","source":source,"key":key}))
                    .await?;
                result
                    .as_str()
                    .map(str::to_owned)
                    .ok_or("invalid collision key".into())
            },
        )
        .await?;
        Ok(new)
    }
    pub fn fetch_pack(
        &mut self,
        tg: &Telegram,
        runtime: &tokio::runtime::Runtime,
        name: &str,
        limit: u64,
        repaintable: &str,
    ) -> Result<Value, String> {
        let pack = tg
            .read_call(
                "getStickerSet",
                &BTreeMap::from([("name".into(), name.into())]),
            )
            .map_err(|e| e.to_string())?;
        let stickers = pack
            .get("stickers")
            .and_then(Value::as_array)
            .ok_or("pack has no stickers")?;
        if !["ask", "keep", "skip"].contains(&repaintable) {
            return Err("invalid repaintable choice".into());
        }
        let repainted = stickers
            .iter()
            .filter(|s| s.get("needs_repainting") == Some(&Value::Bool(true)))
            .count();
        let mut keep = repaintable == "keep";
        if repainted > 0 {
            eprintln!(
                "WARNING: {repainted} emoji are repainted by Telegram; inspect stored art before ingesting"
            );
            if repaintable == "ask" {
                use std::io::{IsTerminal, Write};
                if std::io::stdin().is_terminal() {
                    eprint!("Ingest them anyway? [y/N] ");
                    std::io::stderr().flush().map_err(|e| e.to_string())?;
                    let mut answer = String::new();
                    keep = std::io::stdin().read_line(&mut answer).is_ok()
                        && ["y", "yes"].contains(&answer.trim().to_lowercase().as_str());
                }
            }
        }
        let mut counts = json!({"new":0,"dedup":0,"skipped":0,"failed":0,"repaintable":if keep{0}else{repainted}});
        for sticker in stickers {
            if limit != 0 && counts["new"].as_u64().unwrap_or(0) >= limit {
                break;
            }
            if sticker.get("needs_repainting") == Some(&Value::Bool(true)) && !keep {
                continue;
            }
            let fuid = sticker
                .get("file_unique_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            let known = self
                .cat
                .dispatch(json!({"operation":"seen","file_unique_id":fuid}))?;
            let emojis = sticker
                .get("emoji")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(|s| vec![s])
                .unwrap_or_default();
            let mut labels = json!({"emojis":emojis,"keywords":sticker.get("keywords").cloned().unwrap_or(json!([])),"sources":[name],"file_unique_id":fuid});
            if let Some(key) = known.as_str() {
                labels["content_key"] = json!(key);
                self.cat.merge_labels(&labels)?;
                counts["dedup"] = json!(counts["dedup"].as_u64().unwrap_or(0) + 1);
                continue;
            }
            let result = self.download_ingest(tg, runtime, sticker, labels);
            let field = match result {
                Ok(true) => "new",
                Ok(false) => "dedup",
                Err(_) => "failed",
            };
            counts[field] = json!(counts[field].as_u64().unwrap_or(0) + 1);
        }
        Ok(counts)
    }
    pub fn download_ingest(
        &mut self,
        tg: &Telegram,
        runtime: &tokio::runtime::Runtime,
        sticker: &Value,
        labels: Value,
    ) -> Result<bool, String> {
        let directory = self.data.join("tmp");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        (|| {
            let bytes = tg
                .download_bytes(text(sticker, "file_id")?, 5)
                .map_err(|e| e.to_string())?;
            let (mut file, path) = loop {
                let path = directory.join(format!(
                    "native-{}-{}.{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed),
                    match fmt(sticker) {
                        "video" => "webm",
                        "animated" => "tgs",
                        _ => "dl",
                    }
                ));
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                {
                    Ok(file) => break (file, path),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e.to_string()),
                }
            };
            // Only a successful exclusive create gives this invocation deletion rights.
            let scratch = Download(path);
            use std::io::Write;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            drop(file);
            if let Some(tint) = labels.get("tint").and_then(Value::as_str) {
                runtime.block_on(
                    self.media
                        .call(json!({"operation":"prepare","source":scratch.0,
                    "reencode":true,"tint":tint})),
                )?;
                let mut labels = labels;
                labels
                    .as_object_mut()
                    .ok_or("invalid labels")?
                    .remove("tint");
                runtime.block_on(self.ingest(&scratch.0, labels, false))
            } else {
                runtime.block_on(self.ingest(&scratch.0, labels, true))
            }
        })()
    }
}
