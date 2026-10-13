//! Owned VP9 conversion and pixel-exact stream-copy preparation; originals survive refusal.
use crate::codec_video::VideoDecoder;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(crate) struct Staged(pub PathBuf);
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
impl Staged {
    pub(crate) fn new(output: &Path, extension: &str) -> Result<Self, String> {
        if !output.is_absolute() || std::fs::symlink_metadata(output).is_ok() {
            return Err("codec output must be absolute and must not already exist".into());
        }
        let parent = output.parent().ok_or("codec output has no parent")?;
        std::fs::create_dir_all(parent).map_err(|_| "cannot create codec output directory")?;
        loop {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".video-output-{}-{id}.{extension}",
                std::process::id()
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err("cannot stage video output".into()),
            }
        }
    }

    pub(crate) fn publish(&self, output: &Path) -> Result<u64, String> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.0)
            .map_err(|_| "cannot inspect staged video output")?;
        file.sync_all().map_err(|_| "cannot flush video output")?;
        let bytes = file
            .metadata()
            .map_err(|_| "cannot inspect video output")?
            .len();
        drop(file);
        crate::atomic::publish_new(&self.0, output)
            .map_err(|_| "cannot publish exclusive video output")?;
        Ok(bytes)
    }

    pub(crate) fn clean(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("cannot clean staged video output".into()),
        }
    }
}

pub async fn convert(
    decoder: &VideoDecoder,
    source: &Path,
    output: &Path,
    max_bytes: u64,
) -> Result<Value, String> {
    let input = decoder.decoder_args(source).await?;
    let staged = Staged::new(output, "webm")?;
    let result = async {
        let mut size = 0;
        for crf in [32, 40, 48, 56, 63] {
            let mut args = vec!["-nostdin".into(), "-v".into(), "error".into(), "-y".into(), "-t".into(), "3.0".into()];
            args.extend(input.clone());
            args.extend(["-i".into(), source.to_string_lossy().into_owned(), "-an".into(), "-vf".into(),
                "fps=30,scale=w=100:h=100:force_original_aspect_ratio=decrease:flags=lanczos,format=rgba,pad=100:100:(ow-iw)/2:(oh-ih)/2:color=0x00000000,format=yuva420p".into(),
                "-c:v".into(), "libvpx-vp9".into(), "-pix_fmt".into(), "yuva420p".into(),
                "-b:v".into(), "0".into(), "-crf".into(), crf.to_string(), "-auto-alt-ref".into(), "0".into(),
                "-deadline".into(), "good".into(), "-cpu-used".into(), "2".into(),
                staged.0.to_string_lossy().into_owned()]);
            crate::codec_process::capture(&decoder.root, &decoder.ffmpeg, &args, 1024 * 1024).await?;
            size = std::fs::metadata(&staged.0).map_err(|_| "cannot inspect encoded video")?.len();
            if size <= max_bytes { return staged.publish(output).map(|bytes| json!({"bytes":bytes})); }
        }
        Err(format!("could not fit {} under {max_bytes} bytes (got {size}).", source.file_name().ok_or("source has no filename")?.to_string_lossy()))
    }.await;
    staged.clean()?;
    result
}

pub async fn reencode(decoder: &VideoDecoder, source: &Path) -> Result<bool, String> {
    VideoDecoder::validate_source(source)?;
    let before = std::fs::read(source).map_err(|_| "cannot read video preparation source")?;
    // Use a unique sibling destination; only atomic replacement publishes back to the source.
    let marker = source.with_file_name(format!(
        ".remux-placeholder-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let staged = Staged::new(&marker, "webm")?;
    let decoded = async {
        let args = vec![
            "-nostdin".into(),
            "-v".into(),
            "error".into(),
            "-y".into(),
            "-i".into(),
            source.to_string_lossy().into_owned(),
            "-c".into(),
            "copy".into(),
            staged.0.to_string_lossy().into_owned(),
        ];
        crate::codec_process::capture(&decoder.root, &decoder.ffmpeg, &args, 1024 * 1024).await?;
        let info = decoder.probe(&staged.0).await?;
        if info["duration"]
            .as_f64()
            .ok_or("remux duration is unknown")?
            > 3.0
        {
            return Ok(None);
        }
        std::fs::read(&staged.0)
            .map(Some)
            .map_err(|_| "cannot read remux output".to_owned())
    }
    .await;
    staged.clean()?;
    let Ok(Some(bytes)) = decoded else {
        return Ok(false);
    };
    if bytes == before || bytes.len() > 256 * 1024 {
        return Ok(false);
    }
    crate::atomic::write_bytes(source, &bytes)
        .map_err(|_| "cannot replace video preparation source")?;
    Ok(true)
}
