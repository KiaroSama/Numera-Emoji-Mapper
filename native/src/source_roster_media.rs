//! Original gallery fixture builders and renderer, with only the process boundary adapted.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const LOTTIE_HASH: &str = "f12c38be112dddb2ba6d3217034c8396479e6cfcf86cfeca22fa236381b0f9d1";

pub(crate) fn fixture(root: &Path) -> Result<(PathBuf, Vec<u8>), String> {
    use std::io::Read;
    let path = root.join("tests/fixtures/lottie/red_circle_512.json");
    crate::tooling_environment::reject_links(&path, "roster fixture cannot follow links")?;
    if !crate::tooling_environment::physical_root(
        path.parent().ok_or("roster fixture has no parent")?,
    )?
    .starts_with(root)
    {
        return Err("original roster fixture escaped project".into());
    }
    let file = std::fs::File::open(&path).map_err(|_| "cannot open original roster fixture")?;
    if !file
        .metadata()
        .map_err(|_| "cannot inspect roster fixture")?
        .is_file()
    {
        return Err("original roster fixture must be regular".into());
    }
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read roster fixture")?;
    if bytes.len() > 65536 || crate::codec_raster::digest_hex(&bytes) != LOTTIE_HASH {
        return Err("original roster Lottie fixture identity drift".into());
    }
    Ok((path, bytes))
}

#[pyclass]
struct Encoder {
    workspace: PathBuf,
    ffmpeg: PathBuf,
    calls: Arc<AtomicUsize>,
}
#[pymethods]
impl Encoder {
    fn ffmpeg_path(&self) -> PyResult<String> {
        self.ffmpeg
            .to_str()
            .map(String::from)
            .ok_or_else(|| PyValueError::new_err("original encoder path is not UTF8"))
    }

    #[pyo3(signature = (args, *, capture=false))]
    fn __call__(&self, py: Python<'_>, args: Vec<String>, capture: bool) -> PyResult<()> {
        if !capture || args.len() != 15 {
            return Err(PyValueError::new_err(
                "unexpected original roster encoder call",
            ));
        }
        let output = PathBuf::from(&args[14]);
        let parent = crate::tooling_environment::physical_root(
            output
                .parent()
                .ok_or_else(|| PyValueError::new_err("encoder output has no parent"))?,
        )
        .map_err(PyValueError::new_err)?;
        if parent.parent() != Some(self.workspace.as_path())
            || output.file_name().and_then(|s| s.to_str()) != Some("clip.webm")
            || std::fs::symlink_metadata(&output).is_ok()
        {
            return Err(PyValueError::new_err(
                "original roster encoding escaped its fresh context",
            ));
        }
        let expected = vec![
            self.ffmpeg_path()?,
            "-y".into(),
            "-v".into(),
            "error".into(),
            "-framerate".into(),
            "30".into(),
            "-i".into(),
            parent
                .join("frames")
                .join("%03d.png")
                .to_string_lossy()
                .into_owned(),
            "-c:v".into(),
            "libvpx-vp9".into(),
            "-pix_fmt".into(),
            "yuva420p".into(),
            "-auto-alt-ref".into(),
            "0".into(),
            parent.join("clip.webm").to_string_lossy().into_owned(),
        ];
        // Keep the historical argv exact; no shell or optional codec can enter this port.
        if args != expected {
            return Err(PyValueError::new_err("original roster encoder argv drift"));
        }
        for index in 0..3 {
            let path = parent.join(format!("frames/{index:03}.png"));
            crate::tooling_environment::reject_links(&path, "linked original roster frame")
                .map_err(PyValueError::new_err)?;
            let metadata = std::fs::metadata(&path)?;
            if !metadata.is_file() || metadata.len() > 65536 {
                return Err(PyValueError::new_err("invalid original roster frame"));
            }
        }
        if self.calls.fetch_add(1, Ordering::SeqCst) != 0 {
            return Err(PyValueError::new_err(
                "original roster encoder ran more than once",
            ));
        }
        let environment = std::env::vars_os().collect();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| {
                PyRuntimeError::new_err("cannot initialize original roster process owner")
            })?;
        py.detach(|| {
            runtime.block_on(crate::codec_process::capture_with_budget(
                &parent,
                &self.ffmpeg,
                &args[1..],
                4096,
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
        Ok(())
    }
}

pub(crate) fn build<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
) -> PyResult<(Bound<'py, PyAny>, Bound<'py, PyAny>, Arc<AtomicUsize>)> {
    let ffmpeg = crate::codec_process::executable("ffmpeg").map_err(PyRuntimeError::new_err)?;
    crate::codec_process::executable("ffprobe").map_err(PyRuntimeError::new_err)?;
    for (package, version) in [("Pillow", "12.3.0"), ("rlottie-python", "1.3.8")] {
        let actual: String = py
            .import("importlib.metadata")?
            .call_method1("version", (package,))?
            .extract()?;
        if actual != version {
            return Err(PyValueError::new_err(
                "original roster media dependency version drift",
            ));
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let encoder = Py::new(
        py,
        Encoder {
            workspace: workspace.into(),
            ffmpeg,
            calls: calls.clone(),
        },
    )?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let port = ns.call0()?;
    port.setattr("ffmpeg_path", encoder.bind(py).getattr("ffmpeg_path")?)?;
    port.setattr("_run", encoder)?;
    let bindings = PyDict::new(py);
    bindings.set_item("Image", py.import("PIL.Image")?)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("media", port)?;
    let builders = crate::source_oracle_runtime::namespace(
        py,
        sources[9],
        &["make_png", "encode_vp9"],
        &["str"],
        &bindings,
    )?;
    let fixtures = ns.call0()?;
    fixtures.setattr("HAS_FFMPEG", true)?;
    for name in ["make_png", "encode_vp9"] {
        fixtures.setattr(name, required(&builders, name)?)?;
    }
    let lottie = crate::source_lottie_oracle::namespace(
        py,
        sources[10],
        sources[11],
        &["_load_lottie", "lottie_still_webp", "lottie_preview_webp"],
    )?;
    let media = ns.call0()?;
    for name in ["lottie_still_webp", "lottie_preview_webp"] {
        media.setattr(name, required(&lottie, name)?)?;
    }
    Ok((fixtures, media, calls))
}
