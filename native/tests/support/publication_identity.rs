//! Original fresh-upload window through real publication, catalog and native content readback.
use super::bot_api::{Reply, Server, TOKEN};
use super::bounded_media::BoundedCodec;
use super::tooling_fixture::Root;
use _native::{
    catalog::Catalog,
    config::Config,
    media_native_adapter::NativeMediaAdapter,
    publication::Publisher,
    publish_collection::{Options, publish_locked},
    telegram::Telegram,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SET: &str = "pks1_by_YourEmojiBot";
const VIDEO_SET: &str = "pkv1_by_YourEmojiBot";
fn member(index: usize) -> Value {
    json!({"file_id":format!("fresh-{index}"),"file_unique_id":format!("FRESH-item{index}"),
        "custom_emoji_id":if index==0 {"111111111"} else {"222222222"}})
}
fn listing(name: &str, members: &[Value]) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", name)],
        json!({"ok":true,"result":{"name":name,"stickers":members}}),
    )
}
fn download(replies: &mut Vec<Reply>, sticker: &Value, bytes: &[u8]) {
    let file = sticker["file_id"].as_str().unwrap();
    let name = format!("{file}.png");
    replies.push(Reply::new(
        "getFile",
        &[("file_id", file)],
        json!({"ok":true,"result":{"file_path":name}}),
    ));
    let mut reply = Reply::new("getFile", &[], Value::Null);
    reply.method = format!("GET {name}");
    reply.download = Some(bytes.to_vec());
    replies.push(reply);
}
fn mutation(
    method: &str,
    index: usize,
    bytes: &[u8],
    state: &Path,
    key: &str,
    format: &str,
    name: &str,
) -> Reply {
    let input = json!({"sticker":"attach://file0","format":format,
        "emoji_list":["😀"],"keywords":[format!("item{index}")]});
    let mut fields = BTreeMap::from([
        ("user_id".into(), "111111111".into()),
        ("name".into(), name.into()),
    ]);
    if method == "createNewStickerSet" {
        fields.insert("title".into(), "Pack 1".into());
        fields.insert("sticker_type".into(), "custom_emoji".into());
        fields.insert("stickers".into(), json!([input]).to_string());
    } else {
        fields.insert("sticker".into(), input.to_string());
    }
    let mut reply = Reply::new(method, &[], json!({"ok":true,"result":true}));
    reply.fields = fields;
    reply.files.insert("file0".into(), bytes.to_vec());
    reply.observation = Some((state.into(), "/in_flight/key".into(), json!(key)));
    reply
}

