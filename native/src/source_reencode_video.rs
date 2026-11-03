//! Original video identity and exact subprocess recipes; only process custody is adapted.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[path = "source_media_video.rs"]
mod media_video;

#[derive(Default)]
pub(super) struct Calls {
    total: AtomicUsize,
    encodes: AtomicUsize,
    copies: AtomicUsize,
    completed: AtomicUsize,
    conversions: AtomicUsize,
    lavfi: AtomicUsize,
    alpha_frames: AtomicUsize,
    preview_encodes: AtomicUsize,
    previews: AtomicUsize,
}
impl Calls {
    pub(super) fn observed(&self) -> PyResult<serde_json::Value> {
        let total = self.total.load(Ordering::SeqCst);
        let encodes = self.encodes.load(Ordering::SeqCst);
        let copies = self.copies.load(Ordering::SeqCst);
        let completed = self.completed.load(Ordering::SeqCst);
        let conversions = self.conversions.load(Ordering::SeqCst);
        let lavfi = self.lavfi.load(Ordering::SeqCst);
        let alpha_frames = self.alpha_frames.load(Ordering::SeqCst);
        let preview_encodes = self.preview_encodes.load(Ordering::SeqCst);
        let previews = self.previews.load(Ordering::SeqCst);
        if !(32..=128).contains(&total)
            || encodes != 4
            || copies != 3
            || completed != total
            || conversions != 7
            || lavfi != 6
            || alpha_frames != 1
            || preview_encodes != 1
            || previews != 4
        {
            return Err(PyValueError::new_err(
                "original video process accounting incomplete",
            ));
        }
        Ok(
            serde_json::json!({"total":total,"encodes":encodes,"copies":copies,"completed":completed,
                "conversions":conversions,"lavfi":lavfi,"alpha_frames":alpha_frames,
                "preview_encodes":preview_encodes,"previews":previews}),
        )
    }
}

#[pyclass]
struct Media(Py<PyAny>);
#[pymethods]
impl Media {
    fn __call__(&self, py: Python<'_>) -> Py<PyAny> {
        self.0.clone_ref(py)
    }
}

