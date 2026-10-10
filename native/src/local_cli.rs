//! Local files are converted by codecs; Rust owns catalog, traversal and ingest decisions.
use crate::{
    catalog::Catalog, collector::Collector, config::Config, logging::RunLog,
    media_adapter::MediaAdapter,
};
use serde_json::json;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let stopped = std::fs::read_dir(self.0.join("codecs"))
            .map(|mut entries| {
                entries.all(|entry| {
                    entry.is_ok_and(|entry| {
                        !entry.path().join("codec-cleanup-pending.json").exists()
                    })
                })
            })
            .unwrap_or_else(|e| e.kind() == std::io::ErrorKind::NotFound);
        if stopped {
            let _ = std::fs::remove_dir_all(&self.0);
        } else {
            eprintln!("ERROR: codec cleanup is unresolved; local workspace retained");
        }
    }
}

fn option(args: &[String], i: &mut usize) -> Result<String, String> {
    *i += 1;
    args.get(*i)
        .cloned()
        .ok_or("option requires a value".into())
}

pub async fn choose_format(
    media: &MediaAdapter,
    path: &Path,
    requested: &str,
) -> Result<String, String> {
    if requested != "auto" {
        return if ["static", "video", "animated"].contains(&requested) {
            Ok(requested.into())
        } else {
            Err("unsupported target format".into())
        };
    }
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let format = match extension.as_str() {
        "json" | "tgs" => "animated",
        "mp4" | "webm" | "mov" | "mkv" | "m4v" | "avi" | "apng" => "video",
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "svg" => "static",
        "gif" => {
            if media
                .call(json!({"operation":"animation_info","source":path,"format":"static"}))
                .await?
                == true
            {
                "video"
            } else {
                "static"
            }
        }
        _ => {
            let mut head = Vec::new();
            std::fs::File::open(path)
                .and_then(|file| file.take(16).read_to_end(&mut head))
                .map_err(|e| e.to_string())?;
            if head.starts_with(b"\x1a\x45\xdf\xa3") {
                "video"
            } else if head.starts_with(b"\x1f\x8b") {
                "animated"
            } else {
                "static"
            }
        }
    };
    Ok(format.into())
}

pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|s| s == "--help" || s == "-h") {
        println!(
            "Usage: numera-emoji add-media [FILES...] [--in DIR] [--as auto|static|video|animated] [--emoji EMOJI] [--keywords LIST] [--data-dir DIR] [--phash-threshold -1|0..16]"
        );
        return Ok(0);
    }
    let (mut files, mut directory, mut data, mut format, mut emoji, mut keywords, mut threshold) = (
        Vec::new(),
        None,
        "collection".to_owned(),
        "auto".to_owned(),
        "😀".to_owned(),
        String::new(),
        -1i32,
    );
    let mut i = 0;
    let mut positional = false;
    while i < args.len() {
        match args[i].as_str() {
            "--" if !positional => positional = true,
            "--in" if !positional => directory = Some(PathBuf::from(option(args, &mut i)?)),
            "--data-dir" if !positional => data = option(args, &mut i)?,
            "--as" if !positional => format = option(args, &mut i)?,
            "--emoji" if !positional => emoji = option(args, &mut i)?,
            "--keywords" if !positional => keywords = option(args, &mut i)?,
            "--phash-threshold" if !positional => {
                threshold = option(args, &mut i)?
                    .parse()
                    .map_err(|_| "invalid phash threshold")?;
            }
            value if positional || !value.starts_with('-') => files.push(PathBuf::from(value)),
            value => return Err(format!("unknown add-media option: {value}")),
        }
        i += 1;
    }
    if threshold != -1 && !(0..=16).contains(&threshold) {
        return Err("phash threshold must be -1 or 0..16".into());
    }
    if !["auto", "static", "video", "animated"].contains(&format.as_str()) {
        return Err("unsupported target format".into());
    }
    if let Some(directory) = directory {
        let mut found = std::fs::read_dir(directory)
            .map_err(|e| format!("cannot read --in folder: {e}"))?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        found.retain(|p| p.is_file());
        found.sort();
        found.extend(files);
        files = found;
    }
    if files.is_empty() {
        return Err("no input files (use --in FOLDER or list files)".into());
    }
    let data = config.path(&data);
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let data = data.canonicalize().map_err(|e| e.to_string())?;
    let temporary = data.join("tmp");
    std::fs::create_dir_all(&temporary).map_err(|e| e.to_string())?;
    let workspace = loop {
        let path = temporary.join(format!(
            "local-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => break Scratch(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    let python = if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
        std::env::var_os("PYO3_PYTHON").map(PathBuf::from)
    } else {
        None
    }
    .unwrap_or_else(|| {
        config.root.join(if cfg!(windows) {
            ".venv/Scripts/python.exe"
        } else {
            ".venv/bin/python"
        })
    });
    if python.is_absolute() && !python.is_file() {
        return Err("required exact-media Python environment is missing".into());
    }
    let adapter = MediaAdapter {
        python,
        root: config.root.clone(),
        scratch: workspace.0.join("codecs"),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let mut catalog = Catalog::open(
        &data.join("catalog.db"),
        &crate::logging::utc(),
        &config.root,
    )?;
    let mut collector = Collector {
        cat: &mut catalog,
        media: &adapter,
        data,
        threshold,
    };
    let extras = keywords
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let (mut new, mut dedup, mut failed) = (0u64, 0u64, 0u64);
    for (index, source) in files.iter().enumerate() {
        let result = runtime.block_on(async {
            let source = source.canonicalize().map_err(|e| e.to_string())?;
            let target = choose_format(&adapter, &source, &format).await?;
            let output = workspace.0.join(format!("converted-{index}"));
            let converted = adapter
                .call(json!({"operation":"convert", "source":source,
                "output":output,"target_format":target}))
                .await?;
            let fmt = converted["fmt"]
                .as_str()
                .ok_or("conversion returned no format")?;
            let extension = match fmt {
                "static" => "png",
                "video" => "webm",
                "animated" => "tgs",
                _ => return Err("conversion returned invalid format".into()),
            };
            let actual = output.with_extension(extension);
            std::fs::rename(&output, &actual).map_err(|e| e.to_string())?;
            let stem = source
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("source name is not UTF-8")?;
            let name = source
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("source name is not UTF-8")?;
            let mut labels = vec![stem.to_lowercase()];
            labels.extend(extras.clone());
            collector
                .ingest(
                    &actual,
                    json!({"emojis":[emoji],"keywords":labels,
                "sources":[format!("local:{name}")]}),
                    false,
                )
                .await
        });
        match result {
            Ok(true) => {
                new += 1;
                log.event("INFO", "collector", "local media inserted");
            }
            Ok(false) => {
                dedup += 1;
                log.event("INFO", "collector", "local media deduplicated");
            }
            Err(error) => {
                failed += 1;
                log.event("WARNING", "collector", &error);
            }
        }
    }
    println!("Done. new={new} dedup={dedup} failed={failed}");
    crate::collector::report_stats(collector.cat)?;
    Ok(if failed == 0 {
        0
    } else if new + dedup > 0 {
        3
    } else {
        4
    })
}
