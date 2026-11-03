//! Source-backed restart/adoption/no-resend and closed foreign-tail command scenarios.
use super::{
    Commands, FIRST, SECOND, UTC, download, execute, listing, me, member, mutation, notification,
    set,
};
use crate::bot_api::Server;
use _native::{atomic, catalog::Catalog};
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) fn run(
    commands: &Commands<'_>,
    images: &[PathBuf; 3],
    bytes: &[Vec<u8>; 3],
    keys: &[String; 3],
) {
    for variant in [
        "unrecorded",
        "occupied",
        "ambiguous",
        "closed",
        "closed-target",
    ] {
        let data = commands.root.join(format!("publication-dedup-{variant}"));
        let path = data.join("publish_fixture.json");
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        for index in 0..2 {
            assert!(
                cat.insert(
                    &json!({"content_key":keys[index],"fmt":"static",
                "file_path":images[index],"emojis":["😀"],"keywords":["public | artwork"]}),
                    UTC
                )
                .unwrap()
            );
            cat.record_seen(&format!("SOURCE-{index}"), &keys[index])
                .unwrap();
        }
        let first = member("dedup-first", "SOURCE-0", "1");
        let second = member("dedup-second", "UPLOADED-1", "2");
        let foreign = member("dedup-foreign", "FOREIGN", "9");
        let closed = variant.starts_with("closed");
        let seeded = variant == "unrecorded" || closed;
        let initial = json!({"base":"fixture","sets":if seeded {
            let mut record = set(FIRST, 1, &keys[0]);
            record["keys"] = json!([]);
            record["live"] = json!(if closed {2} else {1});
            json!([record])
        } else { json!([]) },"sent":[],"sent_full":[],"skipped":[],"in_flight":null});
        atomic::write_json(&path, &initial).unwrap();
        let rows = cat.all(None).unwrap();
        drop(cat);
        let mut replies = vec![me()];
        if seeded {
            let live = if closed {
                vec![first.clone(), foreign.clone()]
            } else {
                vec![first.clone()]
            };
            replies.push(listing(FIRST, &live));
            if closed {
                download(&mut replies, "dedup-foreign", &bytes[2]);
            }
        } else {
            let mut create = mutation(&path, FIRST, &keys[0], (&bytes[0], false), true);
            if variant == "occupied" {
                create.payload = json!({"ok":false,"description":"Bad Request: sticker set name is already occupied"});
            }
            replies.push(create);
            replies.push(listing(FIRST, std::slice::from_ref(&first)));
            if variant == "occupied" {
                replies.push(listing(FIRST, std::slice::from_ref(&first)));
            } else {
                download(&mut replies, "dedup-first", &bytes[0]);
            }
        }
        if variant != "closed-target" {
            let name = if closed { SECOND } else { FIRST };
            if !closed {
                replies.push(listing(FIRST, std::slice::from_ref(&first)));
            }
            let mut add = mutation(&path, name, &keys[1], (&bytes[1], false), closed);
            if variant == "ambiguous" {
                add.payload = json!({"ok":true});
            }
            replies.push(add);
            let live = if closed {
                vec![second.clone()]
            } else {
                vec![first.clone(), second.clone()]
            };
            replies.push(listing(name, &live));
            download(&mut replies, "dedup-second", &bytes[1]);
            if closed {
                replies.push(listing(FIRST, &[first.clone(), foreign.clone()]));
                download(&mut replies, "dedup-foreign", &bytes[2]);
            }
            replies.push(listing(name, &live));
            let mut notify = notification(name, false, &path);
            notify.observation = Some((path.clone(), "/sent".into(), json!([])));
            replies.push(notify);
        }
        let expected = replies
            .iter()
            .map(|reply| (reply.method.clone(), reply.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        println!("native collection dedup command: {variant}");
        let extra: &[&str] = if variant == "closed-target" {
            &["--into-pack", "1"]
        } else {
            &[]
        };
        let result = execute(commands, &data, extra, &server, None);
        assert_eq!(
            result.code,
            if variant == "closed-target" { 4 } else { 0 },
            "{variant}: {} {}",
            result.stdout,
            result.stderr
        );
        if variant == "closed-target" {
            assert!(!result.stdout.contains("DONE"));
        }
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), expected.len(), "{variant}");
        for (request, (method, fields)) in requests.iter().zip(expected) {
            assert_eq!(request.method, method, "{variant}");
            assert_eq!(request.form, fields, "{variant}");
        }
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(saved["in_flight"].is_null());
        assert_eq!(
            saved["sets"][0]["keys"],
            if closed {
                json!([keys[0]])
            } else {
                json!(keys[..2])
            }
        );
        assert_eq!(saved["sets"][0]["live"], 2);
        if variant == "closed" {
            assert_eq!(saved["sets"].as_array().unwrap().len(), 2);
            assert_eq!(saved["sets"][1]["keys"], json!([keys[1]]));
            assert_eq!(saved["sets"][1]["live"], 1);
            assert_eq!(saved["sent"], json!([SECOND]));
        } else {
            assert_eq!(saved["sets"].as_array().unwrap().len(), 1);
            assert_eq!(
                saved["sent"],
                if variant == "closed-target" {
                    json!([])
                } else {
                    json!([FIRST])
                }
            );
        }
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        for index in 0..2 {
            assert_eq!(
                cat.dispatch(
                    json!({"operation":"seen","file_unique_id":format!("SOURCE-{index}")})
                )
                .unwrap(),
                keys[index]
            );
            let landed = index == 0 || variant != "closed-target";
            assert_eq!(
                cat.dispatch(
                    json!({"operation":"published","base":"fixture","content_key":keys[index]})
                )
                .unwrap(),
                landed
            );
            assert_eq!(
                cat.dispatch(
                    json!({"operation":"custom_id","base":"fixture","content_key":keys[index]})
                )
                .unwrap(),
                if landed {
                    json!((index + 1).to_string())
                } else {
                    Value::Null
                }
            );
            let mut row = rows[index].clone();
            if landed {
                row["uploaded"] = json!(true);
                row["custom_emoji_id"] = json!((index + 1).to_string());
            }
            assert_eq!(cat.get(&keys[index]).unwrap(), Some(row));
        }
        assert!(
            cat.dispatch(json!({"operation":"seen","file_unique_id":"FOREIGN"}))
                .unwrap()
                .is_null()
        );
        assert_eq!(
            cat.dispatch(json!({"operation":"seen","file_unique_id":"UPLOADED-1"}))
                .unwrap(),
            if variant == "closed-target" {
                Value::Null
            } else {
                json!(keys[1])
            }
        );
        drop(cat);
        let second_set = if closed { SECOND } else { FIRST };
        if variant != "closed-target" {
            let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
            assert_eq!(
                cat.dispatch(json!({"operation":"published_sets"})).unwrap()[&keys[1]],
                second_set
            );
            drop(cat);
            let manifest =
                std::fs::read_to_string(data.join("manifests").join(format!("{second_set}.md")))
                    .unwrap();
            assert!(manifest.contains("public \\| artwork"));
        }
        for request in requests.iter().filter(|request| {
            ["createNewStickerSet", "addStickerToSet"].contains(&request.method.as_str())
        }) {
            let intent = &request
                .observation
                .as_ref()
                .expect("durable mutation snapshot")["in_flight"];
            let key = intent["key"].as_str().unwrap();
            assert!([keys[0].as_str(), keys[1].as_str()].contains(&key));
            assert_eq!(intent["set_name"], if closed { SECOND } else { FIRST });
            assert_eq!(intent["set_index"], if closed { 2 } else { 1 });
            assert_eq!(
                intent["operation"],
                if request.method == "createNewStickerSet" {
                    "create"
                } else {
                    "add"
                }
            );
        }
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "addStickerToSet")
                .count(),
            usize::from(!closed)
        );
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "createNewStickerSet")
                .count(),
            usize::from(!seeded || variant == "closed")
        );
        assert!(!_native::process_owner::cleanup_pending(commands.root));
    }
    occupied_drift(commands, images, bytes, keys);
}

