use crate::support::{
    http::{Reply, Server},
    *,
};
use numera_runtime as native;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
fn database(f: &Fixture) -> Connection {
    std::fs::create_dir_all(f.data()).unwrap();
    let db = Connection::open(f.data().join("catalog.db")).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT,file_path TEXT,format TEXT,keywords TEXT);CREATE TABLE publications(base TEXT,content_key TEXT,set_name TEXT,custom_emoji_id TEXT);").unwrap();
    db
}
fn zip(path: &Path) -> Vec<(String, Vec<u8>)> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..z.len())
        .map(|i| {
            let mut file = z.by_index(i).unwrap();
            let name = file.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            (name, bytes)
        })
        .collect()
}
struct Export {
    f: Fixture,
    logo: PathBuf,
    a: PathBuf,
    b: PathBuf,
    ka: String,
    kb: String,
}
impl Export {
    fn new() -> Self {
        let f = Fixture::new();
        let ka = format!("s:{}", "a".repeat(32));
        let kb = format!("v:{}", "b".repeat(32));
        let logo = crate::identity::png(&f.root.join("logo.png"), [10, 60, 200, 255]);
        let a = crate::identity::png(&f.root.join("media/a.webp"), [10, 60, 200, 255]);
        let b = f.root.join("media/b.webm");
        write(&b, b"\x1a\x45\xdf\xa3 a video");
        let db = database(&f);
        for (key, path, fmt, keyword, cid) in [
            (&ka, &a, "static", "alpha", "902"),
            (&kb, &b, "video", "beta", "901"),
        ] {
            db.execute(
                "INSERT INTO items VALUES(?,?,?,?)",
                params![
                    key,
                    path.to_str().unwrap(),
                    fmt,
                    json!([keyword]).to_string()
                ],
            )
            .unwrap();
            db.execute(
                "INSERT INTO publications VALUES('fixture',?,'mine2_by_bot',?)",
                params![key, cid],
            )
            .unwrap();
        }
        drop(db);
        document(
            &f.root.join("packs/mine2_by_bot.json"),
            &json!({"set_name":"mine2_by_bot","title":"Mine 2","family":"general","pack_index":2,"emoji":[{"slot":1,"role":"brand-logo","custom_emoji_id":"900","history_key":"logo:mine2_by_bot"},{"slot":2,"role":"emoji","custom_emoji_id":"901","history_key":format!("ck:{kb}")},{"slot":3,"role":"emoji","custom_emoji_id":"902","history_key":format!("ck:{ka}")}]}),
        );
        f.current_roster();
        Self {
            f,
            logo,
            a,
            b,
            ka,
            kb,
        }
    }
    fn export(&self, pack: &str) -> Run {
        self.f.run(
            &[
                "pack-archive",
                "--export",
                pack,
                "--zip",
                self.f.root.join("out.zip").to_str().unwrap(),
            ],
            &[("BRAND_LOGO_PATH", self.logo.display().to_string())],
        )
    }
    fn bytes(&self) -> Vec<Vec<u8>> {
        [&self.logo, &self.a, &self.b]
            .into_iter()
            .map(|p| std::fs::read(p).unwrap())
            .collect()
    }
}
// tests.test_pack_export.APackExportsAsAZip.test_files_are_in_slot_order_with_the_manifest_and_sources_untouched
#[test]
fn export_slot_order_manifest_and_all_sources_untouched() {
    let x = Export::new();
    let before = x.bytes();
    x.export("2").code(0);
    let members = zip(&x.f.root.join("out.zip"));
    assert_eq!(
        members.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        [
            "001_logo.png",
            "002_video_bbbbbbbbbbbb.webm",
            "003_static_aaaaaaaaaaaa.webp",
            "_manifest.md"
        ]
    );
    assert!(
        String::from_utf8(members[3].1.clone())
            .unwrap()
            .contains("beta")
    );
    assert_eq!(x.bytes(), before);
    assert!(x.a.is_file() && x.b.is_file());
    assert_eq!(members[0].1, before[0]);
    assert_eq!(members[1].1, before[2]);
    assert_eq!(members[2].1, before[1]);
}
// tests.test_pack_export.APackExportsAsAZip.test_the_set_name_works_as_well_as_the_number
#[test]
fn export_accepts_set_name() {
    let x = Export::new();
    x.export("mine2_by_bot").code(0);
}
// tests.test_pack_export.APackExportsAsAZip.test_a_stale_roster_is_refused
#[test]
fn export_refuses_stale_roster_before_creating_zip() {
    let x = Export::new();
    std::fs::remove_file(x.f.root.join("packs/index.json")).unwrap();
    let r = x.export("2");
    r.code(3);
    assert!(r.out.contains("pack-manifest --refresh"));
    assert!(!x.f.root.join("out.zip").exists());
}
// tests.test_pack_export.APackExportsAsAZip.test_a_missing_file_refuses_instead_of_a_partial_zip
#[test]
fn export_missing_slot_refuses_partial_zip() {
    let x = Export::new();
    let db = Connection::open(x.f.data().join("catalog.db")).unwrap();
    db.execute("DELETE FROM publications WHERE content_key=?", [&x.kb])
        .unwrap();
    drop(db);
    x.f.current_roster();
    let r = x.export("2");
    r.code(1);
    assert!(r.out.contains("slot 2"));
    assert!(!x.f.root.join("out.zip").exists());
}
// tests.test_native_export_cli.NativeExportCLI.test_ordered_complete_export_and_missing_input_preserve_output
#[test]
fn native_export_complete_ordered_zip_and_missing_input_preserve_output() {
    let f = Fixture::new();
    let source = f.data().join("source.png");
    let original = std::fs::read(project().join("assets/numera-emoji-mapper-logo.png")).unwrap();
    write(&source, &original);
    let key = format!("s:{}", "a".repeat(32));
    let name = "fixture_by_YourEmojiBot";
    let db = database(&f);
    db.execute(
        "INSERT INTO items VALUES(?,?,'static','[\"fixture|keyword\"]')",
        params![key, source.to_str().unwrap()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO publications VALUES('fixture',?,?,'222222222')",
        params![key, name],
    )
    .unwrap();
    drop(db);
    document(
        &f.root.join(format!("packs/{name}.json")),
        &json!({"set_name":name,"title":"Fixture","family":"general","pack_index":6,"emoji":[{"slot":1,"role":"brand-logo","custom_emoji_id":"111111111","history_key":format!("logo:{name}")},{"slot":2,"role":"emoji","custom_emoji_id":"222222222","history_key":format!("ck:{key}")}]}),
    );
    f.current_roster();
    let destination = f.root.join("output.zip");
    let env = [(
        "BRAND_LOGO_PATH",
        project()
            .join("assets/numera-emoji-mapper-logo.png")
            .display()
            .to_string(),
    )];
    let run = || {
        f.run(
            &[
                "pack-archive",
                "--export",
                "6",
                "--zip",
                destination.to_str().unwrap(),
            ],
            &env,
        )
    };
    run().code(0);
    let members = zip(&destination);
    assert_eq!(
        members.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        [
            "001_logo.png",
            "002_static_aaaaaaaaaaaa.png",
            "_manifest.md"
        ]
    );
    assert_eq!(members[0].1, original);
    assert_eq!(members[1].1, original);
    assert!(
        String::from_utf8(members[2].1.clone())
            .unwrap()
            .contains("fixture\\|keyword")
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let complete = std::fs::read(&destination).unwrap();
    std::fs::remove_file(&source).unwrap();
    let r = run();
    r.code(1);
    assert!(r.out.contains("slot 2"));
    assert_eq!(std::fs::read(&destination).unwrap(), complete);
    assert!(
        !std::fs::read_dir(&f.root).unwrap().any(|p| p
            .unwrap()
            .path()
            .extension()
            .is_some_and(|e| e == "tmp"))
    );
    let mut index = read_json(&f.root.join("packs/index.json"));
    index["inputs"]["catalog.db"] = Value::Null;
    document(&f.root.join("packs/index.json"), &index);
    run().code(3);
    assert_eq!(std::fs::read(destination).unwrap(), complete);
}
// tests.test_native_archive_cli.NativeArchiveCLI.test_missing_second_source_keeps_first_move_recorded
#[test]
fn archive_partial_commit_survives_missing_later_source_and_retries() {
    let f = Fixture::new();
    let source = f.data().join("first.png");
    let original = std::fs::read(project().join("assets/numera-emoji-mapper-logo.png")).unwrap();
    write(&source, &original);
    let key = format!("s:{}", "a".repeat(32));
    let missing_key = format!("s:{}", "b".repeat(32));
    {
        let mut cat = f.catalog();
        for (k, path, emoji, label, cid) in [
            (&key, source.clone(), "😀", "first", "111111111"),
            (
                &missing_key,
                f.data().join("missing.png"),
                "⭐",
                "missing",
                "222222222",
            ),
        ] {
            cat.insert(&json!({"content_key":k,"fmt":"static","file_path":path,"emojis":[emoji],"keywords":[label],"source":"fixture"}),UTC).unwrap();
            cat.uploaded(&json!({"content_key":k,"custom_emoji_id":cid,"base":"fixture","set_name":"fixture_by_YourEmojiBot"}),UTC).unwrap();
        }
    }
    let db = Connection::open(f.data().join("catalog.db")).unwrap();
    db.execute(
        "UPDATE publications SET set_name=NULL WHERE content_key=?",
        [&missing_key],
    )
    .unwrap();
    document(
        &f.data().join("publish_fixture.json"),
        &json!({"sets":[{"name":"fixture_by_YourEmojiBot","title":"Fixture Pack","fmt":"mixed","index":1,"live":200}]}),
    );
    let bot = Server::new(|r| {
        assert_eq!(r.method(), "getStickerSet");
        Reply::ok(
            json!({"stickers":[{"custom_emoji_id":"000000000"},{"custom_emoji_id":"111111111","emoji":"😀"},{"custom_emoji_id":"222222222","emoji":"⭐"}]}),
        )
    });
    let mut env = bot.environment();
    env.extend([
        (
            "BRAND_LOGO_PATH",
            project()
                .join("assets/numera-emoji-mapper-logo.png")
                .display()
                .to_string(),
        ),
        (
            "EMOJI_ARCHIVE_DIR",
            f.root.join("archive").display().to_string(),
        ),
    ]);
    let run = |args: &[&str], env: &[(&str, String)]| {
        let mut cmd = vec!["pack-archive"];
        cmd.extend_from_slice(args);
        f.run(&cmd, env)
    };
    run(&["--sync"], &env).code(4);
    let archived = f
        .root
        .join("archive/Fixture Pack/002_static_aaaaaaaaaaaa.png");
    assert_eq!(std::fs::read(&archived).unwrap(), original);
    assert!(!source.exists());
    let stored: String = db
        .query_row(
            "SELECT file_path FROM items WHERE content_key=?",
            [&key],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        native::paths::resolve(&f.data(), &stored, &f.root)
            .canonicalize()
            .unwrap(),
        archived.canonicalize().unwrap()
    );
    let history = archived.parent().unwrap().join("_history.json");
    assert!(!history.exists());
    write(&f.data().join("missing.png"), &original);
    db.execute(
        "UPDATE publications SET set_name='fixture_by_YourEmojiBot' WHERE content_key=?",
        [&missing_key],
    )
    .unwrap();
    run(&["--sync"], &env).code(0);
    assert_eq!(std::fs::read(&archived).unwrap(), original);
    assert!(!f.data().join("missing.png").exists());
    assert_eq!(
        std::fs::read(archived.parent().unwrap().join("001_logo.png")).unwrap(),
        original
    );
    assert_eq!(read_json(&history)["emoji"][1]["content_key"], key);
    run(&["--check"], &env).code(0);
    let r = run(&["--sync"], &env);
    r.code(0);
    assert!(r.out.contains("0 file(s) moved"));
    assert_eq!(
        bot.methods(),
        ["getStickerSet", "getStickerSet", "getStickerSet"]
    );
    write(&source, &original);
    db.execute(
        "UPDATE items SET file_path=? WHERE content_key=?",
        params![source.to_str().unwrap(), key],
    )
    .unwrap();
    write(&archived, b"interrupted-copy");
    run(&["--sync"], &env).code(1);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(
        db.query_row(
            "SELECT file_path FROM items WHERE content_key=?",
            [&key],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        source.to_str().unwrap()
    );
    assert!(
        !std::fs::read_dir(archived.parent().unwrap())
            .unwrap()
            .any(|e| {
                let name = e.unwrap().file_name().to_string_lossy().into_owned();
                name.starts_with(".archive-") && name.ends_with(".tmp")
            })
    );
    for (variable, args) in [
        ("COLLECTION_PACK_BASE", vec!["--check"]),
        ("EMOJI_ARCHIVE_DIR", vec!["--check"]),
        ("GENERAL_BOT_TOKEN", vec!["--sync"]),
    ] {
        let mut refused = env.clone();
        refused.push((variable, "".into()));
        run(&args, &refused).code(1);
    }
}
