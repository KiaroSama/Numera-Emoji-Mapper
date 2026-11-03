//! Original identity/uniqueness cases at native comparison and durable resolver seams.
use _native::{
    catalog::Catalog,
    codec_image,
    media_native_adapter::NativeMediaAdapter,
    publication::Publisher,
    reconcile::{self, NearMatch},
    telegram::Telegram,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
#[path = "support/bounded_media.rs"]
mod bounded_media;
#[path = "support/read_only_api.rs"]
mod read_only_api;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bounded_media::BoundedCodec;
use read_only_api::{Fixture, Reply, TOKEN};
use tooling_fixture::Root;

fn png(path: &Path, red: u8, green: u8, blue: u8) {
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let module = py.import("PIL.Image")?;
        assert_eq!(
            module.getattr("__version__")?.extract::<String>()?,
            "12.3.0"
        );
        let image = module.call_method1("new", ("RGBA", (100, 100), (red, green, blue, 255)))?;
        let saved = image.call_method1("save", (path, "PNG"));
        let closed = image.call_method0("close");
        saved?;
        closed?;
        Ok(())
    })
    .unwrap();
}
fn webp(source: &Path, destination: &Path) {
    Python::attach(|py| -> PyResult<()> {
        let opened = py.import("PIL.Image")?.call_method1("open", (source,))?;
        let result = (|| -> PyResult<()> {
            let rgba = opened.call_method1("convert", ("RGBA",))?;
            let options = PyDict::new(py);
            options.set_item("quality", 88)?;
            let saved = rgba.call_method("save", (destination, "WEBP"), Some(&options));
            let closed = rgba.call_method0("close");
            saved?;
            closed?;
            Ok(())
        })();
        let closed = opened.call_method0("close");
        result?;
        closed?;
        Ok(())
    })
    .unwrap();
}
fn raster(path: &Path) -> Value {
    _native::codec_static_identity::fingerprint(path).unwrap()
}
fn item(path: &Path, evidence: &Value, unhashed: bool) -> Value {
    json!({"content_key":evidence["content_key"],"fmt":"static","file_path":path,
        "phash":if unhashed { Value::Null } else { evidence["phash"].clone() }})
}
fn near(
    runtime: &tokio::runtime::Runtime,
    items: &[Value],
    probe: &Path,
    fmt: &str,
    hash: Option<u64>,
) -> (NearMatch, Vec<String>) {
    let mut compared = Vec::new();
    let result = runtime
        .block_on(reconcile::near_catalog(
            items,
            fmt,
            hash,
            "",
            async |item: &Value| {
                let source = Path::new(item["file_path"].as_str().unwrap());
                compared.push(item["content_key"].as_str().unwrap().to_owned());
                Ok(if source.is_file() {
                    codec_image::same_image(source, probe)
                } else {
                    None
                })
            },
        ))
        .unwrap();
    (result, compared)
}

