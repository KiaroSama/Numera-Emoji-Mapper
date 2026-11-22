use super::{identity_process as process, identity_support::*};
use crate::{catalog::Catalog, migration_apply, ownership, reconcile};
use serde_json::{Value, json};
use std::time::Duration;

fn writer_fixture() -> Fixture {
    let f = Fixture::real();
    image::RgbaImage::from_pixel(100, 100, image::Rgba([255, 0, 0, 255]))
        .save(f.data.join("incoming.png"))
        .unwrap();
    f
}
fn release_writer(f: &Fixture, mut child: crate::process_owner::OwnedProcess) {
    std::fs::write(f.root.0.join("writer-release"), b"release").unwrap();
    let status = runtime()
        .block_on(child.wait(Duration::from_secs(10), Duration::from_secs(10)))
        .unwrap();
    child.finish().unwrap();
    assert_eq!(status.code(), Some(0));
}
// tests.test_migration_lifecycle.ActualWriterOwnership.test_first_family_publisher_and_catalog_exclude_migration_in_real_children
#[test]
fn lifecycle_first_family_publisher_and_catalog_exclude_migration_in_real_children() {
    for (kind, stage) in [
        ("hold-publisher", "writer-publisher"),
        ("hold-catalog", "writer-catalog"),
        ("hold-archive", "writer-archive"),
        ("hold-panel", "writer-panel"),
        ("hold-ingest", "writer-ingest"),
    ] {
        let f = writer_fixture();
        let fake = Fake::default();
        let child = process::held(&f, &fake, kind, stage, true);
        assert!(
            f.apply(&fake).unwrap_err().contains("ownership refused"),
            "{kind}"
        );
        assert!(!f.journal().exists(), "{kind}");
        release_writer(&f, child);
    }
}
// tests.test_migration_lifecycle.ActualWriterOwnership.test_pending_journal_blocks_ingest_archive_publisher_and_panel_save
#[test]
fn lifecycle_pending_journal_blocks_ingest_archive_publisher_and_panel_save() {
    use crate::{logging::RunLog, media_native_adapter::NativeMediaAdapter};
    use std::sync::{Arc, Mutex};
    let f = writer_fixture();
    std::fs::write(f.journal(), "{}").unwrap();
    let log = Arc::new(RunLog::open(&f.root.0, "pending-writer", Vec::new()));
    let image = f.data.join("incoming.png");
    let error = crate::local_cli::run_with_codec(
        &f.config,
        &[
            image.to_str().unwrap().into(),
            "--data-dir".into(),
            f.data.to_str().unwrap().into(),
        ],
        &log,
        |_| Ok(Fake::default()),
    )
    .unwrap_err();
    assert!(error.contains("interrupted identity migration"), "{error}");
    assert_ne!(
        crate::publish_cli::run(
            &f.config,
            &[
                "--base",
                "newfamily",
                "--title",
                "Fixture",
                "--dry-run",
                "--no-brand-logo",
                "--data-dir",
                f.data.to_str().unwrap()
            ]
            .map(str::to_owned),
            &log
        )
        .unwrap(),
        0
    );
    let tg = crate::telegram::Telegram::new("test-only-token".into(), "http://127.0.0.1:9".into())
        .unwrap();
    assert!(
        crate::archive_sync::run(&f.config, &tg, &log)
            .unwrap_err()
            .contains("interrupted identity migration")
    );
    let panel = crate::panel_http::Panel {
        root: f.root.0.clone(),
        db: f.data.join("catalog.db"),
        token: "fixture-token".into(),
        options: json!({"show_published":false,"keep_sets":{},"branded":false}),
        branding: None,
        detected_bot: Mutex::new(String::new()),
        view: Mutex::new(json!({"view":[],"by_key":{},"hidden":0})),
        session: json!({}),
        last_refreshed: std::sync::Mutex::new(None),
        utc: || UTC.into(),
        log,
        events: Mutex::new(crate::panel_events::Events::default()),
        previews: crate::panel_preview::Previews::new(
            NativeMediaAdapter {
                executable: "must-not-decode-pending-journal".into(),
                root: f.root.0.clone(),
                scratch: f.data.join("panel-codec"),
            },
            &f.data,
            1,
        ),
    };
    runtime().block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, crate::panel_http::router(Arc::new(panel))).await
        });
        let response = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
            .post(format!("http://{address}/api/save"))
            .header("x-panel-token", "fixture-token")
            .json(&json!({"excluded":[],"known":[]}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 503);
        drop(response);
        server.abort();
        let _ = server.await;
    });
    assert!(image.is_file());
    assert_eq!(
        f.db()
            .query_row("SELECT count(*) FROM items", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}
// tests.test_migration_lifecycle.ActualWriterOwnership.test_canonical_alias_and_second_migration_use_the_same_native_lock
#[test]
fn lifecycle_canonical_alias_and_second_migration_use_the_same_native_lock() {
    let f = Fixture::real();
    std::fs::create_dir(f.data.join("nested")).unwrap();
    let alias = f.data.join("nested/..");
    std::thread::scope(|scope| {
        let (ready, started) = std::sync::mpsc::channel();
        let (release, done) = std::sync::mpsc::channel();
        let data = &f.data;
        let thread = scope.spawn(move || {
            let _owner =
                ownership::Ownership::acquire(data, ownership::Mode::Maintenance, UTC).unwrap();
            ready.send(()).unwrap();
            done.recv_timeout(Duration::from_secs(10)).unwrap();
        });
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            migration_apply::apply(&f.config, &alias, &Fake::default(), &runtime(), None)
                .unwrap_err()
                .contains("ownership refused")
        );
        assert!(
            Catalog::open(&f.data.join("catalog.db"), UTC, &f.root.0)
                .err()
                .unwrap()
                .contains("ownership refused")
        );
        release.send(()).unwrap();
        thread.join().unwrap();
    });
}
fn unchanged_reconcile(cat: &mut Catalog, rec: &Value) -> Value {
    let mut publications = serde_json::Map::new();
    let mut seen = serde_json::Map::new();
    for row in cat.all(None).unwrap() {
        let key = row["content_key"].as_str().unwrap();
        publications.insert(
            key.into(),
            cat.dispatch(json!({"operation":"custom_id","base":"audit","content_key":key}))
                .unwrap(),
        );
    }
    let stickers = (0..3)
        .rev()
        .map(|i| {
            let fuid = format!("FUID{i}");
            seen.insert(
                fuid.clone(),
                cat.dispatch(json!({"operation":"seen","file_unique_id":fuid}))
                    .unwrap(),
            );
            json!({"file_unique_id":format!("FUID{i}"),"custom_emoji_id":format!("CID{i}")})
        })
        .collect::<Vec<_>>();
    reconcile::reconcile(&json!({"set":rec,"stickers":stickers,"resolved":[null,null,null],"publications":publications,"seen":seen})).unwrap()
}
// tests.test_migration_lifecycle.ActualWriterOwnership.test_real_catalog_order_and_unchanged_pack_resume_survive_migration_and_restore
#[test]
fn lifecycle_real_catalog_order_and_unchanged_pack_resume_survive_migration_and_restore() {
    let f = Fixture::real();
    let mut cat = Catalog::open(&f.data.join("catalog.db"), UTC, &f.root.0).unwrap();
    let mut keys = Vec::new();
    let mut mapping = Vec::new();
    let mut names = Vec::new();
    let mut new = Vec::new();
    for i in 0..3u8 {
        let key = format!("v:{}", format!("{i:02x}").repeat(16));
        let name = format!("{i:03}_video_{}.webm", &key[2..14]);
        let path = f.data.join(&name);
        let mut bytes = b"\x1a\x45\xdf\xa3fixture".to_vec();
        bytes.push(i);
        std::fs::write(&path, bytes).unwrap();
        cat.insert(
            &json!({"content_key":key,"fmt":"video","file_path":path,"phash":i}),
            UTC,
        )
        .unwrap();
        cat.uploaded(&json!({"content_key":key,"custom_emoji_id":format!("CID{i}"),"base":"audit","set_name":"audit1"}),UTC).unwrap();
        cat.record_seen(&format!("FUID{i}"), &key).unwrap();
        keys.push(key);
        names.push(name);
        new.push(format!("v:{}", format!("{:02x}", i + 16).repeat(16)));
    }
    let reversed = keys.iter().rev().cloned().collect::<Vec<_>>();
    cat.order(&reversed).unwrap();
    cat.inclusion(&[keys[1].clone()]).unwrap();
    drop(cat);
    let rec = json!({"name":"audit1","index":1,"fmt":"video","live":3,"keys":reversed});
    let state = f.state("audit", &json!({"base":"audit","sets":[rec]}));
    f.plan("audit", &json!({"video":reversed}));
    for i in 0..3 {
        mapping.push((names[i].as_str(), new[i].as_str(), Some(i as u64)));
    }
    let fake = f.fake(&mapping);
    assert_eq!(
        process::cli(&f, &fake, &["migrate-video-keys", "--apply"]),
        0
    );
    let mut cat = Catalog::open(&f.data.join("catalog.db"), UTC, &f.root.0).unwrap();
    let migrated = read_json(&state)["sets"][0].clone();
    assert_eq!(unchanged_reconcile(&mut cat, &migrated)["set"]["live"], 3);
    assert_eq!(
        cat.all(None)
            .unwrap()
            .iter()
            .map(|row| row["included"].as_bool().unwrap())
            .collect::<Vec<_>>(),
        [true, false, true]
    );
    drop(cat);
    let manifest = f.manifest();
    assert_eq!(
        process::cli(
            &f,
            &fake,
            &["restore", "--apply", "--bundle", manifest.to_str().unwrap()]
        ),
        0
    );
    let mut cat = Catalog::open(&f.data.join("catalog.db"), UTC, &f.root.0).unwrap();
    assert_eq!(
        cat.all(None)
            .unwrap()
            .iter()
            .map(|row| row["content_key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        reversed
    );
    assert_eq!(
        cat.all(None)
            .unwrap()
            .iter()
            .map(|row| row["included"].as_bool().unwrap())
            .collect::<Vec<_>>(),
        [true, false, true]
    );
    assert_eq!(unchanged_reconcile(&mut cat, &rec)["set"]["live"], 3);
}
