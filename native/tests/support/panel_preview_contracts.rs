//! Original bounded-Lottie HTTP assertions use the existing persistence router and real codec.
use _native::panel_http::Panel;
use pyo3::prelude::*;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

pub fn check(
    client: &reqwest::blocking::Client,
    url: &str,
    panel: &Arc<Panel>,
    root: &Path,
    deadline: Instant,
) {
    let directory = root.join("preview contracts");
    std::fs::create_dir(&directory).unwrap();
    let good = directory.join("good.tgs");
    let bomb = directory.join("bomb.tgs");
    Python::attach(|py| -> PyResult<()> {
        let gzip = py.import("gzip")?;
        let serializer = py.import("json")?;
        let good_document = serializer.call_method1(
            "loads",
            (json!({"v":"5.5","w":512,"h":512,"fr":60,"ip":0,"op":60,"layers":[]}).to_string(),),
        )?;
        let bad_document = pyo3::types::PyDict::new(py);
        bad_document.set_item("pad", "A".repeat(8 * 1024 * 1024 + 1_000_000))?;
        for (path, document) in [(&good, good_document), (&bomb, bad_document.into_any())] {
            let bytes = serializer
                .call_method1("dumps", (document,))?
                .call_method1("encode", ("utf-8",))?;
            let options = pyo3::types::PyDict::new(py);
            options.set_item("mtime", 0)?;
            let bytes: Vec<u8> = gzip
                .call_method("compress", (bytes,), Some(&options))?
                .extract()?;
            std::fs::write(path, bytes)?;
        }
        Ok(())
    })
    .unwrap();
    let originals = [&good, &bomb]
        .into_iter()
        .map(|path| (path, std::fs::read(path).unwrap()))
        .collect::<Vec<_>>();
    let previous = panel.view.lock().unwrap().clone();
    let before = _native::migration_signature::signature(&panel.db, &[]).unwrap();
    *panel.view.lock().unwrap() = json!({"view":[],"by_key":{"good":good,"bomb":bomb},"hidden":0});
    let mut returned = BTreeMap::new();
    for (key, expected) in [("bomb", 404), ("good", 200), ("nope", 404)] {
        assert!(
            Instant::now() < deadline,
            "preview contract deadline exhausted"
        );
        let response = client.get(format!("{url}preview/{key}")).send().unwrap();
        assert_eq!(response.status().as_u16(), expected, "{key}");
        if key == "good" {
            assert_eq!(response.headers()["content-type"], "image/webp");
        }
        let bytes = response.bytes().unwrap();
        if key == "good" {
            assert!(bytes.len() >= 12);
            assert_eq!(&bytes[..4], b"RIFF");
            assert_eq!(&bytes[8..12], b"WEBP");
        } else {
            assert!(bytes.len() < 1024);
        }
        returned.insert(key, bytes);
    }
    *panel.view.lock().unwrap() = previous;
    assert_eq!(
        _native::migration_signature::signature(&panel.db, &[]).unwrap(),
        before
    );
    for (path, bytes) in originals {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    let cache = root.join("preview");
    let files = std::fs::read_dir(&cache)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(
        files.len(),
        1,
        "refused input must not leave preview staging"
    );
    assert_eq!(
        std::fs::read(&files[0]).unwrap().as_slice(),
        returned["good"].as_ref()
    );
    assert!(
        std::fs::read_dir(root.join("codec"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(!_native::process_owner::cleanup_pending(root));
}