#[test]
fn original_near_image_matrix_keeps_colour_reencodes_unknowns_and_every_rival_distinct() {
    let root = Root::new("reconcile identity café");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let red = root.0.join("red.png");
    let blue = root.0.join("blue.png");
    let a = root.0.join("a.png");
    let b = root.0.join("b.png");
    let probe = root.0.join("probe.png");
    for (path, color) in [
        (&red, (255, 0, 0)),
        (&blue, (0, 0, 255)),
        (&a, (245, 0, 0)),
        (&b, (250, 0, 0)),
        (&probe, (248, 0, 0)),
    ] {
        png(path, color.0, color.1, color.2);
    }
    let red_info = raster(&red);
    let blue_info = raster(&blue);
    // Independent SHA256 of the original uniform 64x64 RGBA pixels, not
    // fingerprint output reused as its own expected value.
    assert_eq!(
        red_info["content_key"],
        "s:0bcc07de1631aa0395013f35790f719b"
    );
    assert_eq!(
        blue_info["content_key"],
        "s:c34fb4331b2d031d7c644860b54a6784"
    );
    assert_eq!(red_info["phash"], 0);
    assert_eq!(blue_info["phash"], 0);
    assert_eq!(
        red_info["phash"], blue_info["phash"],
        "fixture lost structural collision"
    );
    assert_ne!(red_info["content_key"], blue_info["content_key"]);
    let before = [&red, &blue, &a, &b, &probe].map(|path| std::fs::read(path).unwrap());
    let red_item = item(&red, &red_info, false);
    let key = red_info["content_key"].as_str().unwrap().to_owned();
    let hash = blue_info["phash"].as_u64();
    assert_eq!(
        near(
            &runtime,
            std::slice::from_ref(&red_item),
            &blue,
            "static",
            hash
        ),
        (NearMatch::Missing, vec![key.clone()])
    );
    assert_eq!(
        near(
            &runtime,
            std::slice::from_ref(&red_item),
            &red,
            "static",
            red_info["phash"].as_u64()
        ),
        (NearMatch::Unique(key.clone()), vec![key.clone()])
    );
    let encoded = root.0.join("red_reencoded.webp");
    webp(&red, &encoded);
    let encoded_info = raster(&encoded);
    assert_ne!(
        encoded_info["content_key"], red_info["content_key"],
        "fixture must require near matching"
    );
    assert_eq!(
        near(
            &runtime,
            std::slice::from_ref(&red_item),
            &encoded,
            "static",
            encoded_info["phash"].as_u64()
        )
        .0,
        NearMatch::Unique(key.clone())
    );
    let a_item = item(&a, &raster(&a), false);
    let b_item = item(&b, &raster(&b), false);
    let both = vec![a_item.clone(), b_item.clone()];
    let nomination = raster(&probe)["phash"].as_u64();
    let (answer, compared) = near(&runtime, &both, &probe, "static", nomination);
    assert_eq!(answer, NearMatch::Ambiguous);
    assert_eq!(
        compared,
        vec![
            a_item["content_key"].as_str().unwrap(),
            b_item["content_key"].as_str().unwrap()
        ]
    );
    // Both original suites use distinct colour triples; retain both reproductions.
    let original_a = root.0.join("original-a.png");
    let original_b = root.0.join("original-b.png");
    let original_probe = root.0.join("original-probe.png");
    for (path, red) in [
        (&original_a, 200),
        (&original_b, 206),
        (&original_probe, 203),
    ] {
        png(path, red, 0, 0);
    }
    let originals = [
        item(&original_a, &raster(&original_a), false),
        item(&original_b, &raster(&original_b), false),
    ];
    assert_ne!(originals[0]["content_key"], originals[1]["content_key"]);
    assert_eq!(
        near(
            &runtime,
            &originals,
            &original_probe,
            "static",
            raster(&original_probe)["phash"].as_u64()
        )
        .0,
        NearMatch::Ambiguous
    );
    assert_ne!(NearMatch::Missing, NearMatch::Ambiguous);
    assert_ne!(NearMatch::Missing, NearMatch::Undecidable);
    assert_ne!(NearMatch::Ambiguous, NearMatch::Undecidable);
    let missing = root.0.join("missing.png");
    let mut rival = b_item.clone();
    rival["file_path"] = json!(missing);
    let (answer, compared) = near(
        &runtime,
        &[a_item.clone(), rival],
        &probe,
        "static",
        nomination,
    );
    assert_eq!(answer, NearMatch::Undecidable);
    assert_eq!(
        compared.len(),
        2,
        "readable match must not bypass the unknown rival"
    );
    let broken = root.0.join("broken.png");
    std::fs::write(&broken, b"\x89PNG\r\n\x1a\ncorruptcorruptcorrupt").unwrap();
    assert!(_native::codec_static_identity::fingerprint(&broken).is_err());
    let mut rival = b_item;
    rival["file_path"] = json!(broken);
    assert_eq!(
        near(
            &runtime,
            &[a_item.clone(), rival],
            &probe,
            "static",
            nomination
        )
        .0,
        NearMatch::Undecidable
    );
    assert_eq!(
        near(
            &runtime,
            std::slice::from_ref(&a_item),
            &probe,
            "static",
            nomination
        )
        .0,
        NearMatch::Unique(a_item["content_key"].as_str().unwrap().into())
    );
    for (path, expected) in [
        (&blue, NearMatch::Missing),
        (&red, NearMatch::Unique(key.clone())),
    ] {
        assert_eq!(
            near(
                &runtime,
                &[item(&red, &red_info, true)],
                path,
                "static",
                hash
            ),
            (expected, vec![key.clone()])
        );
    }
    for unhashed in [false, true] {
        assert_eq!(
            near(
                &runtime,
                &[item(&missing, &red_info, unhashed)],
                &blue,
                "static",
                hash
            )
            .0,
            NearMatch::Undecidable
        );
    }
    assert_eq!(
        near(&runtime, &[], &blue, "static", hash),
        (NearMatch::Missing, vec![])
    );
    assert_eq!(
        near(&runtime, &[], &broken, "static", None),
        (NearMatch::Undecidable, vec![])
    );
    assert_eq!(
        near(&runtime, &[], &broken, "animated", None),
        (NearMatch::Missing, vec![])
    );
    let (answer, calls) = near(&runtime, &[red_item], &red, "static", None);
    assert_eq!((answer, calls), (NearMatch::Undecidable, vec![]));
    for (path, expected) in [&red, &blue, &a, &b, &probe].into_iter().zip(before) {
        assert_eq!(std::fs::read(path).unwrap(), expected);
    }
    root.clean().unwrap();
}

