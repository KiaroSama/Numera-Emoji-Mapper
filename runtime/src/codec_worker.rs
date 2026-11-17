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
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(format!("{field} must not be a reparse point"));
        }
    }
    Ok(path)
}

fn output_field(request: &Value) -> Result<PathBuf, String> {
    let output = request
        .get("output")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "output must be a non-empty path".to_owned())?;
    if !output.is_absolute() {
        return Err("output must be absolute".into());
    }
    match std::fs::symlink_metadata(&output) {
        Ok(_) => {
            return Err(match request["operation"].as_str() {
                Some("convert") => "conversion output already exists",
                Some("gallery_thumb") => "gallery output already exists",
                _ => "preview output already exists",
            }
            .into());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("cannot inspect codec output".into()),
    }
    Ok(output)
}

fn metadata_result(
    source: &Path,
    operation: &str,
) -> Result<Value, crate::codec_error::CodecError> {
    crate::codec_image::metadata_operation(source, operation)
}

pub fn execute_typed_at(
    root: Option<&Path>,
    request: &Value,
) -> Result<Value, crate::codec_error::CodecError> {
    execute_typed_with(root, request, None)
}

fn execute_typed_with(
    root: Option<&Path>,
    request: &Value,
    log: Option<&crate::logging::RunLog>,
) -> Result<Value, crate::codec_error::CodecError> {
    use crate::codec_error::CodecError;
    if !request.is_object() {
        return Err(CodecError::value("expected a media request object"));
    }
    if let Some(message) = request
        .get("operation")
        .and_then(crate::codec_request::unhashable)
    {
        return Err(CodecError::type_error(message));
    }
    let operation = request
        .get("operation")
        .and_then(Value::as_str)
        .ok_or_else(|| CodecError::value("unsupported media operation"))?;
    if !matches!(
        operation,
        "fingerprint"
            | "compare"
            | "same_image"
            | "collision"
            | "prepare"
            | "preview"
            | "usable"
            | "publication_usable"
            | "dimensions"
            | "image_info"
            | "gallery_thumb"
            | "convert"
            | "animation_info"
    ) {
        return Err(CodecError::value("unsupported media operation"));
    }
    let source = path_field(request, "source").map_err(CodecError::value)?;
    let format = match request
        .get("format")
        .filter(|value| crate::codec_lottie_validate::truthy(value))
    {
        None => crate::codec_format::detect_typed(&source)?,
        Some(Value::String(value)) => value.as_str(),
        Some(value) => {
            if let Some(message) = crate::codec_request::unhashable(value) {
                return Err(CodecError::type_error(message));
            }
            if operation != "convert" {
                return Err(CodecError::value("unsupported media format"));
            }
            "unknown"
        }
    };
    if !matches!(format, "static" | "animated" | "video") && operation != "convert" {
        return Err(CodecError::value("unsupported media format"));
    }
    if operation == "publication_usable" {
        let root =
            root.ok_or_else(|| CodecError::value("publication admission requires a project root"))?;
        return crate::publication_media::execute_typed(root, &source, format);
    }
    if matches!(operation, "image_info" | "dimensions")
        || (operation == "usable" && format == "static")
    {
        return metadata_result(&source, operation);
    }
    if operation == "animation_info" {
        return crate::codec_image::animation_info_typed(&source).map(Value::Bool);
    }
    if operation == "preview" {
        let (size, fps, still) = crate::codec_request::preview(request)?;
        let output = output_field(request).map_err(CodecError::value)?;
        if format == "static" {
            return crate::codec_image::preview_static_typed(&source, &output, size);
        }
        if format == "animated" {
            return crate::codec_lottie_preview::preview_typed(&source, &output, size, fps, still);
        }
        if format == "video" {
            return crate::codec_video_preview::preview_at(
                root, &source, &output, size, fps, still,
            );
        }
    }
    if operation == "fingerprint" && format == "static" {
        let info = crate::codec_image::fingerprint_typed(&source)?;
        let extension = crate::codec_format::extension_typed(&source, "static")?;
        return Ok(
            json!({"content_key":info["content_key"],"phash":info["phash"],"fmt":"static","extension":extension}),
        );
    }
    if operation == "fingerprint" && format == "animated" {
        return crate::codec_lottie_identity::fingerprint_typed(&source);
    }
    if operation == "fingerprint" && format == "video" {
        return crate::codec_video::fingerprint_at(root, &source);
    }
    if operation == "convert" {
        let output = output_field(request).map_err(CodecError::value)?;
        if request.get("target_format").and_then(Value::as_str) == Some("static") {
            crate::codec_image::to_static_png_typed(&source, &output)?;
            return execute_typed_at(
                root,
                &json!({"operation":"fingerprint","source":output,"format":"static"}),
            );
        }
        if request.get("target_format").and_then(Value::as_str) == Some("animated") {
            crate::codec_lottie_package::package_typed(&source, &output)?;
            return crate::codec_lottie_identity::fingerprint_typed(&output);
        }
        if request.get("target_format").and_then(Value::as_str) == Some("video") {
            return crate::codec_video_encode::convert_at(root, &source, &output);
        }
        return Err(CodecError::value("invalid target format"));
    }
    if operation == "prepare"
        && !request
            .get("reencode")
            .unwrap_or(&Value::Bool(false))
            .is_boolean()
    {
        return Err(CodecError::value("reencode must be boolean"));
    }
    if operation == "prepare" {
        let mut warnings = Vec::new();
        let result = if format == "animated" {
            crate::codec_lottie_prepare::prepare(&source, request, &mut warnings)
        } else if format == "static" {
            crate::codec_image::prepare(&source, request, &mut warnings)
        } else {
            crate::codec_video_encode::prepare_at(root, &source, request, &mut warnings)
        };
        for warning in warnings {
            let warning = crate::logging::redact(&warning, &[]);
            if let Some(log) = log {
                log.event("WARNING", "codec", &warning);
            }
            eprintln!("WARNING: {warning}");
        }
        return result;
    }
    if operation == "gallery_thumb" {
        let output = output_field(request).map_err(CodecError::value)?;
        let still = request
            .get("still")
            .unwrap_or(&Value::Bool(false))
            .as_bool()
            .ok_or_else(|| CodecError::value("gallery still must be boolean"))?;
        if format == "animated" {
            return crate::codec_lottie_preview::gallery_typed(&source, &output, still);
        }
        if format == "static" {
            return crate::codec_image::gallery_static_typed(&source, &output);
        }
        if format == "video" {
            return crate::codec_video_preview::gallery_typed(&source, &output);
        }
    }
    if operation == "usable" && format == "animated" {
        crate::codec_lottie_validate::validate_file_typed(&source)?;
        return Ok(json!(true));
    }
    if operation == "usable" && format == "video" {
        return crate::codec_video_validate::usable_at(root, &source);
    }
    if operation == "collision" {
        return crate::codec_format::collision_request(&source, &request["key"]);
    }
    if matches!(operation, "compare" | "same_image") {
        let other = path_field(request, "other").map_err(CodecError::value)?;
        return crate::codec_compare::execute_at(
            root,
            &source,
            &other,
            format,
            operation == "compare",
        );
    }
    Err(CodecError::value("unsupported media operation"))
}

