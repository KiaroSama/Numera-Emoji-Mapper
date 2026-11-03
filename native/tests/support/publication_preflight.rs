//! Original queue validation through the copied command; no publication is permitted.
use super::{Commands, UTC, execute, me};
use crate::bot_api::{Reply, Server};
use _native::{atomic, catalog::Catalog};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(super) fn run(
    commands: &Commands<'_>,
    images: &[PathBuf; 3],
    bytes: &[Vec<u8>; 3],
    keys: &[String; 3],
) {
    for variant in [
        "clean",
        "refused",
        "unknown",
        "partial",
        "missing",
        "precedence",
        "categories",
        "empty",
        "filtered",
    ] {
        let data = commands
            .root
            .join(format!("publication-preflight-{variant}"));
        let state_path = data.join("publish_fixture.json");
        let plan_path = data.join("publish_plan_fixture.json");
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        let total = match variant {
            "empty" => 0,
            "filtered" | "categories" => 4,
            _ => 3,
        };
        let mut item_keys = keys.to_vec();
        item_keys.push("s:preflight-skipped".into());
        for index in 0..total {
            let file = if (variant == "missing" && index == 0)
                || (variant == "categories" && index == 3)
            {
                data.join("missing.png")
            } else {
                images[index % 3].clone()
            };
            assert!(
                cat.insert(
                    &json!({"content_key":item_keys[index],"fmt":"static",
                "file_path":file,"emojis":["😀"],"keywords":["public | artwork"]}),
                    UTC
                )
                .unwrap()
            );
        }
        if variant == "filtered" {
            cat.inclusion(&[keys[1].clone()]).unwrap();
            cat.uploaded(
                &json!({"content_key":keys[2],"custom_emoji_id":"3",
                "base":"fixture","set_name":"fixture0_by_YourEmojiBot"}),
                UTC,
            )
            .unwrap();
            atomic::write_json(
                &state_path,
                &json!({"base":"fixture","sets":[],
                "sent":[],"sent_full":[],"skipped":[item_keys[3]],"in_flight":null}),
            )
            .unwrap();
        }
        let rows = cat.all(None).unwrap();
        let publications = cat.dispatch(json!({"operation":"published_sets"})).unwrap();
        drop(cat);
        let state_bytes = std::fs::read(&state_path).ok();
        assert!(!plan_path.exists());
        let checked = match variant {
            "empty" => vec![],
            "filtered" => vec![0],
            "missing" => vec![1, 2],
            _ => vec![0, 1, 2],
        };
        let mut replies = vec![me()];
        for &index in &checked {
            let payload = match variant {
                "refused" if index == 2 => json!({"ok":false,
                    "description":"Bad Request: wrong file type"}),
                "precedence" | "categories" if index == 0 => json!({"ok":false,
                    "description":"Bad Request: STICKER_VIDEO_LONG"}),
                // Missing-result transport classification is not literal socket-loss equivalence.
                "unknown" => json!({"ok":true}),
                "partial" | "precedence" | "categories" if index == 1 => json!({"ok":true}),
                _ => json!({"ok":true,"result":{"file_id":format!("validated-{index}")}}),
            };
            let mut reply = Reply::new("uploadStickerFile", &[], payload);
            reply.fields = BTreeMap::from([
                ("user_id".into(), "111111111".into()),
                ("sticker_format".into(), "static".into()),
            ]);
            reply.files.insert("sticker".into(), bytes[index].clone());
            replies.push(reply);
        }
        let expected = replies
            .iter()
            .map(|reply| (reply.method.clone(), reply.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        println!("native collection preflight command: {variant}");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            execute(commands, &data, &["--preflight"], &server, None)
        }));
        let requests = server.finish();
        let result = match result {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        let requests = requests.unwrap();
        assert_eq!(
            result.code,
            match variant {
                "refused" | "missing" | "precedence" | "categories" => 4,
                "unknown" | "partial" => 3,
                _ => 0,
            },
            "{variant}: {} {}",
            result.stdout,
            result.stderr
        );
        let counts = match variant {
            "clean" => "3 file(s) queued -- 3 accepted, 0 refused, 0 missing, 0 not checked.",
            "refused" => "3 file(s) queued -- 2 accepted, 1 refused, 0 missing, 0 not checked.",
            "unknown" => "3 file(s) queued -- 0 accepted, 0 refused, 0 missing, 3 not checked.",
            "partial" => "3 file(s) queued -- 2 accepted, 0 refused, 0 missing, 1 not checked.",
            "missing" => "3 file(s) queued -- 2 accepted, 0 refused, 1 missing, 0 not checked.",
            "precedence" => "3 file(s) queued -- 1 accepted, 1 refused, 0 missing, 1 not checked.",
            "categories" => "4 file(s) queued -- 1 accepted, 1 refused, 1 missing, 1 not checked.",
            "empty" => "0 file(s) queued -- 0 accepted, 0 refused, 0 missing, 0 not checked.",
            "filtered" => "1 file(s) queued -- 1 accepted, 0 refused, 0 missing, 0 not checked.",
            _ => unreachable!(),
        };
        assert!(
            result.stdout.contains(&format!("PREFLIGHT: {counts}")),
            "{}",
            result.stdout
        );
        assert_eq!(
            result.stdout.contains("acceptable to Telegram"),
            matches!(variant, "clean" | "filtered")
        );
        match variant {
            "clean" => assert!(
                result
                    .stdout
                    .contains("All 3 queued file(s) are acceptable")
            ),
            "filtered" => assert!(
                result
                    .stdout
                    .contains("All 1 queued file(s) are acceptable")
            ),
            "refused" => {
                assert!(result.stdout.contains(&format!("REFUSED {}", keys[2])));
                assert!(result.stdout.contains("wrong file type"));
            }
            "unknown" => {
                assert!(!result.stdout.contains("REFUSED"));
                assert!(result.stdout.contains("proves nothing about them"));
                for key in keys {
                    assert!(result.stdout.contains(&format!("UNCHECKED {key}")));
                }
            }
            "partial" => {
                assert!(result.stdout.contains(&format!("UNCHECKED {}", keys[1])));
                assert!(result.stdout.contains("proves nothing about them"));
            }
            "precedence" | "categories" => {
                assert!(result.stdout.contains(&format!("REFUSED {}", keys[0])));
                assert!(result.stdout.contains(&format!("UNCHECKED {}", keys[1])));
                assert!(result.stdout.contains("STICKER_VIDEO_LONG"));
                assert!(result.stdout.contains("Fix or deselect these"));
                if variant == "categories" {
                    assert!(result.stdout.contains(&format!("MISSING {}", item_keys[3])));
                }
            }
            "missing" => assert!(result.stdout.contains(&format!("MISSING {}", keys[0]))),
            "empty" => assert!(
                result
                    .stdout
                    .contains("Nothing was queued, so nothing was validated.")
            ),
            _ => unreachable!(),
        }
        assert!(!result.stdout.contains("DONE"));
        assert_eq!(requests.len(), checked.len() + 1);
        assert_eq!(requests.len(), expected.len());
        for (request, (method, fields)) in requests.iter().zip(expected) {
            assert_eq!(request.method, method);
            assert_eq!(request.form, fields);
            assert!(["getMe", "uploadStickerFile"].contains(&request.method.as_str()));
        }
        assert_eq!(std::fs::read(&state_path).ok(), state_bytes);
        assert!(!plan_path.exists());
        assert!(!data.join("manifests").exists());
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        assert_eq!(cat.all(None).unwrap(), rows);
        assert_eq!(
            cat.dispatch(json!({"operation":"published_sets"})).unwrap(),
            publications
        );
        for index in 0..total {
            assert_eq!(
                cat.dispatch(json!({"operation":"custom_id","base":"fixture",
                "content_key":item_keys[index]}))
                    .unwrap(),
                if variant == "filtered" && index == 2 {
                    json!("3")
                } else {
                    Value::Null
                }
            );
        }
        drop(cat);
        for (image, original) in images.iter().zip(bytes) {
            assert_eq!(std::fs::read(image).unwrap(), *original);
        }
        let lease =
            _native::locks::Lease::acquire(&data.join("publish_fixture.lock"), "after preflight")
                .unwrap();
        drop(lease);
        assert!(!_native::process_owner::cleanup_pending(commands.root));
    }
}
