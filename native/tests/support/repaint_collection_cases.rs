//! Original per-sticker gate scenarios through the real native collector and codec owner.
use _native::{catalog::Catalog, collector::Collector, media_adapter::MediaCodec};
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::path::Path;

pub fn check(
    directory: &Path,
    project: &Path,
    media: &super::bounded_media::BoundedCodec,
    runtime: &tokio::runtime::Runtime,
) {
    Python::initialize();
    let images = Python::attach(|py| -> PyResult<Vec<Vec<u8>>> {
        let module = py.import("PIL.Image")?;
        let mut images = Vec::new();
        for i in 0..4 {
            let image = module.call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))?;
            let buffer = match py.import("io").and_then(|io| io.call_method0("BytesIO")) {
                Ok(buffer) => buffer,
                Err(error) => {
                    image.call_method0("close")?;
                    return Err(error);
                }
            };
            let result = (|| -> PyResult<Vec<u8>> {
                for x in i..i + 30 {
                    for y in i..i + 30 {
                        image.call_method1("putpixel", ((x, y), (10 * i, 60, 200, 255)))?;
                    }
                }
                image.call_method1("save", (&buffer, "PNG"))?;
                buffer.call_method0("getvalue")?.extract()
            })();
            let image_closed = image.call_method0("close");
            let buffer_closed = buffer.call_method0("close");
            images.push(result?);
            image_closed?;
            buffer_closed?;
        }
        Ok(images)
    })
    .unwrap();
    for (name, mode, count, marked, wanted) in [
        ("skip", "skip", 4, true, vec![0, 2]),
        ("keep", "keep", 4, true, vec![0, 1, 2, 3]),
        ("ordinary", "skip", 3, false, vec![0, 1, 2]),
    ] {
        let data = directory.join(format!("repaint {name}"));
        std::fs::create_dir(&data).unwrap();
        let stickers = (0..count)
            .map(|i| {
                let mut sticker = super::sticker(&i.to_string(), &format!("FU-{i}"), "😀", "");
                if marked && matches!(i, 1 | 3) {
                    sticker["needs_repainting"] = json!(true);
                }
                sticker
            })
            .collect::<Vec<Value>>();
        let mut script = vec![super::pack(&stickers)];
        for i in &wanted {
            script.extend(super::download(
                &i.to_string(),
                &format!("{i}.png"),
                &images[*i],
            ));
        }
        let mut server = super::Fixture::new(script);
        let native = media.with_scratch(data.join("tmp/codecs"));
        let mut catalog = Catalog::open(&data.join("catalog.db"), "fixture UTC", project).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let tg =
                _native::telegram::Telegram::new(super::TOKEN.into(), server.base.clone()).unwrap();
            let mut collector = Collector {
                cat: &mut catalog,
                media: &native,
                data: data.clone(),
                threshold: -1,
            };
            assert_eq!(
                collector
                    .fetch_pack(&tg, runtime, "fixture", 0, mode)
                    .unwrap(),
                json!({"new":wanted.len(),"dedup":0,"skipped":0,"failed":0,
                    "repaintable":if name == "skip" { 2 } else { 0 }})
            );
            let rows = collector.cat.all(None).unwrap();
            assert_eq!(rows.len(), wanted.len());
            for i in 0..count {
                let key = collector
                    .cat
                    .dispatch(json!({"operation":"seen","file_unique_id":format!("FU-{i}")}))
                    .unwrap();
                if wanted.contains(&i) {
                    assert!(rows.iter().any(|row| row["content_key"] == key));
                    assert!(key.is_string());
                } else {
                    assert!(key.is_null(), "skipped sticker acquired an identity");
                }
            }
            super::assert_empty_temporary(&data);
            rows
        }));
        let requests = server.finish();
        let rows = result.unwrap();
        let requests = requests.unwrap();
        assert_eq!(requests.len(), 1 + wanted.len() * 2);
        let downloaded = wanted
            .iter()
            .map(|i| format!("/file/bot{}/{i}.png", super::TOKEN))
            .collect::<Vec<_>>();
        assert_eq!(
            requests
                .iter()
                .skip(2)
                .step_by(2)
                .cloned()
                .collect::<Vec<_>>(),
            downloaded
        );
        drop(catalog);
        let reopened = Catalog::open(&data.join("catalog.db"), "observe", project).unwrap();
        assert_eq!(reopened.all(None).unwrap(), rows);
        drop(reopened);
        assert!(
            std::time::Instant::now() < media.deadline,
            "repaint collection deadline elapsed; evidence retained"
        );
    }
}
