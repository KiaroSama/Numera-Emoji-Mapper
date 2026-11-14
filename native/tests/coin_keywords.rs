//! Exact pinned UTF-8/CRLF CSV, name retention and distinct image admission policy.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[test]
fn keyword_writer_preserves_source_csv_names_order_and_verified_image_admission() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/keyword-writer.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "7c157ebff8fd5dbb8ec28c87ce8e85b70559601a8671b01c2510fe4e9f70f75f"
    );
    for case in fixture["valid"].as_array().unwrap() {
        let path = root.join(case["input"].as_str().unwrap());
        let hash: String = Sha256::digest(std::fs::read(&path).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(hash, case["input_sha256"]);
        assert_eq!(
            _native::codec_image::valid_logo(&path),
            case["expected"].as_bool().unwrap()
        );
    }
    let scratch = root
        .join("logs/tmp")
        .join(format!("keyword writer {}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let path = scratch.join(format!("keywords-{index}.csv"));
        std::fs::write(&path, case["old"].as_str().unwrap().as_bytes()).unwrap();
        let mut rows = case["rows"].as_object().unwrap().clone();
        for row in rows.values_mut() {
            if let Some(input) = row.get("path").and_then(Value::as_str) {
                row["path"] = Value::String(root.join(input).to_string_lossy().into_owned());
            }
        }
        _native::coin_keywords::write(&rows, &path).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            case["expected"].as_str().unwrap().as_bytes()
        );
        std::fs::remove_file(&path).unwrap();
    }
    let corrupt = scratch.join("corrupt.png");
    std::fs::write(&corrupt, b"not PNG").unwrap();
    assert!(!_native::codec_image::valid_logo(&corrupt));
    std::fs::remove_file(corrupt).unwrap();
    let broken = scratch.join("broken.csv");
    std::fs::write(&broken, b"other,name\r\nx,old\r\n").unwrap();
    assert!(_native::coin_keywords::write(&serde_json::Map::new(), &broken).is_err());
    assert_eq!(std::fs::read(&broken).unwrap(), b"other,name\r\nx,old\r\n");
    std::fs::remove_file(broken).unwrap();
    original_cached_image_and_name_assertions(&scratch);
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0);
    std::fs::remove_dir(scratch).unwrap();
}

fn original_cached_image_and_name_assertions(scratch: &std::path::Path) {
    use pyo3::prelude::*;
    use serde_json::json;
    const OLD: &str =
        "ticker,name,format,file,keywords\nbtc,Bitcoin,svg,logos/svg/btc.svg,\"btc, Bitcoin\"\n";
    let images = scratch.join("logos/png");
    std::fs::create_dir_all(&images).unwrap();
    let bad = images.join("bad.png");
    let good = images.join("ok.png");
    let bad_bytes = b"<html>429 Too Many Requests</html>";
    std::fs::write(&bad, bad_bytes).unwrap();
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = py.import("PIL.Image")?;
        assert_eq!(
            module.getattr("__version__")?.extract::<String>()?,
            "12.3.0"
        );
        let image = module.call_method1("new", ("RGBA", (8, 8), (1, 2, 3, 255)))?;
        let saved = image.call_method1("save", (&good, "PNG"));
        let closed = image.call_method0("close");
        saved?;
        closed?;
        Ok(())
    })
    .unwrap();
    let good_bytes = std::fs::read(&good).unwrap();
    let path = scratch.join("keywords.csv");
    let rows = json!({
        "bad":{"ticker":"bad","name":"","format":"png","file":"logos/png/bad.png"},
        "ok":{"ticker":"ok","name":"","format":"png","file":"logos/png/ok.png"},
    });
    _native::coin_keywords::write(rows.as_object().unwrap(), &path).unwrap();
    let mut csv = csv::Reader::from_path(&path).unwrap();
    assert_eq!(
        csv.headers().unwrap().iter().collect::<Vec<_>>(),
        ["ticker", "name", "format", "file", "keywords"]
    );
    let actual = csv.records().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(actual.len(), 1);
    assert_eq!(
        actual[0].iter().collect::<Vec<_>>(),
        ["ok", "", "png", "logos/png/ok.png", "ok"]
    );
    drop(csv);
    assert_eq!(std::fs::read(&bad).unwrap(), bad_bytes);
    assert_eq!(std::fs::read(&good).unwrap(), good_bytes);
    std::fs::write(&path, OLD.as_bytes()).unwrap();
    let rows = json!({"btc":{"ticker":"btc","name":"","format":"svg","file":"logos/svg/btc.svg"}});
    _native::coin_keywords::write(rows.as_object().unwrap(), &path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"ticker,name,format,file,keywords\r\nbtc,Bitcoin,svg,logos/svg/btc.svg,\"btc, Bitcoin\"\r\n");
    assert_eq!(
        _native::coin_keywords::old_names(&path).unwrap()["btc"],
        "Bitcoin"
    );
    assert!(!scratch.join("keywords.csv.tmp").exists());
    assert_eq!(std::fs::read_dir(scratch).unwrap().count(), 2);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(bad).unwrap();
    std::fs::remove_file(good).unwrap();
    std::fs::remove_dir(images).unwrap();
    std::fs::remove_dir(scratch.join("logos")).unwrap();
}
