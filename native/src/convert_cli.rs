//! Source grouping, resume/quarantine and output ownership stay native; pixels use exact codecs.
use crate::{config::Config, logging::RunLog, media_adapter::MediaAdapter};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
const PRIORITY: [&str; 7] = ["svg", "png", "webp", "gif", "bmp", "jpg", "jpeg"];
struct Staging(PathBuf);
impl Staging {
    fn new(parent: &Path) -> Result<Self, String> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        loop {
            let path = parent.join(format!(
                "convert-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
    }
    fn pending(&self) -> bool {
        std::fs::read_dir(self.0.join("codecs"))
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
impl Drop for Staging {
    fn drop(&mut self) {
        if !self.pending()
            && let Err(error) = std::fs::remove_dir_all(&self.0)
        {
            eprintln!("WARNING: conversion staging cleanup failed: {error}");
        }
    }
}
fn groups(path: &Path) -> Result<Vec<Vec<PathBuf>>, String> {
    let mut groups = BTreeMap::<String, Vec<PathBuf>>::new();
    let mut files = std::fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    files.sort();
    for path in files {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if path.is_file() && PRIORITY.contains(&ext.as_str()) {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("source filename is not UTF-8")?
                .to_lowercase();
            groups.entry(name).or_default().push(path);
        }
    }
    for paths in groups.values_mut() {
        paths.sort_by_key(|p| {
            PRIORITY.iter().position(|ext| {
                p.extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(ext))
            })
        });
    }
    Ok(groups.into_values().collect())
}
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("Usage: numera-emoji make-emoji-pngs [--in DIR] [--out DIR] [--limit N]");
        return Ok(0);
    }
    let (mut input, mut output, mut limit) = (None::<PathBuf>, None::<PathBuf>, 0u64);
    let mut i = 0;
    while i < args.len() {
        let take = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            args.get(*i)
                .cloned()
                .ok_or("option requires a value".into())
        };
        match args[i].as_str() {
            "--in" => input = Some(take(&mut i)?.into()),
            "--out" => output = Some(take(&mut i)?.into()),
            "--limit" => {
                limit = take(&mut i)?
                    .parse()
                    .map_err(|_| "--limit must be 0 or greater")?
            }
            value => return Err(format!("unknown make-emoji-pngs option: {value}")),
        }
        i += 1;
    }
    let legacy = input.is_none();
    let input = input.unwrap_or_else(|| config.root.join("coins/logos"));
    if !legacy && !input.is_dir() {
        return Err("source folder not found".into());
    }
    let output = if legacy {
        config.root.join("coins/logos/emoji")
    } else {
        output.unwrap_or_else(|| {
            input.with_file_name(format!(
                "{}_emoji",
                input
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("images")
            ))
        })
    };
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let display_output = output.clone();
    let output = output.canonicalize().map_err(|e| e.to_string())?;
    let marker_root = if legacy {
        output.parent().ok_or("legacy output has no parent")?
    } else {
        &output
    };
    let marker = marker_root.join(".svg_cur");
    let skip = marker_root.join(".svg_skip.txt");
    let mut quarantined = match std::fs::read_to_string(&skip) {
        Ok(text) => text
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase)
            .collect::<BTreeSet<_>>(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeSet::new(),
        Err(e) => return Err(e.to_string()),
    };
    if marker.is_file() {
        let culprit = std::fs::read_to_string(&marker)
            .map_err(|e| e.to_string())?
            .trim()
            .to_lowercase();
        if !culprit.is_empty() {
            quarantined.insert(culprit.clone());
            println!(
                "QUARANTINE: '{culprit}' was interrupted mid-conversion; recorded in .svg_skip.txt for review."
            );
            std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&skip)
                .and_then(|mut f| writeln!(f, "{culprit}"))
                .map_err(|e| e.to_string())?;
        }
        std::fs::remove_file(&marker).map_err(|e| e.to_string())?;
    }
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
        scratch: output.join(".native-codec"),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let mut sources = if legacy {
        let mut sources = Vec::new();
        for (folder, extension) in [("svg", "svg"), ("png", "png")] {
            let directory = input.join(folder);
            if !directory.is_dir() {
                continue;
            }
            let mut paths = std::fs::read_dir(&directory)
                .map_err(|e| e.to_string())?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<Vec<_>>>()
                .map_err(|e| e.to_string())?;
            paths.sort();
            sources.extend(
                paths
                    .into_iter()
                    .filter(|path| {
                        path.extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
                    })
                    .map(|path| vec![path]),
            );
        }
        sources
    } else {
        groups(&input)?
    };
    let (mut made, mut svg_ok, mut raster_ok) = (0u64, 0u64, 0u64);
    let mut failed = BTreeSet::new();
    let mut done = BTreeSet::new();
    for group in sources.iter_mut() {
        if limit != 0 && made >= limit {
            break;
        }
        let name = group[0]
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("source filename is not UTF-8")?
            .to_lowercase();
        let svg = group[0]
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("svg"));
        if done.contains(&name) || quarantined.contains(&name) && (!legacy || svg) {
            continue;
        }
        let out = output.join(format!("{name}.png"));
        let current = out.metadata().ok().and_then(|m| m.modified().ok());
        let newer = current.is_some_and(|time| {
            group.iter().all(|p| {
                p.metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .is_some_and(|s| s <= time)
            })
        });
        let valid = newer
            && runtime.block_on(
                media.call(json!({"operation":"image_info","source":out,"format":"static"})),
            ) == Ok(json!({"size":[100,100],"mode":"RGBA","blank":false}));
        if valid {
            failed.remove(&name);
            done.insert(name);
            continue;
        }
        std::fs::write(&marker, name.as_bytes()).map_err(|e| e.to_string())?;
        let mut success = false;
        for source in group {
            let source = source.canonicalize().map_err(|e| e.to_string())?;
            let staging = Staging::new(&media.scratch)?;
            let staged = staging.0.join("output.png");
            let adapter = MediaAdapter {
                python: media.python.clone(),
                root: media.root.clone(),
                scratch: staging.0.join("codecs"),
            };
            match runtime.block_on(adapter.call(json!({"operation":"convert","source":source,"output":staged,"target_format":"static"}))) {
                Ok(_) => {
                    if runtime.block_on(adapter.call(json!({"operation":"image_info","source":staged,"format":"static"})))?
                        != json!({"size":[100,100],"mode":"RGBA","blank":false}) {
                        return Err("conversion produced an invalid emoji; original output preserved".into());
                    }
                    std::fs::OpenOptions::new().write(true).open(&staged).and_then(|file| file.sync_all()).map_err(|e| e.to_string())?;
                    std::fs::rename(&staged,&out).map_err(|e|e.to_string())?;
                    if source.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("svg")) { svg_ok += 1; } else { raster_ok += 1; }
                    success=true;made+=1;done.insert(name.clone());break
                }
                Err(error) => {
                    if staging.pending() { return Err(error); }
                    log.event("WARNING","convert",&error);
                }
            }
        }
        std::fs::remove_file(&marker).map_err(|e| e.to_string())?;
        if success {
            failed.remove(&name);
        } else {
            failed.insert(name);
        }
    }
    let failed = failed.len();
    if !quarantined.is_empty() {
        println!(
            "REVIEW: {} source(s) quarantined and skipped: {}. Fix them and delete their lines from {} to retry.",
            quarantined.len(),
            quarantined.into_iter().collect::<Vec<_>>().join(", "),
            skip.display()
        );
    }
    let total = std::fs::read_dir(&output)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "png"))
        .count();
    if legacy {
        println!(
            "DONE: made {made} this run (svg={svg_ok}, png={raster_ok}, failed={failed}); total emoji PNGs: {total}."
        );
    } else {
        println!(
            "DONE: made {made} this run (svg={svg_ok}, raster={raster_ok}, failed={failed}); total emoji PNGs in {}: {total}.",
            display_output.display()
        );
    }
    Ok(if failed == 0 {
        0
    } else if made > 0 {
        3
    } else {
        4
    })
}
