//! Real source image bytes and pinned cache/download/request traces, no live network or sleeps.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;
#[test]
fn valid_cache_skips_download_and_error_bodies_never_become_logo_files() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/gecko-fetch.json")).unwrap();
    for case in fixture["tickers"].as_array().unwrap() {
        assert_eq!(
            _native::coin_gecko_fetch::safe_ticker(case["input"].as_str().unwrap()),
            case["expected"]
        );
    }
    let bytes = std::fs::read(root.join(fixture["input"].as_str().unwrap())).unwrap();
    let digest = |b: &[u8]| {
        Sha256::digest(b)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    assert_eq!(digest(&bytes), fixture["input_sha256"]);
    let scratch = root
        .join("logs/tmp")
        .join(format!("gecko fetch {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let directory = scratch.join(index.to_string());
        std::fs::create_dir_all(directory.join("logos/png")).unwrap();
        let path = directory.join("logos/png/btc.png");
        match case["initial"].as_str().unwrap() {
            "valid" => std::fs::write(&path, &bytes).unwrap(),
            "corrupt" => std::fs::write(&path, b"error page").unwrap(),
            _ => {}
        }
        let mut calls = Vec::new();
        let mut waits = Vec::new();
        let result=_native::coin_gecko_fetch::fetch_with(&directory,1,Duration::from_secs(2),|url,binary,retries|{
            calls.push(serde_json::json!([url,binary,retries]));
            if !binary{
                if case["outcome"]=="failure"{return Err("controlled unavailable".into());}
                Ok(_native::coin_gecko_fetch::Payload::Json(serde_json::json!([{"symbol":"BTC","name":"Bitcoin","image":"http://example.invalid/btc.png"}])))
            }else{Ok(_native::coin_gecko_fetch::Payload::Bytes(if case["outcome"]=="valid"{bytes.clone()}else{b"error page".to_vec()}))}
        },|duration|{waits.push(duration.as_secs_f64());Ok(())}).unwrap();
        assert_eq!(serde_json::json!(result), case["expected"]["code"]);
        assert_eq!(serde_json::json!(calls), case["expected"]["calls"]);
        assert_eq!(serde_json::json!(waits), case["expected"]["waits"]);
        assert_eq!(
            std::fs::read(directory.join("keywords.csv")).unwrap(),
            case["expected"]["csv"].as_str().unwrap().as_bytes()
        );
        assert_eq!(
            serde_json::json!(std::fs::read(&path).ok().map(|b| digest(&b))),
            case["expected"]["destination_sha256"]
        );
    }
    original_cache_assertions(&scratch);
    std::fs::remove_dir_all(scratch).unwrap();
}

fn png(color: (u8, u8, u8, u8)) -> Vec<u8> {
    use pyo3::prelude::*;
    Python::initialize();
    Python::attach(|py| -> PyResult<Vec<u8>> {
        let module = py.import("PIL.Image")?;
        assert_eq!(
            module.getattr("__version__")?.extract::<String>()?,
            "12.3.0"
        );
        let buffer = py.import("io")?.getattr("BytesIO")?.call0()?;
        let result: PyResult<Vec<u8>> = (|| {
            let image = module.call_method1("new", ("RGBA", (32, 32), color))?;
            let saved = image.call_method1("save", (&buffer, "PNG"));
            let closed_image = image.call_method0("close");
            saved?;
            closed_image?;
            buffer.call_method0("getvalue")?.extract()
        })();
        let closed_buffer = buffer.call_method0("close");
        let bytes = result?;
        closed_buffer?;
        Ok(bytes)
    })
    .unwrap()
}

fn csv_rows(path: &std::path::Path) -> Vec<Vec<String>> {
    let mut reader = csv::Reader::from_path(path).unwrap();
    assert_eq!(
        reader.headers().unwrap().iter().collect::<Vec<_>>(),
        ["ticker", "name", "format", "file", "keywords"]
    );
    reader
        .records()
        .map(|row| row.unwrap().iter().map(str::to_owned).collect())
        .collect()
}

fn original_cache_assertions(scratch: &std::path::Path) {
    use _native::coin_gecko_fetch::{Payload, fetch_with};
    use serde_json::json;
    const HTML: &[u8] = b"<html><body>429 Too Many Requests</body></html>";
    const MARKET: &str = "https://api.coingecko.com/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page=1&sparkline=false";
    const IMAGE: &str = "http://img.test/btc.png";
    let good = png((20, 120, 200, 255));
    let blank = png((0, 0, 0, 0));
    assert!(good.len() > 60);
    let directory = scratch.join("original image validation");
    std::fs::create_dir(&directory).unwrap();
    for (name, bytes, accepted) in [
        ("good.png", good.as_slice(), true),
        ("html.png", HTML, false),
        ("cut.png", &good[..60], false),
        ("blank.png", blank.as_slice(), false),
    ] {
        let path = directory.join(name);
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(_native::codec_image::valid_logo(&path), accepted, "{name}");
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    for (name, cached, downloaded) in [
        ("rejected download", None, HTML),
        ("corrupt cache", Some(HTML), good.as_slice()),
        ("valid cache", Some(good.as_slice()), HTML),
    ] {
        let directory = scratch.join(name);
        let images = directory.join("logos/png");
        std::fs::create_dir_all(&images).unwrap();
        let path = images.join("btc.png");
        if let Some(bytes) = cached {
            std::fs::write(&path, bytes).unwrap();
        }
        let mut calls = Vec::new();
        let mut waits = Vec::new();
        assert_eq!(
            fetch_with(
                &directory,
                1,
                Duration::ZERO,
                |url, binary, retries| {
                    calls.push(json!([url, binary, retries]));
                    if binary {
                        assert_eq!(url, IMAGE);
                        Ok(Payload::Bytes(downloaded.to_vec()))
                    } else {
                        assert_eq!(url, MARKET);
                        Ok(Payload::Json(
                            json!([{"symbol":"BTC","name":"Bitcoin","image":IMAGE}]),
                        ))
                    }
                },
                |delay| {
                    waits.push(delay);
                    Ok(())
                }
            )
            .unwrap(),
            0
        );
        let mut expected = vec![json!([MARKET, false, 6])];
        if name != "valid cache" {
            expected.push(json!([IMAGE, true, 3]));
        }
        assert_eq!(calls, expected);
        let rows = csv_rows(&directory.join("keywords.csv"));
        if name == "rejected download" {
            assert!(!path.exists());
            assert!(rows.is_empty());
            assert_eq!(std::fs::read_dir(&images).unwrap().count(), 0);
            assert_eq!(waits, [Duration::ZERO]);
        } else {
            assert_eq!(std::fs::read(&path).unwrap(), good);
            assert!(_native::codec_image::valid_logo(&path));
            assert_eq!(std::fs::read_dir(&images).unwrap().count(), 1);
            assert_eq!(
                rows,
                [vec![
                    "btc",
                    "Bitcoin",
                    "png",
                    "logos/png/btc.png",
                    "btc, Bitcoin"
                ]]
            );
            assert_eq!(
                waits,
                if name == "valid cache" {
                    vec![Duration::ZERO]
                } else {
                    vec![Duration::from_millis(50), Duration::ZERO]
                }
            );
        }
        assert_eq!(
            std::fs::read_dir(directory.join("logos")).unwrap().count(),
            1
        );
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
    }
    let directory = scratch.join("original leftover sweep");
    let images = directory.join("logos/png");
    std::fs::create_dir_all(&images).unwrap();
    for (name, bytes) in [
        ("junk.png", HTML),
        ("cut.png", &good[..60]),
        ("ok.png", good.as_slice()),
    ] {
        std::fs::write(images.join(name), bytes).unwrap();
    }
    let mut calls = Vec::new();
    assert_eq!(
        fetch_with(
            &directory,
            1,
            Duration::ZERO,
            |url, binary, retries| {
                calls.push(json!([url, binary, retries]));
                assert!(!binary);
                Ok(Payload::Json(json!([])))
            },
            |_| panic!("empty pages must not wait")
        )
        .unwrap(),
        0
    );
    assert_eq!(calls, [json!([MARKET, false, 6])]);
    assert_eq!(
        csv_rows(&directory.join("keywords.csv")),
        [vec!["ok", "", "png", "logos/png/ok.png", "ok"]]
    );
    assert_eq!(std::fs::read_dir(&images).unwrap().count(), 3);
    for (name, bytes) in [
        ("junk.png", HTML),
        ("cut.png", &good[..60]),
        ("ok.png", good.as_slice()),
    ] {
        assert_eq!(std::fs::read(images.join(name)).unwrap(), bytes);
    }
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
    assert!(!_native::process_owner::cleanup_pending(scratch));
}
