//! Local files are converted by codecs; Rust owns catalog, traversal and ingest decisions.
use crate::{
    catalog::Catalog, collector::Collector, config::Config, logging::RunLog,
    media_adapter::MediaCodec, media_native_adapter::NativeMediaAdapter,
};
use serde_json::json;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let codecs = self.0.join("codecs");
        let stopped = !crate::process_owner::cleanup_pending(&codecs)
            // Quiescence alone does not make retained request/response evidence disposable.
            && std::fs::read_dir(&codecs)
                .map(|mut entries| entries.next().is_none())
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
    media: &impl MediaCodec,
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
    run_with_codec(config, args, log, |scratch| {
        NativeMediaAdapter::for_project(&config.root, scratch)
    })
}

pub fn run_with_codec<M: MediaCodec>(
    config: &Config,
    args: &[String],
    log: &RunLog,
    make_media: impl FnOnce(PathBuf) -> Result<M, String>,
) -> Result<u8, String> {
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
        return Err(format!(
            "phash threshold {threshold} is out of range: use -1 to disable near-duplicate merging, or 0..16"
        ));
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
    let adapter = make_media(workspace.0.join("codecs"))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let mut catalog = Catalog::open(
        &data.join("catalog.db"),
        &crate::logging::utc(),
        &config.root,
    )?;
    #[cfg(all(test, not(numera_native_runtime)))]
    crate::migration_crash_tests::barrier("writer-ingest");
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
    Ok(crate::collector::ingest_exit_code(
        u64::from(new > 0 || dedup > 0),
        failed,
    ))
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
        let root = parent.join(format!("local cleanup guard {}", std::process::id()));
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
