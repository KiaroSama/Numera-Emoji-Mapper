//! Native gallery/cache orchestration embeds exact codec bytes and unchanged incumbent assets.
use crate::{bot_payloads::escape, media_adapter::MediaAdapter};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
fn joined(value: &Value) -> String {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}
fn uri(path: &Path) -> Result<String, String> {
    let mime = if path.extension().and_then(|s| s.to_str()) == Some("webm") {
        "video/webm"
    } else {
        "image/webp"
    };
    Ok(format!(
        "data:{mime};base64,{}",
        STANDARD.encode(std::fs::read(path).map_err(|e| e.to_string())?)
    ))
}
struct RenderStaging(PathBuf);
impl RenderStaging {
    fn pending(&self) -> bool {
        std::fs::read_dir(self.0.join("codec"))
            .map(|mut entries| {
                entries.any(|entry| {
                    entry.is_err()
                        || entry.is_ok_and(|entry| {
                            entry.path().join("codec-cleanup-pending.json").exists()
                        })
                })
            })
            .unwrap_or_else(|error| error.kind() != std::io::ErrorKind::NotFound)
    }
}
impl Drop for RenderStaging {
    fn drop(&mut self) {
        if !self.pending()
            && let Err(error) = std::fs::remove_dir_all(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("WARNING: gallery staging cleanup failed: {error}");
        }
    }
}
async fn render_thumb(
    media: &MediaAdapter,
    source: &Path,
    fmt: &str,
    dest: &Path,
    still: bool,
) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = dest.parent().ok_or("gallery cache has no parent")?;
    let workspace = loop {
        let path = parent.join(format!(
            ".render-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => break path,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    let workspace = RenderStaging(workspace);
    let adapter = MediaAdapter {
        python: media.python.clone(),
        root: media.root.clone(),
        scratch: workspace.0.join("codec"),
    };
    let output = workspace
        .0
        .join(dest.file_name().ok_or("gallery cache has no filename")?);
    let result = async {
        adapter.call(json!({"operation":"gallery_thumb","source":source,"format":fmt,"output":output,"still":still})).await?;
        if output.is_symlink() || output.metadata().map_err(|e| e.to_string())?.len() == 0 {
            return Err("gallery codec returned empty/unsafe output".into());
        }
        // Publish complete bytes without overwriting another renderer's cache.
        match crate::atomic::publish_new(&output, dest) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && dest.is_file() && !dest.is_symlink() => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }.await;
    if workspace.pending() {
        return Err("gallery codec cleanup unresolved; staging retained".into());
    }
    std::fs::remove_dir_all(&workspace.0).map_err(|e| e.to_string())?;
    result
}

async fn thumb(
    media: &MediaAdapter,
    id: &str,
    source: &Path,
    fmt: &str,
    cache: &Path,
) -> Result<(String, String, Option<String>), String> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("unsafe gallery custom emoji ID".into());
    }
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let ext = if fmt == "video" { "webm" } else { "webp" };
    let path = [
        cache.join(format!("{id}.webp")),
        cache.join(format!("{id}.webm")),
    ]
    .into_iter()
    .find(|p| p.is_file() && !p.is_symlink())
    .unwrap_or_else(|| cache.join(format!("{id}.{ext}")));
    if path.is_symlink() {
        return Err("gallery cache must not be a symlink".into());
    }
    if !path.is_file() {
        render_thumb(media, source, fmt, &path, false).await?;
    }
    let still = if fmt == "animated" {
        let still = cache.join(format!("{id}_still.webp"));
        if still.is_symlink() {
            return Err("gallery still cache must not be a symlink".into());
        }
        let ready = if still.is_file() {
            true
        } else {
            match render_thumb(media, source, fmt, &still, true).await {
                Ok(()) => true,
                Err(error) if error.starts_with("gallery codec cleanup unresolved") => {
                    return Err(error);
                }
                Err(_) => false,
            }
        };
        if ready { Some(uri(&still)?) } else { None }
    } else {
        None
    };
    Ok((
        if path.extension().and_then(|s| s.to_str()) == Some("webm") {
            "video"
        } else if fmt == "animated" {
            "anim"
        } else {
            "img"
        }
        .into(),
        uri(&path)?,
        still,
    ))
}
pub async fn render(
    doc: &Value,
    media: &MediaAdapter,
    art: &std::collections::BTreeMap<String, PathBuf>,
    cache: &Path,
) -> Result<String, String> {
    let mut thumbnails = std::collections::BTreeMap::new();
    for entry in doc["emoji"].as_array().ok_or("roster has no emoji list")? {
        let id = entry["custom_emoji_id"]
            .as_str()
            .ok_or("entry has no custom emoji ID")?;
        let fmt = entry["format"]
            .as_str()
            .ok_or("roster entry has no format")?;
        if let Some(source) = art.get(id).filter(|s| s.is_file()) {
            match thumb(media, id, source, fmt, cache).await {
                Ok(got) => {
                    thumbnails.insert(id.to_owned(), got);
                }
                Err(error) if error.starts_with("gallery codec cleanup unresolved") => {
                    return Err(error);
                }
                Err(error) => eprintln!(
                    "WARNING: gallery thumbnail failed: {}",
                    crate::logging::redact(&error, &[])
                ),
            }
        }
    }
    render_page(doc, &thumbnails)
}

