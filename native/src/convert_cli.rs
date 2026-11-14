//! Source grouping, resume/quarantine and output ownership stay native; pixels use exact codecs.
use crate::{
    config::Config, logging::RunLog, media_adapter::MediaCodec,
    media_native_adapter::NativeMediaAdapter,
};
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
        let codecs = self.0.join("codecs");
        crate::process_owner::cleanup_pending(&codecs)
            // Settled failure evidence still belongs to this run if relocation failed.
            || std::fs::read_dir(&codecs)
                .map(|mut entries| entries.next().is_some())
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
    let media =
        NativeMediaAdapter::for_project(&config.root, config.root.join("logs/tmp/native-codec"))?;
    run_with_codec(config, args, log, |scratch| media.with_scratch(scratch))
}

pub fn run_with_codec<M: MediaCodec>(
    config: &Config,
    args: &[String],
    log: &RunLog,
    make_media: impl FnOnce(PathBuf) -> M,
) -> Result<u8, String> {
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
    let _output_owner =
        crate::locks::Lease::acquire(&output.join(".convert.lock"), &crate::logging::iso_utc())
            .map_err(|e| format!("conversion output is already owned: {e}"))?;
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
    // Production IPC stays inside the installation even when the explicitly
    // selected artwork/output folders belong to another directory or volume.
    #[cfg(numera_native_runtime)]
    let scratch = crate::tooling_environment::confined_directory(
        &config.root,
        "logs/tmp/native-convert",
        true,
    )?;
    #[cfg(not(numera_native_runtime))]
    let scratch = output.join(".native-codec");
    let media = make_media(scratch.clone());
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
            let staging = Staging::new(&scratch)?;
            let staged = staging.0.join("output.png");
            let adapter = media.with_scratch(staging.0.join("codecs"));
            match runtime.block_on(adapter.call(json!({"operation":"convert","source":source,"output":staged,"target_format":"static"}))) {
                Ok(_) => {
                    if runtime.block_on(adapter.call(json!({"operation":"image_info","source":staged,"format":"static"})))?
                        != json!({"size":[100,100],"mode":"RGBA","blank":false}) {
                        return Err("conversion produced an invalid emoji; original output preserved".into());
                    }
                    std::fs::OpenOptions::new().write(true).open(&staged).and_then(|file| file.sync_all()).map_err(|e| e.to_string())?;
                    #[cfg(not(numera_native_runtime))]
                    std::fs::rename(&staged,&out).map_err(|e|e.to_string())?;
                    #[cfg(numera_native_runtime)]
                    {
                        use std::io::Read;
                        // A verified 100x100 RGBA PNG is small. Bound the read
                        // and publish through an output sibling so cross-volume
                        // conversions retain atomic replacement as well.
                        const MAX_PNG: u64 = 4 * 1024 * 1024;
                        let mut bytes = Vec::new();
                        std::fs::File::open(&staged)
                            .and_then(|file| file.take(MAX_PNG + 1).read_to_end(&mut bytes))
                            .map_err(|error| error.to_string())?;
                        if bytes.len() as u64 > MAX_PNG {
                            return Err("converted PNG exceeds byte limit; original output preserved".into());
                        }
                        crate::atomic::write_bytes(&out, &bytes).map_err(|error| error.to_string())?;
                    }
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

#[cfg(all(test, not(numera_native_runtime)))]
mod cleanup_tests {
    use super::*;

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
        let root = parent.join(format!("convert cleanup guard {}", std::process::id()));
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
            drop(Staging(workspace.clone()));
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
