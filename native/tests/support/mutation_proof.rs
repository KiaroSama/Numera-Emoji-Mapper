//! Original retry-proof decisions through the real public mutation and finite HTTP script.
use super::bot_api::{Reply, Server, TOKEN};
use super::tooling_fixture::Root;
use _native::telegram::{ApiError, Telegram};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

pub(super) const SET: &str = "s1_by_YourEmojiBot";
fn sticker(id: &str) -> Value {
    json!({"file_unique_id":id,"file_id":id})
}
pub(super) fn listing(members: &[Value]) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", SET)],
        json!({"ok":true,"result":{"stickers":members}}),
    )
}
fn absent() -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", SET)],
        json!({"ok":false,"description":"STICKERSET_INVALID"}),
    )
}
pub(super) fn mutation(creating: bool, repaint: bool, bytes: &[u8], answer: Value) -> Reply {
    let input =
        json!({"sticker":"attach://file0","format":"static","emoji_list":["😀"],"keywords":["kw"]});
    let mut reply = Reply::new(
        if creating {
            "createNewStickerSet"
        } else {
            "addStickerToSet"
        },
        &[],
        answer,
    );
    reply.fields = BTreeMap::from([
        ("user_id".into(), "111111111".into()),
        ("name".into(), SET.into()),
    ]);
    if creating {
        reply.fields.extend([
            ("title".into(), "Pack".into()),
            ("sticker_type".into(), "custom_emoji".into()),
            ("stickers".into(), json!([input]).to_string()),
        ]);
        if repaint {
            reply
                .fields
                .insert("needs_repainting".into(), "true".into());
        }
    } else {
        reply.fields.insert("sticker".into(), input.to_string());
    }
    reply.files.insert("file0".into(), bytes.into());
    reply
}
fn run_case(root: &Root, source: &Path, bytes: &[u8], name: &str) {
    println!("mutation proof started: {name}");
    let creating = name.starts_with("create-");
    let no_snapshot = name == "add-no-snapshot";
    let size_read = name.starts_with("size-");
    let initial = if size_read {
        vec![sticker("old")]
    } else if name == "add-duplicate" {
        vec![sticker("same"), sticker("same")]
    } else {
        vec![sticker("a"), sticker("b")]
    };
    let after = match name {
        "add-two" => vec![
            sticker("a"),
            sticker("b"),
            sticker("THEIRS"),
            sticker("MINE"),
        ],
        "add-replaced" => vec![sticker("a"), sticker("THEIRS"), sticker("MINE")],
        "add-missing-ids" => vec![json!({"i":0}), json!({"i":1})],
        "add-duplicate" => vec![sticker("same"), sticker("same")],
        "add-no-change" => initial.clone(),
        "add-foreign" => vec![sticker("a"), sticker("b"), sticker("THEIRS")],
        "create-populated" => vec![sticker("a"), sticker("b")],
        "create-ours" => vec![sticker("ours")],
        "create-foreign" => vec![sticker("foreign")],
        "create-unreadable" => vec![sticker("?")],
        "size-foreign" => vec![sticker("old"), sticker("THEIRS")],
        "size-unreadable" => vec![sticker("old"), sticker("UNREADABLE")],
        "size-ours" => vec![sticker("old"), sticker("THEIRS"), sticker("OURS")],
        _ => vec![sticker("a"), sticker("b"), sticker("MINE")],
    };
    let negative = matches!(
        name,
        "add-no-change" | "add-foreign" | "create-foreign" | "create-missing"
    );
    let positive = matches!(name, "add-ours" | "create-ours");
    let repaint = name == "create-repaint";
    let clean = matches!(name, "add-clean" | "create-repaint" | "create-default");
    let mut replies = Vec::new();
    let mut progress = None;
    if !creating && !no_snapshot {
        replies.push(listing(&initial));
    }
    replies.push(mutation(
        creating,
        repaint,
        bytes,
        if clean {
            json!({"ok":true,"result":true})
        } else {
            json!([])
        },
    ));
    if !clean {
        let probe = if size_read {
            listing(&[sticker("old"), sticker("OURS")])
        } else if name == "create-missing" {
            absent()
        } else {
            listing(&after)
        };
        if matches!(name, "add-no-change" | "create-missing") {
            // A real proof read separates the recorded 2s settle and 3s retry waits.
            let (probe, reached, release) = probe.hold();
            replies.push(probe);
            progress = Some((reached, release));
        } else {
            replies.push(probe);
        }
        if negative {
            replies.push(mutation(
                creating,
                repaint,
                bytes,
                json!({"ok":false,"description":"fixture retry rejected"}),
            ));
        }
        if size_read {
            replies.push(if name == "size-missing" {
                absent()
            } else {
                listing(&after)
            });
        } else if !creating && positive {
            replies.push(listing(&after));
        }
    }
    let expected = replies
        .iter()
        .map(|r| (r.method.clone(), r.fields.clone()))
        .collect::<Vec<_>>();
    let mut server = Server::new(replies);
    let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
    let request = json!({"name":SET,"source":source,"format":"static","create":creating,
        "title":"Pack","user_id":111111111,"emojis":["😀"],"keywords":["kw"],"needs_repainting":repaint,
        "expected_before":if no_snapshot {Value::Null} else if size_read {json!(1)} else {json!(2)}});
    let mut compared = Vec::new();
    let mut waits = Vec::new();
    let result = std::thread::scope(|scope| {
        let observer = progress.map(|(mut reached, release)| {
            scope.spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime
                    .block_on(async {
                        tokio::time::timeout(std::time::Duration::from_secs(4), &mut reached).await
                    })
                    .unwrap()
                    .unwrap();
                println!("mutation proof live read reached: {name}");
                release.send(()).unwrap();
                runtime.shutdown_timeout(std::time::Duration::from_secs(1));
            })
        });
        let result = _native::telegram_mutation::mutate_with_wait(
            &tg,
            &request,
            |_, member, path, fmt| {
                assert_eq!(Path::new(path), source);
                assert_eq!(fmt, "static");
                println!("mutation proof content compared: {name}");
                let id = member["file_unique_id"].as_str().unwrap();
                compared.push(id.to_owned());
                Ok(if size_read {
                    match id {
                        "OURS" => Some(true),
                        "THEIRS" => Some(false),
                        _ => None,
                    }
                } else if name.contains("unreadable") {
                    None
                } else {
                    Some(!name.contains("foreign"))
                })
            },
            |duration| waits.push(duration),
        );
        if let Some(observer) = observer {
            observer.join().unwrap();
        }
        result
    });
    if negative {
        assert!(
            matches!(result, Err(ApiError::Rejected(_))),
            "{name}: {result:?}"
        );
    } else if clean || creating && positive {
        assert_eq!(result.unwrap(), Value::Null, "{name}");
    } else if !creating && positive || name == "size-ours" {
        assert_eq!(result.unwrap(), json!(after.len()), "{name}");
    } else {
        assert!(
            matches!(result, Err(ApiError::Ambiguous(_))),
            "{name}: {result:?}"
        );
    }
    let expected_waits = if clean {
        vec![]
    } else if negative {
        vec![2, 3]
    } else {
        vec![2]
    };
    assert_eq!(
        waits,
        expected_waits
            .into_iter()
            .map(std::time::Duration::from_secs)
            .collect::<Vec<_>>(),
        "{name}: retry and proof settle delays"
    );
    let expected_comparisons: &[&str] = match name {
        "add-ours" => &["MINE", "MINE"],
        "add-foreign" => &["THEIRS"],
        "add-unreadable" => &["MINE"],
        "create-ours" => &["ours"],
        "create-foreign" => &["foreign"],
        "create-unreadable" => &["?"],
        "size-foreign" => &["OURS", "THEIRS"],
        "size-unreadable" => &["OURS", "UNREADABLE"],
        "size-ours" => &["OURS", "THEIRS", "OURS"],
        "size-missing" => &["OURS"],
        _ => &[],
    };
    assert_eq!(compared, expected_comparisons, "{name}");
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), expected.len());
    for (actual, (method, fields)) in requests.iter().zip(expected) {
        assert_eq!(actual.method, method);
        assert_eq!(actual.form, fields);
    }
    assert_eq!(std::fs::read(source).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    println!("mutation proof completed: {name}");
}

