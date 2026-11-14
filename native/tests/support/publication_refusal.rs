//! Recorded collection refusals through the same copied executable and finite wire owner.
use super::{Commands, FIRST, SECOND, UTC, execute, listing, me, member, set};
use crate::bot_api::{Reply, Server};
use _native::{atomic, catalog::Catalog};
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) fn run(commands: &Commands<'_>, images: &[PathBuf; 3], keys: &[String; 3]) {
    for variant in [
        "empty",
        "shrunken",
        "missing",
        "unknown",
        "older-missing",
        "older-unknown",
        "target-missing",
        "target-full",
        "intent-missing",
        "intent-unknown",
    ] {
        let data = commands.root.join(format!("publication-refusal-{variant}"));
        let path = data.join("publish_fixture.json");
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        for index in 0..2 {
            assert!(
                cat.insert(
                    &json!({"content_key":keys[index],"fmt":"static",
                "file_path":images[index],"emojis":["😀"],"keywords":["synthetic"]}),
                    UTC
                )
                .unwrap()
            );
            let name = if variant.starts_with("older-") && index == 1 {
                SECOND
            } else {
                FIRST
            };
            cat.record_seen(
                if index == 0 {
                    "old-unique"
                } else {
                    "later-unique"
                },
                &keys[index],
            )
            .unwrap();
            cat.uploaded(
                &json!({"content_key":keys[index],"custom_emoji_id":format!("{}", index + 1),
                "base":"fixture","set_name":name}),
                UTC,
            )
            .unwrap();
        }
        let older = variant.starts_with("older-");
        let mut first = set(FIRST, 1, &keys[0]);
        if !older {
            first["keys"] = json!(keys[..2]);
            first["live"] = json!(2);
        }
        let mut sets = vec![first];
        if older {
            sets.push(set(SECOND, 2, &keys[1]));
        }
        let intent = if variant.starts_with("intent-") {
            json!({"key":keys[1],"operation":"add","set_name":FIRST,"set_index":1,
                "expected_before":2,"format":"static","started_utc":UTC})
        } else {
            Value::Null
        };
        let initial = json!({"base":"fixture","sets":sets,"sent":[],"sent_full":[],
            "skipped":[],"in_flight":intent});
        atomic::write_json(&path, &initial).unwrap();
        let state_bytes = std::fs::read(&path).unwrap();
        let rows = cat.all(None).unwrap();
        drop(cat);
        let one = member("old-file", "old-unique", "1");
        let two = member("later-file", "later-unique", "2");
        let mut replies = vec![me()];
        if older {
            replies.push(listing(SECOND, std::slice::from_ref(&two)));
        }
        match variant {
            "target-missing" => {}
            "target-full" => replies.push(listing(FIRST, &[one.clone(), two.clone()])),
            "empty" => replies.push(listing(FIRST, &[])),
            "shrunken" => replies.push(listing(FIRST, std::slice::from_ref(&one))),
            _ => replies.push(Reply::new(
                "getStickerSet",
                &[("name", FIRST)],
                if variant.ends_with("unknown") {
                    json!({"ok":true})
                } else {
                    json!({"ok":false,"description":"Bad Request: STICKERSET_INVALID"})
                },
            )),
        }
        let expected = replies
            .iter()
            .map(|reply| (reply.method.clone(), reply.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        let extra: &[&str] = match variant {
            "target-missing" => &["--into-pack", "9"],
            "target-full" => &["--per-set", "2", "--into-pack", "1"],
            _ => &[],
        };
        println!("native collection refusal command: {variant}");
        let result = execute(commands, &data, extra, &server, None);
        let code = if variant.ends_with("unknown") { 3 } else { 4 };
        assert_eq!(
            result.code, code,
            "{variant}: {} {}",
            result.stdout, result.stderr
        );
        assert!(
            !result.stdout.contains("DONE"),
            "{variant}: {}",
            result.stdout
        );
        let diagnostic = match variant {
            "empty" | "shrunken" => "smaller than recorded",
            "missing" | "older-missing" => "no longer exists",
            "unknown" | "older-unknown" => "live state unknown",
            "target-missing" => "not recorded",
            "target-full" => "full",
            "intent-missing" => "set is missing",
            "intent-unknown" => "cannot determine live state",
            _ => unreachable!(),
        };
        assert!(
            result.stderr.contains(diagnostic),
            "{variant}: {}",
            result.stderr
        );
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), expected.len(), "{variant}");
        for (request, (method, form)) in requests.iter().zip(expected) {
            assert_eq!(request.method, method, "{variant}");
            assert_eq!(request.form, form, "{variant}");
            assert!(request.files.is_empty());
        }
        assert!(!requests.iter().any(|request| {
            [
                "createNewStickerSet",
                "addStickerToSet",
                "sendMessage",
                "POST /publish",
            ]
            .contains(&request.method.as_str())
        }));
        assert_eq!(std::fs::read(&path).unwrap(), state_bytes, "{variant}");
        let saved: Value = serde_json::from_slice(&state_bytes).unwrap();
        assert_eq!(saved, initial);
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        assert_eq!(cat.all(None).unwrap(), rows, "{variant}");
        for (index, key) in keys[..2].iter().enumerate() {
            assert_eq!(
                cat.dispatch(json!({"operation":"custom_id","base":"fixture",
                "content_key":key}))
                    .unwrap(),
                format!("{}", index + 1)
            );
        }
        drop(cat);
        assert!(!data.join("manifests").exists());
        assert!(!_native::process_owner::cleanup_pending(commands.root));
    }
}
