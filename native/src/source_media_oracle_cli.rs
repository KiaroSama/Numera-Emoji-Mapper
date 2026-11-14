//! Contributor media-oracle commands; no operator configuration or production codec calls.
use crate::logging::RunLog;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";

fn write_new(path: &Path, value: &Value) -> Result<(), String> {
    let staged = crate::codec_video_encode::Staged::new(path, "json")?;
    let result = (|| {
        let mut bytes = serde_json::to_vec_pretty(value)
            .map_err(|_| "cannot serialize historical media observation")?;
        bytes.push(b'\n');
        crate::atomic::write_bytes(&staged.0, &bytes)
            .map_err(|_| "cannot persist historical media observation")?;
        staged.publish(path).map(|_| ())
    })();
    staged.clean()?;
    result
}

pub fn run(root: &Path, command: &str, args: &[String]) -> u8 {
    let (source_path, dependency_path, outputs) = match command {
        "source-lottie-render" => (
            "emojikit/media_lottie.py",
            "emojikit/errors.py",
            [
                ("lottie-preview-source.json", "preview"),
                ("gallery-codec-source.json", "gallery"),
            ],
        ),
        "source-static-images" => (
            "emojikit/media.py",
            "coins/_paprika_api.py",
            [
                ("static-convert-source.json", "static"),
                ("provider-image-source.json", "provider"),
            ],
        ),
        _ => {
            eprintln!("ERROR: unsupported historical media command");
            return 1;
        }
    };
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Usage: numera-emoji {command} --output-dir DIRECTORY\nGenerate fixed historical media observations. The directory must be new and project-contained; CPython 3.11.9 and the pinned external codecs are required. No production configuration is loaded."
        );
        return 0;
    }
    let log = std::sync::Arc::new(RunLog::open(root, command, vec![]));
    log.install_panic_hook();
    let result = (|| -> Result<(), String> {
        if args.len() != 2 || args[0] != "--output-dir" || args[1].is_empty() {
            return Err(format!("{command} requires --output-dir DIRECTORY"));
        }
        let root = crate::tooling_environment::physical_root(root)?;
        let output = PathBuf::from(&args[1]);
        let output = if output.is_absolute() {
            output
        } else {
            root.join(output)
        };
        if output
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err("media oracle output cannot contain parent traversal".into());
        }
        crate::tooling_environment::reject_links(
            &output,
            "media oracle output cannot follow links",
        )?;
        match std::fs::symlink_metadata(&output) {
            Ok(_) => return Err("media oracle output directory must be new".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("cannot inspect media oracle output directory".into()),
        }
        crate::codec_worker::physical_directory(
            &root,
            output.parent().ok_or("media oracle output has no parent")?,
        )?;
        let (first, second) = if command == "source-lottie-render" {
            crate::source_lottie_render::fixtures()?
        } else {
            crate::source_static_oracle::fixtures()?
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "cannot initialize historical media process monitor")?;
        let executable =
            std::env::current_exe().map_err(|_| "cannot locate historical media worker")?;
        let (source, dependency) = runtime.block_on(async {
            let source =
                crate::source_git_inventory::read_source(&root, BASELINE, source_path).await?;
            let dependency =
                crate::source_git_inventory::read_source(&root, BASELINE, dependency_path).await?;
            if command == "source-lottie-render" {
                crate::source_lottie_oracle::source_identity(&source, &dependency)?;
            } else {
                crate::source_static_oracle::source_identity(&source, &dependency)?;
            }
            Ok::<_, String>((source, dependency))
        })?;
        let parent = crate::tooling_environment::confined_directory(&root, "logs/tmp", true)?;
        let workspace = loop {
            let path = parent.join(format!(
                "media-source-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => break path,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err("cannot create historical media scratch".into()),
            }
        };
        let report = if command == "source-lottie-render" {
            runtime.block_on(crate::source_oracle_worker::generate_lottie_render(
                &root,
                &executable,
                &workspace,
                &source,
                &dependency,
                &first,
                &second,
            ))?
        } else {
            runtime.block_on(crate::source_oracle_worker::generate_static(
                &root,
                &executable,
                &workspace,
                (&source, &dependency),
                (&first, &second),
            ))?
        };
        if crate::process_owner::cleanup_pending(&workspace)
            || std::fs::read_dir(&workspace)
                .map_err(|_| "cannot inspect media oracle cleanup")?
                .next()
                .is_some()
        {
            return Err("historical media scratch remains unresolved".into());
        }
        std::fs::create_dir(&output)
            .map_err(|_| "cannot create exclusive media output directory")?;
        for (name, key) in outputs {
            write_new(
                &output.join(name),
                report
                    .get(key)
                    .ok_or("missing historical media observation")?,
            )?;
        }
        std::fs::remove_dir(&workspace).map_err(|_| "cannot clean historical media scratch")?;
        let message = if command == "source-lottie-render" {
            "Pinned Lottie source oracle: 3 preview cases plus 2 animated gallery cases; no target output used."
        } else {
            "Pinned static source oracle: 5 conversion cases plus 7 provider image cases; no target output used."
        };
        log.event("INFO", command, message);
        println!("{message}");
        Ok(())
    })();
    let code = match result {
        Ok(()) => 0,
        Err(error) => {
            log.event("ERROR", command, &error);
            1
        }
    };
    log.finish(i32::from(code));
    code
}
