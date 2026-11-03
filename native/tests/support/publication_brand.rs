//! Original logo-first publication maps both artwork IDs after the noncatalog first slot.
use super::{Commands, FIRST, UTC, download, execute, listing, me, member, mutation, notification};
use crate::bot_api::Server;
use _native::catalog::Catalog;
use serde_json::{Value, json};
use std::path::PathBuf;

pub(super) fn run(
    commands: &Commands<'_>,
    images: &[PathBuf; 3],
    bytes: &[Vec<u8>; 3],
    keys: &[String; 3],
    logo_source: &std::path::Path,
) {
    family(commands, images, bytes, keys, logo_source, false);
    family(commands, images, bytes, keys, logo_source, true);
}

fn family(
    commands: &Commands<'_>,
    images: &[PathBuf; 3],
    bytes: &[Vec<u8>; 3],
    keys: &[String; 3],
    logo_source: &std::path::Path,
    animated: bool,
) {
    let format = if animated { "animated" } else { "static" };
    let name = if animated {
        "fixturea1_by_YourEmojiBot"
    } else {
        FIRST
    };
    let data = commands.root.join(if animated {
        "publication-logo-animated-items"
    } else {
        "publication-logo-two-items"
    });
    let path = data.join("publish_fixture.json");
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    let (mut images, mut bytes, mut keys) = ((*images).clone(), (*bytes).clone(), (*keys).clone());
    if animated {
        for (slot, count, key) in [
            (0, 50, "a:b58b08a69844a4a26c1f1019a18483cd"),
            (2, 51, "a:c68eca5291054dcb8de1087839541b8d"),
        ] {
            // Literal original inputs; publication admission does not validate animations.
            bytes[slot] = [vec![0x1f, 0x8b], vec![b'x'; count]].concat();
            images[slot] = data.join(format!("item-{slot}.tgs"));
            std::fs::write(&images[slot], &bytes[slot]).unwrap();
            keys[slot] = key.into();
        }
    }
    for i in [0, 2] {
        assert!(
            cat.insert(
                &json!({"content_key":keys[i],"fmt":format,"file_path":images[i],
            "emojis":["😀"],"keywords":["public | artwork"]}),
                UTC
            )
            .unwrap()
        );
    }
    let rows = cat.all(None).unwrap();
    drop(cat);
    let logo = member("brand-logo", "brand-logo-unique", "1");
    let mut first = member("brand-first", "brand-first-unique", "2");
    let mut second = member("brand-second", "brand-second-unique", "3");
    for sticker in [&mut first, &mut second] {
        sticker["is_animated"] = json!(animated);
    }
    let add = |slot| {
        let mut reply = mutation(&path, name, &keys[slot], (&bytes[slot], false), false);
        reply.fields.insert(
            "sticker".into(),
            json!({
                "sticker":"attach://file0","format":format,
                "emoji_list":["😀"],"keywords":["public | artwork"]
            })
            .to_string(),
        );
        reply
    };
    let mut replies = vec![
        me(),
        mutation(&path, name, &keys[0], (&bytes[1], true), true),
        listing(name, std::slice::from_ref(&logo)),
    ];
    download(&mut replies, "brand-logo", &bytes[1]);
    replies.push(listing(name, std::slice::from_ref(&logo)));
    replies.push(add(0));
    replies.push(listing(name, &[logo.clone(), first.clone()]));
    download(&mut replies, "brand-first", &bytes[0]);
    replies.push(listing(name, &[logo.clone(), first.clone()]));
    replies.push(add(2));
    replies.push(listing(
        name,
        &[logo.clone(), first.clone(), second.clone()],
    ));
    download(&mut replies, "brand-second", &bytes[2]);
    replies.push(notification(name, true, &path));
    replies.push(listing(name, &[logo, first, second]));
    replies.push(notification(name, false, &path));
    let expected = replies
        .iter()
        .map(|r| (r.method.clone(), r.fields.clone()))
        .collect::<Vec<_>>();
    let mut server = Server::new(replies);
    println!("native collection static logo plus two {format} items command");
    let options: &[&str] = if animated {
        &["--per-set", "3", "--formats", "animated"]
    } else {
        &["--per-set", "3"]
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute(commands, &data, options, &server, Some(logo_source))
    }));
    let requests = server.finish();
    let result = match result {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    };
    let requests = requests.unwrap();
    assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
    assert!(result.stdout.contains("\nDONE.\n"));
    assert_eq!(requests.len(), expected.len());
    for (request, (method, fields)) in requests.iter().zip(expected) {
        assert_eq!(request.method, method);
        assert_eq!(request.form, fields);
    }
    let state: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(state["sets"].as_array().unwrap().len(), 1);
    assert_eq!(
        state["sets"][0]["fmt"],
        if animated { "animated" } else { "mixed" }
    );
    assert_eq!(state["sets"][0]["keys"], json!([keys[0], keys[2]]));
    assert_eq!(state["sets"][0]["logo"], true);
    assert_eq!(state["sets"][0]["live"], 3);
    assert_eq!(state["sent"], json!([name]));
    assert_eq!(state["sent_full"], json!([name]));
    assert_eq!(state["skipped"], json!([]));
    assert!(state["in_flight"].is_null());
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    for (key, cid) in [(&keys[0], "2"), (&keys[2], "3")] {
        assert_eq!(
            cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":key}))
                .unwrap(),
            cid
        );
        assert_eq!(
            cat.dispatch(json!({"operation":"published_sets"})).unwrap()[key],
            name
        );
    }
    for (slot, row) in [0, 2].into_iter().zip(rows) {
        let mut expected = row;
        expected["uploaded"] = json!(true);
        expected["custom_emoji_id"] = json!(if slot == 0 { "2" } else { "3" });
        assert_eq!(cat.get(&keys[slot]).unwrap(), Some(expected));
        assert_eq!(std::fs::read(&images[slot]).unwrap(), bytes[slot]);
    }
    assert!(
        cat.get(&keys[1]).unwrap().is_none(),
        "logo is not catalog artwork"
    );
    drop(cat);
    let manifest =
        std::fs::read_to_string(data.join("manifests").join(format!("{name}.md"))).unwrap();
    assert!(manifest.contains("|  3 emoji\n"));
    assert!(manifest.contains("| 1 | brand logo | — |"));
    assert!(manifest.contains("| 2 | public \\| artwork | 2 |"));
    assert!(manifest.contains("| 3 | public \\| artwork | 3 |"));
    let mutations = requests
        .iter()
        .filter(|r| ["createNewStickerSet", "addStickerToSet"].contains(&r.method.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(mutations.len(), 3);
    for (i, request) in mutations.iter().enumerate() {
        let intent = &request.observation.as_ref().unwrap()["in_flight"];
        assert_eq!(
            intent["expected_before"],
            if i == 0 { Value::Null } else { json!(i) }
        );
        assert_eq!(
            intent["key"],
            if i < 2 {
                json!(keys[0])
            } else {
                json!(keys[2])
            }
        );
        assert_eq!(intent["format"], format);
        let input: Value =
            serde_json::from_str(&request.form[if i == 0 { "stickers" } else { "sticker" }])
                .unwrap();
        let input = if i == 0 { &input[0] } else { &input };
        assert_eq!(input["format"], if i == 0 { "static" } else { format });
        assert_eq!(
            input["emoji_list"],
            json!([if i == 0 { "✅" } else { "😀" }])
        );
    }
    assert_eq!(
        std::fs::read_dir(data.join("tmp/native-codec"))
            .unwrap()
            .count(),
        0
    );
    let lease = _native::locks::Lease::acquire(
        &data.join("publish_fixture.lock"),
        "after branded publication",
    )
    .unwrap();
    drop(lease);
    assert!(!_native::process_owner::cleanup_pending(commands.root));
}
