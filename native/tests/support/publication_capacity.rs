//! Real command transitions after a definite refusal preserve the created container and item identity.
use super::{
    Commands, FIRST, SECOND, UTC, download, execute, listing, me, member, mutation, notification,
    set,
};
use crate::bot_api::{Reply, Server};
use _native::{atomic, catalog::Catalog};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) fn run(
    commands: &Commands<'_>,
    images: &[PathBuf; 3],
    bytes: &[Vec<u8>; 3],
    keys: &[String; 3],
    logo_source: &Path,
) {
    for permanent in [false, true] {
        logo_followup(commands, images, bytes, keys, logo_source, permanent);
    }
    for variant in ["roll", "target", "occupied-full"] {
        capacity(commands, images, bytes, keys, variant);
    }
}

fn refused(mut reply: Reply, reason: &str) -> Reply {
    reply.payload = json!({"ok":false,"description":format!("Bad Request: {reason}")});
    reply
}

fn logo_followup(
    commands: &Commands<'_>,
    images: &[PathBuf; 3],
    bytes: &[Vec<u8>; 3],
    keys: &[String; 3],
    logo_source: &Path,
    permanent: bool,
) {
    let data = commands
        .root
        .join(format!("publication-logo-refusal-{permanent}"));
    let path = data.join("publish_fixture.json");
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    for i in [0, 2] {
        assert!(
            cat.insert(
                &json!({"content_key":keys[i],"fmt":"static","file_path":images[i],
            "emojis":["😀"],"keywords":["public | artwork"]}),
                UTC
            )
            .unwrap()
        );
    }
    drop(cat);
    let logo = member("refusal-logo", "refusal-logo-unique", "1");
    let placed = member("refusal-placed", "refusal-placed-unique", "2");
    let retried = member("refusal-retried", "refusal-retried-unique", "3");
    let mut replies = vec![
        me(),
        mutation(&path, FIRST, &keys[0], (&bytes[1], true), true),
        listing(FIRST, std::slice::from_ref(&logo)),
    ];
    download(&mut replies, "refusal-logo", &bytes[1]);
    replies.push(listing(FIRST, std::slice::from_ref(&logo)));
    replies.push(refused(
        mutation(&path, FIRST, &keys[0], (&bytes[0], false), false),
        if permanent {
            "wrong file type"
        } else {
            "temporary fixture refusal"
        },
    ));
    replies.push(listing(FIRST, std::slice::from_ref(&logo)));
    replies.push(mutation(&path, FIRST, &keys[2], (&bytes[2], false), false));
    let mut live = vec![logo, placed];
    replies.push(listing(FIRST, &live));
    download(&mut replies, "refusal-placed", &bytes[2]);
    replies.push(listing(FIRST, &live));
    let first_len = replies.len();
    replies.push(me());
    replies.push(listing(FIRST, &live));
    if !permanent {
        replies.push(listing(FIRST, &live));
        replies.push(mutation(&path, FIRST, &keys[0], (&bytes[0], false), false));
        live.push(retried);
        replies.push(listing(FIRST, &live));
        download(&mut replies, "refusal-retried", &bytes[0]);
        replies.push(notification(FIRST, true, &path));
    }
    replies.push(listing(FIRST, &live));
    replies.push(notification(FIRST, false, &path));
    let expected = replies
        .iter()
        .map(|r| (r.method.clone(), r.fields.clone()))
        .collect::<Vec<_>>();
    let mut server = Server::new(replies);
    println!("native collection logo followup refusal: permanent={permanent}");
    let result = execute(
        commands,
        &data,
        &["--per-set", "3"],
        &server,
        Some(logo_source),
    );
    assert_eq!(
        result.code,
        if permanent { 0 } else { 3 },
        "{} {}",
        result.stdout,
        result.stderr
    );
    assert!(result.stdout.contains(if permanent {
        "\nDONE.\n"
    } else {
        "DONE: 1 uploaded, 1 failed."
    }));
    let first_state: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(first_state["sets"].as_array().unwrap().len(), 1);
    assert_eq!(first_state["sets"][0]["keys"], json!([keys[2]]));
    assert_eq!(first_state["sets"][0]["live"], 2);
    assert_eq!(first_state["sets"][0]["logo"], true);
    assert_eq!(first_state["sent"], json!([]));
    assert_eq!(first_state["sent_full"], json!([]));
    assert_eq!(
        first_state["skipped"],
        if permanent {
            json!([keys[0]])
        } else {
            json!([])
        }
    );
    assert!(first_state["in_flight"].is_null());
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    assert_eq!(
        cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":keys[2]}))
            .unwrap(),
        "2"
    );
    assert_eq!(
        cat.dispatch(json!({"operation":"published","base":"fixture","content_key":keys[0]}))
            .unwrap(),
        false
    );
    drop(cat);
    let repeated = execute(
        commands,
        &data,
        &["--per-set", "3"],
        &server,
        Some(logo_source),
    );
    assert_eq!(repeated.code, 0, "{} {}", repeated.stdout, repeated.stderr);
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), expected.len());
    for (request, (method, fields)) in requests.iter().zip(expected) {
        assert_eq!(request.method, method);
        assert_eq!(request.form, fields);
    }
    assert!(
        !requests[..first_len]
            .iter()
            .any(|r| r.method == "sendMessage")
    );
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "createNewStickerSet")
            .count(),
        1
    );
    let state: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(state["sets"].as_array().unwrap().len(), 1);
    assert_eq!(state["sets"][0]["index"], 1);
    assert_eq!(
        state["sets"][0]["keys"],
        if permanent {
            json!([keys[2]])
        } else {
            json!([keys[2], keys[0]])
        }
    );
    assert_eq!(state["sets"][0]["live"], if permanent { 2 } else { 3 });
    assert_eq!(state["sent"], json!([FIRST]));
    assert_eq!(
        state["sent_full"],
        if permanent { json!([]) } else { json!([FIRST]) }
    );
    assert!(state["in_flight"].is_null());
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    for (key, cid) in [
        (&keys[2], json!("2")),
        (&keys[0], if permanent { Value::Null } else { json!("3") }),
    ] {
        assert_eq!(
            cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":key}))
                .unwrap(),
            cid
        );
    }
    assert!(
        cat.get(&keys[1]).unwrap().is_none(),
        "brand logo is not a catalog item"
    );
    drop(cat);
    let adds = requests
        .iter()
        .filter(|r| r.method == "addStickerToSet")
        .collect::<Vec<_>>();
    let expected_before: &[u64] = if permanent { &[1, 1] } else { &[1, 1, 2] };
    assert_eq!(adds.len(), expected_before.len());
    for (request, before) in adds.into_iter().zip(expected_before) {
        let intent = &request.observation.as_ref().unwrap()["in_flight"];
        assert_eq!(intent["operation"], "add");
        assert_eq!(intent["set_name"], FIRST);
        assert_eq!(intent["set_index"], 1);
        assert_eq!(intent["expected_before"], *before);
    }
    assert!(!_native::process_owner::cleanup_pending(commands.root));
}

