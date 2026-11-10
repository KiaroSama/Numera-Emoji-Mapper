//! Native-owned historical render observations; no native codec supplies expected bytes.
use crate::codec_raster::OpenImage;
use crate::source_git_inventory::Source;
use crate::source_lottie_oracle::hash as digest_hex;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const PREVIEW_HASH: &str = "8f301692a45669d72fc3962ef4bb51c5af9765c6841acdbae919796a6b0364d1";
const GALLERY_HASH: &str = "b52cf02336bfd4faa5eb1e582b67ba5b8728afd4314894ccb21f0e3a4664b4e6";

fn recipes(preview: &Value, gallery: &Value) -> Result<Value, String> {
    let mut preview = preview.clone();
    let mut gallery = gallery.clone();
    for case in preview["cases"]
        .as_array_mut()
        .filter(|cases| cases.len() == 3)
        .ok_or("Lottie render requires three preview cases")?
    {
        case.as_object_mut()
            .ok_or("invalid preview case")?
            .remove("expected");
    }
    for case in gallery["cases"]
        .as_array_mut()
        .filter(|cases| cases.len() == 5)
        .ok_or("Lottie render requires five gallery cases")?
    {
        if case["format"] == "animated" {
            let case = case.as_object_mut().ok_or("invalid gallery case")?;
            case.remove("sha256");
            case.remove("bytes");
        }
    }
    Ok(json!([preview, gallery]))
}

pub fn fixtures() -> Result<(Value, Value), String> {
    let preview =
        include_str!("../../tests/fixtures/native/lottie-preview.json").replace("\r\n", "\n");
    let gallery =
        include_str!("../../tests/fixtures/native/gallery-codec.json").replace("\r\n", "\n");
    if digest_hex(preview.as_bytes()) != PREVIEW_HASH
        || digest_hex(gallery.as_bytes()) != GALLERY_HASH
    {
        return Err("pinned original Lottie render fixture bytes drift".into());
    }
    Ok((
        serde_json::from_str(&preview).map_err(|_| "invalid original preview fixture")?,
        serde_json::from_str(&gallery).map_err(|_| "invalid original gallery fixture")?,
    ))
}

pub fn fixture_identity(preview: &Value, gallery: &Value) -> Result<(), String> {
    let (original_preview, original_gallery) = fixtures()?;
    if recipes(preview, gallery)? != recipes(&original_preview, &original_gallery)? {
        return Err("Lottie render recipe drift; refusing execution".into());
    }
    Ok(())
}

fn input(root: &Path, case: &Value) -> Result<(PathBuf, Vec<u8>), String> {
    use std::io::Read;
    let relative = case["input"].as_str().ok_or("render input is missing")?;
    let path = Path::new(relative);
    if !relative.starts_with("tests/fixtures/")
        || relative.contains(['\\', ':'])
        || path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err("render input must be a confined fixture".into());
    }
    let path = root.join(path);
    crate::tooling_environment::reject_links(&path, "render fixture cannot follow links")?;
    if !crate::migration_bundle::resolved(&path)?.starts_with(root) {
        return Err("render fixture escaped project root".into());
    }
    let file = std::fs::File::open(&path).map_err(|_| "cannot open render fixture")?;
    if !file
        .metadata()
        .map_err(|_| "cannot inspect render fixture")?
        .is_file()
    {
        return Err("render fixture must be regular".into());
    }
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read render fixture")?;
    if bytes.len() > 2 * 1024 * 1024
        || case["input_sha256"].as_str() != Some(digest_hex(&bytes).as_str())
    {
        return Err("render input identity drift".into());
    }
    Ok((path, bytes))
}

fn render(
    py: Python<'_>,
    globals: &Bound<'_, PyDict>,
    source: &Path,
    output: &Path,
    options: (u32, u32, u32, bool),
) -> PyResult<()> {
    let (size, fps, quality, still) = options;
    let options = PyDict::new(py);
    options.set_item("size", size)?;
    options.set_item("quality", quality)?;
    if !still {
        options.set_item("fps", fps)?;
    }
    let path = py.import("pathlib")?.getattr("Path")?;
    globals
        .get_item(if still {
            "lottie_still_webp"
        } else {
            "lottie_preview_webp"
        })?
        .ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err("missing historical render function")
        })?
        .call(
            (
                path.call1((source.to_path_buf(),))?,
                path.call1((output.to_path_buf(),))?,
            ),
            Some(&options),
        )?;
    Ok(())
}

fn observation(py: Python<'_>, path: &Path) -> PyResult<Value> {
    let image = OpenImage(
        py.import("PIL.Image")?
            .call_method1("open", (path.to_path_buf(),))?,
    );
    let count: usize = image.0.getattr("n_frames")?.extract()?;
    let mut frames = Vec::with_capacity(count);
    for index in 0..count {
        image.0.call_method1("seek", (index,))?;
        image.0.call_method0("load")?;
        let rgba = OpenImage(image.0.call_method1("convert", ("RGBA",))?);
        let raw = rgba.0.call_method0("tobytes")?.cast_into::<PyBytes>()?;
        let hash = digest_hex(raw.as_bytes());
        rgba.close()?;
        let duration: Option<u64> = image
            .0
            .getattr("info")?
            .call_method1("get", ("duration",))?
            .extract()?;
        frames.push(json!({"rgba_sha256":hash,"duration":duration}));
    }
    let size: (u32, u32) = image.0.getattr("size")?.extract()?;
    image.close()?;
    let bytes = std::fs::read(path)?;
    Ok(
        json!({"bytes":bytes.len(),"webp_sha256":digest_hex(&bytes),"size":[size.0,size.1],"frames":frames}),
    )
}