pub fn run(group: &str) {
    let root = Root::new("transport mutation proof café");
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let row = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "opaque")
        .unwrap();
    let input = project.join(row["input"].as_str().unwrap());
    assert_eq!(
        _native::tooling_manifest::digest(&input).unwrap(),
        row["input_sha256"]
    );
    let bytes = std::fs::read(input).unwrap();
    let source = root.0.join("ours.png");
    std::fs::write(&source, &bytes).unwrap();
    let cases: &[&str] = match group {
        "add-unknown-a" => &[
            "add-two",
            "add-replaced",
            "add-no-snapshot",
            "add-missing-ids",
        ],
        "add-unknown-b" => &["add-duplicate", "add-unreadable", "add-ours", "add-clean"],
        "add-negative" => &["add-no-change", "add-foreign"],
        "create-positive" => &["create-ours", "create-populated", "create-unreadable"],
        "create-negative" => &["create-foreign", "create-missing"],
        "repaint" => &["create-repaint", "create-default"],
        "size" => &[
            "size-foreign",
            "size-unreadable",
            "size-ours",
            "size-missing",
        ],
        _ => panic!("unknown mutation fixture group"),
    };
    for name in cases {
        run_case(&root, &source, &bytes, name);
    }
    root.clean().unwrap();
}
