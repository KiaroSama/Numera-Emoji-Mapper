//! Original two-run mutation cap and logo-first creation through the copied production command.
use super::bot_api::{Reply, Server, assert_rgba, rgba_signature};
use super::plan_family::{Family, Fixture, name, read};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

fn listing(pack: usize, members: &[Value]) -> Reply {
    Reply::new(
        "getStickerSet",
        &[("name", &name(pack))],
        json!({"ok":true,"result":{"name":name(pack),"stickers":members}}),
    )
}
fn get_me() -> Reply {
    Reply::new(
        "getMe",
        &[],
        json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
    )
}
fn sticker(index: usize) -> Value {
    json!({"custom_emoji_id":format!("444000{index:03}"),"file_id":format!("new-{index}"),
        "file_unique_id":format!("new-unique-{index}")})
}
fn input(index: usize) -> Value {
    json!({"sticker":"attach://file0","format":"static","emoji_list":["😀"],
        "keywords":[format!("item{}",index+1)]})
}
fn download(replies: &mut Vec<Reply>, member: &Value, bytes: &[u8]) {
    let file = member["file_id"].as_str().unwrap();
    let path = format!("{file}.png");
    replies.push(Reply::new(
        "getFile",
        &[("file_id", file)],
        json!({"ok":true,"result":{"file_path":path}}),
    ));
    replies.push(Reply {
        method: format!("GET {path}"),
        fields: BTreeMap::new(),
        payload: Value::Null,
        status: axum::http::StatusCode::OK,
        headers: axum::http::HeaderMap::new(),
        observation: None,
        files: BTreeMap::new(),
        upload_sha256: BTreeMap::new(),
        upload_rgba: BTreeMap::new(),
        download: Some(bytes.to_vec()),
        download_upload: None,
        held: None,
    });
}
fn add(
    replies: &mut Vec<Reply>,
    fixture: &Fixture,
    family: &Family,
    pack: usize,
    index: usize,
    live: &mut Vec<Value>,
) {
    replies.push(listing(pack, live));
    let form = input(index).to_string();
    let mut addition = Reply::new(
        "addStickerToSet",
        &[
            ("user_id", "111111111"),
            ("name", &name(pack)),
            ("sticker", &form),
        ],
        json!({"ok":true,"result":true}),
    );
    addition
        .files
        .insert("file0".into(), fixture.bytes[index].clone());
    addition.observation = Some((
        family.data.join("publish_cryptoemoji.json"),
        "/in_flight/key".into(),
        json!(family.keys[index]),
    ));
    replies.push(addition);
    let member = sticker(index);
    live.push(member.clone());
    replies.push(listing(pack, live));
    download(replies, &member, &fixture.bytes[index]);
}
fn finish_command(
    fixture: &Fixture,
    family: &Family,
    replies: Vec<Reply>,
    logo: bool,
    code: i32,
) -> Vec<String> {
    let started = Instant::now();
    let expected = replies
        .iter()
        .map(|reply| (reply.method.clone(), reply.fields.clone()))
        .collect::<Vec<_>>();
    let mut server = Server::new(replies);
    let result = fixture.command(family, &server.base, logo);
    assert_eq!(
        result.code, code,
        "stdout={} stderr={}",
        result.stdout, result.stderr
    );
    let observed = server.finish().unwrap();
    assert_eq!(observed.len(), expected.len());
    for (request, (method, fields)) in observed.iter().zip(&expected) {
        assert_eq!(&request.method, method);
        assert_eq!(&request.form, fields);
    }
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "bounded publication command exceeded30s; retained"
    );
    assert_eq!(
        std::fs::read(family.data.join("pack_plan.json")).unwrap(),
        family.plan_bytes
    );
    let scratch = family.data.join("tmp");
    assert_eq!(
        std::fs::read_dir(scratch.join("native-codec"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        std::fs::read_dir(&scratch)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>(),
        [std::ffi::OsString::from("native-codec")]
    );
    if logo {
        // Preserve the old implication that the published logo is the exact
        // generated cache artifact, independently of its historical PNG encoder.
        let cached = std::fs::read_dir(family.data.join("brand"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(cached.len(), 1);
        let created = observed
            .iter()
            .find(|request| request.method == "createNewStickerSet")
            .unwrap();
        assert_eq!(std::fs::read(&cached[0]).unwrap(), created.files["file0"]);
    }
    observed.into_iter().map(|request| request.method).collect()
}
fn writes(methods: &[String]) -> usize {
    methods
        .iter()
        .filter(|method| {
            matches!(
                method.as_str(),
                "deleteStickerFromSet" | "addStickerToSet" | "createNewStickerSet"
            )
        })
        .count()
}

fn budget(fixture: &Fixture) {
    let mut family = Family::create(
        &fixture.root.0,
        "bounded-family",
        &fixture.images,
        [10, 2],
        5,
    );
    assert_eq!(family.initial.len(), 12);
    family.plan(json!(
        family
            .keys
            .iter()
            .map(|key| json!([key, 2]))
            .collect::<Vec<_>>()
    ));
    let rows =
        _native::catalog_read::plan_rows(&family.data.join("catalog.db"), "cryptoemoji").unwrap();
    assert_eq!(
        _native::plan_steps::compute(&json!({"plan":read(&family.data.join("pack_plan.json")),
        "state":family.state,"items":rows.0,"ids":rows.1}))
        .unwrap()["counted"],
        25
    );
    let mut source = family.live[0].clone();
    let mut destination = family.live[1].clone();
    let mut replies = vec![get_me()];
    for index in 0..10 {
        replies.push(listing(1, &source));
        assert_eq!(source[1]["file_id"], format!("old-{index}"));
        let mut deletion = Reply::new(
            "deleteStickerFromSet",
            &[("sticker", &format!("old-{index}"))],
            json!({"ok":true,"result":true}),
        );
        deletion.observation = Some((
            family.data.join("plan_apply_cryptoemoji.json"),
            "/intent/key".into(),
            json!(family.keys[index]),
        ));
        replies.push(deletion);
        source.remove(1);
        replies.push(listing(1, &source));
    }
    replies.push(listing(2, &destination));
    for index in 0..10 {
        add(&mut replies, fixture, &family, 2, index, &mut destination);
    }
    replies.push(listing(2, &destination));
    let first = finish_command(fixture, &family, replies, false, 3);
    assert_eq!(writes(&first), 20);
    assert_eq!(
        first
            .iter()
            .filter(|method| *method == "deleteStickerFromSet")
            .count(),
        10
    );
    assert_eq!(
        first
            .iter()
            .filter(|method| *method == "addStickerToSet")
            .count(),
        10
    );
    assert!(
        !first
            .iter()
            .any(|method| method == "setStickerPositionInSet")
    );
    let mut cids = (0..10)
        .map(|index| {
            (
                index,
                sticker(index)["custom_emoji_id"].as_str().unwrap().into(),
            )
        })
        .collect::<BTreeMap<_, String>>();
    for index in 10..12 {
        cids.insert(index, format!("111000{index:03}"));
    }
    let mut intermediate = vec![10, 11];
    intermediate.extend(0..10);
    family.check(&fixture.root.0, &[vec![], intermediate], &cids);
    let retired = family.journal();
    assert!(retired["intent"].is_null());
    assert_eq!(retired["retired"].as_array().unwrap().len(), 10);
    for index in 0..10 {
        assert_eq!(retired["retired"][index]["key"], family.keys[index]);
        assert_eq!(
            retired["retired"][index]["cid"],
            format!("111000{index:03}")
        );
    }
    let mut replies = vec![get_me(), listing(2, &destination)];
    for index in 12..17 {
        add(&mut replies, fixture, &family, 2, index, &mut destination);
        cids.insert(
            index,
            sticker(index)["custom_emoji_id"].as_str().unwrap().into(),
        );
    }
    replies.extend([listing(2, &destination), listing(2, &destination)]);
    for index in 0..10 {
        // Original panel order puts the ten moved artworks before the two existing ones.
        let slot = index + 1;
        replies.push(Reply::new(
            "setStickerPositionInSet",
            &[
                ("sticker", &format!("new-{index}")),
                ("position", &slot.to_string()),
            ],
            json!({"ok":true,"result":true}),
        ));
        let at = destination
            .iter()
            .position(|member| member["file_id"] == format!("new-{index}"))
            .unwrap();
        let member = destination.remove(at);
        destination.insert(slot, member);
    }
    let second = finish_command(fixture, &family, replies, false, 0);
    assert_eq!(writes(&second), 5);
    assert_eq!(
        second
            .iter()
            .filter(|method| *method == "setStickerPositionInSet")
            .count(),
        10
    );
    assert_eq!(
        first
            .iter()
            .chain(&second)
            .filter(|method| *method == "deleteStickerFromSet")
            .count(),
        10
    );
    assert_eq!(
        first
            .iter()
            .chain(&second)
            .filter(|method| *method == "addStickerToSet")
            .count(),
        15
    );
    family.check(&fixture.root.0, &[vec![], (0..17).collect()], &cids);
    assert_eq!(source.len(), 1, "only the source logo remains");
    assert_eq!(source[0], family.live[0][0]);
    assert_eq!(destination[0], family.live[1][0]);
    assert_eq!(
        destination[1..]
            .iter()
            .map(|member| member["custom_emoji_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        (0..17)
            .map(|index| cids[&index].as_str())
            .collect::<Vec<_>>()
    );
    let state = std::fs::read(family.data.join("publish_cryptoemoji.json")).unwrap();
    let journal = std::fs::read(family.data.join("plan_apply_cryptoemoji.json")).unwrap();
    let third = finish_command(fixture, &family, vec![get_me()], false, 0);
    assert_eq!(writes(&third), 0);
    assert_eq!(
        std::fs::read(family.data.join("publish_cryptoemoji.json")).unwrap(),
        state
    );
    assert_eq!(
        std::fs::read(family.data.join("plan_apply_cryptoemoji.json")).unwrap(),
        journal
    );
    family.check(&fixture.root.0, &[vec![], (0..17).collect()], &cids);
}

fn logo(fixture: &Fixture) {
    let mut family = Family::create(&fixture.root.0, "logo-family", &fixture.images, [1, 1], 2);
    family.plan(json!([
        [family.keys[0], 1],
        [family.keys[1], 2],
        [family.keys[2], 3],
        [family.keys[3], 3]
    ]));
    let rows =
        _native::catalog_read::plan_rows(&family.data.join("catalog.db"), "cryptoemoji").unwrap();
    let steps =
        _native::plan_steps::compute(&json!({"plan":read(&family.data.join("pack_plan.json")),
        "state":family.state,"items":rows.0,"ids":rows.1}))
        .unwrap();
    assert_eq!(steps["create"], 3);
    assert_eq!(steps["counted"], 3);
    let logo = json!({"custom_emoji_id":"999999999","file_id":"new-logo","file_unique_id":"new-logo-unique"});
    let form = json!([{"sticker":"attach://file0","format":"static","emoji_list":["✅"],"keywords":["logo"]}]).to_string();
    let mut creation = Reply::new(
        "createNewStickerSet",
        &[
            ("user_id", "111111111"),
            ("name", &name(3)),
            ("title", "Test Pack 3"),
            ("sticker_type", "custom_emoji"),
            ("stickers", &form),
        ],
        json!({"ok":true,"result":true}),
    );
    // Fixture::new supplies the independent historical Pillow PNG for this
    // seed. Verify transmitted dimensions, mode and every decoded RGBA sample
    // at request time; the replacement PNG encoder need not share its deflate bytes.
    let expected_logo = rgba_signature(&fixture.logo_bytes).unwrap();
    assert_eq!(expected_logo["size"], json!([100, 100]));
    assert_eq!(expected_logo["mode"], "RGBA");
    creation
        .upload_rgba
        .insert("file0".into(), expected_logo.clone());
    creation.observation = Some((
        family.data.join("publish_cryptoemoji.json"),
        "/in_flight/operation".into(),
        json!("create"),
    ));
    let mut live = vec![logo.clone()];
    let mut replies = vec![
        get_me(),
        listing(2, &family.live[1]),
        creation,
        listing(3, &live),
    ];
    download(&mut replies, &logo, &fixture.logo_bytes);
    add(&mut replies, fixture, &family, 3, 2, &mut live);
    add(&mut replies, fixture, &family, 3, 3, &mut live);
    replies.extend([
        listing(1, &family.live[0]),
        listing(2, &family.live[1]),
        listing(3, &live),
        Reply::new(
            "sendMessage",
            &[
                ("chat_id", "111111111"),
                ("disable_web_page_preview", "true"),
                (
                    "text",
                    &format!("✅ Test Pack 3\nhttps://t.me/addemoji/{}", name(3)),
                ),
            ],
            json!({"ok":true,"result":{"message_id":1}}),
        ),
    ]);
    let methods = finish_command(fixture, &family, replies, true, 0);
    assert_eq!(
        methods
            .iter()
            .filter(|method| matches!(method.as_str(), "createNewStickerSet" | "addStickerToSet"))
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["createNewStickerSet", "addStickerToSet", "addStickerToSet"]
    );
    let cids = BTreeMap::from([
        (0, "111000000".into()),
        (1, "111000001".into()),
        (2, sticker(2)["custom_emoji_id"].as_str().unwrap().into()),
        (3, sticker(3)["custom_emoji_id"].as_str().unwrap().into()),
    ]);
    family.check(&fixture.root.0, &[vec![0], vec![1], vec![2, 3]], &cids);
    let saved = family.saved();
    assert_eq!(
        &saved["sets"].as_array().unwrap()[..2],
        &family.state["sets"].as_array().unwrap()[..2]
    );
    assert_eq!(saved["sets"][2]["title"], "Test Pack 3");
    assert_eq!(saved["sent"], json!([name(1), name(2), name(3)]));
    assert_eq!(live[0]["file_unique_id"], "new-logo-unique");
    assert_eq!(live.len(), 3);
    assert!(!family.data.join("plan_apply_cryptoemoji.json").exists());
    let cached = std::fs::read_dir(family.data.join("brand"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(cached.len(), 1);
    assert_rgba(&std::fs::read(&cached[0]).unwrap(), &expected_logo);
}

pub fn run() {
    let fixture = Fixture::new();
    budget(&fixture);
    logo(&fixture);
    fixture.finish();
}
