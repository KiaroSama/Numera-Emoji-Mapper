//! Native worker request boundary; third-party codecs never own application policy.
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_RESPONSE: AtomicU64 = AtomicU64::new(0);

fn staged_response(parent: &Path) -> Result<(PathBuf, std::fs::File), String> {
    loop {
        let next = NEXT_RESPONSE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".codec-response-{}-{next}.tmp", std::process::id()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("cannot stage exclusive codec response".into()),
        }
    }
}

fn path_field(request: &Value, field: &str) -> Result<PathBuf, String> {
    let raw = request
        .get(field)
        .and_then(Value::as_str)
        .filter(|raw| !raw.is_empty())
        .ok_or_else(|| format!("{field} must be a non-empty path"))?;
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(format!("{field} must be absolute"));
    }
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|_| format!("{field} must be a regular non-symlink file"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(format!("{field} must be a regular non-symlink file"));
    }
    Ok(path)
}

pub fn execute(request: &Value) -> Result<Value, String> {
    if !request.is_object() {
        return Err("expected a media request object".into());
    }
    let operation = request
        .get("operation")
        .and_then(Value::as_str)
        .ok_or("unsupported media operation")?;
    if !matches!(
        operation,
        "image_info"
            | "dimensions"
            | "fingerprint"
            | "convert"
            | "compare"
            | "same_image"
            | "preview"
            | "prepare"
    ) {
        return Err("unsupported native codec operation".into());
    }
    if request.get("format").and_then(Value::as_str) == Some("animated") {
        let source = path_field(request, "source")?;
        return match operation {
            "fingerprint" => crate::codec_lottie_identity::fingerprint(&source),
            "prepare" => {
                let reencode = request.get("reencode").cloned().unwrap_or(json!(false));
                let reencode = reencode.as_bool().ok_or("reencode must be boolean")?;
                if reencode {
                    crate::codec_lottie_prepare::rewrite(&source, None)?;
                }
                if let Some(tint) = request.get("tint") {
                    let tint = tint.as_str().ok_or("tint must be #RRGGBB")?;
                    let rgb = crate::codec_tint::parse_tint(tint)?;
                    if !crate::codec_lottie_prepare::rewrite(&source, Some(rgb))? {
                        return Err("requested tint could not be applied".into());
                    }
                }
                crate::codec_lottie_identity::fingerprint(&source)
            }
            "preview" => {
                request
                    .get("key")
                    .and_then(Value::as_str)
                    .filter(|key| !key.contains(['/', '\\']))
                    .ok_or("invalid preview parameters")?;
                let size = request.get("size").cloned().unwrap_or(json!(104));
                let fps = request.get("fps").cloned().unwrap_or(json!(15));
                let still = request.get("still").cloned().unwrap_or(json!(false));
                let size = size
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or("invalid preview parameters")?;
                let fps = fps
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or("invalid preview parameters")?;
                let still = still.as_bool().ok_or("invalid preview parameters")?;
                let output = request
                    .get("output")
                    .and_then(Value::as_str)
                    .filter(|path| !path.is_empty())
                    .map(PathBuf::from)
                    .ok_or("output must be a non-empty path")?;
                crate::codec_lottie_preview::preview(&source, &output, size, fps, still)
            }
            "convert"
                if request.get("target_format").and_then(Value::as_str) == Some("animated") =>
            {
                let output = request
                    .get("output")
                    .and_then(Value::as_str)
                    .filter(|path| !path.is_empty())
                    .map(PathBuf::from)
                    .ok_or("output must be a non-empty path")?;
                crate::codec_lottie_package::package(&source, &output)?;
                crate::codec_lottie_identity::fingerprint(&output)
            }
            _ => Err("animated codec operation is not yet supported".into()),
        };
    }
    if request.get("format").and_then(Value::as_str) != Some("static") {
        return Err("static codec operation requires static format".into());
    }
    let source = path_field(request, "source")?;
    if operation == "preview" {
        return Err("static preview operation is not yet supported".into());
    }
    if operation == "prepare" {
        let reencode = request.get("reencode").cloned().unwrap_or(json!(false));
        let reencode = reencode.as_bool().ok_or("reencode must be boolean")?;
        if request.get("tint").is_some() {
            return Err("static tint preparation is not yet supported".into());
        }
        if reencode {
            crate::codec_image::reencode_static(&source)?;
        }
    }
    if matches!(operation, "compare" | "same_image") {
        let other = path_field(request, "other")?;
        if operation == "compare"
            && std::fs::metadata(&source).ok().map(|m| m.len())
                == std::fs::metadata(&other).ok().map(|m| m.len())
            && std::fs::read(&source)
                .ok()
                .zip(std::fs::read(&other).ok())
                .is_some_and(|(a, b)| a == b)
        {
            return Ok(json!(true));
        }
        return Ok(json!(crate::codec_image::same_image(&source, &other)));
    }
    if operation == "convert" {
        if request.get("target_format").and_then(Value::as_str) != Some("static") {
            return Err("invalid target format".into());
        }
        let output = request
            .get("output")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .ok_or("output must be a non-empty path")?;
        crate::codec_image::to_static_png(&source, &output)?;
        return execute(&json!({"operation":"fingerprint","source":output,"format":"static"}));
    }
    let info = crate::codec_image::inspect_and_fingerprint(&source)?;
    Ok(match operation {
        "image_info" => json!({"size":info["size"],"mode":info["mode"],"blank":info["blank"]}),
        "dimensions" => info["size"].clone(),
        _ => {
            let mut header = [0_u8; 16];
            let count = std::fs::File::open(&source)
                .and_then(|mut input| input.read(&mut header))
                .map_err(|_| "cannot read static media header".to_owned())?;
            let header = &header[..count];
            let extension = if header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WEBP") {
                ".webp"
            } else if header.starts_with(b"GIF") {
                ".gif"
            } else {
                ".png"
            };
            json!({"content_key":info["content_key"],"phash":info["phash"],"fmt":"static","extension":extension})
        }
    })
}

