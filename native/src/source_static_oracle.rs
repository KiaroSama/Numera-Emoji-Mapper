//! Fixed historical static/provider observations; no native conversion supplies expectations.
use crate::codec_raster::{OpenImage, digest_hex as hash};
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const MEDIA_HASH: &str = "2e9c453de37a858db4b86cb3303bda9e2d2a2271fe12ac918924f97df8a42f53";
const PROVIDER_HASH: &str = "34742583025adaad3c7513ce0031c13d32759e29cfa8076609150a0badeb1acc";

pub fn source_identity(media: &Source, provider: &Source) -> Result<(), String> {
    for (source, path, expected) in [
        (media, "emojikit/media.py", MEDIA_HASH),
        (provider, "coins/_paprika_api.py", PROVIDER_HASH),
    ] {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != expected
            || hash(source.text.as_bytes()) != expected
        {
            return Err("static oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

pub fn fixtures() -> Result<(Value, Value), String> {
    let mut values = Vec::new();
    for (text, expected) in [
        (
            include_str!("../../tests/fixtures/native/static-convert.json"),
            "3d9ee8fa644edb16854b0c2fc8f323c32a8250179c0a4c007572178bba34ec50",
        ),
        (
            include_str!("../../tests/fixtures/native/provider-image.json"),
            "ee26433758106a054b876a46f83e6f2934e1590584949cad875dc4c53958752a",
        ),
    ] {
        let text = text.replace("\r\n", "\n");
        if hash(text.as_bytes()) != expected {
            return Err("pinned original static/provider fixture bytes drift".into());
        }
        values.push(
            serde_json::from_str(&text).map_err(|_| "invalid original static/provider fixture")?,
        );
    }
    let provider = values.pop().ok_or("missing provider fixture")?;
    let static_fixture = values.pop().ok_or("missing static fixture")?;
    Ok((static_fixture, provider))
}

fn recipe(fixture: &Value, count: usize) -> Result<Value, String> {
    let mut value = fixture.clone();
    let cases = value["cases"]
        .as_array_mut()
        .filter(|cases| cases.len() == count)
        .ok_or("static oracle requires its exact original case count")?;
    for case in cases {
        let case = case.as_object_mut().ok_or("invalid static oracle case")?;
        case.remove("expected");
        case.remove("error");
    }
    Ok(value)
}

pub fn fixture_identity(static_fixture: &Value, provider_fixture: &Value) -> Result<(), String> {
    let (original_static, original_provider) = fixtures()?;
    if recipe(static_fixture, 5)? != recipe(&original_static, 5)?
        || recipe(provider_fixture, 7)? != recipe(&original_provider, 7)?
    {
        return Err("static oracle recipe drift; refusing execution".into());
    }
    Ok(())
}

pub(crate) struct Input {
    pub(crate) path: Option<PathBuf>,
    pub(crate) bytes: Vec<u8>,
}

pub(crate) fn input(root: &Path, case: &Value) -> Result<Input, String> {
    use std::io::Read;
    let Some(relative) = case.get("input").filter(|value| !value.is_null()) else {
        let bytes = case["data"]
            .as_str()
            .ok_or("missing inline provider input")?
            .as_bytes()
            .to_vec();
        if bytes.len() > 2 * 1024 * 1024
            || case
                .get("input_sha256")
                .is_some_and(|expected| expected.as_str() != Some(hash(&bytes).as_str()))
        {
            return Err("static oracle input identity drift".into());
        }
        return Ok(Input { path: None, bytes });
    };
    let relative = relative.as_str().ok_or("invalid static oracle input")?;
    let path = Path::new(relative);
    if !relative.starts_with("tests/fixtures/")
        || relative.contains(['\\', ':'])
        || path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err("static oracle input must be a confined fixture".into());
    }
    let path = root.join(path);
    crate::tooling_environment::reject_links(&path, "static oracle input cannot follow links")?;
    if !crate::migration_bundle::resolved(&path)?.starts_with(root) {
        return Err("static oracle input escaped project root".into());
    }
    let file = std::fs::File::open(&path).map_err(|_| "cannot open static oracle input")?;
    if !file
        .metadata()
        .map_err(|_| "cannot inspect static oracle input")?
        .is_file()
    {
        return Err("static oracle input must be regular".into());
    }
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read static oracle input")?;
    if bytes.len() > 2 * 1024 * 1024 || case["input_sha256"].as_str() != Some(hash(&bytes).as_str())
    {
        return Err("static oracle input identity drift".into());
    }
    Ok(Input {
        path: Some(path),
        bytes,
    })
}

pub(crate) fn media_namespace<'py>(
    py: Python<'py>,
    media: &Source,
) -> PyResult<Bound<'py, PyDict>> {
    let bindings = PyDict::new(py);
    bindings.set_item("Image", py.import("PIL.Image")?)?;
    bindings.set_item("Path", py.import("pathlib")?.getattr("Path")?)?;
    bindings.set_item("io", py.import("io")?)?;
    bindings.set_item("SIZE", 100)?;
    bindings.set_item("VISIBLE_ALPHA", 10)?;
    bindings.set_item("BLANK_MAX_VISIBLE", 8)?;
    // The original independent driver deliberately substituted RuntimeError.
    // This reproduces its observation contract, not actual MediaError class parity.
    bindings.set_item(
        "MediaError",
        py.import("builtins")?.getattr("RuntimeError")?,
    )?;
    let builtins = ["min", "max", "round", "sum", "bytes", "str", "Exception"];
    crate::source_oracle_runtime::namespace(
        py,
        media,
        &[
            "_trim",
            "fit_100",
            "render_svg",
            "_load_image",
            "is_blank_image",
            "to_static_png",
        ],
        &builtins,
        &bindings,
    )
}

pub(crate) fn namespaces<'py>(
    py: Python<'py>,
    media: &Source,
    provider: &Source,
) -> PyResult<(Bound<'py, PyDict>, Bound<'py, PyDict>)> {
    let media = media_namespace(py, media)?;
    let bindings = PyDict::new(py);
    bindings.set_item("Image", py.import("PIL.Image")?)?;
    bindings.set_item("io", py.import("io")?)?;
    for (alias, name) in [("_fit_100", "fit_100"), ("_is_blank", "is_blank_image")] {
        bindings.set_item(
            alias,
            media.get_item(name)?.ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err("missing original media function")
            })?,
        )?;
    }
    let provider = crate::source_oracle_runtime::namespace(
        py,
        provider,
        &["to_emoji_png"],
        &["Exception"],
        &bindings,
    )?;
    Ok((media, provider))
}