pub fn run() {
    assert_eq!(
        std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
        Ok("1")
    );
    let root = Root::new("fresh publication identity café");
    let project = _native::tooling_environment::physical_root(
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
    )
    .unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    // Source-pinned raster inputs replace the original generated 100x100 images.
    // This fixture proves identity policy, not Telegram's remote dimensions validator.
    let inputs = ["opaque", "alpha-edge", "transparent-rgb"].map(|name| {
        let row = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == name)
            .unwrap();
        let path = project.join(row["input"].as_str().unwrap());
        assert_eq!(
            _native::tooling_manifest::digest(&path).unwrap(),
            row["input_sha256"]
        );
        let bytes = std::fs::read(&path).unwrap();
        (path, bytes, row["expected"].clone())
    });
    let video: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/video-timeline.json"
    ))
    .unwrap();
    assert_eq!(video["source_revision"], fixture["source_revision"]);
    let video_inputs = ["green", "blue", "spiked"].map(|name| {
        let row = video["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == name)
            .unwrap();
        let path = project.join(row["input"].as_str().unwrap());
        assert_eq!(
            _native::tooling_manifest::digest(&path).unwrap(),
            row["input_sha256"]
        );
        let bytes = std::fs::read(&path).unwrap();
        (path, bytes, row["expected"].clone())
    });
    // These original streams collide at10fps but differ at their native30fps timeline.
    assert_eq!(
        video_inputs[1].2["content_key"],
        video_inputs[2].2["content_key"]
    );
    assert_ne!(
        video_inputs[1].2["rgba_sha256"],
        video_inputs[2].2["rgba_sha256"]
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let media = BoundedCodec {
        native: NativeMediaAdapter {
            executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
            root: project.clone(),
            scratch: root.0.join("unused-codec"),
        },
        deadline: Instant::now() + Duration::from_secs(60),
    };
    let config = Config::load_with_environment(
        root.0.clone(),
        &BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("BRAND_LOGO_BOTS".into(), String::new()),
        ]),
    )
    .unwrap();
    for variant in [
        "normal",
        "reordered",
        "foreign",
        "silent",
        "raced",
        "video",
        "video-collision",
    ] {
        let video = variant.starts_with("video");
        let format = if video { "video" } else { "static" };
        let name = if video { VIDEO_SET } else { SET };
        let inputs = if video { &video_inputs } else { &inputs };
        let keys = inputs[..2]
            .iter()
            .map(|(_, _, row)| row["content_key"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let mut first = member(0);
        let mut second = member(1);
        let mut foreign = json!({"file_id":"foreign","file_unique_id":"SOMEONE-ELSE",
            "custom_emoji_id":"333333333"});
        for sticker in [&mut first, &mut second, &mut foreign] {
            sticker["is_video"] = json!(video);
        }
        let started = Instant::now();
        let data = root.0.join(variant);
        let state_path = data.join("publish_pk.json");
        let mut cat = Catalog::open(&data.join("catalog.db"), "fixture UTC", &root.0).unwrap();
        for (index, (path, _, expected)) in inputs[..2].iter().enumerate() {
            assert!(
                cat.insert(
                    &json!({"content_key":keys[index],"fmt":format,
                "file_path":path,"phash":expected["phash"],"emojis":["😀"],
                "keywords":[format!("item{index}")]}),
                    "fixture UTC"
                )
                .unwrap()
            );
        }
        cat.order(&keys).unwrap();
        let rows = cat.all(None).unwrap();
        let mut state = json!({"base":"pk","sets":[],"sent":[name],"sent_full":[],"skipped":[]});
        _native::atomic::write_json(&state_path, &state).unwrap();
        let mut replies = vec![
            mutation(
                "createNewStickerSet",
                0,
                &inputs[0].1,
                &state_path,
                &keys[0],
                format,
                name,
            ),
            listing(name, std::slice::from_ref(&first)),
        ];
        download(&mut replies, &first, &inputs[0].1);
        replies.push(listing(name, std::slice::from_ref(&first)));
        replies.push(mutation(
            "addStickerToSet",
            1,
            &inputs[1].1,
            &state_path,
            &keys[1],
            format,
            name,
        ));
        if variant == "silent" {
            replies.push(listing(name, std::slice::from_ref(&first)));
        } else if variant == "raced" {
            replies.push(listing(
                name,
                &[first.clone(), second.clone(), foreign.clone()],
            ));
        } else {
            replies.push(listing(name, &[first.clone(), second.clone()]));
            download(
                &mut replies,
                &second,
                &inputs[if variant == "video-collision" { 2 } else { 1 }].1,
            );
            if variant != "video-collision" {
                let final_live = match variant {
                    "normal" | "video" => vec![first.clone(), second.clone()],
                    "reordered" => vec![second.clone(), first.clone()],
                    "foreign" => vec![foreign.clone(), second.clone()],
                    _ => unreachable!(),
                };
                replies.push(listing(name, &final_live));
                if variant == "foreign" {
                    download(&mut replies, &foreign, &inputs[2].1);
                }
            }
        }
        let expected = replies
            .iter()
            .map(|reply| (reply.method.clone(), reply.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let options = Options {
            base: "pk".into(),
            title: "Pack".into(),
            bot: "YourEmojiBot".into(),
            user_id: 111111111,
            fmt: format.into(),
            per_set: 200,
            new_set: false,
            into_pack: None,
            repaint: false,
            default_emoji: "😀".into(),
            logo: None,
        };
        println!("native fresh publication content: {variant}");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            publish_locked(
                &mut Publisher {
                    tg: &tg,
                    cat: &mut cat,
                    media: &media,
                    data: data.clone(),
                    runtime: &runtime,
                    config: Some(&config),
                },
                &options,
                &keys,
                &mut state,
            )
        }));
        let requests = server.finish();
        let result = match result {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        let requests = requests.unwrap();
        if matches!(variant, "normal" | "video") {
            assert_eq!(result.unwrap(), json!({"uploaded":2,"failed":0}));
        } else {
            let error = result.unwrap_err();
            assert!(
                error.contains(if matches!(variant, "silent" | "raced") {
                    "cannot identify exactly one new sticker"
                } else if variant == "video-collision" {
                    "new live sticker content is not proven ours"
                } else {
                    "live position 0"
                }),
                "{variant}: {error}"
            );
        }
        let confirmed = if matches!(variant, "silent" | "raced" | "video-collision") {
            1
        } else {
            2
        };
        let saved: Value = serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
        assert_eq!(saved, state);
        assert_eq!(state["sets"][0]["keys"], json!(&keys[..confirmed]));
        assert_eq!(state["sets"][0]["live"], confirmed);
        assert_eq!(state["sets"][0]["fmt"], format);
        assert_eq!(state["sets"][0]["name"], name);
        assert_eq!(state["skipped"], json!([]));
        if confirmed == 1 {
            assert_eq!(state["in_flight"]["key"], keys[1]);
            assert_eq!(state["in_flight"]["operation"], "add");
            assert!(!data.join("manifests").exists());
        } else {
            assert!(state["in_flight"].is_null());
        }
        if matches!(variant, "normal" | "video") {
            let manifest =
                std::fs::read_to_string(data.join("manifests").join(format!("{name}.md"))).unwrap();
            assert!(manifest.contains(&format!("format: {format}  |  2 emoji")));
            assert!(manifest.contains("| 1 | item0 | 111111111 |"));
            assert!(manifest.contains("| 2 | item1 | 222222222 |"));
        }
        assert_eq!(requests.len(), expected.len());
        for (request, (method, fields)) in requests.iter().zip(&expected) {
            assert_eq!(&request.method, method);
            assert_eq!(&request.form, fields);
        }
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "createNewStickerSet")
                .count(),
            1
        );
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "addStickerToSet")
                .count(),
            1
        );
        drop(tg);
        drop(cat);
        let mut cat = Catalog::open(&data.join("catalog.db"), "reopen UTC", &root.0).unwrap();
        for index in 0..2 {
            let cid = if index < confirmed {
                member(index)["custom_emoji_id"].clone()
            } else {
                Value::Null
            };
            assert_eq!(
                cat.dispatch(
                    json!({"operation":"custom_id","base":"pk","content_key":keys[index]})
                )
                .unwrap(),
                cid
            );
            assert_eq!(
                cat.dispatch(
                    json!({"operation":"seen","file_unique_id":format!("FRESH-item{index}")})
                )
                .unwrap(),
                if index < confirmed {
                    json!(keys[index])
                } else {
                    Value::Null
                }
            );
            let mut row = rows[index].clone();
            if index < confirmed {
                row["uploaded"] = json!(true);
                row["custom_emoji_id"] = cid;
            }
            assert_eq!(cat.get(&keys[index]).unwrap(), Some(row));
        }
        assert!(
            cat.dispatch(json!({"operation":"seen","file_unique_id":"SOMEONE-ELSE"}))
                .unwrap()
                .is_null()
        );
        drop(cat);
        assert_eq!(std::fs::read_dir(&media.native.scratch).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(data.join("tmp")).unwrap().count(), 0);
        for (path, bytes, _) in inputs {
            assert_eq!(&std::fs::read(path).unwrap(), bytes);
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "{variant} publication exceeded30s; retained"
        );
    }
    runtime.shutdown_timeout(Duration::from_secs(1));
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    root.clean().unwrap();
}