pub fn execute(request: &Value) -> Result<Value, String> {
    execute_at(None, request)
}

pub fn execute_at(root: Option<&Path>, request: &Value) -> Result<Value, String> {
    execute_typed_at(root, request).map_err(|error| error.message)
}

/// Verify the adapter's scratch directory before any worker or codec can use it.
pub fn physical_directory(root: &Path, directory: &Path) -> Result<(), String> {
    if !directory.is_absolute()
        || directory
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("codec directory must be absolute without parent traversal".into());
    }
    let root = root
        .canonicalize()
        .map_err(|_| "cannot resolve codec project root")?;
    let ancestor = directory
        .ancestors()
        .find(|path| std::fs::symlink_metadata(path).is_ok())
        .ok_or("codec directory has no existing parent")?;
    envelope_parent(&root, &ancestor.join("directory-check"))?;
    let mut current = ancestor.to_path_buf();
    for part in directory
        .strip_prefix(ancestor)
        .map_err(|_| "invalid codec directory")?
        .components()
    {
        current.push(part);
        match std::fs::create_dir(&current) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err("cannot create codec directory".into()),
        }
        envelope_parent(&root, &current.join("directory-check"))?;
    }
    Ok(())
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
    let log = std::sync::Arc::new(crate::logging::RunLog::open(
        root,
        "codec-worker",
        Vec::new(),
    ));
    log.install_panic_hook();
    let result = (|| -> Result<i32, String> {
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
            .map_err(|_| crate::codec_error::CodecError::value("invalid codec request JSON"))
            .and_then(|request| {
                if let Some(output) = request.get("output").and_then(Value::as_str) {
                    let output = Path::new(output);
                    if !output.is_absolute()
                        || envelope_parent(root, output)
                            .map_err(crate::codec_error::CodecError::value)?
                            != response_parent
                    {
                        return Err("codec output must stay in its owned envelope directory".into());
                    }
                    if output == request_path
                        || output == response_path
                        || output
                            .file_name()
                            .is_some_and(|name| name == "codec-cleanup-pending.json")
                    {
                        return Err(
                            "codec output must not alias its request, response or ownership marker"
                                .into(),
                        );
                    }
                }
                // Nested capture scratch belongs to this envelope, not a sibling project run.
                execute_typed_with(Some(&request_parent), &request, Some(&log))
            });
        let reply = match crate::codec_error::CodecError::worker_reply(operation) {
            Ok(reply) => reply,
            Err(error) => {
                // Source main does not catch this class. Refuse without inventing an error response.
                let diagnostic = format!("{}: {}", error.kind, error.message);
                let diagnostic = crate::logging::redact(&diagnostic, &[]);
                log.event("ERROR", "codec", &diagnostic);
                eprintln!("{diagnostic}");
                return Ok(1);
            }
        };
        if let Some(error) = &reply.error {
            log.event("ERROR", "codec", error);
        }
        let encoded =
            serde_json::to_vec(&reply.envelope).map_err(|_| "cannot serialize codec response")?;
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
        Ok(reply.exit)
    })();
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            log.event("ERROR", "codec", &error);
            2
        }
    };
    log.finish(code);
    code
}
