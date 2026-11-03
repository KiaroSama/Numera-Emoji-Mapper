//! Read-only pinned-source inventory; explicit exclusive output, no module imports or live services.
use crate::{config::Config, logging::RunLog};
use std::path::PathBuf;

pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!(
            "Usage: numera-emoji source-inventory --baseline SHA [--output FILE]\nRead pinned Git Python sources without importing them. Inherited/decorated/dynamic discovery may remain incomplete. Output must be new and physically project-contained; no source is changed or retired."
        );
        return Ok(0);
    }
    let mut baseline = None;
    let mut output = None;
    let mut index = 0;
    while index < args.len() {
        let option = args[index].as_str();
        if !matches!(option, "--baseline" | "--output") {
            return Err(format!("unknown source inventory option: {option}"));
        }
        index += 1;
        let value = args
            .get(index)
            .filter(|value| !value.is_empty())
            .ok_or("source inventory option requires a value")?;
        let slot = if option == "--baseline" {
            &mut baseline
        } else {
            &mut output
        };
        if slot.replace(value.clone()).is_some() {
            return Err(format!("duplicate source inventory option: {option}"));
        }
        index += 1;
    }
    let baseline = baseline.ok_or("source inventory requires --baseline exact Git revision")?;
    if baseline.len() != 40
        || !baseline
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("source snapshot requires an exact lowercase40hex revision".into());
    }
    let root = crate::tooling_environment::physical_root(&config.root)?;
    let output = output.map(|path| {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            path
        } else {
            root.join(path)
        }
    });
    if let Some(path) = &output {
        if std::fs::symlink_metadata(path).is_ok() {
            return Err("source inventory output already exists".into());
        }
        let parent = path
            .parent()
            .ok_or("source inventory output has no parent")?;
        crate::codec_worker::physical_directory(&root, parent)?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "cannot initialize source inventory process monitor")?;
    let sources = runtime.block_on(crate::source_git_inventory::snapshot(&root, &baseline))?;
    let inventory = crate::source_git_inventory::inventory(&sources)?;
    if let Some(path) = output {
        let staged = crate::codec_video_encode::Staged::new(&path, "json")?;
        let result = (|| {
            let mut bytes = serde_json::to_vec_pretty(&inventory)
                .map_err(|_| "cannot serialize source inventory")?;
            bytes.push(b'\n');
            crate::atomic::write_bytes(&staged.0, &bytes)
                .map_err(|_| "cannot persist source inventory")?;
            staged.publish(&path).map(|_| ())
        })();
        staged.clean()?;
        result?;
    }
    let cases = inventory["files"]
        .as_array()
        .ok_or("source inventory requires files")?
        .iter()
        .map(|file| file["test_cases"].as_array().map_or(0, Vec::len))
        .sum::<usize>();
    let complete = inventory["discovery_complete"] == true;
    println!(
        "source_files={} source_cases={} discovery_complete={complete}",
        sources.len(),
        cases
    );
    log.event(
        "INFO",
        "source-inventory",
        &format!(
            "read-only source_files={} source_cases={cases} discovery_complete={complete}",
            sources.len()
        ),
    );
    Ok(0)
}
