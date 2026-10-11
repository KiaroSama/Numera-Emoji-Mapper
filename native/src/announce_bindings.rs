//! Python coin announcement exports delegate routing, formatting and HTTP to native code.
use crate::{announce, config::Config, logging, telegram::Telegram};
use pyo3::prelude::*;
use serde_json::Value;
use std::path::PathBuf;

#[pyfunction]
pub fn announce_packs_json(
    py: Python<'_>,
    root: String,
    token: String,
    base: String,
    request: String,
) -> PyResult<String> {
    let request: Value = serde_json::from_str(&request)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    let packs = request["packs"]
        .as_array()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("packs must be a list"))?;
    if packs
        .iter()
        .any(|pack| !pack.is_object() || pack["name"].as_str().is_none_or(str::is_empty))
    {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "each pack must have a non-empty name",
        ));
    }
    let owner = request["owner"].as_u64().ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err("owner must be a non-negative integer")
    })?;
    let field = |key: &str| -> PyResult<&str> {
        request[key]
            .as_str()
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err(format!("missing {key}")))
    };
    let (bot, note, style) = (field("bot")?, field("note")?, field("style")?);
    let config =
        Config::load(PathBuf::from(root)).map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
    if request["worker_only"] == true
        && (config.value("WORKER_PUBLISH_URL").is_empty()
            || config.value("WORKER_PUBLISH_SECRET").is_empty())
    {
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "WORKER_PUBLISH_URL and WORKER_PUBLISH_SECRET must both be set",
        ));
    }
    let mut secrets = config.secret_values();
    secrets.push(token.clone());
    py.detach(|| {
        let result = if request["worker_only"] == true {
            let timeout = request["timeout"].as_f64().ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err("timeout must be positive finite seconds")
            })?;
            announce::worker(&config, packs, bot, note, style, timeout)
                .map(|()| "the worker".into())
        } else {
            let tg = Telegram::new(token, base).map_err(|e| {
                pyo3::exceptions::PyRuntimeError::new_err(logging::redact(&e.to_string(), &secrets))
            })?;
            announce::packs(&config, &tg, owner, packs, bot, note, style)
        };
        result.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(logging::redact(&e, &secrets)))
    })
}
