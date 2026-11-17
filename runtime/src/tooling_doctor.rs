//! Native installation/configuration checks, without a separate language runtime.
use crate::{config::Config, logging::RunLog, native_raster, tooling_environment};
use std::path::Path;

pub fn unset_keys(template: &str, mut present: impl FnMut(&str) -> bool) -> Vec<String> {
    template
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let (key, _) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                return None;
            }
            (!present(key)).then(|| key.to_owned())
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn codec_roundtrip() -> Result<(), String> {
    let pixels = image::RgbaImage::from_raw(2, 1, vec![7, 11, 13, 0, 251, 3, 5, 255])
        .ok_or("cannot construct native codec check")?;
    let png = native_raster::encode_png(&pixels).map_err(|error| error.to_string())?;
    let decoded = native_raster::decode(&png).map_err(|error| error.to_string())?;
    if decoded.rgba != pixels {
        return Err("native PNG pixel roundtrip failed".into());
    }
    // WebP may discard hidden RGB at alpha zero; visible premultiplied pixels remain exact.
    let webp = native_raster::encode_webp(&pixels, Default::default())
        .map_err(|error| error.to_string())?;
    let decoded = native_raster::decode(&webp).map_err(|error| error.to_string())?;
    if native_raster::premultiplied_pixels(&decoded.rgba)
        != native_raster::premultiplied_pixels(&pixels)
    {
        return Err("native WebP pixel roundtrip failed".into());
    }
    Ok(())
}

pub fn run(root: &Path, args: &[String]) -> i32 {
    if args == ["--help"] || args == ["-h"] {
        println!(
            "Usage: numera-emoji doctor\nCheck native codecs, panel assets and local configuration. No installation or network access."
        );
        return 0;
    }
    let config = match Config::load(root.into()) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("ERROR: {}", crate::logging::redact(&error, &[]));
            return 1;
        }
    };
    let log = std::sync::Arc::new(RunLog::open(root, "doctor", config.secret_values()));
    log.install_panic_hook();
    let result = (|| -> Result<(), String> {
        if !args.is_empty() {
            return Err("doctor accepts no arguments".into());
        }
        let root = tooling_environment::physical_root(root)?;
        let panel = root.join("assets/panel.html");
        tooling_environment::reject_links(&panel, "panel asset cannot follow links")?;
        if !panel.is_file() {
            return Err("panel assets are missing beside this installation".into());
        }
        codec_roundtrip()?;
        println!("[OK] Native PNG and WebP codecs passed pixel roundtrips.");
        if root.join("package.json").is_file() {
            crate::package::verify(&root)?;
            println!("[OK] Installed package contents match their manifest.");
        }
        if !["ffmpeg", "ffprobe"]
            .iter()
            .all(|name| crate::codec_process::executable(name).is_ok())
        {
            println!("[!] ffmpeg/ffprobe not found: video emoji disabled.");
        }
        let template = root.join(".env.example");
        if template.is_file() {
            tooling_environment::reject_links(
                &template,
                "configuration template cannot follow links",
            )?;
            let text = std::fs::read_to_string(template).map_err(|error| error.to_string())?;
            for key in unset_keys(&text, |key| !config.value(key).is_empty()) {
                println!("[!] unset: {key}");
            }
        }
        println!("[OK] Environment check complete.");
        Ok(())
    })();
    let code = match result {
        Ok(()) => 0,
        Err(error) => {
            log.event("ERROR", "doctor", &error);
            1
        }
    };
    log.finish(code);
    code
}