#[test]
fn original_resolver_refusals_preserve_durable_identifiers_and_remove_only_owned_downloads() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let project = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    let root = Root::new("resolver identity café");
    let red = root.0.join("red.png");
    let blue = root.0.join("blue.png");
    let a = root.0.join("a.png");
    let b = root.0.join("b.png");
    let probe = root.0.join("probe.png");
    for (path, color) in [
        (&red, (255, 0, 0)),
        (&blue, (0, 0, 255)),
        (&a, (245, 0, 0)),
        (&b, (250, 0, 0)),
        (&probe, (248, 0, 0)),
    ] {
        png(path, color.0, color.1, color.2);
    }
    let metadata = [&red, &a, &b].map(|path| raster(path));
    let bodies = [&red, &blue, &probe].map(|path| std::fs::read(path).unwrap());
    let native = BoundedCodec {
        native: NativeMediaAdapter {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
            root: project.clone(),
            scratch: root.0.join("envelopes"),
        },
        deadline: Instant::now() + Duration::from_secs(60),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    for label in [
        "foreign",
        "ours",
        "ambiguous",
        "missing",
        "unknown-rival",
        "enumeration",
        "download-failure",
    ] {
        let path = root.0.join(label);
        std::fs::create_dir(&path).unwrap();
        let database = path.join("catalog.db");
        let mut cat = Catalog::open(&database, "fixture", &project).unwrap();
        let entries = if matches!(label, "ambiguous" | "unknown-rival") {
            vec![(&a, &metadata[1]), (&b, &metadata[2])]
        } else {
            vec![(&red, &metadata[0])]
        };
        for (index, (file, evidence)) in entries.iter().enumerate() {
            let source = if label == "missing" || (label == "unknown-rival" && index == 1) {
                path.join("missing.png")
            } else {
                file.to_path_buf()
            };
            cat.insert(&item(&source, evidence, false), "fixture")
                .unwrap();
        }
        if label == "enumeration" {
            let db = rusqlite::Connection::open(&database).unwrap();
            db.execute_batch("ALTER TABLE items DROP COLUMN position")
                .unwrap();
            assert!(
                cat.get(metadata[0]["content_key"].as_str().unwrap())
                    .unwrap()
                    .is_some()
            );
            assert!(cat.all(Some("static")).is_err());
        }
        let temp = path.join("tmp");
        std::fs::create_dir(&temp).unwrap();
        let sentinel = temp.join("resolve-caller.dl");
        std::fs::write(&sentinel, b"caller-owned sentinel").unwrap();
        let script = if label == "download-failure" {
            vec![Reply::envelope(
                "getFile",
                "file_id",
                "fid",
                json!({"ok":false,"description":"fixture download refused"}),
            )]
        } else {
            let body = if label == "ours" {
                &bodies[0]
            } else if matches!(label, "ambiguous" | "unknown-rival") {
                &bodies[2]
            } else {
                &bodies[1]
            };
            vec![
                Reply::api(
                    "getFile",
                    "file_id",
                    "fid",
                    json!({"file_path":"probe.png"}),
                ),
                Reply::file("probe.png", body),
            ]
        };
        let requests = script.len();
        let mut server = Fixture::new(script);
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let result = Publisher {
            tg: &tg,
            cat: &mut cat,
            media: &native,
            data: path.clone(),
            runtime: &runtime,
            config: None,
        }
        .resolve(&json!({"file_unique_id":"fuid-probe","file_id":"fid"}));
        match label {
            "foreign" => assert_eq!(result.unwrap(), None),
            "ours" => assert_eq!(
                result.unwrap().as_deref(),
                metadata[0]["content_key"].as_str()
            ),
            "ambiguous" => assert!(result.unwrap_err().contains("more than one")),
            "missing" | "unknown-rival" => assert!(result.unwrap_err().contains("unexamined")),
            "enumeration" => assert!(result.unwrap_err().contains("position")),
            "download-failure" => assert!(result.is_err()),
            _ => unreachable!(),
        }
        assert_eq!(server.finish().unwrap().len(), requests);
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"caller-owned sentinel");
        assert_eq!(
            std::fs::read_dir(&temp).unwrap().count(),
            1,
            "{label} leaked owned download"
        );
        drop(cat);
        let mut reopened = Catalog::open(&database, "reopen", &project).unwrap();
        let actual = reopened
            .dispatch(json!({"operation":"seen","file_unique_id":"fuid-probe"}))
            .unwrap();
        if label == "ours" {
            assert_eq!(actual, metadata[0]["content_key"]);
        } else {
            assert!(actual.is_null(), "{label} bound a foreign or unproven FUID");
        }
        for (_, evidence) in entries {
            assert!(reopened.dispatch(json!({"operation":"custom_id","base":"pk","content_key":evidence["content_key"]})).unwrap().is_null());
        }
    }
    for (path, original) in [&red, &blue, &probe].into_iter().zip(bodies) {
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
    assert!(
        !native.native.scratch.exists(),
        "resolver ignored invocation-owned codec storage"
    );
    runtime.shutdown_timeout(Duration::from_secs(1));
    root.clean().unwrap();
}
