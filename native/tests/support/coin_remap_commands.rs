//! Original remap command outcomes through the same copied artifact, capture and finite observer.
use super::{Files, Harness, Reply, Server, document, download, listing, member};
use serde_json::json;

fn artwork() -> [Vec<u8>; 4] {
    use pyo3::prelude::*;
    Python::initialize();
    Python::attach(|py| -> PyResult<_> {
        let module = py.import("PIL.Image")?;
        let mut images = Vec::new();
        for colour in [
            (200, 20, 20, 255),
            (20, 200, 20, 255),
            (201, 20, 20, 255),
            (20, 20, 200, 255),
        ] {
            let image = module.call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))?;
            let buffer = py.import("io")?.getattr("BytesIO")?.call0()?;
            let result = (|| -> PyResult<Vec<u8>> {
                image.call_method1("paste", (colour, (20, 20, 80, 80)))?;
                image.call_method1("save", (&buffer, "PNG"))?;
                buffer.call_method0("getvalue")?.extract()
            })();
            let image_closed = image.call_method0("close");
            let buffer_closed = buffer.call_method0("close");
            let bytes = result?;
            image_closed?;
            buffer_closed?;
            images.push(bytes);
        }
        Ok(images.try_into().unwrap())
    })
    .unwrap()
}

pub(super) fn check(harness: &Harness) {
    let [red, green, close, blue] = artwork();
    for scenario in [
        "incomplete",
        "ambiguous",
        "complete",
        "far",
        "no-local",
        "uncalibrated",
        "family-held",
    ] {
        let files = Files::new(&harness.root, scenario);
        let initial = match scenario {
            "incomplete" => Some(json!({"btc":"GOOD-OLD"})),
            "complete" | "family-held" => Some(json!({"btc":"STALE"})),
            _ => None,
        };
        if let Some(initial) = &initial {
            _native::atomic::write_json(&files.output, initial).unwrap();
        }
        let before = initial
            .as_ref()
            .map(|_| std::fs::read(&files.output).unwrap());
        if scenario != "no-local" {
            std::fs::write(files.images.join("btc.png"), &red).unwrap();
        }
        if scenario == "complete" {
            std::fs::write(files.images.join("eth.png"), &green).unwrap();
        }
        if scenario == "far" {
            std::fs::write(files.images.join("ghost.png"), &blue).unwrap();
        }
        let state_before = std::fs::read(&files.state).unwrap();
        let mut replies = Vec::new();
        if !matches!(scenario, "uncalibrated" | "family-held") {
            let members = if matches!(scenario, "incomplete" | "ambiguous" | "complete") {
                vec![member("a"), member("b")]
            } else {
                vec![member("a")]
            };
            replies.push(listing(&members));
            download(&mut replies, "a", &red);
            if scenario == "incomplete" {
                // Definite refusal exhausts no retry sleep and leaves b unresolved.
                let mut reply = Reply::new(
                    "getFile",
                    &[("file_id", "b")],
                    json!({"ok":false,"description":"controlled download refusal"}),
                );
                reply.status = axum::http::StatusCode::BAD_REQUEST;
                replies.push(reply);
            } else if matches!(scenario, "ambiguous" | "complete") {
                download(
                    &mut replies,
                    "b",
                    if scenario == "ambiguous" {
                        &close
                    } else {
                        &green
                    },
                );
            }
        }
        let lock = _native::locks::pack_family_path(&harness.root, "cryptoemoji");
        let lease = (scenario == "family-held")
            .then(|| _native::locks::Lease::acquire(&lock, "held remap family").unwrap());
        let mut server = Server::new(replies);
        let extra: &[&str] = match scenario {
            "no-local" => &[],
            "uncalibrated" => &["--apply"],
            "far" => &["--max-distance", "50", "--apply"],
            _ => &["--max-distance", "100", "--apply"],
        };
        let result = harness.capture(&files, extra, &server);
        let expected = match scenario {
            "incomplete" | "ambiguous" => 3,
            "no-local" | "family-held" => 4,
            "uncalibrated" => 2,
            _ => 0,
        };
        assert_eq!(
            result.code, expected,
            "{scenario}: {} {}",
            result.stdout, result.stderr
        );
        let requests = server.finish().unwrap();
        assert_eq!(
            requests.len(),
            match scenario {
                "uncalibrated" | "family-held" => 0,
                "incomplete" => 4,
                "ambiguous" | "complete" => 5,
                _ => 3,
            }
        );
        assert!(requests.iter().all(|request| matches!(
            request.method.as_str(),
            "getStickerSet" | "getFile" | "GET a.png" | "GET b.png"
        )));
        assert!(!result.stderr.contains("Traceback"));
        assert!(!result.stdout.contains(super::bot_api::TOKEN));
        assert!(!result.stderr.contains(super::bot_api::TOKEN));
        if matches!(scenario, "complete" | "far") {
            assert_eq!(
                document(&files.output),
                if scenario == "complete" {
                    json!({"btc":"a","eth":"b"})
                } else {
                    json!({"btc":"a"})
                }
            );
            if let Some(before) = &before {
                assert_eq!(
                    std::fs::read(files.output.with_extension("prebroken.json")).unwrap(),
                    *before
                );
            } else {
                assert!(!files.output.with_extension("prebroken.json").exists());
            }
        } else if let Some(before) = &before {
            assert_eq!(std::fs::read(&files.output).unwrap(), *before);
        } else {
            assert!(!files.output.exists());
        }
        if matches!(scenario, "incomplete" | "ambiguous") {
            assert_eq!(
                document(&files.candidate),
                if scenario == "incomplete" {
                    json!({"btc":"a"})
                } else {
                    json!({})
                }
            );
            assert!(!files.output.with_extension("prebroken.json").exists());
        } else {
            assert!(!files.candidate.exists());
        }
        if matches!(scenario, "uncalibrated" | "family-held") {
            assert!(!files.cache.exists());
        } else {
            let cache = document(&files.cache);
            assert!(!cache.to_string().contains(super::bot_api::TOKEN));
            if scenario == "incomplete" {
                assert_eq!(
                    cache["sigs"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                    ["a"]
                );
                assert!(cache["errors"].get("b").is_some());
                assert!(cache["sets"].get(super::SET).is_none());
            } else {
                assert_eq!(cache["errors"], json!({}));
            }
        }
        assert_eq!(std::fs::read(&files.state).unwrap(), state_before);
        if scenario != "no-local" {
            assert_eq!(std::fs::read(files.images.join("btc.png")).unwrap(), red);
        }
        assert!(std::fs::read_dir(&files.directory).unwrap().all(|entry| {
            matches!(
                entry.unwrap().file_name().to_str(),
                Some(
                    "images"
                        | "state.json"
                        | "cache.json"
                        | "map.json"
                        | "map.prebroken.json"
                        | "candidate.json"
                )
            )
        }));
        if lease.is_some() {
            assert!(_native::locks::Lease::acquire(&lock, "still held").is_err());
        }
        drop(lease);
        harness.released();
    }
    family_race(harness, &red);
}

fn family_race(harness: &Harness, red: &[u8]) {
    let files = Files::new(&harness.root, "family-read-race");
    std::fs::write(files.images.join("btc.png"), red).unwrap();
    _native::atomic::write_json(&files.output, &json!({"btc":"STALE"})).unwrap();
    let (held, ready, release) = listing(&[member("a")]).hold();
    let mut replies = vec![held];
    download(&mut replies, "a", red);
    let mut server = Server::new(replies);
    let family = _native::locks::pack_family_path(&harness.root, "cryptoemoji");
    let output = files.output.clone();
    let observer = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(5), ready)
                .await
                .unwrap()
                .unwrap();
        });
        // The competitor reaches the same family-first boundary during the actual live read.
        assert!(_native::locks::Lease::acquire(&family, "concurrent provider").is_err());
        assert_eq!(document(&output), json!({"btc":"STALE"}));
        release.send(()).unwrap();
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        harness.capture(&files, &["--max-distance", "100", "--apply"], &server)
    }));
    let observed = observer.join();
    let requests = server.finish();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    let result = result.unwrap();
    observed.unwrap();
    assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
    assert_eq!(requests.unwrap().len(), 3);
    assert_eq!(document(&files.output), json!({"btc":"a"}));
    assert_eq!(
        document(&files.output.with_extension("prebroken.json")),
        json!({"btc":"STALE"})
    );
    harness.released();
    // The original provider fixture retries after remap releases both leases.
    let family = _native::locks::Lease::acquire(
        &_native::locks::pack_family_path(&harness.root, "cryptoemoji"),
        "provider retry",
    )
    .unwrap();
    let map = _native::locks::Lease::acquire(
        &harness.root.join(".locks/canonical_map.lock"),
        "provider retry",
    )
    .unwrap();
    let mut current = document(&files.output);
    current["new"] = json!("z");
    _native::atomic::write_json(&files.output, &current).unwrap();
    drop(map);
    drop(family);
    assert_eq!(document(&files.output), json!({"btc":"a","new":"z"}));
    assert_eq!(std::fs::read(files.images.join("btc.png")).unwrap(), red);
    harness.released();
}