fn occupied_drift(
    commands: &Commands<'_>,
    images: &[PathBuf; 3],
    bytes: &[Vec<u8>; 3],
    keys: &[String; 3],
) {
    let data = commands.root.join("publication-occupied-drift");
    let path = data.join("publish_fixture.json");
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    for index in 0..2 {
        assert!(
            cat.insert(
                &json!({"content_key":keys[index],"fmt":"static",
            "file_path":images[index],"emojis":["😀"],"keywords":["public | artwork"]}),
                UTC
            )
            .unwrap()
        );
    }
    let rows = cat.all(None).unwrap();
    drop(cat);
    let ghost = member("occupied-ghost", "occupied-ghost-unique", "9");
    let mut saved_bytes = None;
    for repeat in [false, true] {
        let mut replies = vec![me()];
        if !repeat {
            let mut attempt = mutation(&path, FIRST, &keys[0], (&bytes[0], false), true);
            attempt.payload = json!({"ok":false,"description":"Bad Request: sticker set name is already occupied"});
            replies.push(attempt);
        }
        replies.push(listing(FIRST, std::slice::from_ref(&ghost)));
        replies.push(listing(FIRST, std::slice::from_ref(&ghost)));
        download(&mut replies, "occupied-ghost", b"not an image");
        let expected = replies
            .iter()
            .map(|r| (r.method.clone(), r.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        println!("native collection occupied drift command: repeat={repeat}");
        let result = execute(commands, &data, &[], &server, None);
        assert_eq!(result.code, 4, "{} {}", result.stdout, result.stderr);
        assert!(!result.stdout.contains("DONE"));
        assert!(
            result
                .stderr
                .contains("live position 0 could not be examined"),
            "{}",
            result.stderr
        );
        assert!(!result.stderr.contains("will retry"));
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), expected.len());
        for (request, (method, fields)) in requests.iter().zip(expected) {
            assert_eq!(request.method, method);
            assert_eq!(request.form, fields);
            assert!(!["addStickerToSet", "sendMessage"].contains(&request.method.as_str()));
        }
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "createNewStickerSet")
                .count(),
            usize::from(!repeat)
        );
        let raw = std::fs::read(&path).unwrap();
        if let Some(before) = &saved_bytes {
            assert_eq!(&raw, before);
        }
        saved_bytes = Some(raw.clone());
        let state: Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(state["sets"].as_array().unwrap().len(), 1);
        assert_eq!(state["sets"][0]["name"], FIRST);
        assert_eq!(state["sets"][0]["index"], 1);
        assert_eq!(state["sets"][0]["keys"], json!([]));
        assert_eq!(state["sets"][0]["live"], 0);
        assert_eq!(state["sent"], json!([]));
        assert_eq!(state["in_flight"]["operation"], "create");
        assert_eq!(state["in_flight"]["key"], keys[0]);
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        assert_eq!(cat.all(None).unwrap(), rows);
        for key in &keys[..2] {
            assert_eq!(
                cat.dispatch(json!({"operation":"published","base":"fixture",
                "content_key":key}))
                    .unwrap(),
                false
            );
        }
        assert!(
            cat.dispatch(json!({"operation":"seen","file_unique_id":"occupied-ghost-unique"}))
                .unwrap()
                .is_null()
        );
        drop(cat);
        assert!(!data.join("manifests").exists());
        assert!(!_native::process_owner::cleanup_pending(commands.root));
    }
}