pub type Thumbnail = (String, String, Option<String>);

pub fn render_page(
    doc: &Value,
    thumbnails: &std::collections::BTreeMap<String, Thumbnail>,
) -> Result<String, String> {
    let mut cards = Vec::new();
    let (mut drawn, mut moving) = (0, 0);
    for entry in doc["emoji"].as_array().ok_or("roster has no emoji list")? {
        let id = entry["custom_emoji_id"]
            .as_str()
            .ok_or("entry has no custom emoji ID")?;
        let fmt = entry["format"]
            .as_str()
            .ok_or("roster entry has no format")?;
        let got = thumbnails.get(id);
        let index = entry["index"].as_u64().ok_or("entry has no index")?;
        let (art, thumb_class) = if let Some((kind, uri, still)) = got {
            drawn += 1;
            if kind == "video" {
                moving += 1;
                (
                    format!(
                        "<video src=\"{uri}\" data-play=\"1\" loop muted playsinline preload=\"none\"></video>"
                    ),
                    "thumb",
                )
            } else if let Some(still) = still {
                moving += 1;
                (
                    format!(
                        "<img src=\"{still}\" data-still=\"{still}\" data-anim=\"{uri}\" alt=\"emoji {index}\">"
                    ),
                    "thumb",
                )
            } else {
                (
                    format!("<img src=\"{uri}\" alt=\"emoji {index}\" loading=\"lazy\">"),
                    "thumb",
                )
            }
        } else {
            ("no preview".into(), "thumb empty")
        };
        let original = joined(&entry["source_emoji_ids"]);
        let prior = joined(&entry["previous_custom_emoji_ids"]);
        let mut ids = format!(
            "<span class=\"id now\" data-id=\"{}\" title=\"click to copy\"><span class=\"k\">this pack</span>{}</span>",
            escape(id),
            escape(id)
        );
        for (class, label, value) in [
            ("was", "original pack", &original),
            ("mine", "this pack, before", &prior),
        ] {
            if !value.is_empty() {
                ids += &format!(
                    "<span class=\"id {class}\" data-id=\"{}\" title=\"click to copy\"><span class=\"k\">{label}</span>{}</span>",
                    escape(value),
                    escape(value)
                );
            }
        }
        let logo = entry["role"] == "brand-logo";
        let class = if logo {
            "logo".into()
        } else {
            format!("fmt-{}", escape(fmt))
        };
        let name = entry["name"].as_str().unwrap_or("");
        let glyph = entry["glyph"].as_str().unwrap_or("");
        cards.push(format!("<div class=\"card {class}\" data-index=\"{index}\" data-slot=\"{}\" data-custom-emoji-id=\"{}\" data-format=\"{}\" data-glyph=\"{}\" data-source-emoji-ids=\"{}\" data-previous-custom-emoji-ids=\"{}\"><div class=\"hdr\"><span class=\"pos\">{index}</span><span class=\"badge\">{}</span></div><div class=\"{thumb_class}\">{art}</div><div class=\"glyph\" title=\"the glyph this sticker carries\">{}</div><div class=\"nm\">{}</div>{ids}</div>",entry["slot"],escape(id),escape(fmt),escape(glyph),escape(&original),escape(&prior),if logo{"logo".into()}else{escape(fmt)},escape(if glyph.is_empty(){"—"}else{glyph}),escape(name)));
    }
    let name = escape(doc["set_name"].as_str().ok_or("roster has no set name")?);
    let title = doc["title"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(escape)
        .unwrap_or_else(|| name.clone());
    let link = escape(doc["link"].as_str().ok_or("roster has no link")?);
    let captured = escape(
        doc["captured_utc"]
            .as_str()
            .ok_or("roster has no capture time")?,
    );
    let family = escape(doc["family"].as_str().ok_or("roster has no family")?);
    let pack_index = doc
        .get("pack_index")
        .filter(|v| !v.is_null())
        .map(ToString::to_string)
        .unwrap_or("None".into());
    let count = doc["count"].as_u64().ok_or("roster has no count")?;
    let mut bytes = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut bytes, formatter);
    serde::Serialize::serialize(doc, &mut serializer).map_err(|e| e.to_string())?;
    let mut script = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    for (from, to) in [
        ("<", r"\u003c"),
        (">", r"\u003e"),
        ("\u{2028}", r"\u2028"),
        ("\u{2029}", r"\u2029"),
    ] {
        script = script.replace(from, to);
    }
    // Python source literals normalize CRLF while include_str preserves checkout bytes.
    let css = include_str!("../../assets/pack-gallery.css").replace("\r\n", "\n");
    let js = include_str!("../../assets/pack-gallery.js").replace("\r\n", "\n");
    let zero_note = if family == "general" {
        "<b>#</b> counts from 0 and emoji 0 is the brand logo; <b>slot</b> is the position Telegram shows. <b>was</b> is the id this emoji had in the pack it came from."
    } else {
        "<b>#</b> counts from 0; <b>slot</b> is the position Telegram shows. This family carries no brand logo, so emoji 0 is a real coin."
    };
    Ok(format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title} - emoji roster</title>
<style>{css}</style></head><body class="bg-checker">
<header>
<h1>{title}</h1>
<div class="meta">
<code>{name}</code> &middot;
{count} emoji ({drawn} with a preview) &middot;
{family} pack {pack_index} &middot;
<a href="{link}">{link}</a><br>
Captured {captured} live from Telegram.
{zero_note}
</div>
<div class="bar">
<button id="top" title="Jump to the first card">&#8593; Top</button>
<button id="bot" title="Jump to the last card">&#8595; Bottom</button>
<span class="sep"></span>
<button id="bg" title="Switch the preview backdrop so black / hollow / faint emoji are visible">Backdrop: Checker</button>
<button id="anim" class="switch" aria-pressed="true" title="Freeze every animation on its first frame. The lightest this page gets -- nothing is decoding."><span class="knob"></span><span id="animLabel">Animation: On</span></button>
<span class="sep"></span>
<span class="meta">{moving} of {count} animate &middot; click either id to copy it</span>
</div></header>
<main class="grid">
{}
</main>
<script type="application/json" id="roster">
{script}
</script>
<script>{js}</script>
</body></html>
"#,
        cards.join("\n")
    ))
}
