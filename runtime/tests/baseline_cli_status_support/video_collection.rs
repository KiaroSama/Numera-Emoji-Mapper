use crate::{
    identity::art,
    support::{
        http::{Reply, Server},
        *,
    },
};
use numera_runtime as native;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};
pub const RED: [u8; 4] = [220, 20, 20, 255];
pub const BLUE: [u8; 4] = [20, 20, 220, 255];
pub fn encode(root: &Path, name: &str, colors: &[[u8; 4]]) -> PathBuf {
    let source = root.join(format!("{name}.rgba"));
    let raw = colors
        .iter()
        .flat_map(|c| c.repeat(100 * 100))
        .collect::<Vec<_>>();
    write(&source, raw);
    let out = root.join(format!("{name}.webm"));
    let mut args = [
        "-y",
        "-v",
        "error",
        "-f",
        "rawvideo",
        "-pixel_format",
        "rgba",
        "-video_size",
        "100x100",
        "-framerate",
        "30",
        "-i",
    ]
    .map(str::to_owned)
    .to_vec();
    args.push(source.display().to_string());
    args.extend(
        [
            "-c:v",
            "libvpx-vp9",
            "-threads",
            "1",
            "-cpu-used",
            "8",
            "-lossless",
            "1",
            "-pix_fmt",
            "yuva420p",
            "-auto-alt-ref",
            "0",
        ]
        .map(str::to_owned),
    );
    args.push(out.display().to_string());
    let env = std::env::vars_os().collect();
    runtime()
        .block_on(native::codec_process::capture_with_budget(
            root,
            &native::codec_process::executable("ffmpeg").unwrap(),
            &args,
            1024 * 1024,
            None,
            &env,
            native::codec_process::Budget {
                wall: Duration::from_secs(30),
                idle: Duration::from_secs(30),
                warning: None,
            },
        ))
        .unwrap();
    std::fs::remove_file(source).unwrap();
    out
}
pub fn media(f: &Fixture) -> native::media_native_adapter::NativeMediaAdapter {
    native::media_native_adapter::NativeMediaAdapter {
        executable: f.binary.clone(),
        root: f.root.clone(),
        scratch: f.root.join("expected-codec"),
    }
}
// tests.test_native_collect_partial.MixedCollectionFailures.test_good_formats_survive_corrupt_download_and_unresolved_id
#[test]
fn real_mixed_formats_survive_corrupt_download_and_unresolved_id() {
    let f = Fixture::new();
    let static_path = art(&f, "numera-emoji-mapper-logo.png", "source.png");
    let video = encode(&f.root, "motion", &[RED, BLUE, RED]);
    let animated = project().join("tests/fixtures/lottie/red_circle_512.tgs");
    let sources = BTreeMap::from([
        ("static".to_owned(), static_path),
        ("animated".into(), animated),
        ("video".into(), video),
    ]);
    let mut payloads = BTreeMap::new();
    let mut expected = BTreeMap::new();
    let codec = media(&f);
    for (fmt, path) in sources {
        let bytes = std::fs::read(&path).unwrap();
        payloads.insert(fmt.clone(), bytes.clone());
        let copy = f.root.join(format!(
            "expected-{}{}",
            fmt,
            path.extension()
                .map(|e| format!(".{}", e.to_str().unwrap()))
                .unwrap_or_default()
        ));
        write(&copy, bytes);
        let prepared =
            runtime()
                .block_on(codec.call(
                    json!({"operation":"prepare","source":copy,"format":fmt,"reencode":true}),
                ))
                .unwrap();
        expected.insert(fmt, prepared["content_key"].clone());
    }
    payloads.insert("bad".into(), b"not a decodable image".to_vec());
    let ids = (0..5)
        .map(|i| (111111111111111_u64 + i).to_string())
        .collect::<Vec<_>>();
    let stickers=["static","animated","video","bad"].into_iter().enumerate().map(|(i,fmt)|json!({"custom_emoji_id":ids[i],"file_id":fmt,"file_unique_id":format!("fixture-{fmt}"),"emoji":"😀","is_video":fmt=="video","is_animated":fmt=="animated"})).collect::<Vec<_>>();
    let bot = Server::new(move |r| {
        if r.verb == "GET" {
            Reply::Bytes(200, payloads[r.method()].clone())
        } else {
            match r.method() {
                "getMe" => Reply::ok(json!({"username":"YourEmojiBot"})),
                "getCustomEmojiStickers" => Reply::ok(json!(stickers)),
                "getFile" => Reply::ok(json!({"file_path":r.form()["file_id"]})),
                _ => panic!("unexpected {}", r.method()),
            }
        }
    });
    let mut args = vec!["fetch-emoji-ids", "--repaintable", "keep"];
    for id in &ids {
        args.extend(["--id", id]);
    }
    let r = f.run(&args, &bot.environment());
    r.code(3);
    assert!(
        r.out
            .contains("Done. unique_ids=5 new=3 dedup=0 failed=1 missing=1"),
        "{}",
        r.out
    );
    let mut cat = f.catalog();
    let rows = cat.all(None).unwrap();
    assert_eq!(
        rows.iter()
            .map(|item| (
                item["fmt"].as_str().unwrap().to_owned(),
                item["content_key"].clone()
            ))
            .collect::<BTreeMap<_, _>>(),
        expected
    );
    for item in rows {
        let fmt = item["fmt"].as_str().unwrap();
        assert_eq!(
            cat.dispatch(json!({"operation":"seen","file_unique_id":format!("fixture-{fmt}")}))
                .unwrap(),
            item["content_key"]
        );
        assert!(Path::new(item["file_path"].as_str().unwrap()).is_file());
    }
    assert_eq!(
        cat.dispatch(json!({"operation":"seen","file_unique_id":"fixture-bad"}))
            .unwrap(),
        Value::Null
    );
    let tmp = f.data().join("tmp");
    assert!(!std::fs::read_dir(&tmp).unwrap().any(|e| {
        let name = e.unwrap().file_name().to_string_lossy().into_owned();
        name.starts_with("native-") && name.contains('.')
    }));
    if tmp.join("native-codec").exists() {
        assert_eq!(
            std::fs::read_dir(tmp.join("native-codec")).unwrap().count(),
            0
        );
    }
    let calls = bot.observed.lock().unwrap();
    assert_eq!(
        calls
            .iter()
            .filter(|r| r.verb == "GET")
            .map(|r| r.method().to_owned())
            .collect::<std::collections::BTreeSet<_>>(),
        ["static", "animated", "video", "bad"]
            .map(str::to_owned)
            .into_iter()
            .collect()
    );
    assert!(
        !calls
            .iter()
            .any(|r| ["createNewStickerSet", "addStickerToSet"].contains(&r.method()))
    );
}
// tests.test_native_video_visibility.NativeVideoVisibility.test_blank_refuses_publication_but_fade_in_is_visible
#[test]
fn real_blank_vp9_skips_publication_and_fade_in_is_visible() {
    let f = Fixture::new();
    let blank = encode(&f.root, "blank", &[[0, 0, 0, 0]; 30]);
    let fade = encode(
        &f.root,
        "fade",
        &[[[0, 0, 0, 0]; 15].as_slice(), [RED; 15].as_slice()].concat(),
    );
    assert!(
        runtime()
            .block_on(native::publication_media::usable(&f.root, &fade, "video"))
            .unwrap()
    );
    let fingerprint = runtime()
        .block_on(
            media(&f).call(json!({"operation":"fingerprint","source":blank,"format":"video"})),
        )
        .unwrap();
    let key = fingerprint["content_key"].as_str().unwrap();
    {
        let mut cat = f.catalog();
        cat.insert(&json!({"content_key":key,"fmt":"video","file_path":blank,"phash":fingerprint["phash"]}),UTC).unwrap();
    }
    let bot = Server::new(|r| {
        assert_eq!(r.method(), "getMe", "blank media reached a wire mutation");
        Reply::ok(json!({"username":"YourEmojiBot"}))
    });
    let r = f.run(
        &[
            "build-collection",
            "--base",
            "fixture",
            "--title",
            "Fixture",
            "--mixed",
            "--no-brand-logo",
            "--user-id",
            "1",
        ],
        &bot.environment(),
    );
    r.code(0);
    assert_eq!(bot.methods(), ["getMe"]);
    let state = read_json(&f.data().join("publish_fixture.json"));
    assert!(state["skipped"].as_array().unwrap().contains(&json!(key)));
    assert_eq!(state["sets"], json!([]));
}
