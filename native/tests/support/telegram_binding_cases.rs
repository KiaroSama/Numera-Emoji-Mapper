//! Registered compatibility observations; JSON transport faults are not TCP-disconnect parity.
use super::bot_api::{Reply, Server, TOKEN};
use super::mutation_proof::{SET, listing, mutation};
use super::tooling_fixture::Root;
use pyo3::exceptions::PyLookupError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyModule};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

fn member(file: &str, unique: &str, id: &str) -> Value {
    json!({"file_id":file,"file_unique_id":unique,"custom_emoji_id":id})
}
fn download(replies: &mut Vec<Reply>, file: &str, bytes: &[u8]) {
    replies.push(Reply::new(
        "getFile",
        &[("file_id", file)],
        json!({"ok":true,"result":{"file_path":"proof.png"}}),
    ));
    let mut body = Reply::new("getFile", &[], Value::Null);
    body.method = "GET proof.png".into();
    body.download = Some(bytes.into());
    replies.push(body);
}

#[pyclass]
struct Compare {
    source: PathBuf,
    ours: Vec<u8>,
    observed: Mutex<Vec<Vec<u8>>>,
}
#[pymethods]
impl Compare {
    fn __call__(
        &self,
        data: &Bound<'_, PyBytes>,
        source: PathBuf,
        format: &str,
    ) -> PyResult<Option<bool>> {
        assert_eq!(source, self.source);
        assert_eq!(format, "static");
        self.observed.lock().unwrap().push(data.as_bytes().into());
        println!("registered Telegram content comparison observed");
        // A fixed byte-comparison port, not the retained wrapper's tolerant codec comparison.
        Ok(Some(data.as_bytes() == self.ours))
    }
}
#[pyclass]
struct Applied {
    failure: Option<Py<PyAny>>,
    calls: usize,
}
#[pymethods]
impl Applied {
    fn __call__(&mut self, py: Python<'_>) -> PyResult<Option<bool>> {
        self.calls += 1;
        println!("registered Telegram applied callback observed");
        match &self.failure {
            Some(error) => Err(PyErr::from_value(error.bind(py).clone())),
            None => Ok(None),
        }
    }
}
fn envelope(value: Bound<'_, PyAny>) -> PyResult<Value> {
    Ok(serde_json::from_str(&value.extract::<String>()?).unwrap())
}
fn request(source: &Path, creating: bool, count: usize) -> Value {
    json!({"name":SET,"source":source,"format":"static","create":creating,
        "title":"Pack","user_id":111111111,"emojis":["😀"],"keywords":["kw"],
        "needs_repainting":false,"expected_before":count})
}
fn art(name: &str) -> Vec<u8> {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap();
    let path = project.join(case["input"].as_str().unwrap());
    assert_eq!(
        _native::tooling_manifest::digest(&path).unwrap(),
        case["input_sha256"]
    );
    std::fs::read(path).unwrap()
}