#[pyclass]
struct Process {
    root: PathBuf,
    workspace: PathBuf,
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
    calls: Arc<Calls>,
}
impl Process {
    fn input(&self, raw: &str) -> PyResult<()> {
        let path = Path::new(raw);
        crate::tooling_environment::reject_links(path, "original video input cannot follow links")
            .map_err(PyValueError::new_err)?;
        let parent = crate::tooling_environment::physical_root(
            path.parent()
                .ok_or_else(|| PyValueError::new_err("video input has no parent"))?,
        )
        .map_err(PyValueError::new_err)?;
        let metadata = std::fs::symlink_metadata(path)?;
        if !path.is_absolute()
            || parent.parent() != Some(self.workspace.as_path())
            || !metadata.is_file()
            || metadata.len() > 2 * 1024 * 1024
        {
            return Err(PyValueError::new_err(
                "original video input escaped its context",
            ));
        }
        Ok(())
    }
    fn output(&self, raw: &str) -> PyResult<()> {
        let path = Path::new(raw);
        let parent = crate::tooling_environment::physical_root(
            path.parent()
                .ok_or_else(|| PyValueError::new_err("video output has no parent"))?,
        )
        .map_err(PyValueError::new_err)?;
        if !path.is_absolute()
            || parent.parent() != Some(self.workspace.as_path())
            || !matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("webm" | "mp4")
            )
        {
            return Err(PyValueError::new_err(
                "original video output must be fresh and confined",
            ));
        }
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(PyValueError::new_err(
                "original video output freshness could not be established",
            )),
        }
    }
    fn capture<'py>(&self, py: Python<'py>, args: Vec<String>) -> PyResult<Bound<'py, PyAny>> {
        if args.len() > 27 || self.calls.total.fetch_add(1, Ordering::SeqCst) >= 128 {
            return Err(PyValueError::new_err(
                "original video command bound exceeded",
            ));
        }
        let strings = args.iter().map(String::as_str).collect::<Vec<_>>();
        let ffmpeg = self
            .ffmpeg
            .to_str()
            .ok_or_else(|| PyValueError::new_err("ffmpeg path is not UTF8"))?;
        let ffprobe = self
            .ffprobe
            .to_str()
            .ok_or_else(|| PyValueError::new_err("ffprobe path is not UTF8"))?;
        let (tool, limit) = if strings.len() == 8 && strings[0] == ffprobe {
            if strings[1..7]
                != [
                    "-v",
                    "error",
                    "-show_entries",
                    "stream=index,codec_type,codec_name,width,height,avg_frame_rate:format=duration,format_name",
                    "-of",
                    "json",
                ]
            {
                return Err(PyValueError::new_err("original video probe argv drift"));
            }
            self.input(strings[7])?;
            (&self.ffprobe, 1024 * 1024)
        } else if strings == [ffmpeg, "-v", "error", "-decoders"] {
            (&self.ffmpeg, 1024 * 1024)
        } else if strings.len() == 18
            && strings[..17]
                == [
                    ffmpeg,
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=100x100:rate=30:duration=1",
                    "-frames:v",
                    "1",
                    "-c:v",
                    "libvpx-vp9",
                    "-pix_fmt",
                    "yuva420p",
                    "-crf",
                    "50",
                    "-b:v",
                    "0",
                    "-an",
                ]
        {
            self.output(strings[17])?;
            self.calls.encodes.fetch_add(1, Ordering::SeqCst);
            (&self.ffmpeg, 1024 * 1024)
        } else if strings.len() == 7
            && strings[..3] == [ffmpeg, "-y", "-i"]
            && strings[4..6] == ["-c", "copy"]
        {
            self.input(strings[3])?;
            self.output(strings[6])?;
            self.calls.copies.fetch_add(1, Ordering::SeqCst);
            (&self.ffmpeg, 1024 * 1024)
        } else if strings.len() == 9
            && strings[..5] == [ffmpeg, "-v", "error", "-y", "-i"]
            && strings[6..8] == ["-c", "copy"]
        {
            self.input(strings[5])?;
            self.output(strings[8])?;
            self.calls.copies.fetch_add(1, Ordering::SeqCst);
            (&self.ffmpeg, 1024 * 1024)
        } else if strings.len() == 15
            && strings[..8]
                == [
                    ffmpeg,
                    "-v",
                    "error",
                    "-t",
                    "3.0",
                    "-c:v",
                    "libvpx-vp9",
                    "-i",
                ]
            && strings[9..11] == ["-an", "-vf"]
            && matches!(
                strings[11],
                "fps=10,scale=64:64,format=rgba" | "scale=64:64,format=rgba"
            )
            && strings[12..] == ["-f", "rawvideo", "-"]
        {
            self.input(strings[8])?;
            (&self.ffmpeg, 16 * 1024 * 1024)
        } else if let Some(recipe) = media_video::classify(&strings, ffmpeg) {
            let limit = match recipe {
                media_video::Recipe::Conversion { input, output } => {
                    self.input(strings[input])?;
                    self.output(strings[output])?;
                    self.calls.conversions.fetch_add(1, Ordering::SeqCst);
                    1024 * 1024
                }
                media_video::Recipe::Lavfi { output } => {
                    self.output(strings[output])?;
                    self.calls.lavfi.fetch_add(1, Ordering::SeqCst);
                    1024 * 1024
                }
                media_video::Recipe::AlphaFrame { input } => {
                    self.input(strings[input])?;
                    self.calls.alpha_frames.fetch_add(1, Ordering::SeqCst);
                    1024 * 1024
                }
                media_video::Recipe::Preview { input } => {
                    self.input(strings[input])?;
                    self.calls.previews.fetch_add(1, Ordering::SeqCst);
                    104 * 104 * 4 * 90
                }
                media_video::Recipe::PreviewEncode { input, output } => {
                    self.input(strings[input])?;
                    self.output(strings[output])?;
                    self.calls.preview_encodes.fetch_add(1, Ordering::SeqCst);
                    1024 * 1024
                }
            };
            (&self.ffmpeg, limit)
        } else {
            return Err(PyValueError::new_err(
                "unexpected original video argv; refusing execution",
            ));
        };
        let environment = std::env::vars_os().collect();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| PyRuntimeError::new_err("cannot initialize original video owner"))?;
        let captured = py
            .detach(|| {
                runtime.block_on(crate::codec_process::capture_with_budget(
                    &self.root,
                    tool,
                    &args[1..],
                    limit,
                    None,
                    &environment,
                    crate::codec_process::Budget {
                        wall: Duration::from_secs(5),
                        idle: Duration::from_secs(3),
                        warning: None,
                    },
                ))
            })
            .map_err(PyRuntimeError::new_err)?;
        self.calls.completed.fetch_add(1, Ordering::SeqCst);
        py.import("subprocess")?
            .getattr("CompletedProcess")?
            .call1((
                args,
                0,
                PyBytes::new(py, &captured.stdout),
                PyBytes::new(py, &captured.stderr),
            ))
    }
}
#[pymethods]
impl Process {
    fn ffmpeg_path(&self) -> PyResult<String> {
        self.ffmpeg
            .to_str()
            .map(String::from)
            .ok_or_else(|| PyValueError::new_err("ffmpeg path is not UTF8"))
    }
    fn ffprobe_path(&self) -> PyResult<String> {
        self.ffprobe
            .to_str()
            .map(String::from)
            .ok_or_else(|| PyValueError::new_err("ffprobe path is not UTF8"))
    }
    #[pyo3(signature=(args, *, capture=false, timeout=None))]
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        args: Vec<String>,
        capture: bool,
        timeout: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let output = args.last().map(String::as_str);
        let preview_encode = matches!(
            media_video::classify(
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
                self.ffmpeg.to_str().unwrap_or(""),
            ),
            Some(media_video::Recipe::PreviewEncode { .. })
        );
        let expected_capture = matches!(output, Some("-" | "-decoders"))
            || args.first().map(String::as_str) == self.ffprobe.to_str()
            || args.get(2).is_some_and(|arg| arg == "-t")
            || preview_encode;
        if capture != expected_capture || timeout != preview_encode.then_some(30) {
            return Err(PyValueError::new_err(
                "original media capture contract drift",
            ));
        }
        self.capture(py, args)
    }
    #[pyo3(signature=(args, *, capture_output, check, timeout=None))]
    fn run<'py>(
        &self,
        py: Python<'py>,
        args: Vec<String>,
        capture_output: bool,
        check: bool,
        timeout: Option<u64>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let alpha_frame =
            args.iter().any(|arg| arg == "-pix_fmt") && args.last().is_some_and(|arg| arg == "-");
        if !capture_output || !check || timeout != if alpha_frame { None } else { Some(120) } {
            return Err(PyValueError::new_err("original subprocess contract drift"));
        }
        self.capture(py, args)
    }
}