fn observation(py: Python<'_>, path: &Path) -> PyResult<Value> {
    crate::tooling_environment::reject_links(path, "static oracle output cannot follow links")
        .map_err(pyo3::exceptions::PyValueError::new_err)?;
    if !std::fs::symlink_metadata(path)?.is_file() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "static oracle output must be regular",
        ));
    }
    let image = OpenImage(
        py.import("PIL.Image")?
            .call_method1("open", (path.to_path_buf(),))?,
    );
    image.0.call_method0("load")?;
    let size: (u32, u32) = image.0.getattr("size")?.extract()?;
    let mode: String = image.0.getattr("mode")?.extract()?;
    let rgba = OpenImage(image.0.call_method1("convert", ("RGBA",))?);
    let pixels = rgba.0.call_method0("tobytes")?.cast_into::<PyBytes>()?;
    let rgba_sha256 = hash(pixels.as_bytes());
    rgba.close()?;
    image.close()?;
    let bytes = std::fs::read(path)?;
    Ok(
        json!({"size":[size.0,size.1],"mode":mode,"bytes":bytes.len(),
        "png_sha256":hash(&bytes),"rgba_sha256":rgba_sha256}),
    )
}

fn absent(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err("refused static/provider input left an output or unknown state".into()),
    }
}

fn invariant(actual: &Value, original: &Value) -> Result<(), String> {
    for field in ["size", "mode", "rgba_sha256"] {
        if actual[field] != original[field] {
            return Err(format!("historical static/provider {field} drift"));
        }
    }
    Ok(())
}

