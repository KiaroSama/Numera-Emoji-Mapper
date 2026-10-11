//! Retained Python coin consumers use native transport, retries and live upload proofs.
use crate::telegram::{
    ApiError, SetState, Telegram, Upload, input_sticker, trim_keywords, usable_fuids,
};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn encode(result: crate::telegram::Result<Value>) -> PyResult<String> {
    let value = match result {
        Ok(value) => json!({"ok":true,"result":value}),
        Err(error) => {
            let (kind, method, seconds) = match &error {
                ApiError::Rejected(_) => ("rejected", None, None),
                ApiError::Transport(_) => ("transport", None, None),
                ApiError::Ambiguous(_) => ("ambiguous", None, None),
                ApiError::FloodWait { method, seconds } => {
                    ("flood", Some(method.as_str()), Some(*seconds))
                }
                ApiError::Invalid(_) => ("invalid", None, None),
            };
            json!({"ok":false,"kind":kind,"error":error.to_string(),"method":method,"seconds":seconds})
        }
    };
    serde_json::to_string(&value)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}
fn object(raw: &str) -> PyResult<Value> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    if !value.is_object() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "request must be an object",
        ));
    }
    Ok(value)
}
fn text<'a>(value: &'a Value, key: &str) -> crate::telegram::Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| ApiError::Invalid(format!("missing {key}")))
}
fn fields(value: &Value) -> PyResult<BTreeMap<String, String>> {
    serde_json::from_value(value.clone())
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}
fn uploads(files: Vec<(String, String, String, Vec<u8>)>) -> Vec<Upload> {
    files
        .into_iter()
        .map(|(field, filename, mime, bytes)| Upload {
            field,
            filename,
            mime,
            bytes,
        })
        .collect()
}
fn callback(
    check: &Py<PyAny>,
    failure: &mut Option<PyErr>,
) -> crate::telegram::Result<Option<bool>> {
    match Python::attach(|py| check.call0(py)?.extract::<Option<bool>>(py)) {
        Ok(value) => Ok(value),
        Err(error) => {
            *failure = Some(error);
            Err(ApiError::Invalid("Python applied check failed".into()))
        }
    }
}
fn compare(
    tg: &Telegram,
    sticker: &Value,
    source: &str,
    fmt: &str,
    codec: &Py<PyAny>,
    failure: &mut Option<PyErr>,
) -> Option<bool> {
    let file_id = sticker["file_id"].as_str()?;
    let bytes = tg.download_bytes(file_id, 5).ok()?;
    match Python::attach(|py| {
        codec
            .call1(py, (PyBytes::new(py, &bytes), source, fmt))?
            .extract::<Option<bool>>(py)
    }) {
        Ok(value) => value,
        Err(error) => {
            *failure = Some(error);
            None
        }
    }
}
fn live(tg: &Telegram, name: &str) -> Option<Vec<Value>> {
    match tg.probe(name) {
        (SetState::Exists, Some(set)) => set["stickers"].as_array().cloned(),
        _ => None,
    }
}