fn capacity(
    commands: &Commands<'_>,
    images: &[PathBuf; 3],
    bytes: &[Vec<u8>; 3],
    keys: &[String; 3],
    variant: &str,
) {
    let occupied = variant == "occupied-full";
    let target = variant == "target";
    let data = commands
        .root
        .join(format!("publication-capacity-{variant}"));
    let path = data.join("publish_fixture.json");
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
    cat.record_seen("capacity-seed-unique", &keys[0]).unwrap();
    cat.uploaded(
        &json!({"content_key":keys[0],"custom_emoji_id":"1","base":"fixture","set_name":FIRST}),
        UTC,
    )
    .unwrap();
    let initial = json!({"base":"fixture","sets":if occupied { json!([]) } else { json!([set(FIRST,1,&keys[0])]) },
        "sent":if occupied { json!([]) } else { json!([FIRST]) },"sent_full":[],"skipped":[],"in_flight":null});
    atomic::write_json(&path, &initial).unwrap();
    let rows = cat.all(None).unwrap();
    drop(cat);
    let seed = member("capacity-seed", "capacity-seed-unique", "1");
    let added = member("capacity-added", "capacity-added-unique", "2");
    let mut replies = vec![me()];
    if occupied {
        replies.push(refused(
            mutation(&path, FIRST, &keys[1], (&bytes[1], false), true),
            "sticker set name is already occupied",
        ));
        replies.push(listing(FIRST, std::slice::from_ref(&seed)));
        replies.push(listing(FIRST, std::slice::from_ref(&seed)));
        replies.push(listing(FIRST, std::slice::from_ref(&seed)));
        replies.push(mutation(&path, FIRST, &keys[1], (&bytes[1], false), false));
        replies.push(listing(FIRST, &[seed.clone(), added.clone()]));
        download(&mut replies, "capacity-added", &bytes[1]);
        replies.push(notification(FIRST, true, &path));
        replies.push(listing(FIRST, &[seed.clone(), added.clone()]));
        replies.push(notification(FIRST, false, &path));
    } else {
        replies.push(listing(FIRST, std::slice::from_ref(&seed)));
        replies.push(listing(FIRST, std::slice::from_ref(&seed)));
        replies.push(refused(
            mutation(&path, FIRST, &keys[1], (&bytes[1], false), false),
            "STICKERS_TOO_MUCH",
        ));
        if !target {
            replies.push(mutation(&path, SECOND, &keys[1], (&bytes[1], false), true));
            replies.push(listing(SECOND, std::slice::from_ref(&added)));
            download(&mut replies, "capacity-added", &bytes[1]);
            replies.push(listing(FIRST, std::slice::from_ref(&seed)));
            replies.push(listing(SECOND, std::slice::from_ref(&added)));
            replies.push(notification(SECOND, false, &path));
        }
    }
    let expected = replies
        .iter()
        .map(|r| (r.method.clone(), r.fields.clone()))
        .collect::<Vec<_>>();
    let mut server = Server::new(replies);
    println!("native collection capacity command: {variant}");
    let extra: &[&str] = if target {
        &["--into-pack", "1"]
    } else if occupied {
        &["--per-set", "2"]
    } else {
        &[]
    };
    let result = execute(commands, &data, extra, &server, None);
    assert_eq!(
        result.code,
        if target { 4 } else { 0 },
        "{} {}",
        result.stdout,
        result.stderr
    );
    assert_eq!(result.stdout.contains("DONE"), !target);
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), expected.len());
    for (request, (method, fields)) in requests.iter().zip(expected) {
        assert_eq!(request.method, method);
        assert_eq!(request.form, fields);
    }
    let state: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(state["in_flight"].is_null());
    if target {
        assert_eq!(state, initial);
        assert!(result.stderr.contains("requested pack is full"));
    } else if occupied {
        assert_eq!(state["sets"].as_array().unwrap().len(), 1);
        assert_eq!(state["sets"][0]["keys"], json!(keys[..2]));
        assert_eq!(state["sets"][0]["live"], 2);
        assert_eq!(state["sent"], json!([FIRST]));
        assert_eq!(state["sent_full"], json!([FIRST]));
    } else {
        assert_eq!(state["sets"].as_array().unwrap().len(), 2);
        assert_eq!(state["sets"][0], initial["sets"][0]);
        assert_eq!(state["sets"][1]["keys"], json!([keys[1]]));
        assert_eq!(state["sets"][1]["live"], 1);
        assert_eq!(state["sent"], json!([FIRST, SECOND]));
        assert_eq!(state["sent_full"], json!([]));
    }
    let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
    assert_eq!(cat.get(&keys[0]).unwrap(), Some(rows[0].clone()));
    assert_eq!(
        cat.dispatch(json!({"operation":"custom_id","base":"fixture","content_key":keys[1]}))
            .unwrap(),
        if target { Value::Null } else { json!("2") }
    );
    if !target {
        assert_eq!(
            cat.dispatch(json!({"operation":"published_sets"})).unwrap()[&keys[1]],
            if occupied { FIRST } else { SECOND }
        );
    }
    drop(cat);
    assert!(!_native::process_owner::cleanup_pending(commands.root));
}
