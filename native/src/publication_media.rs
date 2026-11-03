//! Publication blank-media admission is distinct from strict creation-time codec validation.
use pyo3::prelude::*;
use std::path::{Path, PathBuf};

pub fn video_sample_usable(raw: &[u8]) -> bool {
    let frames = raw
        .as_chunks::<{ crate::codec_video_timeline::FRAME_BYTES }>()
        .0;
    frames.is_empty()
        || frames
            .iter()
            .any(|frame| !crate::codec_raster::blank_rgba(frame))
}

fn static_usable_external(py: Python<'_>, source: &Path) -> PyResult<Option<bool>> {
    let module = py.import("PIL.Image")?;
    let image = match module.call_method1("open", (source.to_path_buf(),)) {
        Ok(image) => crate::codec_raster::OpenImage(image),
        Err(error) if error.is_instance_of::<pyo3::exceptions::PyException>(py) => {
            return Ok(Some(true));
        }
        Err(error) => return Err(error),
    };
    let result = crate::codec_raster::blank_image_external(py, &image.0)
        .map(|blank| blank.map(|blank| !blank));
    image.close()?;
    result
}

#[pyfunction]
pub fn publication_static_usable(py: Python<'_>, source: PathBuf) -> PyResult<bool> {
    crate::codec_video::VideoDecoder::validate_source(&source)
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    // PyResult cannot represent native uncertainty; native callers avoid this compatibility adapter.
    static_usable_external(py, &source)?.ok_or_else(|| {
        pyo3::exceptions::PyRuntimeError::new_err("native alpha histogram is unverified")
    })
}

fn static_usable(source: &Path) -> Result<bool, crate::codec_error::CodecError> {
    use crate::codec_error::CodecError;
    crate::codec_video::VideoDecoder::validate_source(source).map_err(CodecError::unverified)?;
    Python::initialize();
    Python::attach(|py| {
        static_usable_external(py, source).map_err(|error| CodecError::from_python(py, error))
    })?
    .ok_or_else(|| CodecError::unverified("native alpha histogram is unverified"))
}

pub(crate) fn execute_typed(
    root: &Path,
    source: &Path,
    format: &str,
) -> Result<serde_json::Value, crate::codec_error::CodecError> {
    use crate::codec_error::CodecError;
    crate::tooling_environment::physical_root(root).map_err(CodecError::unverified)?;
    if format == "static" {
        return static_usable(source).map(serde_json::Value::Bool);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| CodecError::value("cannot initialize publication admission runtime"))?;
    runtime
        .block_on(usable(root, source, format))
        .map(serde_json::Value::Bool)
        .map_err(CodecError::unverified)
}

pub async fn usable(root: &Path, source: &Path, format: &str) -> Result<bool, String> {
    crate::tooling_environment::physical_root(root)?;
    crate::codec_video::VideoDecoder::validate_source(source)?;
    match format {
        "static" => static_usable(source).map_err(|error| error.message),
        "animated" => Ok(true),
        "video" => {
            let decoder = match crate::codec_video::VideoDecoder::new(root) {
                Ok(decoder) => decoder,
                // Missing tools supply no blankness evidence; the upload decides instead.
                Err(_) => return Ok(true),
            };
            let input = match decoder.decoder_args_settled(source).await? {
                Ok(input) => input,
                Err(_) => return Ok(true),
            };
            let mut args = vec![
                "-nostdin".into(),
                "-v".into(),
                "error".into(),
                "-t".into(),
                "3.0".into(),
            ];
            args.extend(input);
            args.extend([
                "-i".into(),
                source.to_string_lossy().into_owned(),
                "-an".into(),
                "-threads".into(),
                "1".into(),
                "-filter_threads".into(),
                "1".into(),
                "-vf".into(),
                "fps=4,scale=64:64,format=rgba".into(),
                "-f".into(),
                "rawvideo".into(),
                "-".into(),
            ]);
            let environment = std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>();
            let config = crate::config::Config::load(root.to_path_buf())?;
            let budget =
                crate::codec_process::codec_budget(Some(config.value("EMOJI_FFMPEG_TIMEOUT")))?;
            match crate::codec_process::capture_settled(
                root,
                &decoder.ffmpeg,
                &args,
                16 * 1024 * 1024,
                None,
                &environment,
                budget,
            )
            .await?
            {
                Ok(captured) => Ok(video_sample_usable(&captured.stdout)),
                // A failed command is optional evidence only after its owner and scratch settle.
                Err(_) => Ok(true),
            }
        }
        // The original helper is not a validator; even a family tag supplies no blankness evidence.
        _ => Ok(true),
    }
}