pub(super) fn build<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: [&Source; 4],
    media: &Bound<'py, pyo3::types::PyModule>,
    identity: &Bound<'py, PyDict>,
) -> PyResult<(Bound<'py, PyAny>, Arc<Calls>)> {
    let [media_source, identity_source, video_source, errors] = sources;
    let calls = Arc::new(Calls::default());
    let root = workspace.join("process");
    std::fs::create_dir(&root)?;
    let process = Py::new(
        py,
        Process {
            root,
            workspace: workspace.into(),
            ffmpeg: crate::codec_process::executable("ffmpeg").map_err(PyRuntimeError::new_err)?,
            ffprobe: crate::codec_process::executable("ffprobe")
                .map_err(PyRuntimeError::new_err)?,
            calls: calls.clone(),
        },
    )?;
    media.setattr("_run", process.bind(py))?;
    for name in ["ffmpeg_path", "ffprobe_path"] {
        media.setattr(name, process.bind(py).getattr(name)?)?;
    }
    let bindings = PyDict::new(py);
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("json", py.import("json")?)?;
    bindings.set_item("VideoInfo", media.getattr("VideoInfo")?)?;
    bindings.set_item("ffprobe_path", media.getattr("ffprobe_path")?)?;
    bindings.set_item("_run", process.bind(py))?;
    let probe = crate::source_oracle_runtime::namespace(
        py,
        media_source,
        &["probe_video", "_fps"],
        &[
            "next",
            "any",
            "int",
            "str",
            "float",
            "TypeError",
            "ValueError",
            "ZeroDivisionError",
        ],
        &bindings,
    )?;
    media.setattr("probe_video", required(&probe, "probe_video")?)?;
    let format_bindings = PyDict::new(py);
    format_bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    format_bindings.set_item("_GZIP_MAGIC", media.getattr("_GZIP_MAGIC")?)?;
    let format_constants =
        crate::source_oracle_runtime::namespace(py, media_source, &[], &[], &format_bindings)?;
    for name in [
        "_EBML_MAGIC",
        "_PNG_MAGIC",
        "_GIF_MAGIC",
        "_RIFF_MAGIC",
        "RASTER_EXTS",
    ] {
        crate::source_oracle_runtime::assignment(py, media_source, name, &format_constants)?;
        format_bindings.set_item(name, required(&format_constants, name)?)?;
    }
    let format = crate::source_oracle_runtime::namespace(
        py,
        media_source,
        &["detect_format", "detect_format_bytes"],
        &["open"],
        &format_bindings,
    )?;
    media.setattr("detect_format", required(&format, "detect_format")?)?;
    let bindings = PyDict::new(py);
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("shutil", py.import("shutil")?)?;
    bindings.set_item("log", media.getattr("log")?)?;
    let exceptions = crate::source_oracle_runtime::namespace(
        py,
        errors,
        &["MediaError", "UndecodableVideo"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    bindings.set_item(
        "UndecodableVideo",
        required(&exceptions, "UndecodableVideo")?,
    )?;
    bindings.set_item(
        "_media",
        Py::new(py, Media(media.clone().into_any().unbind()))?,
    )?;
    let constants = crate::source_oracle_runtime::namespace(py, video_source, &[], &[], &bindings)?;
    for name in [
        "FRAME_SIDE",
        "FRAME_BYTES",
        "SAMPLE_FPS",
        "_ALPHA_CAPABLE",
        "_DECODER_CACHE_MAX",
        "_FRAME_CACHE_MAX",
    ] {
        crate::source_oracle_runtime::assignment(py, video_source, name, &constants)?;
        bindings.set_item(name, required(&constants, name)?)?;
    }
    for name in ["_decoder_cache", "_frame_cache"] {
        bindings.set_item(
            name,
            py.import("collections")?.getattr("OrderedDict")?.call0()?,
        )?;
    }
    bindings.set_item("_available", PyDict::new(py))?;
    let video = crate::source_oracle_runtime::namespace(
        py,
        video_source,
        &[
            "_ffmpeg_identity",
            "decoder_available",
            "decoder_args",
            "frames_rgba",
            "first_frame_bytes",
        ],
        &["str", "list", "len", "OSError", "Exception"],
        &bindings,
    )?;
    let video = crate::source_roster_modules::rebind(py, &video, "_original_reencode_video")?;
    identity.set_item("video_decode", &video)?;
    media.setattr("video_decode", &video)?;
    let helpers = crate::source_oracle_runtime::namespace(
        py,
        identity_source,
        &[
            "_video_frames_rgba",
            "_video_content_digest",
            "_first_video_frame",
            "_frame_image",
        ],
        &[],
        identity,
    )?;
    for name in [
        "_video_frames_rgba",
        "_video_content_digest",
        "_first_video_frame",
        "_frame_image",
    ] {
        identity.set_item(name, required(&helpers, name)?)?;
    }
    Ok((process.bind(py).getattr("run")?, calls))
}
