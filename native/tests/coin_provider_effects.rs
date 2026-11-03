//! Original provider publication assertions through real files, multipart and content readback.
use _native::{
    coin_provider_effects, coin_provider_recover::Files, coin_provider_stage::Stage,
    telegram::Telegram,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[path = "support/bot_api.rs"]
mod bot_api;
#[path = "support/coin_provider_resume.rs"]
mod coin_provider_resume;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use bot_api::{Reply, Server};

const SET: &str = "cryptoemoji1_by_YourEmojiBot";
const NEXT_SET: &str = "cryptoemoji2_by_YourEmojiBot";

fn document(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn member(index: usize) -> Value {
    json!({"custom_emoji_id":format!("c{index}"),"file_id":format!("f{index}"),
        "file_unique_id":format!("u{index}")})
}
fn listing(name: &str, members: &[Value]) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", name)],
        json!({"ok":true,"result":{"stickers":members}}),
    )
}
fn me() -> Reply {
    Reply::new(
        "getMe",
        &[],
        json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
    )
}
fn download(replies: &mut Vec<Reply>, index: usize, bytes: &[u8]) {
    let id = format!("f{index}");
    let path = format!("f{index}.png");
    replies.push(Reply::new(
        "getFile",
        &[("file_id", &id)],
        json!({"ok":true,"result":{"file_path":path}}),
    ));
    let mut reply = Reply::new("getFile", &[], Value::Null);
    reply.method = format!("GET {path}");
    reply.download = Some(bytes.to_vec());
    replies.push(reply);
}
fn mutation(
    files: &Files,
    name: &str,
    before: usize,
    ticker: &str,
    bytes: &[u8],
    outcome: Value,
) -> Reply {
    let creating = name == NEXT_SET;
    let input = json!({"sticker":"attach://file0","format":"static","emoji_list":["🪙"],"keywords":[ticker]});
    let mut reply = Reply::new(
        if creating {
            "createNewStickerSet"
        } else {
            "addStickerToSet"
        },
        &[],
        outcome,
    );
    reply.fields.insert("user_id".into(), "1".into());
    reply.fields.insert("name".into(), name.into());
    if creating {
        reply.fields.insert("title".into(), "Title 2".into());
        reply
            .fields
            .insert("sticker_type".into(), "custom_emoji".into());
        reply
            .fields
            .insert("stickers".into(), json!([input]).to_string());
    } else {
        reply.fields.insert("sticker".into(), input.to_string());
    }
    reply.files.insert("file0".into(), bytes.to_vec());
    reply.observation = Some((
        files.state.clone(),
        "/provider_in_flight/expected_before".into(),
        json!(before),
    ));
    reply
}

fn ramps() -> [Vec<u8>; 2] {
    use pyo3::prelude::*;
    use pyo3::types::PyList;
    Python::initialize();
    Python::attach(|py| -> PyResult<_> {
        let module = py.import("PIL.Image")?;
        assert_eq!(
            module.getattr("__version__")?.extract::<String>()?,
            "12.3.0"
        );
        let mut images = Vec::new();
        for reverse in [false, true] {
            let buffer = py.import("io")?.getattr("BytesIO")?.call0()?;
            let result: PyResult<Vec<u8>> = (|| {
                let image = module.call_method1("new", ("RGBA", (100, 100)))?;
                let result = (|| {
                    let pixels = (0..100)
                        .flat_map(|_| {
                            (0..100).map(|x| {
                                let v = if reverse { (99 - x) * 2 } else { x * 2 };
                                (v, v, v, 255)
                            })
                        })
                        .collect::<Vec<_>>();
                    image.call_method1("putdata", (PyList::new(py, pixels)?,))?;
                    image.call_method1("save", (&buffer, "PNG"))?;
                    buffer.call_method0("getvalue")?.extract::<Vec<u8>>()
                })();
                let closed = image.call_method0("close");
                let bytes = result?;
                closed?;
                Ok(bytes)
            })();
            let closed = buffer.call_method0("close");
            let bytes = result?;
            closed?;
            images.push(bytes);
        }
        Ok([images.remove(0), images.remove(0)])
    })
    .unwrap()
}