#[pyclass]
pub struct NativeTelegram {
    client: Telegram,
}
#[pymethods]
impl NativeTelegram {
    #[new]
    fn new(token: String, base: String) -> PyResult<Self> {
        Telegram::new(token, base)
            .map(|client| Self { client })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
    fn read_call(&self, py: Python<'_>, method: String, data: String) -> PyResult<String> {
        let data = fields(&object(&data)?)?;
        py.detach(|| self.client.read_call(&method, &data))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
            .and_then(|v| {
                serde_json::to_string(&v)
                    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
            })
    }
    fn probe(&self, py: Python<'_>, name: String) -> PyResult<String> {
        let (state, value) = py.detach(|| self.client.probe(&name));
        serde_json::to_string(&json!([
            match state {
                SetState::Exists => "exists",
                SetState::Missing => "missing",
                SetState::Unknown => "unknown",
            },
            value
        ]))
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
    #[pyo3(signature = (token, base, request, files, applied=None))]
    fn call_json(
        &self,
        py: Python<'_>,
        token: String,
        base: String,
        request: String,
        files: Vec<(String, String, String, Vec<u8>)>,
        applied: Option<Py<PyAny>>,
    ) -> PyResult<String> {
        let request = object(&request)?;
        let method = request["method"]
            .as_str()
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("missing method"))?;
        let data = fields(&request["data"])?;
        let retries = request["retries"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid retries"))?;
        let max_wait = request["max_wait"].as_u64();
        let files = uploads(files);
        let mut failure = None;
        let result = py.detach(|| {
            let mut tg = Telegram::new(token, base)?;
            tg.max_flood_wait = max_wait;
            if let Some(check) = &applied {
                tg.call(
                    method,
                    &data,
                    &files,
                    retries,
                    Some(|| callback(check, &mut failure)),
                )
            } else {
                tg.call(
                    method,
                    &data,
                    &files,
                    retries,
                    None::<fn() -> crate::telegram::Result<Option<bool>>>,
                )
            }
        });
        if let Some(error) = failure {
            return Err(error);
        }
        encode(result)
    }
    fn download(
        &self,
        py: Python<'_>,
        file_id: String,
        retries: u32,
    ) -> PyResult<(Py<PyBytes>, String)> {
        let result = py.detach(|| self.client.download_bytes(&file_id, retries));
        match result {
            Ok(bytes) => Ok((PyBytes::new(py, &bytes).unbind(), encode(Ok(Value::Null))?)),
            Err(error) => Ok((PyBytes::new(py, &[]).unbind(), encode(Err(error))?)),
        }
    }
    fn live_count(&self, py: Python<'_>, name: String) -> PyResult<String> {
        encode(py.detach(|| {
            match self.client.probe(&name) {
                (SetState::Exists, Some(set)) => set["stickers"]
                    .as_array()
                    .map(|v| json!(v.len()))
                    .ok_or_else(|| ApiError::Ambiguous("live set has no stickers".into())),
                (SetState::Missing, _) => Ok(json!(0)),
                _ => Err(ApiError::Ambiguous(format!(
                    "live state of {name} is unknown"
                ))),
            }
        }))
    }
    fn fuids(&self, py: Python<'_>, name: String) -> PyResult<String> {
        let result = py.detach(|| live(&self.client, &name).and_then(|v| usable_fuids(&v)));
        serde_json::to_string(&result)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
    fn matches(
        &self,
        py: Python<'_>,
        sticker: String,
        source: String,
        fmt: String,
        codec: Py<PyAny>,
    ) -> PyResult<Option<bool>> {
        let sticker = object(&sticker)?;
        let mut failure = None;
        let result =
            py.detach(|| compare(&self.client, &sticker, &source, &fmt, &codec, &mut failure));
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(result)
    }
    fn mutate_json(
        &self,
        py: Python<'_>,
        token: String,
        base: String,
        request: String,
        codec: Py<PyAny>,
        max_wait: Option<u64>,
    ) -> PyResult<String> {
        let request = object(&request)?;
        let mut failure = None;
        let result = py.detach(|| mutation(token, base, &request, &codec, max_wait, &mut failure));
        if let Some(error) = failure {
            return Err(error);
        }
        encode(result)
    }
    fn sticker_json(
        &self,
        fmt: String,
        emojis: Vec<String>,
        keywords: Vec<String>,
    ) -> PyResult<String> {
        encode(input_sticker(&fmt, &emojis, &keywords))
    }
    fn keywords(&self, keywords: Vec<String>) -> Vec<String> {
        trim_keywords(&keywords)
    }
}

fn mutation(
    token: String,
    base: String,
    request: &Value,
    codec: &Py<PyAny>,
    max_wait: Option<u64>,
    failure: &mut Option<PyErr>,
) -> crate::telegram::Result<Value> {
    let mut tg = Telegram::new(token, base)?;
    tg.max_flood_wait = max_wait;
    let name = text(request, "name")?;
    let source = text(request, "source")?;
    let fmt = text(request, "format")?;
    let creating = request["create"]
        .as_bool()
        .ok_or_else(|| ApiError::Invalid("missing create flag".into()))?;
    let user = request["user_id"]
        .as_u64()
        .ok_or_else(|| ApiError::Invalid("invalid user ID".into()))?;
    let emojis: Vec<String> = serde_json::from_value(request["emojis"].clone())
        .map_err(|e| ApiError::Invalid(e.to_string()))?;
    let keywords: Vec<String> = serde_json::from_value(request["keywords"].clone())
        .map_err(|e| ApiError::Invalid(e.to_string()))?;
    let input = input_sticker(fmt, &emojis, &keywords)?;
    let file = crate::telegram::upload(std::path::Path::new(source), "file0")?;
    let before = if !creating && !request["expected_before"].is_null() {
        live(&tg, name).and_then(|stickers| usable_fuids(&stickers))
    } else {
        None
    };
    let mut data = BTreeMap::from([
        ("user_id".into(), user.to_string()),
        ("name".into(), name.into()),
    ]);
    let method = if creating {
        data.insert("title".into(), text(request, "title")?.into());
        data.insert("sticker_type".into(), "custom_emoji".into());
        data.insert("stickers".into(), json!([input]).to_string());
        if request["needs_repainting"] == true {
            data.insert("needs_repainting".into(), "true".into());
        }
        "createNewStickerSet"
    } else {
        data.insert("sticker".into(), input.to_string());
        "addStickerToSet"
    };
    let mut fired = false;
    {
        let mut check = || -> crate::telegram::Result<Option<bool>> {
            fired = true;
            let (state, set) = tg.probe(name);
            if state == SetState::Unknown {
                return Ok(None);
            }
            if state == SetState::Missing {
                return Ok(if creating { Some(false) } else { None });
            }
            let Some(stickers) = set.and_then(|set| set["stickers"].as_array().cloned()) else {
                return Ok(None);
            };
            if creating {
                return Ok(if stickers.len() == 1 {
                    compare(&tg, &stickers[0], source, fmt, codec, failure)
                } else {
                    None
                });
            }
            let (Some(old), Some(now)) = (&before, usable_fuids(&stickers)) else {
                return Ok(None);
            };
            let new = now.difference(old).collect::<Vec<_>>();
            if new.is_empty() {
                return Ok(Some(false));
            }
            if new.len() != 1 {
                return Ok(None);
            }
            let sticker = stickers.iter().find(|st| {
                st["file_unique_id"]
                    .as_str()
                    .is_some_and(|id| id == new[0].as_str())
            });
            Ok(sticker.and_then(|st| compare(&tg, st, source, fmt, codec, failure)))
        };
        tg.call(method, &data, &[file], 5, Some(&mut check))?;
    }
    if creating || !fired {
        return Ok(Value::Null);
    }
    let stickers = live(&tg, name).ok_or_else(|| {
        ApiError::Ambiguous(
            "addStickerToSet applied after retry but live state is unknown or missing".into(),
        )
    })?;
    if let (Some(old), Some(now)) = (&before, usable_fuids(&stickers)) {
        let mut found = false;
        for sticker in &stickers {
            if sticker["file_unique_id"]
                .as_str()
                .is_some_and(|id| now.contains(id) && !old.contains(id))
                && compare(&tg, sticker, source, fmt, codec, failure) == Some(true)
            {
                found = true;
                break;
            }
        }
        if !found {
            return Err(ApiError::Ambiguous(
                "addStickerToSet readback does not prove our source; count cannot be trusted"
                    .into(),
            ));
        }
    }
    Ok(json!(stickers.len()))
}
