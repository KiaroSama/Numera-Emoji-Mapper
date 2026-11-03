//! Original storage-only catalog assertions; synthetic bytes never stand in for decoded media.
use _native::{catalog::Catalog, locks::Lease};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

const UTC: &str = "2026-10-07 00:00:00 UTC";

fn source_identity() {
    let inventory: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    assert_eq!(
        inventory["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let files = inventory["files"].as_array().unwrap();
    for (path, hash) in [
        (
            "tests/test_catalog.py",
            "33391dab27bc8d005844839047b8683a14212c910cf0240f76f7c08fb26ea062",
        ),
        (
            "tests/test_catalog_order.py",
            "4cf89c5ae515a92b7bbdc397642a691ff0f6879e64169fd6b798a8ef607010b0",
        ),
    ] {
        let source = files
            .iter()
            .filter(|file| file["source"] == path)
            .collect::<Vec<_>>();
        assert_eq!(source.len(), 1);
        assert_eq!(source[0]["sha256"], hash);
    }
}

fn project() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

fn item(root: &Path, key: &str, name: &str) -> Value {
    let path = root.join(name);
    std::fs::write(&path, b"x").unwrap();
    json!({"content_key":key,"fmt":"static","file_path":path})
}

fn seen(catalog: &mut Catalog, fuid: &str) -> Value {
    catalog
        .dispatch(json!({"operation":"seen","file_unique_id":fuid}))
        .unwrap()
}

#[test]
fn original_fuid_conflicts_are_first_writer_wins_idempotent_and_persistent() {
    source_identity();
    let root = Root::new("catalog fuid");
    let database = root.0.join("data/catalog.db");
    let mut catalog = Catalog::open(&database, UTC, project()).unwrap();
    for (key, name) in [("s:one", "a.png"), ("s:two", "b.png")] {
        assert!(catalog.insert(&item(&root.0, key, name), UTC).unwrap());
        catalog.record_seen("FUID1", key).unwrap();
    }
    assert_eq!(seen(&mut catalog, "FUID1"), "s:one");
    catalog.record_seen("FUID1", "s:one").unwrap();
    assert_eq!(seen(&mut catalog, "FUID1"), "s:one");
    assert_eq!(seen(&mut catalog, "OTHER"), Value::Null);
    catalog.record_seen("FX", "s:one").unwrap();
    drop(catalog);
    let mut reopened = Catalog::open(&database, UTC, project()).unwrap();
    assert!(reopened.get("s:one").unwrap().is_some());
    assert_eq!(seen(&mut reopened, "FX"), "s:one");
    assert_eq!(seen(&mut reopened, "FUID1"), "s:one");
    drop(reopened);
    root.clean().unwrap();
}

fn uploaded(catalog: &mut Catalog, key: &str, base: Option<&str>, id: Option<&str>) {
    catalog
        .uploaded(
            &json!({"content_key":key,"base":base,"custom_emoji_id":id,
        "set_name":base.map(|base| format!("{base}1"))}),
            UTC,
        )
        .unwrap();
}

#[test]
fn original_publication_families_forget_inclusion_and_null_ids_remain_independent() {
    source_identity();
    let root = Root::new("catalog publications");
    let mut catalog = Catalog::open(&root.0.join("data/catalog.db"), UTC, project()).unwrap();
    for index in 0..3 {
        assert!(
            catalog
                .insert(
                    &item(&root.0, &format!("s:k{index}"), &format!("m{index}.png")),
                    UTC
                )
                .unwrap()
        );
    }
    assert_eq!(catalog.pending(None, Some("one")).unwrap().len(), 3);
    for row in catalog.pending(None, Some("one")).unwrap() {
        let key = row["content_key"].as_str().unwrap();
        uploaded(&mut catalog, key, Some("one"), Some(&format!("cid_{key}")));
    }
    assert!(catalog.pending(None, Some("one")).unwrap().is_empty());
    assert_eq!(catalog.pending(None, Some("two")).unwrap().len(), 3);
    for (operation, base, expected) in [
        ("published", "one", json!(true)),
        ("published", "two", json!(false)),
        ("custom_id", "one", json!("cid_s:k0")),
        ("custom_id", "two", Value::Null),
    ] {
        assert_eq!(
            catalog
                .dispatch(json!({"operation":operation,"base":base,"content_key":"s:k0"}))
                .unwrap(),
            expected
        );
    }
    assert_eq!(
        catalog
            .dispatch(json!({"operation":"forget","base":"one"}))
            .unwrap(),
        3
    );
    assert_eq!(catalog.pending(None, Some("one")).unwrap().len(), 3);
    catalog.inclusion(&["s:k0".into()]).unwrap();
    let pending = catalog.pending(None, Some("one")).unwrap();
    assert_eq!(
        pending
            .iter()
            .map(|row| row["content_key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["s:k1", "s:k2"]
    );
    drop(catalog);
    root.clean().unwrap();
}

#[test]
fn original_global_pending_filters_and_upload_metadata_match_source() {
    source_identity();
    let root = Root::new("catalog global pending");
    let mut catalog = Catalog::open(&root.0.join("data/catalog.db"), UTC, project()).unwrap();
    let path = root.0.join("f.png");
    std::fs::write(&path, b"x").unwrap();
    for (key, format) in [("s:a", "static"), ("v:b", "video")] {
        assert!(
            catalog
                .insert(
                    &json!({"content_key":key,"fmt":format,"file_path":path}),
                    UTC
                )
                .unwrap()
        );
    }
    assert_eq!(catalog.pending(None, None).unwrap().len(), 2);
    let static_rows = catalog.pending(Some("static"), None).unwrap();
    assert_eq!(static_rows.len(), 1);
    assert_eq!(static_rows[0]["content_key"], "s:a");
    uploaded(&mut catalog, "s:a", None, Some("cid123"));
    assert!(catalog.pending(Some("static"), None).unwrap().is_empty());
    let uploaded_row = catalog.get("s:a").unwrap().unwrap();
    assert_eq!(uploaded_row["custom_emoji_id"], "cid123");
    assert_eq!(uploaded_row["uploaded"], true);
    let remaining = catalog.pending(None, None).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0]["content_key"], "v:b");
    assert_eq!(std::fs::read(path).unwrap(), b"x");
    drop(catalog);
    root.clean().unwrap();
}

#[test]
fn original_legacy_uploads_can_be_adopted_by_only_one_family() {
    source_identity();
    let root = Root::new("catalog legacy");
    let database = root.0.join("data/catalog.db");
    let mut catalog = Catalog::open(&database, UTC, project()).unwrap();
    assert!(catalog.insert(&item(&root.0, "s:k", "m.png"), UTC).unwrap());
    uploaded(&mut catalog, "s:k", None, Some("cid"));
    catalog
        .dispatch(json!({"operation":"set_meta","key":"publications_migrated","value":"0"}))
        .unwrap();
    drop(catalog);
    let mut catalog = Catalog::open(&database, UTC, project()).unwrap();
    assert_eq!(
        catalog.dispatch(json!({"operation":"bases"})).unwrap(),
        json!(["__legacy__"])
    );
    assert_eq!(
        catalog
            .dispatch(json!({"operation":"adopt","base":"one"}))
            .unwrap(),
        1
    );
    assert_eq!(
        catalog
            .dispatch(json!({"operation":"published","base":"one","content_key":"s:k"}))
            .unwrap(),
        true
    );
    assert_eq!(
        catalog
            .dispatch(json!({"operation":"adopt","base":"two"}))
            .unwrap(),
        0
    );
    assert_eq!(
        catalog
            .dispatch(json!({"operation":"published","base":"two","content_key":"s:k"}))
            .unwrap(),
        false
    );
    drop(catalog);
    let mut reopened = Catalog::open(&database, UTC, project()).unwrap();
    assert_eq!(
        reopened.dispatch(json!({"operation":"bases"})).unwrap(),
        json!(["one"])
    );
    drop(reopened);
    root.clean().unwrap();
}

#[test]
fn original_large_order_and_unlisted_relative_order_use_real_sqlite() {
    source_identity();
    let root = Root::new("catalog order");
    let database = root.0.join("data/catalog.db");
    let mut catalog = Catalog::open(&database, UTC, project()).unwrap();
    for index in 0..3 {
        assert!(
            catalog
                .insert(
                    &item(&root.0, &format!("s:k{index}"), &format!("m{index}.png")),
                    UTC
                )
                .unwrap()
        );
    }
    assert_eq!(catalog.order(&["s:k2".into()]).unwrap(), 1);
    assert_eq!(
        catalog
            .all(None)
            .unwrap()
            .iter()
            .map(|row| row["content_key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["s:k2", "s:k0", "s:k1"]
    );
    let mut keys = vec!["s:k2".into()];
    keys.extend((0..40_000).map(|index| format!("s:ghost{index}")));
    assert_eq!(catalog.order(&keys).unwrap(), 1);
    assert_eq!(catalog.all(None).unwrap()[0]["content_key"], "s:k2");
    drop(catalog);
    root.clean().unwrap();
}

#[test]
fn original_saved_order_new_item_tail_and_metadata_survive_reopen() {
    source_identity();
    let root = Root::new("catalog persisted order");
    let database = root.0.join("data/catalog.db");
    let artwork = project().join("tests/fixtures/native/static-codec/alpha-edge.png");
    assert_eq!(
        _native::tooling_manifest::digest(&artwork).unwrap(),
        "d359d28b44f1871b588f04dc95ca56f8716b18768b6b91ef3f9aad66ed01aa71"
    );
    let keys = (0..5)
        .map(|index| format!("s:k{index:030}"))
        .collect::<Vec<_>>();
    let mut catalog = Catalog::open(&database, UTC, project()).unwrap();
    for (index, key) in keys.iter().enumerate() {
        let path = root.0.join(format!("m{index}.png"));
        std::fs::copy(&artwork, &path).unwrap();
        assert!(
            catalog
                .insert(
                    &json!({"content_key":key,"fmt":"static",
            "file_path":path,"keywords":[format!("k{index}")]}),
                    UTC
                )
                .unwrap()
        );
    }
    let ordered_keys = |catalog: &Catalog| {
        catalog
            .all(None)
            .unwrap()
            .into_iter()
            .map(|row| row["content_key"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(ordered_keys(&catalog), keys);
    let reversed = keys.iter().rev().cloned().collect::<Vec<_>>();
    assert_eq!(catalog.order(&reversed).unwrap(), 5);
    drop(catalog);
    let mut catalog = Catalog::open(&database, UTC, project()).unwrap();
    assert_eq!(ordered_keys(&catalog), reversed);
    let saved = [2, 0, 1, 3, 4].map(|index| keys[index].clone());
    assert_eq!(catalog.order(&saved).unwrap(), 5);
    let new_path = root.0.join("m5.png");
    std::fs::copy(&artwork, &new_path).unwrap();
    assert!(
        catalog
            .insert(
                &json!({"content_key":"s:k99","fmt":"static",
        "file_path":new_path,"keywords":["k99"]}),
                UTC
            )
            .unwrap()
    );
    assert_eq!(catalog.meta("order_seeded").unwrap(), None);
    catalog
        .dispatch(json!({"operation":"set_meta","key":"order_seeded","value":"1"}))
        .unwrap();
    assert_eq!(catalog.meta("order_seeded").unwrap().as_deref(), Some("1"));
    drop(catalog);
    let catalog = Catalog::open(&database, UTC, project()).unwrap();
    let actual = ordered_keys(&catalog);
    assert_eq!(&actual[..5], saved.as_slice());
    assert_eq!(actual.len(), 6);
    assert_eq!(actual.last().unwrap(), "s:k99");
    assert_eq!(catalog.meta("order_seeded").unwrap().as_deref(), Some("1"));
    for index in 0..6 {
        assert_eq!(
            _native::tooling_manifest::digest(&root.0.join(format!("m{index}.png"))).unwrap(),
            "d359d28b44f1871b588f04dc95ca56f8716b18768b6b91ef3f9aad66ed01aa71"
        );
    }
    drop(catalog);
    root.clean().unwrap();
}

#[test]
fn original_stats_inclusion_and_full_unsigned_hash_survive_readback() {
    source_identity();
    let root = Root::new("catalog statistics");
    let mut catalog = Catalog::open(&root.0.join("data/catalog.db"), UTC, project()).unwrap();
    for (key, name) in [("s:a", "a.png"), ("s:b", "b.png")] {
        assert!(catalog.insert(&item(&root.0, key, name), UTC).unwrap());
    }
    uploaded(&mut catalog, "s:a", None, None);
    assert_eq!(
        catalog.stats().unwrap()["static"],
        json!({"total":2,"uploaded":1,"excluded":0,"pending":1})
    );
    catalog.inclusion(&["s:b".into()]).unwrap();
    let pending = catalog.pending(Some("static"), None).unwrap();
    assert!(pending.is_empty());
    assert_eq!(
        catalog.stats().unwrap()["static"],
        json!({"total":2,"uploaded":1,"excluded":1,"pending":0})
    );
    let mut big = item(&root.0, "s:big", "big.png");
    big["phash"] = json!(u64::MAX);
    assert!(catalog.insert(&big, UTC).unwrap());
    assert_eq!(catalog.get("s:big").unwrap().unwrap()["phash"], u64::MAX);
    drop(catalog);
    root.clean().unwrap();
}

#[test]
fn original_duplicate_labels_and_only_losing_file_cleanup_use_verified_ingest() {
    source_identity();
    let root = Root::new("catalog duplicates");
    let mut catalog = Catalog::open(&root.0.join("data/catalog.db"), UTC, project()).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    {
        let mut add = |incoming: Value| {
            runtime
                .block_on(_native::ingest::add_verified(
                    &mut catalog,
                    incoming,
                    UTC,
                    -1,
                    async |a: &Path, b: &Path, _format: &str| {
                        Ok(Some(
                            std::fs::read(a).map_err(|error| error.to_string())?
                                == std::fs::read(b).map_err(|error| error.to_string())?,
                        ))
                    },
                    async |_path: &Path, _key: &str| {
                        Err("unexpected collision for identical fixture bytes".into())
                    },
                ))
                .unwrap()
        };
        let mut first = item(&root.0, "s:same", "first.png");
        first["emojis"] = json!(["😀"]);
        first["keywords"] = json!(["a"]);
        first["sources"] = json!(["p1"]);
        assert_eq!(add(first), ("s:same".into(), true));
        let mut second = item(&root.0, "s:same", "second.png");
        second["emojis"] = json!(["🔥"]);
        second["keywords"] = json!(["b"]);
        second["sources"] = json!(["p2"]);
        assert_eq!(add(second), ("s:same".into(), false));
        assert_eq!(std::fs::read(root.0.join("first.png")).unwrap(), b"x");
        assert_eq!(
            std::fs::symlink_metadata(root.0.join("second.png"))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
        assert_eq!(
            add(
                json!({"content_key":"s:same","fmt":"static","file_path":root.0.join("first.png")})
            ),
            ("s:same".into(), false)
        );
    }
    assert_eq!(std::fs::read(root.0.join("first.png")).unwrap(), b"x");
    let merged = catalog.get("s:same").unwrap().unwrap();
    assert_eq!(merged["emojis"], json!(["😀", "🔥"]));
    assert_eq!(merged["keywords"], json!(["a", "b"]));
    assert_eq!(merged["sources"], json!(["p1", "p2"]));
    drop(catalog);
    root.clean().unwrap();
}

#[test]
fn original_exclusion_statistics_use_the_same_pending_predicate() {
    source_identity();
    let root = Root::new("catalog exclusions");
    let mut catalog = Catalog::open(&root.0.join("data/catalog.db"), UTC, project()).unwrap();
    for index in 0..3 {
        assert!(
            catalog
                .insert(
                    &item(&root.0, &format!("s:k{index}"), &format!("m{index}.png")),
                    UTC
                )
                .unwrap()
        );
    }
    catalog.inclusion(&["s:k0".into()]).unwrap();
    let stats = catalog.stats().unwrap();
    let pending = catalog.pending(Some("static"), None).unwrap();
    assert_eq!(stats["static"]["total"], 3);
    assert_eq!(stats["static"]["excluded"], 1);
    assert_eq!(stats["static"]["pending"], pending.len());
    assert_eq!(stats["static"]["pending"], 2);
    drop(catalog);
    root.clean().unwrap();
}

#[test]
fn original_near_hashes_only_nominate_verified_duplicates_and_preserve_signed_bits() {
    source_identity();
    let root = Root::new("catalog near hashes");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    // Each original case gets its own catalog: a second rival would change nomination ambiguity.
    for (index, first_hash, second_hash, expected_new) in [
        (0, 0_u64, 0b0011_u64, false),
        (1, 0, 0b11111111, true),
        (2, u64::MAX, u64::MAX, false),
    ] {
        let data = root.0.join(index.to_string());
        let mut catalog = Catalog::open(&data.join("catalog.db"), UTC, project()).unwrap();
        let mut first = item(&data, "s:k1", "first.png");
        first["phash"] = json!(first_hash);
        first["emojis"] = json!(["A"]);
        assert!(catalog.insert(&first, UTC).unwrap());
        assert_eq!(catalog.get("s:k1").unwrap().unwrap()["phash"], first_hash);
        let mut second = item(&data, "s:k2", "second.png");
        second["phash"] = json!(second_hash);
        second["emojis"] = json!(["B"]);
        let mut comparisons = 0;
        let actual = runtime
            .block_on(_native::ingest::add_verified(
                &mut catalog,
                second,
                UTC,
                4,
                async |a: &Path, b: &Path, _format: &str| {
                    comparisons += 1;
                    Ok(Some(
                        std::fs::read(a).map_err(|error| error.to_string())?
                            == std::fs::read(b).map_err(|error| error.to_string())?,
                    ))
                },
                async |_path: &Path, _key: &str| {
                    Err("unexpected collision in source near-hash case".into())
                },
            ))
            .unwrap();
        assert_eq!(
            actual,
            (
                if expected_new { "s:k2" } else { "s:k1" }.into(),
                expected_new
            )
        );
        assert_eq!(comparisons, usize::from(!expected_new));
        assert_eq!(catalog.get("s:k2").unwrap().is_some(), expected_new);
        assert_eq!(
            catalog.get("s:k1").unwrap().unwrap()["emojis"],
            if expected_new {
                json!(["A"])
            } else {
                json!(["A", "B"])
            }
        );
        assert_eq!(std::fs::read(data.join("first.png")).unwrap(), b"x");
        if expected_new {
            assert_eq!(std::fs::read(data.join("second.png")).unwrap(), b"x");
        } else {
            assert_eq!(
                std::fs::symlink_metadata(data.join("second.png"))
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound
            );
        }
        drop(catalog);
    }
    root.clean().unwrap();
}

#[test]
fn failed_initialization_releases_the_database_and_canonical_writer_lease() {
    source_identity();
    let root = Root::new("catalog init refusal");
    let data = root.0.join("data");
    std::fs::create_dir(&data).unwrap();
    let database = data.join("catalog.db");
    let observer = Connection::open(&database).unwrap();
    observer
        .execute_batch("CREATE TABLE items(wrong_column TEXT)")
        .unwrap();
    observer.close().unwrap();
    assert!(Catalog::open(&database, UTC, project()).is_err());
    let lease = Lease::acquire(&data.join(".maintenance.lock"), "after init refusal").unwrap();
    drop(lease);
    assert!(data.join(".maintenance.lock").is_file());
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let exclusive = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&database)
            .expect("failed initialization must release its SQLite handle");
        drop(exclusive);
    }
    let moved: PathBuf = data.join("closed.db");
    std::fs::rename(&database, &moved).unwrap();
    drop(Catalog::open(&data.join("fresh.db"), UTC, project()).unwrap());
    root.clean().unwrap();
}
