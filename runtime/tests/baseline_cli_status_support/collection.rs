use crate::{
    identity::{art, fingerprint, insert},
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
    sync::{Arc, Mutex},
};
pub fn shape(path: &Path, index: u32) -> PathBuf {
    let mut im = image::RgbaImage::new(100, 100);
    for x in index..index + 30 {
        for y in index..index + 30 {
            im[(x, y)] = image::Rgba([(10 * index % 256) as u8, 60, 200, 255]);
        }
    }
    write(path, native::native_raster::encode_png(&im).unwrap());
    path.into()
}
fn pack_server(payloads: BTreeMap<String, Vec<u8>>, stickers: Vec<Value>) -> Server {
    Server::new(move |r| {
        if r.verb == "GET" {
            Reply::Bytes(200, payloads[r.method()].clone())
        } else {
            match r.method() {
                "getMe" => Reply::ok(json!({"username":"YourEmojiBot"})),
                "getStickerSet" => Reply::ok(json!({"title":"pack","stickers":stickers})),
                "getFile" => {
                    let form = r.form();
                    let id = &form["file_id"];
                    Reply::ok(
                        json!({"file_path":if id=="fixture-file"{"fixture.png"}else{id.as_str()}}),
                    )
                }
                _ => panic!("unexpected {}", r.method()),
            }
        }
    })
}
fn downloads(bot: &Server) -> Vec<String> {
    bot.observed
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.verb == "GET")
        .map(|r| r.method().to_owned())
        .collect()
}
fn fetch_limit(limit: u32, known: &[u32], count: u32) {
    let f = Fixture::new();
    let mut payloads = BTreeMap::new();
    let mut stickers = Vec::new();
    for i in 0..count {
        let path = shape(&f.root.join(format!("source-{i}.png")), i);
        payloads.insert(i.to_string(), std::fs::read(&path).unwrap());
        stickers
            .push(json!({"file_id":i.to_string(),"file_unique_id":format!("FU-{i}"),"emoji":"😀"}));
    }
    let bot = pack_server(payloads, stickers);
    {
        let mut cat = f.catalog();
        for i in known {
            let path = shape(&f.data().join(format!("media/static/seed{i}.png")), *i);
            cat.insert(&json!({"content_key":format!("s:seed{i:030}"),"fmt":"static","file_path":path,"file_unique_id":format!("FU-{i}")}),UTC).unwrap();
            cat.record_seen(&format!("FU-{i}"), &format!("s:seed{i:030}"))
                .unwrap();
        }
    }
    let r = f.run(
        &[
            "fetch-pack",
            "pack",
            "--limit",
            &limit.to_string(),
            "--repaintable",
            "keep",
        ],
        &bot.environment(),
    );
    r.code(0);
    let new = if limit == 0 {
        count - known.len() as u32
    } else {
        limit
    };
    assert!(
        r.out
            .contains(&format!("new={new} dedup={} failed=0", known.len())),
        "{}",
        r.out
    );
    assert_eq!(
        downloads(&bot),
        (known.len() as u32..known.len() as u32 + new)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
    );
}
// tests.test_fetch_pack_limit.FetchPackLimitTest.test_known_leading_stickers_do_not_consume_the_limit
#[test]
fn known_leading_stickers_do_not_consume_new_limit() {
    fetch_limit(1, &[0, 1], 4);
}
// tests.test_fetch_pack_limit.FetchPackLimitTest.test_limit_still_stops_the_loop
#[test]
fn new_item_limit_stops_before_next_download() {
    fetch_limit(2, &[], 5);
}
// tests.test_fetch_pack_limit.FetchPackLimitTest.test_zero_limit_fetches_everything
#[test]
fn zero_limit_downloads_every_sticker() {
    fetch_limit(0, &[], 3);
}
fn named_packs(args: &[&str], expected: &[&str], missing: bool) {
    let f = Fixture::new();
    let payloads = (1..=2)
        .map(|i| {
            let path = shape(&f.root.join(format!("payload-{i}.png")), i);
            (format!("{i}.png"), std::fs::read(path).unwrap())
        })
        .collect::<BTreeMap<_, _>>();
    let fetched = Arc::new(Mutex::new(Vec::new()));
    let fetch = fetched.clone();
    let bot = Server::new(move |r| {
        if r.verb == "GET" {
            Reply::Bytes(200, payloads[r.method()].clone())
        } else {
            match r.method() {
                "getCustomEmojiStickers" => {
                    let ids: Vec<String> =
                        serde_json::from_str(&r.form()["custom_emoji_ids"]).unwrap();
                    let values: Vec<Value> = ids
                        .iter()
                        .filter_map(|id| match id.as_str() {
                            "5000000000000000001" | "5000000000000000002" => {
                                Some(json!({"custom_emoji_id":id,"set_name":"alpha_by_bot"}))
                            }
                            "5000000000000000003" => {
                                Some(json!({"custom_emoji_id":id,"set_name":"beta_by_bot"}))
                            }
                            _ => None,
                        })
                        .collect();
                    Reply::ok(json!(values))
                }
                "getStickerSet" => {
                    let name = r.form()["name"].clone();
                    fetch.lock().unwrap().push(name.clone());
                    let n = fetch.lock().unwrap().len();
                    Reply::ok(
                        json!({"title":name,"stickers":[{"file_id":n.to_string(),"file_unique_id":format!("FU-{name}"),"emoji":"😀"}]}),
                    )
                }
                "getFile" => Reply::ok(json!({"file_path":format!("{}.png",r.form()["file_id"])})),
                _ => panic!("unexpected {}", r.method()),
            }
        }
    });
    let mut command = vec!["fetch-pack"];
    command.extend_from_slice(args);
    command.extend(["--repaintable", "keep"]);
    let r = f.run(&command, &bot.environment());
    r.code(if missing { 3 } else { 0 });
    assert_eq!(*fetched.lock().unwrap(), expected);
    if missing {
        assert!(r.err.contains("5999999999999999999"), "{}", r.err);
    }
}
// tests.test_fetch_pack_ids.APackCanBeNamedByOneOfItsEmoji.test_two_ids_of_one_pack_fetch_it_once
#[test]
fn two_emoji_ids_of_same_pack_fetch_once() {
    named_packs(
        &["5000000000000000001", "premium-id:5000000000000000002"],
        &["alpha_by_bot"],
        false,
    );
}
// tests.test_fetch_pack_ids.APackCanBeNamedByOneOfItsEmoji.test_names_and_ids_mix_and_nothing_is_fetched_twice
#[test]
fn mixed_pack_names_and_ids_preserve_first_occurrence() {
    named_packs(
        &[
            "https://t.me/addemoji/beta_by_bot",
            "5000000000000000003",
            "5000000000000000001",
        ],
        &["beta_by_bot", "alpha_by_bot"],
        false,
    );
}
// tests.test_fetch_pack_ids.APackCanBeNamedByOneOfItsEmoji.test_an_id_that_names_no_pack_is_reported_and_partial
#[test]
fn unknown_pack_id_is_logged_and_partial() {
    named_packs(
        &["5000000000000000001", "5999999999999999999"],
        &["alpha_by_bot"],
        true,
    );
}
// tests.test_native_fetch_cli.NativePackCommand.test_download_reencode_and_seen_file_skip_through_native_cli
#[test]
fn native_pack_reencodes_then_skips_known_file_on_repeat() {
    let f = Fixture::new();
    let source = art(&f, "numera-emoji-mapper-logo.png", "source.png");
    let payload = std::fs::read(&source).unwrap();
    native::codec_image::reencode_static(&source).unwrap();
    let expected = fingerprint(&source);
    let bot = pack_server(
        BTreeMap::from([("fixture.png".into(), payload)]),
        vec![
            json!({"file_id":"fixture-file","file_unique_id":"fixture-unique","emoji":"😀","keywords":["public-art"]}),
        ],
    );
    for n in 0..2 {
        let r = f.run(
            &[
                "fetch-pack",
                "fixture",
                "--repaintable",
                "skip",
                "--limit",
                "1",
            ],
            &bot.environment(),
        );
        r.code(0);
        assert_eq!(
            r.out,
            format!(
                "Done. new={} dedup={} failed=0\n  catalog static: 1 total (1 pending upload)\n",
                u8::from(n == 0),
                u8::from(n == 1)
            )
        );
    }
    let mut cat = f.catalog();
    let rows = cat.all(None).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["content_key"], expected["content_key"]);
    assert_eq!(rows[0]["phash"], expected["phash"]);
    assert_eq!(
        cat.dispatch(json!({"operation":"seen","file_unique_id":"fixture-unique"}))
            .unwrap(),
        expected["content_key"]
    );
    assert_eq!(
        native::codec_image::same_image(Path::new(rows[0]["file_path"].as_str().unwrap()), &source),
        Some(true)
    );
    assert_eq!(
        bot.methods(),
        ["getStickerSet", "getFile", "fixture.png", "getStickerSet"]
    );
}
// tests.test_native_local_cli.NativeLocalCommand.test_real_image_ingest_and_repeat_match_source_identity
#[test]
fn native_local_real_image_repeat_matches_identity() {
    let f = Fixture::new();
    let source = project().join("assets/numera-emoji-mapper-logo.png");
    let expected = art(&f, "numera-emoji-mapper-logo.png", "expected.png");
    let fp = fingerprint(&expected);
    for n in 0..2 {
        let r = f.run(
            &[
                "add-media",
                source.to_str().unwrap(),
                "--as",
                "static",
                "--emoji",
                "😀",
                "--phash-threshold",
                "-1",
            ],
            &[],
        );
        r.code(0);
        assert_eq!(
            r.out,
            format!(
                "Done. new={} dedup={} failed=0\n  catalog static: 1 total (1 pending upload)\n",
                u8::from(n == 0),
                u8::from(n == 1)
            )
        );
    }
    let rows = f.catalog().all(None).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["content_key"], fp["content_key"]);
    assert_eq!(rows[0]["phash"], fp["phash"]);
    assert_eq!(
        native::codec_image::same_image(
            Path::new(rows[0]["file_path"].as_str().unwrap()),
            &expected
        ),
        Some(true)
    );
    assert_eq!(std::fs::read_dir(f.data().join("tmp")).unwrap().count(), 0);
}
// tests.test_native_local_cli.NativeLocalCommand.test_undecodable_catalog_rival_refuses_merge_and_retains_incoming_bytes
#[test]
fn local_undecodable_rival_retains_incoming_and_prior_rows() {
    let f = Fixture::new();
    let expected = art(&f, "numera-emoji-mapper-logo.png", "expected.png");
    let mut item = fingerprint(&expected);
    let rival = f.root.join("rival.png");
    write(&rival, b"undecodable retained catalog media");
    item["fmt"] = json!("static");
    item["file_path"] = json!(rival);
    item["emojis"] = json!(["⭐"]);
    item["keywords"] = json!(["original"]);
    let prior = {
        let mut cat = f.catalog();
        cat.insert(&item, UTC).unwrap();
        cat.record_seen("prior-unique", item["content_key"].as_str().unwrap())
            .unwrap();
        cat.all(None).unwrap()
    };
    let source = project().join("assets/numera-emoji-mapper-logo.png");
    let r = f.run(
        &["add-media", source.to_str().unwrap(), "--as", "static"],
        &[],
    );
    r.code(4);
    assert!(r.out.contains("new=0 dedup=0 failed=1"));
    let mut cat = f.catalog();
    assert_eq!(cat.all(None).unwrap(), prior);
    assert_eq!(
        cat.dispatch(json!({"operation":"seen","file_unique_id":"prior-unique"}))
            .unwrap(),
        item["content_key"]
    );
    assert_eq!(
        std::fs::read(rival).unwrap(),
        b"undecodable retained catalog media"
    );
    let retained = std::fs::read_dir(f.data().join("media/static"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(retained.len(), 1);
    assert_eq!(
        native::codec_image::decoded(&retained[0]).unwrap().rgba,
        native::codec_image::decoded(&expected).unwrap().rgba
    );
    assert_eq!(std::fs::read_dir(f.data().join("tmp")).unwrap().count(), 0);
}
// tests.test_native_ids_cli.NativeTintCommand.test_known_file_unique_id_does_not_skip_requested_repaint
#[test]
fn known_file_identity_does_not_skip_requested_tint() {
    let f = Fixture::new();
    let source = art(&f, "numera-emoji-mapper-logo.png", "source.png");
    let payload = std::fs::read(&source).unwrap();
    let expected = f.root.join("expected.png");
    write(&expected, &payload);
    native::codec_image::reencode_static(&expected).unwrap();
    assert!(native::codec_image::repaint_static(&expected, [255, 0, 0]).unwrap());
    let key = fingerprint(&expected)["content_key"].clone();
    let original = {
        let mut cat = f.catalog();
        let key = insert(&mut cat, &source, json!({"emojis":["😀"]}));
        cat.record_seen("known-source", &key).unwrap();
        key
    };
    let bot = Server::new(move |r| {
        if r.verb == "GET" {
            Reply::Bytes(200, payload.clone())
        } else {
            match r.method() {
                "getMe" => Reply::ok(json!({"username":"YourEmojiBot"})),
                "getCustomEmojiStickers" => Reply::ok(
                    json!([{"custom_emoji_id":"111111111111111","file_id":"source-file","file_unique_id":"known-source","emoji":"😀","needs_repainting":true}]),
                ),
                "getFile" => Reply::ok(json!({"file_path":"source.png"})),
                _ => panic!("unexpected {}", r.method()),
            }
        }
    });
    let r = f.run(
        &[
            "fetch-emoji-ids",
            "--id",
            "111111111111111",
            "--tint",
            "#FF0000",
        ],
        &bot.environment(),
    );
    r.code(0);
    assert!(
        r.out
            .contains("Done. unique_ids=1 new=1 dedup=0 failed=0 missing=0 repainted=1\n")
    );
    assert!(
        r.out
            .contains("  catalog static: 2 total (2 pending upload)\n")
    );
    assert_eq!(downloads(&bot), ["source.png"]);
    let mut cat = f.catalog();
    assert_eq!(cat.all(None).unwrap().len(), 2);
    let tinted = cat.get(key.as_str().unwrap()).unwrap().unwrap();
    assert!(
        tinted["keywords"]
            .as_array()
            .unwrap()
            .contains(&json!("tint:#FF0000"))
    );
    assert_eq!(
        native::codec_image::same_image(
            Path::new(tinted["file_path"].as_str().unwrap()),
            &expected
        ),
        Some(true)
    );
    assert_eq!(
        cat.dispatch(json!({"operation":"seen","file_unique_id":"known-source"}))
            .unwrap(),
        original
    );
}