fn files(directory: PathBuf, before: usize) -> Files {
    std::fs::create_dir(&directory).unwrap();
    let files = Files {
        state: directory.join("state.json"),
        mapping: directory.join("map.json"),
        published: directory.join("logos/emoji"),
        base: "cryptoemoji".into(),
        root: directory,
    };
    _native::atomic::write_json(&files.state, &json!({"sets":[{"index":1,"name":SET,"title":"Title 1","live":before}],"provider_added":[]})).unwrap();
    _native::atomic::write_json(&files.mapping, &json!({"btc":"c-btc","other":"preserved"}))
        .unwrap();
    files
}
fn released(files: &Files) {
    for path in [
        _native::locks::pack_family_path(&files.root, &files.base),
        files.root.join(".locks/canonical_map.lock"),
    ] {
        assert!(path.is_file(), "stable lock file must not be unlinked");
        drop(_native::locks::Lease::acquire(&path, "provider release assertion").unwrap());
    }
    assert!(!_native::process_owner::cleanup_pending(&files.root));
}

#[test]
fn native_provider_effects_upload_once_and_preserve_existing_mapping() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = tooling_fixture::Root::new("provider effects original assertions");
    let [ours, foreign] = ramps();
    // The original opaque ramps are increasing/decreasing: all adjacent comparisons are false/true.
    assert_eq!(_native::codec_image::provider_dhash(&ours).unwrap(), 0);
    assert_eq!(
        _native::codec_image::provider_dhash(&foreign).unwrap(),
        u64::MAX
    );
    for scenario in [
        "normal",
        "foreign-tail",
        "duplicate",
        "rejected",
        "ambiguous",
        "rollover",
        "mapped-other",
        "mapped-only",
    ] {
        let before = if scenario == "rollover" { 200 } else { 2 };
        let files = files(root.0.join(scenario), before);
        if matches!(scenario, "mapped-other" | "mapped-only") {
            _native::atomic::write_json(&files.mapping, &json!({"btc":"c-btc","other":"preserved","aaa":"c-from-the-other-provider","zzz":"written-by-the-other-tool"})).unwrap();
        }
        let mut stage = Stage::new(&files.published).unwrap();
        if scenario == "normal" {
            std::fs::create_dir_all(&files.published).unwrap();
            std::fs::write(files.published.join("aaa.png"), &foreign).unwrap();
        }
        let source = stage.source("aaa").unwrap();
        std::fs::write(&source, &ours).unwrap();
        if scenario == "mapped-only" {
            std::fs::create_dir_all(&files.published).unwrap();
            let published = files.published.join("aaa.png");
            std::fs::write(&published, &foreign).unwrap();
            let before = [
                std::fs::read(&files.state).unwrap(),
                std::fs::read(&files.mapping).unwrap(),
            ];
            let mut server = Server::new(vec![me()]);
            let tg = Telegram::new(bot_api::TOKEN.into(), server.base.clone()).unwrap();
            let (added, failed, mapping) = coin_provider_effects::run(
                &files,
                &tg,
                &mut stage,
                &["aaa".into()],
                "Title",
                1,
                &serde_json::Map::new(),
            )
            .unwrap();
            assert_eq!((added, failed), (0, 0));
            assert_eq!(mapping["aaa"], "c-from-the-other-provider");
            assert_eq!(mapping["zzz"], "written-by-the-other-tool");
            assert_eq!(document(&files.mapping), Value::Object(mapping));
            assert_eq!(std::fs::read(&files.state).unwrap(), before[0]);
            assert_eq!(std::fs::read(&files.mapping).unwrap(), before[1]);
            assert_eq!(std::fs::read(&published).unwrap(), foreign);
            assert!(!stage.directory().exists());
            assert_eq!(server.finish().unwrap().len(), 1);
            released(&files);
            continue;
        }
        let ticker = if scenario == "mapped-other" {
            "bbb"
        } else {
            "aaa"
        };
        if ticker == "bbb" || scenario == "duplicate" {
            std::fs::write(stage.source("bbb").unwrap(), &ours).unwrap();
        }
        if scenario == "duplicate" {
            std::fs::write(stage.source("ccc").unwrap(), &ours).unwrap();
        }
        let old = (1..=before).map(member).collect::<Vec<_>>();
        let creating = scenario == "rollover";
        let name = if creating { NEXT_SET } else { SET };
        let position = if creating { 0 } else { before };
        let mut live = if creating { Vec::new() } else { old.clone() };
        live.push(member(before + 1));
        if matches!(scenario, "foreign-tail" | "duplicate") {
            live.push(member(before + 2));
        }
        let mut replies = vec![me(), listing(SET, &old)];
        if !creating {
            replies.push(listing(SET, &old));
        }
        let outcome = match scenario {
            "rejected" => json!({"ok":false,"description":"STICKER_PNG_NOPNG"}),
            // Missing result is an ambiguous answered request, never a transport retry.
            "ambiguous" => json!({"ok":true}),
            _ => json!({"ok":true,"result":true}),
        };
        let mut change = mutation(&files, name, position, ticker, &ours, outcome);
        if creating {
            change.observation = Some((
                files.state.clone(),
                "/sets".into(),
                document(&files.state)["sets"].clone(),
            ));
        }
        replies.push(change);
        if scenario != "rejected" {
            replies.push(listing(name, &live));
            download(&mut replies, before + 1, &ours);
            if matches!(scenario, "foreign-tail" | "duplicate") {
                download(
                    &mut replies,
                    before + 2,
                    if scenario == "duplicate" {
                        &ours
                    } else {
                        &foreign
                    },
                );
            }
        }
        if scenario == "normal" {
            replies.push(me());
        }
        let mut server = Server::new(replies);
        let tg = Telegram::new(bot_api::TOKEN.into(), server.base.clone()).unwrap();
        let tickers = if scenario == "duplicate" {
            vec!["aaa".into(), "bbb".into(), "ccc".into()]
        } else if ticker == "bbb" {
            vec!["aaa".into(), "bbb".into()]
        } else {
            vec!["aaa".into()]
        };
        let (added, failed, mapping) = coin_provider_effects::run(
            &files,
            &tg,
            &mut stage,
            &tickers,
            "Title",
            1,
            &serde_json::Map::new(),
        )
        .unwrap();
        let uncertain = scenario == "duplicate";
        let rejected = scenario == "rejected";
        assert_eq!(
            (added, failed),
            if uncertain {
                (0, 3)
            } else if rejected {
                (0, 1)
            } else {
                (1, 0)
            },
            "{scenario}"
        );
        assert_eq!(mapping["btc"], "c-btc");
        assert_eq!(mapping["other"], "preserved");
        assert_eq!(Value::Object(mapping.clone()), document(&files.mapping));
        let state = document(&files.state);
        if uncertain {
            assert!(!mapping.contains_key("aaa"));
            assert_eq!(state["provider_in_flight"]["key"], "aaa");
            assert_eq!(state["provider_in_flight"]["operation"], "add");
            assert_eq!(state["provider_in_flight"]["expected_before"], 2);
            assert_eq!(state["provider_in_flight"]["source_dhash"], 0);
            assert_eq!(
                PathBuf::from(state["provider_in_flight"]["source_path"].as_str().unwrap()),
                source
            );
            assert_eq!(std::fs::read(&source).unwrap(), ours);
            assert_eq!(std::fs::read_dir(stage.directory()).unwrap().count(), 1);
            for ticker in ["bbb", "ccc"] {
                assert!(!mapping.contains_key(ticker));
                assert!(!files.published.join(format!("{ticker}.png")).exists());
            }
            assert_eq!(state["provider_added"], json!([]));
            assert!(!files.published.join("aaa.png").exists());
        } else {
            assert!(state["provider_in_flight"].is_null());
            assert!(!source.exists());
            assert!(!stage.directory().exists());
            if rejected {
                assert!(!mapping.contains_key("aaa"));
                assert_eq!(state["provider_added"], json!([]));
                assert!(!files.published.join("aaa.png").exists());
            } else {
                assert_eq!(mapping[ticker], format!("c{}", before + 1));
                assert_eq!(
                    std::fs::read(files.published.join(format!("{ticker}.png"))).unwrap(),
                    ours
                );
                assert_eq!(state["provider_added"], json!([ticker]));
                if scenario == "foreign-tail" {
                    assert_ne!(mapping[ticker], live.last().unwrap()["custom_emoji_id"]);
                }
                if scenario == "mapped-other" {
                    assert_eq!(mapping["aaa"], "c-from-the-other-provider");
                    assert_eq!(mapping["zzz"], "written-by-the-other-tool");
                    assert!(!files.published.join("aaa.png").exists());
                }
            }
        }
        assert_eq!(
            state["sets"].as_array().unwrap().len(),
            if creating { 2 } else { 1 }
        );
        if creating {
            assert_eq!(
                state["sets"][1],
                json!({"index":2,"name":NEXT_SET,"title":"Title 2","live":1})
            );
        }
        if scenario == "normal" {
            let state_bytes = std::fs::read(&files.state).unwrap();
            let mut next = Stage::new(&files.published).unwrap();
            assert_eq!(
                coin_provider_effects::run(
                    &files,
                    &tg,
                    &mut next,
                    &["aaa".into()],
                    "Title",
                    1,
                    &serde_json::Map::new()
                )
                .unwrap(),
                (0, 0, mapping)
            );
            assert_eq!(std::fs::read(&files.state).unwrap(), state_bytes);
            assert!(!next.directory().exists());
        }
        let requests = server.finish().unwrap();
        let changes = requests
            .iter()
            .filter(|r| matches!(r.method.as_str(), "addStickerToSet" | "createNewStickerSet"))
            .collect::<Vec<_>>();
        assert_eq!(changes.len(), 1, "no extra send after {scenario}");
        assert_eq!(
            changes[0].method,
            if creating {
                "createNewStickerSet"
            } else {
                "addStickerToSet"
            }
        );
        let intent = &changes[0].observation.as_ref().unwrap()["provider_in_flight"];
        assert_eq!(intent["expected_before"], position);
        assert_eq!(intent["key"], ticker);
        assert_eq!(intent["set_name"], name);
        assert_eq!(intent["source_dhash"], 0);
        if uncertain {
            let saved = std::fs::read(&files.state).unwrap();
            let map = std::fs::read(&files.mapping).unwrap();
            let mut observer = Server::new(vec![
                me(),
                Reply::new(
                    "getStickerSet",
                    &[("name", SET)],
                    json!({"ok":false,"description":"controlled unavailable"}),
                ),
            ]);
            let tg = Telegram::new(bot_api::TOKEN.into(), observer.base.clone()).unwrap();
            let mut next = Stage::new(&files.published).unwrap();
            let result = coin_provider_effects::run(
                &files,
                &tg,
                &mut next,
                &["aaa".into()],
                "Title",
                1,
                &serde_json::Map::new(),
            )
            .unwrap();
            assert_eq!((result.0, result.1), (0, 1));
            assert_eq!(std::fs::read(&files.state).unwrap(), saved);
            assert_eq!(std::fs::read(&files.mapping).unwrap(), map);
            assert_eq!(std::fs::read(&source).unwrap(), ours);
            assert_eq!(observer.finish().unwrap().len(), 2);
            assert!(!next.directory().exists());
        }
        released(&files);
    }
    coin_provider_resume::check(&root.0, &ours, &foreign);
    held_ownership(&root.0, &ours);
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    root.clean().unwrap();
}

