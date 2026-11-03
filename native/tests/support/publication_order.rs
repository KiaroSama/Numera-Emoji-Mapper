//! Original mixed family and current panel order through persisted catalog and actual command.
use super::{Commands, FIRST, UTC, download, execute, listing, me, member, mutation, notification};
use crate::bot_api::{Reply, Server};
use _native::{atomic, catalog::Catalog, resume};
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) fn run(
    commands: &Commands<'_>,
    images: &[PathBuf; 3],
    bytes: &[Vec<u8>; 3],
    keys: &[String; 3],
) {
    let data = commands.root.join("publication-mixed-order");
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    let formats = ["static", "animated", "static", "video"];
    let mixed_keys = formats
        .iter()
        .enumerate()
        .map(|(i, fmt)| format!("{}:item{i:030}", &fmt[..1]))
        .collect::<Vec<_>>();
    for (i, fmt) in formats.iter().enumerate() {
        assert!(
            cat.insert(
                &json!({"content_key":mixed_keys[i],"fmt":fmt,"file_path":images[0],
            "emojis":["😀"],"keywords":[format!("i{i}")]}),
                UTC
            )
            .unwrap()
        );
    }
    cat.order(&mixed_keys).unwrap();
    let frozen = resume::freeze(json!({}), &cat.all(None).unwrap(), &["mixed".into()]).unwrap();
    assert_eq!(frozen, json!({"mixed":mixed_keys}));
    assert_eq!(
        frozen["mixed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().chars().next().unwrap())
            .collect::<Vec<_>>(),
        ['s', 'a', 's', 'v']
    );
    drop(cat);
    let state = json!({"base":"b","sets":[{"fmt":"mixed","index":1,"name":"b1_by_bot",
        "title":"T 1","live":1,"logo":true,"keys":[]}],"sent":[],"sent_full":[],"skipped":[]});
    let path = data.join("publish_b.json");
    atomic::write_json(&path, &state).unwrap();
    let loaded: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        resume::collection(loaded, "b").unwrap()["sets"][0]["fmt"],
        "mixed"
    );
    let mut bad = state.clone();
    bad["sets"][0]["fmt"] = json!("sideways");
    assert!(resume::collection(bad, "b").is_err());
    let mut invalid = state.clone();
    invalid["sent_full"] = json!("nope");
    assert!(resume::collection(invalid, "b").is_err());

    let data = commands.root.join("publication-current-panel-order");
    let path = data.join("publish_fixture.json");
    let plan_path = data.join("publish_plan_fixture.json");
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    for i in 0..2 {
        assert!(
            cat.insert(
                &json!({"content_key":keys[i],"fmt":"static","file_path":images[i],
            "emojis":["😀"],"keywords":["public | artwork"]}),
                UTC
            )
            .unwrap()
        );
    }
    atomic::write_json(&plan_path, &json!({"mixed":keys[..2]})).unwrap();
    cat.order(&[keys[1].clone(), keys[0].clone()]).unwrap();
    drop(cat);
    let first = member("order-first", "order-first-unique", "1");
    let second = member("order-second", "order-second-unique", "2");
    let mut replies = vec![
        me(),
        mutation(&path, FIRST, &keys[1], (&bytes[1], false), true),
        listing(FIRST, std::slice::from_ref(&first)),
    ];
    download(&mut replies, "order-first", &bytes[1]);
    replies.push(listing(FIRST, std::slice::from_ref(&first)));
    replies.push(mutation(&path, FIRST, &keys[0], (&bytes[0], false), false));
    replies.push(listing(FIRST, &[first.clone(), second.clone()]));
    download(&mut replies, "order-second", &bytes[0]);
    replies.push(listing(FIRST, &[first, second]));
    replies.push(notification(FIRST, false, &path));
    let expected = replies
        .iter()
        .map(|r| (r.method.clone(), r.fields.clone()))
        .collect::<Vec<_>>();
    let mut server = Server::new(replies);
    println!("native collection current panel order command");
    let result = execute(commands, &data, &[], &server, None);
    assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), expected.len());
    for (request, (method, fields)) in requests.iter().zip(expected) {
        assert_eq!(request.method, method);
        assert_eq!(request.form, fields);
    }
    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["sets"][0]["name"], FIRST);
    assert_eq!(saved["sets"][0]["keys"], json!([keys[1], keys[0]]));
    assert_eq!(saved["sets"][0]["live"], 2);
    assert_eq!(saved["sets"][0]["fmt"], "mixed");
    let stored_plan: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
    assert_eq!(
        stored_plan,
        json!({"mixed":keys[..2]}),
        "stored membership is not upload order"
    );
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    assert_eq!(
        cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":keys[1]}))
            .unwrap(),
        "1"
    );
    assert_eq!(
        cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":keys[0]}))
            .unwrap(),
        "2"
    );
    assert_eq!(
        cat.all(None)
            .unwrap()
            .iter()
            .map(|r| r["content_key"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        vec![keys[1].clone(), keys[0].clone()]
    );
    drop(cat);
    assert!(!_native::process_owner::cleanup_pending(commands.root));
    recorded_titles(commands, images, bytes, keys);
}

fn recorded_titles(
    commands: &Commands<'_>,
    images: &[PathBuf; 3],
    bytes: &[Vec<u8>; 3],
    keys: &[String; 3],
) {
    const STATIC: &str = "fixtures1_by_YourEmojiBot";
    const VIDEO: &str = "fixturev1_by_YourEmojiBot";
    for cross_format in [false, true] {
        let data = commands
            .root
            .join(format!("publication-title-{cross_format}"));
        let path = data.join("publish_fixture.json");
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        for i in 0..if cross_format { 1 } else { 2 } {
            assert!(
                cat.insert(
                    &json!({"content_key":keys[i],"fmt":"static","file_path":images[i],
                "emojis":["😀"],"keywords":["public | artwork"]}),
                    UTC
                )
                .unwrap()
            );
        }
        let title = if cross_format {
            "Fixture 2"
        } else {
            "Curated title — kept"
        };
        let initial = json!({"base":"fixture","sets":[{"fmt":if cross_format {"video"} else {"static"},
            "index":1,"name":if cross_format {VIDEO} else {STATIC},"title":if cross_format {"Video from earlier run"} else {title},
            "live":if cross_format {0} else {1},"logo":false,"keys":if cross_format {json!([])} else {json!([keys[0]])}}],
            "sent":if cross_format {json!([VIDEO])} else {json!([STATIC])},"sent_full":[],"skipped":[],"in_flight":null});
        atomic::write_json(&path, &initial).unwrap();
        let old = member("title-old", "title-old-unique", "1");
        let added = member("title-added", "title-added-unique", "2");
        if !cross_format {
            cat.record_seen("title-old-unique", &keys[0]).unwrap();
            cat.uploaded(&json!({"content_key":keys[0],"custom_emoji_id":"1","base":"fixture","set_name":STATIC}), UTC).unwrap();
        }
        drop(cat);
        let mut replies = vec![me()];
        if !cross_format {
            replies.push(listing(STATIC, std::slice::from_ref(&old)));
            replies.push(listing(STATIC, std::slice::from_ref(&old)));
        }
        let i = usize::from(!cross_format);
        let mut upload = mutation(&path, STATIC, &keys[i], (&bytes[i], false), cross_format);
        if cross_format {
            upload.fields.insert("title".into(), title.into());
        }
        replies.push(upload);
        let live = if cross_format {
            vec![added.clone()]
        } else {
            vec![old, added]
        };
        replies.push(listing(STATIC, &live));
        download(&mut replies, "title-added", &bytes[i]);
        let notify = |full: bool| {
            let mut reply = Reply::new(
                "sendMessage",
                &[
                    ("chat_id", "111111111"),
                    (
                        "text",
                        &format!("✅ {title}\nhttps://t.me/addemoji/{STATIC}"),
                    ),
                    ("disable_web_page_preview", "true"),
                ],
                json!({"ok":true,"result":{"message_id":1}}),
            );
            reply.observation = Some((
                path.clone(),
                if full { "/sent_full" } else { "/sent" }.into(),
                if full { json!([]) } else { json!([VIDEO]) },
            ));
            reply
        };
        if !cross_format {
            replies.push(notify(true));
        }
        replies.push(listing(STATIC, &live));
        if cross_format {
            replies.push(notify(false));
            replies.push(listing(VIDEO, &[]));
        }
        let expected = replies
            .iter()
            .map(|r| (r.method.clone(), r.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        println!("native collection recorded title: cross-format={cross_format}");
        let result = execute(
            commands,
            &data,
            &[
                "--formats",
                if cross_format {
                    "static,video"
                } else {
                    "static"
                },
                "--per-set",
                "2",
            ],
            &server,
            None,
        );
        assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), expected.len());
        for (request, (method, fields)) in requests.iter().zip(expected) {
            assert_eq!(request.method, method);
            assert_eq!(request.form, fields);
            if request.method == "createNewStickerSet" || request.method == "addStickerToSet" {
                assert_eq!(request.form["name"], STATIC);
            }
        }
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let at = usize::from(cross_format);
        assert_eq!(saved["sets"].as_array().unwrap().len(), at + 1);
        assert_eq!(saved["sets"][at]["title"], title);
        assert_eq!(saved["sets"][at]["name"], STATIC);
        assert_eq!(saved["sets"][at]["index"], 1);
        assert_eq!(saved["sets"][at]["fmt"], "static");
        assert_eq!(
            saved["sets"][at]["keys"],
            if cross_format {
                json!([keys[0]])
            } else {
                json!(keys[..2])
            }
        );
        assert_eq!(saved["sets"][at]["live"], if cross_format { 1 } else { 2 });
        assert_eq!(
            saved["sent"],
            if cross_format {
                json!([VIDEO, STATIC])
            } else {
                json!([STATIC])
            }
        );
        assert_eq!(
            saved["sent_full"],
            if cross_format {
                json!([])
            } else {
                json!([STATIC])
            }
        );
        assert!(saved["in_flight"].is_null());
        if cross_format {
            assert_eq!(saved["sets"][0], initial["sets"][0]);
        }
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        assert_eq!(
            cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":keys[i]}))
                .unwrap(),
            "2"
        );
        assert_eq!(
            cat.dispatch(json!({"operation":"published_sets"})).unwrap()[&keys[i]],
            STATIC
        );
        drop(cat);
        assert!(!_native::process_owner::cleanup_pending(commands.root));
    }
}