fn envelope_parent(root: &Path, path: &Path) -> Result<PathBuf, String> {
    if path
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("codec envelope must not traverse its parent".into());
    }
    let root = root
        .canonicalize()
        .map_err(|_| "cannot resolve codec project root")?;
    let parent = path.parent().ok_or("codec envelope has no parent")?;
    let physical = parent
        .canonicalize()
        .map_err(|_| "codec envelope parent must exist")?;
    if !physical.starts_with(&root) {
        return Err("codec envelope must stay inside project".into());
    }
    for current in parent.ancestors() {
        let metadata =
            std::fs::symlink_metadata(current).map_err(|_| "codec envelope parent must exist")?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("codec envelope parent must be a physical directory".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("codec envelope parent must not be a reparse point".into());
            }
        }
        if current.canonicalize().ok().as_ref() == Some(&root) {
            return Ok(physical);
        }
    }
    Err("codec envelope must stay inside project".into())
}

pub fn run(root: &Path, arguments: &[String]) -> i32 {
    let log = crate::logging::RunLog::open(root, "codec-worker", Vec::new());
    let result = (|| -> Result<bool, String> {
        if arguments.len() != 2 {
            return Err("codec-worker requires request and response paths".into());
        }
        let request_path = PathBuf::from(&arguments[0]);
        let response_path = PathBuf::from(&arguments[1]);
        if !request_path.is_absolute() || !response_path.is_absolute() {
            return Err("codec envelope paths must be absolute".into());
        }
        let request_parent = envelope_parent(root, &request_path)?;
        let response_parent = envelope_parent(root, &response_path)?;
        if request_parent != response_parent || request_path == response_path {
            return Err("codec envelope must use distinct siblings in one owned directory".into());
        }
        let metadata = std::fs::symlink_metadata(&request_path)
            .map_err(|_| "codec request must be a regular file".to_owned())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 65536 {
            return Err("codec request must be a regular non-symlink file under64KiB".into());
        }
        if std::fs::symlink_metadata(&response_path).is_ok() {
            return Err("codec response already exists".into());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&request_path)
            .map_err(|_| "cannot open codec request".to_owned())?
            .take(65537)
            .read_to_end(&mut bytes)
            .map_err(|_| "cannot read codec request".to_owned())?;
        if bytes.len() > 65536 {
            return Err("codec request exceeds64KiB".into());
        }
        let operation = serde_json::from_slice::<Value>(&bytes)
            .map_err(|_| "invalid codec request JSON".to_owned())
            .and_then(|request| {
                if let Some(output) = request.get("output").and_then(Value::as_str) {
                    let output = Path::new(output);
                    if !output.is_absolute() || envelope_parent(root, output)? != response_parent {
                        return Err("codec output must stay in its owned envelope directory".into());
                    }
                }
                execute(&request)
            });
        let success = operation.is_ok();
        let response = match operation {
            Ok(value) => json!({"ok":true,"result":value}),
            Err(error) => json!({"ok":false,"error":crate::logging::redact(&error,&[])}),
        };
        let encoded =
            serde_json::to_vec(&response).map_err(|_| "cannot serialize codec response")?;
        if encoded.len() > 65536 {
            return Err("codec response exceeds64KiB".into());
        }
        let (staged, mut output) = staged_response(&response_parent)?;
        let persisted = output.write_all(&encoded).and_then(|_| output.sync_all());
        drop(output);
        let publish = persisted
            .and_then(|_| crate::atomic::publish_new(&staged, &response_path))
            .map_err(|_| "cannot publish exclusive codec response".to_owned());
        let _ = std::fs::remove_file(&staged);
        publish?;
        Ok(success)
    })();
    let code = match result {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(error) => {
            log.event("ERROR", "codec", &error);
            2
        }
    };
    log.finish(code);
    code
}