fn held_ownership(root: &Path, bytes: &[u8]) {
    for kind in ["family", "map"] {
        let files = files(root.join(format!("held-{kind}")), 2);
        let before = [
            std::fs::read(&files.state).unwrap(),
            std::fs::read(&files.mapping).unwrap(),
        ];
        let mut stage = Stage::new(&files.published).unwrap();
        std::fs::write(stage.source("aaa").unwrap(), bytes).unwrap();
        let family = _native::locks::pack_family_path(&files.root, &files.base);
        let map = files.root.join(".locks/canonical_map.lock");
        let path = if kind == "family" { &family } else { &map };
        let lease =
            _native::locks::Lease::acquire(path, "held original provider assertion").unwrap();
        let mut server = Server::new(vec![]);
        let tg = Telegram::new(bot_api::TOKEN.into(), server.base.clone()).unwrap();
        assert!(
            coin_provider_effects::run(
                &files,
                &tg,
                &mut stage,
                &["aaa".into()],
                "Title",
                1,
                &serde_json::Map::new()
            )
            .is_err()
        );
        assert!(server.finish().unwrap().is_empty());
        assert_eq!(std::fs::read(&files.state).unwrap(), before[0]);
        assert_eq!(std::fs::read(&files.mapping).unwrap(), before[1]);
        assert!(!stage.directory().exists());
        if kind == "map" {
            // A refused canonical lease must release the family lease acquired just before it.
            drop(_native::locks::Lease::acquire(&family, "not stranded by map refusal").unwrap());
        }
        assert!(_native::locks::Lease::acquire(path, "still held").is_err());
        drop(lease);
        drop(_native::locks::Lease::acquire(path, "released after refusal").unwrap());
        assert!(path.is_file());
    }
}