pub fn generate(
    root: &Path,
    workspace: &Path,
    media: &Source,
    provider: &Source,
    static_fixture: &Value,
    provider_fixture: &Value,
) -> Result<Value, String> {
    source_identity(media, provider)?;
    fixture_identity(static_fixture, provider_fixture)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if !workspace.starts_with(root.join("logs/tmp")) || workspace == root.join("logs/tmp") {
        return Err("static oracle workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect static oracle workspace")?
        .next()
        .is_some()
    {
        return Err("static oracle workspace must be empty".into());
    }
    let (mut observed_static, mut observed_provider) = fixtures()?;
    let static_cases = static_fixture["cases"]
        .as_array()
        .ok_or("missing static cases")?;
    let provider_cases = provider_fixture["cases"]
        .as_array()
        .ok_or("missing provider cases")?;
    // Every input is admitted before an original definition can execute.
    let inputs = static_cases
        .iter()
        .chain(provider_cases)
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
        if observed_static["Pillow"].as_str() != Some(pillow.as_str())
            || observed_provider["Pillow"].as_str() != Some(pillow.as_str())
        {
            return Err("static oracle requires its pinned external Pillow version".into());
        }
        let resvg: String = py
            .import("importlib.metadata")
            .and_then(|module| module.call_method1("version", ("resvg-py",)))
            .and_then(|value| value.extract())
            .map_err(|_| "cannot inspect external resvg-py version")?;
        if resvg != "0.5.0" {
            return Err("static oracle requires resvg-py0.5.0".into());
        }
        let (media_namespace, provider_namespace) = namespaces(py, media, provider)
            .map_err(|error| format!("static oracle namespace failed: {error}"))?;
        let path = py
            .import("pathlib")
            .and_then(|module| module.getattr("Path"))
            .map_err(|error| format!("static oracle path API failed: {error}"))?;
        let mut outputs = Vec::new();
        for (index, input) in inputs[..static_cases.len()].iter().enumerate() {
            let output = workspace.join(format!("static-{index}.png"));
            let source = input
                .path
                .as_ref()
                .ok_or("static case requires a physical input")?;
            let function = media_namespace
                .get_item("to_static_png")
                .map_err(|_| "cannot inspect original static converter")?
                .ok_or("missing original static converter")?;
            let source = path
                .call1((source.to_path_buf(),))
                .map_err(|error| format!("static input path failed: {error}"))?;
            let destination = path
                .call1((output.clone(),))
                .map_err(|error| format!("static output path failed: {error}"))?;
            match function.call1((source, destination)) {
                Ok(_) => {
                    if observed_static["cases"][index].get("error").is_some() {
                        return Err("historical static refusal became success".into());
                    }
                    let actual = observation(py, &output).map_err(|error| {
                        format!("historical static observation failed: {error}")
                    })?;
                    invariant(&actual, &observed_static["cases"][index]["expected"])?;
                    observed_static["cases"][index]["expected"] = actual;
                    outputs.push(output);
                }
                Err(error) if error.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py) => {
                    let message = error
                        .value(py)
                        .str()
                        .map_err(|_| "cannot observe original static refusal")?
                        .to_string();
                    if observed_static["cases"][index]["error"].as_str() != Some(message.as_str()) {
                        return Err("historical static refusal drift".into());
                    }
                    absent(&output)?;
                    observed_static["cases"][index]["error"] = json!(message);
                }
                Err(error) => return Err(format!("historical static conversion failed: {error}")),
            }
        }
        let function = provider_namespace
            .get_item("to_emoji_png")
            .map_err(|_| "cannot inspect original provider converter")?
            .ok_or("missing original provider converter")?;
        for (index, input) in inputs[static_cases.len()..].iter().enumerate() {
            let output = workspace.join(format!("provider-{index}.png"));
            let destination = path
                .call1((output.clone(),))
                .map_err(|error| format!("provider output path failed: {error}"))?;
            let accepted: bool = function
                .call1((PyBytes::new(py, &input.bytes), destination))
                .and_then(|value| value.extract())
                .map_err(|error| format!("historical provider conversion failed: {error}"))?;
            if observed_provider["cases"][index]["expected"]["accepted"] != json!(accepted) {
                return Err("historical provider acceptance drift".into());
            }
            observed_provider["cases"][index]["expected"]["accepted"] = json!(accepted);
            if accepted {
                let actual = observation(py, &output)
                    .map_err(|error| format!("historical provider observation failed: {error}"))?;
                invariant(&actual, &observed_provider["cases"][index]["expected"])?;
                for field in ["size", "mode", "rgba_sha256", "png_sha256"] {
                    observed_provider["cases"][index]["expected"][field] = actual[field].clone();
                }
                outputs.push(output);
            } else {
                absent(&output)?;
            }
        }
        for input in &inputs {
            if let Some(path) = &input.path
                && std::fs::read(path)
                    .map_err(|_| "cannot verify static oracle input preservation")?
                    != input.bytes
            {
                return Err("historical static/provider conversion changed input".into());
            }
        }
        // No cleanup on error: retain generated evidence until the owning worker is stopped.
        for output in outputs {
            std::fs::remove_file(output).map_err(|_| "cannot clean static oracle output")?;
        }
        Ok(
            json!({"static":observed_static,"provider":observed_provider,"cases_checked":12,
            "source_runtime":"CPython3.11.9","source_hash":media.sha256,"dependency_source_hash":provider.sha256,
            "supplied_ports":["MediaError=RuntimeError"],"platform_specific_bytes":true}),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_oracle_admits_original_recipes_not_coordinated_input_output_drift() {
        let (static_fixture, provider_fixture) = fixtures().unwrap();
        fixture_identity(&static_fixture, &provider_fixture).unwrap();
        let mut observations = static_fixture.clone();
        observations["cases"][0]["expected"] = json!({"size":[1,1]});
        observations["cases"][2]["error"] = json!("untrusted output");
        let mut provider = provider_fixture.clone();
        provider["cases"][0]["expected"] = json!({"accepted":true});
        fixture_identity(&observations, &provider).unwrap();
        for mutation in ["input", "hash", "order", "extra"] {
            let mut changed = static_fixture.clone();
            match mutation {
                "input" => changed["cases"][0]["input"] = json!("tests/fixtures/other.png"),
                "hash" => changed["cases"][0]["input_sha256"] = json!("0".repeat(64)),
                "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
                "extra" => changed["unchecked"] = json!(true),
                _ => unreachable!(),
            }
            changed["cases"][0]["expected"] = json!({"size":[1,1]});
            assert!(
                fixture_identity(&changed, &provider_fixture).is_err(),
                "{mutation}"
            );
        }
        let mut changed = provider_fixture.clone();
        changed["cases"][6]["data"] = json!("different corrupt input");
        changed["cases"][6]["expected"] = json!({"accepted":true});
        assert!(fixture_identity(&static_fixture, &changed).is_err());
    }

    #[test]
    fn inline_oracle_bytes_accept_null_paths_but_refuse_hash_drift_and_excess() {
        let root = Path::new("not-an-oracle-directory");
        let original = json!({"data":"not an image"});
        let bytes = input(root, &original).unwrap();
        assert!(bytes.path.is_none());
        assert_eq!(bytes.bytes, b"not an image");
        let admitted = json!({"input":null,"data":"not an image",
            "input_sha256":"5464533c9647b67eb320c40ccc5959537c09102ae75388f6a7675b433e745c9d"});
        let bytes = input(root, &admitted).unwrap();
        assert!(bytes.path.is_none());
        assert_eq!(bytes.bytes, b"not an image");
        let mut changed = admitted.clone();
        changed["input_sha256"] = json!("0".repeat(64));
        assert!(input(root, &changed).is_err());
        assert!(input(root, &json!({"data":"x".repeat(2 * 1024 * 1024 + 1)})).is_err());
        assert_eq!(admitted["data"], "not an image");
        assert_eq!(original, json!({"data":"not an image"}));
    }

    #[test]
    fn static_oracle_refuses_forged_source_before_runtime_or_workspace_inspection() {
        let (static_fixture, provider_fixture) = fixtures().unwrap();
        let source = Source {
            baseline_revision: BASELINE.into(),
            path: "emojikit/media.py".into(),
            sha256: MEDIA_HASH.into(),
            text: "must not execute".into(),
        };
        let provider = Source {
            baseline_revision: BASELINE.into(),
            path: "coins/_paprika_api.py".into(),
            sha256: PROVIDER_HASH.into(),
            text: "must not execute".into(),
        };
        assert!(
            source_identity(&source, &provider)
                .unwrap_err()
                .contains("source identity drift")
        );
        assert!(
            generate(
                Path::new("missing"),
                Path::new("missing"),
                &source,
                &provider,
                &static_fixture,
                &provider_fixture
            )
            .unwrap_err()
            .contains("source identity drift")
        );
    }
}