pub fn check(mode: &str) {
    let directory = std::env::current_dir()
        .unwrap()
        .join("registered Telegram assertions");
    std::fs::create_dir(&directory).unwrap();
    let root = Root(_native::tooling_environment::physical_root(&directory).unwrap());
    let ours = art("opaque");
    let foreign = art("alpha-edge");
    assert_ne!(ours, foreign);
    let source = root.0.join("source.png");
    std::fs::write(&source, &ours).unwrap();
    let old = member("old", "old-unique", "111111111");
    let theirs = member("foreign", "foreign-new", "222222222");
    let own = member("ours", "ours-new", "333333333");
    let initial = vec![old.clone()];
    let mut replies = Vec::new();
    match mode {
        "foreign" => {
            replies.push(listing(&initial));
            replies.push(mutation(false, false, &ours, json!([])));
            replies.push(listing(&[old.clone(), theirs.clone()]));
            download(&mut replies, "foreign", &foreign);
            replies.push(mutation(
                false,
                false,
                &ours,
                json!({"ok":true,"result":true}),
            ));
            replies.push(listing(&[old.clone(), theirs, own]));
            download(&mut replies, "foreign", &foreign);
            download(&mut replies, "ours", &ours);
        }
        "unknown" => {
            replies.push(listing(&initial));
            replies.push(mutation(false, false, &ours, json!([])));
            replies.push(Reply::new("getStickerSet", &[("name", SET)], json!([])));
        }
        "wrong-create" => {
            replies.push(mutation(true, false, &ours, json!([])));
            replies.push(listing(&[old.clone(), own]));
        }
        "public" => {
            replies.push(Reply::new(
                "getMe",
                &[],
                json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
            ));
            replies.push(listing(&initial));
            download(&mut replies, "old", &ours);
            replies.push(listing(&initial));
            replies.push(mutation(false, false, &ours, json!([])));
            let after = [old.clone(), own.clone()];
            replies.push(listing(&after));
            download(&mut replies, "ours", &ours);
            replies.push(listing(&after));
            download(&mut replies, "ours", &ours);
            replies.push(listing(&after));
            replies.push(mutation(
                false,
                false,
                &ours,
                json!({"ok":true,"result":true}),
            ));
            replies.push(mutation(
                true,
                false,
                &ours,
                json!({"ok":true,"result":true}),
            ));
            replies.push(Reply::new(
                "deleteStickerSet",
                &[("name", SET)],
                json!({"ok":false,"description":"definite rejection"}),
            ));
            replies.push(Reply::new("sendMessage", &[("chat_id","1"),("text","fixture")], json!({"ok":false,"description":"retry after 301","parameters":{"retry_after":301}})));
            for _ in 0..2 {
                replies.push(Reply::new("replaceStickerInSet", &[], json!([])));
            }
        }
        _ => panic!("unknown registered compatibility mode"),
    }
    let expected = replies
        .iter()
        .map(|reply| {
            (
                reply.method.clone(),
                reply.fields.clone(),
                reply.files.clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut server = Server::new(replies);
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = PyModule::new(py, "_registered_telegram_compatibility")?;
        _native::_native(&module)?;
        let client = module
            .getattr("NativeTelegram")?
            .call1((TOKEN, &server.base))?;
        let compare = Py::new(
            py,
            Compare {
                source: source.clone(),
                ours: ours.clone(),
                observed: Mutex::new(Vec::new()),
            },
        )?;
        let invoke = |creating, count| -> PyResult<Value> {
            envelope(client.call_method1(
                "mutate_json",
                (
                    TOKEN,
                    &server.base,
                    request(&source, creating, count).to_string(),
                    compare.bind(py),
                    py.None(),
                ),
            )?)
        };
        if mode == "public" {
            let call = |method: &str, data: Value, limit: Option<u64>| -> PyResult<Value> {
                let request = json!({"method":method,"data":data,"retries":1,"max_wait":limit});
                envelope(client.call_method1(
                    "call_json",
                    (
                        TOKEN,
                        &server.base,
                        request.to_string(),
                        Vec::<(String, String, String, Vec<u8>)>::new(),
                    ),
                )?)
            };
            assert_eq!(
                call("getMe", json!({}), None)?,
                json!({"ok":true,"result":{"username":"YourEmojiBot"}})
            );
            let ids = envelope(client.call_method1("fuids", (SET,))?)?;
            assert_eq!(ids, json!(["old-unique"]));
            let result = client.call_method1("download", ("old", 1))?;
            assert_eq!(result.get_item(0)?.cast::<PyBytes>()?.as_bytes(), ours);
            assert_eq!(
                envelope(result.get_item(1)?)?,
                json!({"ok":true,"result":null})
            );
            assert_eq!(invoke(false, 1)?, json!({"ok":true,"result":2}));
            assert_eq!(invoke(false, 2)?, json!({"ok":true,"result":null}));
            assert_eq!(invoke(true, 0)?, json!({"ok":true,"result":null}));
            let rejected = call("deleteStickerSet", json!({"name":SET}), None)?;
            assert_eq!(rejected["ok"], false);
            assert_eq!(rejected["kind"], "rejected");
            let flood = call(
                "sendMessage",
                json!({"chat_id":"1","text":"fixture"}),
                Some(300),
            )?;
            assert_eq!(flood["kind"], "flood");
            assert_eq!(flood["method"], "sendMessage");
            assert_eq!(flood["seconds"], 301);
            let failure = py
                .get_type::<PyLookupError>()
                .call1(("caller callback failure",))?;
            for broken in [false, true] {
                let callback = Py::new(
                    py,
                    Applied {
                        failure: broken.then(|| failure.clone().unbind()),
                        calls: 0,
                    },
                )?;
                let request =
                    json!({"method":"replaceStickerInSet","data":{},"retries":1,"max_wait":null});
                let result = client.call_method1(
                    "call_json",
                    (
                        TOKEN,
                        &server.base,
                        request.to_string(),
                        Vec::<(String, String, String, Vec<u8>)>::new(),
                        callback.bind(py),
                    ),
                );
                if broken {
                    let error = result.unwrap_err();
                    assert!(error.value(py).is(&failure));
                    assert!(error.is_instance_of::<PyLookupError>(py));
                } else {
                    let ambiguous = envelope(result?)?;
                    assert_eq!(ambiguous["ok"], false);
                    assert_eq!(ambiguous["kind"], "ambiguous");
                }
                assert_eq!(callback.borrow(py).calls, 1);
            }
            assert_eq!(
                *compare.borrow(py).observed.lock().unwrap(),
                vec![ours.clone(), ours.clone()]
            );
        } else {
            let actual = invoke(mode == "wrong-create", 1)?;
            if mode == "foreign" {
                assert_eq!(actual, json!({"ok":true,"result":3}));
                assert_eq!(
                    *compare.borrow(py).observed.lock().unwrap(),
                    vec![foreign.clone(), foreign.clone(), ours.clone()]
                );
            } else {
                assert_eq!(actual["ok"], false);
                assert_eq!(actual["kind"], "ambiguous");
                assert!(compare.borrow(py).observed.lock().unwrap().is_empty());
            }
        }
        Ok(())
    })
    .unwrap();
    let actual = server.finish().unwrap();
    assert_eq!(actual.len(), expected.len());
    for (request, (method, fields, files)) in actual.iter().zip(expected) {
        assert_eq!(request.method, method);
        assert_eq!(request.form, fields);
        assert_eq!(request.files, files);
    }
    assert_eq!(
        actual
            .iter()
            .filter(|request| request.method == "addStickerToSet")
            .count(),
        match mode {
            "foreign" | "public" => 2,
            "unknown" => 1,
            _ => 0,
        }
    );
    assert_eq!(std::fs::read(&source).unwrap(), ours);
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    root.clean().unwrap();
}
