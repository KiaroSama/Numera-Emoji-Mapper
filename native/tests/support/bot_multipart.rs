//! Typed external stdlib MIME parsing keeps upload assertions out of handwritten boundary logic.
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use std::collections::BTreeMap;

type Parts = (BTreeMap<String, String>, BTreeMap<String, Vec<u8>>);

pub fn parse(content_type: &str, body: &[u8]) -> Result<Parts, String> {
    if body.len() > 65536 || content_type.len() > 256 || content_type.contains(['\r', '\n']) {
        return Err("excessive or invalid fixture multipart request".into());
    }
    let mut encoded =
        format!("Content-Type: {content_type}\r\nMIME-Version: 1.0\r\n\r\n").into_bytes();
    encoded.extend_from_slice(body);
    Python::initialize();
    Python::attach(|py| -> PyResult<_> {
        let options = PyDict::new(py);
        options.set_item("policy", py.import("email.policy")?.getattr("default")?)?;
        let message = py
            .import("email.parser")?
            .getattr("BytesParser")?
            .call((), Some(&options))?
            .call_method1("parsebytes", (PyBytes::new(py, &encoded),))?;
        let invalid = || pyo3::exceptions::PyValueError::new_err("malformed fixture MIME tree");
        if !message.call_method0("is_multipart")?.extract::<bool>()?
            || message.getattr("defects")?.len()? != 0
        {
            return Err(invalid());
        }
        let mut fields = BTreeMap::new();
        let mut files = BTreeMap::new();
        for part in message.call_method0("iter_parts")?.try_iter()? {
            let part = part?;
            if fields.len() + files.len() >= 8
                || part.getattr("defects")?.len()? != 0
                || part.call_method0("is_multipart")?.extract::<bool>()?
                || part
                    .call_method0("get_content_disposition")?
                    .extract::<String>()?
                    != "form-data"
            {
                return Err(invalid());
            }
            let options = PyDict::new(py);
            options.set_item("header", "content-disposition")?;
            let name: String = part
                .call_method("get_param", ("name",), Some(&options))?
                .extract()?;
            if name.is_empty() || fields.contains_key(&name) || files.contains_key(&name) {
                return Err(invalid());
            }
            let options = PyDict::new(py);
            options.set_item("decode", true)?;
            let bytes = part
                .call_method("get_payload", (), Some(&options))?
                .cast_into::<PyBytes>()?;
            if part.call_method0("get_filename")?.is_none() {
                let value = std::str::from_utf8(bytes.as_bytes())
                    .map_err(|_| invalid())?
                    .to_owned();
                fields.insert(name, value);
            } else {
                files.insert(name, bytes.as_bytes().to_vec());
            }
        }
        Ok((fields, files))
    })
    .map_err(|_| "cannot parse bounded fixture multipart request".into())
}