pub fn generate(
    root: &Path,
    workspace: &Path,
    source: &Source,
    errors: &Source,
    preview: &Value,
    gallery: &Value,
) -> Result<Value, String> {
    crate::source_lottie_oracle::source_identity(source, errors)?;
    fixture_identity(preview, gallery)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if !workspace.starts_with(root.join("logs/tmp")) || workspace == root.join("logs/tmp") {
        return Err("render workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect render workspace")?
        .next()
        .is_some()
    {
        return Err("render workspace must be empty".into());
    }
    let preview_cases = preview["cases"].as_array().ok_or("missing preview cases")?;
    let gallery_cases = gallery["cases"].as_array().ok_or("missing gallery cases")?;
    // Admit all actual bytes before any historical definition executes.
    let inputs = preview_cases
        .iter()
        .chain(
            gallery_cases
                .iter()
                .filter(|case| case["format"] == "animated"),
        )
        .map(|case| input(&root, case))
        .collect::<Result<Vec<_>, _>>()?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        let pillow: String = py
            .import("PIL.Image")
            .and_then(|module| module.getattr("__version__"))
            .and_then(|value| value.extract())
            .map_err(|_| "cannot inspect external Pillow version")?;
        if preview["Pillow"].as_str() != Some(pillow.as_str()) {
            return Err("Lottie render requires its pinned external Pillow version".into());
        }
        let rlottie: String = py
            .import("importlib.metadata")
            .and_then(|module| module.call_method1("version", ("rlottie-python",)))
            .and_then(|value| value.extract())
            .map_err(|_| "cannot inspect external rlottie-python version")?;
        if rlottie != "1.3.8" {
            return Err("Lottie render requires rlottie-python1.3.8".into());
        }
        let globals = crate::source_lottie_oracle::namespace(
            py,
            source,
            errors,
            &["_load_lottie", "lottie_still_webp", "lottie_preview_webp"],
        )
        .map_err(|error| format!("historical render namespace failed: {error}"))?;
        let (mut observed_preview, mut observed_gallery) = fixtures()?;
        let original_preview = observed_preview.clone();
        for (index, case) in preview_cases.iter().enumerate() {
            let output = workspace.join(format!("preview-{index}.webp"));
            render(
                py,
                &globals,
                &inputs[index].0,
                &output,
                (
                    52,
                    15,
                    60,
                    case["still"].as_bool().ok_or("invalid preview still")?,
                ),
            )
            .map_err(|error| format!("historical preview failed: {error}"))?;
            let actual = observation(py, &output)
                .map_err(|error| format!("historical frame observation failed: {error}"))?;
            let durations = |value: &Value| -> Result<Vec<Value>, String> {
                Ok(value["frames"]
                    .as_array()
                    .ok_or("missing original render frames")?
                    .iter()
                    .map(|frame| frame["duration"].clone())
                    .collect())
            };
            let expected = &original_preview["cases"][index]["expected"];
            if actual["size"] != expected["size"] || durations(&actual)? != durations(expected)? {
                return Err("historical render size/duration drift".into());
            }
            observed_preview["cases"][index]["expected"] = actual;
            std::fs::remove_file(output).map_err(|_| "cannot clean historical preview")?;
        }
        for (offset, (index, case)) in gallery_cases
            .iter()
            .enumerate()
            .filter(|(_, case)| case["format"] == "animated")
            .enumerate()
        {
            let input_index = preview_cases.len() + offset;
            let output = workspace.join(format!("gallery-{index}.webp"));
            render(
                py,
                &globals,
                &inputs[input_index].0,
                &output,
                (
                    88,
                    9,
                    65,
                    case["still"].as_bool().ok_or("invalid gallery still")?,
                ),
            )
            .map_err(|error| format!("historical gallery failed: {error}"))?;
            let bytes = std::fs::read(&output).map_err(|_| "cannot observe historical gallery")?;
            observed_gallery["cases"][index]["sha256"] = json!(digest_hex(&bytes));
            observed_gallery["cases"][index]["bytes"] = json!(bytes.len());
            std::fs::remove_file(output).map_err(|_| "cannot clean historical gallery")?;
        }
        for (path, bytes) in &inputs {
            if std::fs::read(path).map_err(|_| "cannot verify render input preservation")? != *bytes
            {
                return Err("historical renderer changed input".into());
            }
        }
        Ok(
            json!({"preview":observed_preview,"gallery":observed_gallery,"cases_checked":5,
            "source_runtime":"CPython3.11.9","source_hash":source.sha256,"dependency_source_hash":errors.sha256,
            "supplied_ports":[],"platform_specific_bytes":true}),
        )
    })
}
