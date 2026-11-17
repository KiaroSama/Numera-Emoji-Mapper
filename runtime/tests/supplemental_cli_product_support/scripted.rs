//! Ordered reply adapter over the single shared owned numeric-loopback fixture.
use crate::http;
use serde_json::Value;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};
pub const TOKEN: &str = "test-only-token";
#[derive(Clone)]
pub struct Request {
    pub method: String,
    pub form: BTreeMap<String, String>,
    pub files: BTreeMap<String, Vec<u8>>,
}
pub struct Reply {
    method: String,
    fields: BTreeMap<String, String>,
    payload: Value,
    pub observation: Option<(std::path::PathBuf, String, Value)>,
}
impl Reply {
    pub fn new(method: &str, fields: &[(&str, &str)], payload: Value) -> Self {
        Self {
            method: method.into(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            payload,
            observation: None,
        }
    }
}
pub struct Server {
    pub base: String,
    inner: Option<http::Server>,
    pending: Arc<Mutex<VecDeque<Reply>>>,
    observed: Arc<Mutex<Vec<Request>>>,
}
impl Server {
    pub fn new(replies: Vec<Reply>) -> Self {
        let pending = Arc::new(Mutex::new(VecDeque::from(replies)));
        let observed = Arc::new(Mutex::new(Vec::new()));
        let queue = pending.clone();
        let transcript = observed.clone();
        let inner = http::Server::new(move |request| {
            let reply = queue
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected extra native supplemental request");
            assert_eq!(request.verb, "POST");
            assert_eq!(request.path, format!("/bot{TOKEN}/{}", reply.method));
            assert_eq!(request.method(), reply.method);
            assert!(
                !request
                    .headers
                    .get("content-type")
                    .is_some_and(|value| value.starts_with("multipart/")),
                "scripted read/delete/send fixture must never receive uploads"
            );
            let form = request.form();
            for (key, value) in reply.fields {
                assert_eq!(form.get(&key), Some(&value), "{} field {key}", reply.method);
            }
            if let Some((path, pointer, expected)) = reply.observation {
                use std::io::Read;
                numera_runtime::tooling_environment::reject_links(&path, "linked bot observation")
                    .unwrap();
                let file = std::fs::File::open(path).unwrap();
                let metadata = file.metadata().unwrap();
                assert!(metadata.is_file() && metadata.len() <= 16384);
                let mut bytes = Vec::new();
                file.take(16385).read_to_end(&mut bytes).unwrap();
                assert!(bytes.len() <= 16384);
                let document: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(
                    document.pointer(&pointer),
                    Some(&expected),
                    "durable observation before {}",
                    reply.method
                );
            }
            transcript.lock().unwrap().push(Request {
                method: request.method().into(),
                form,
                files: BTreeMap::new(),
            });
            http::Reply::json(reply.payload)
        });
        Self {
            base: inner.base.clone(),
            inner: Some(inner),
            pending,
            observed,
        }
    }
    pub fn finish(&mut self) -> Result<Vec<Request>, String> {
        drop(self.inner.take());
        let remaining = self.pending.lock().unwrap().len();
        if remaining != 0 {
            return Err(format!(
                "{remaining} native supplemental replies were not requested"
            ));
        }
        Ok(self.observed.lock().unwrap().clone())
    }
}
