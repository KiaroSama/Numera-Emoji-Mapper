use crate::support::{
    http::{Reply, Server},
    *,
};
use image::{Rgba, RgbaImage};
use numera_runtime::{self as native, reconcile::NearMatch};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
pub fn png(path: &Path, color: [u8; 4]) -> PathBuf {
    write(
        path,
        native::native_raster::encode_png(&RgbaImage::from_pixel(100, 100, Rgba(color))).unwrap(),
    );
    path.into()
}
pub fn art(f: &Fixture, name: &str, out: &str) -> PathBuf {
    let path = f.root.join(out);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    native::codec_image::to_static_png(&project().join(format!("assets/{name}")), &path).unwrap();
    let reference = frozen_art(name);
    assert_eq!(
        native::codec_image::decoded(&path).unwrap().rgba,
        native::codec_image::decoded(&reference).unwrap().rgba
    );
    path
}
pub fn frozen_art(name: &str) -> PathBuf {
    let folder = project().join("runtime/tests/baseline_cli_status_support/fixtures");
    let manifest = read_json(&folder.join("provenance.json"));
    assert_eq!(manifest["Pillow"], "12.3.0");
    let case = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["output"] == name)
        .unwrap();
    assert_eq!(
        native::package::digest(&project().join(case["input"].as_str().unwrap())).unwrap(),
        case["input_sha256"]
    );
    let path = folder.join(name);
    assert_eq!(
        native::package::digest(&path).unwrap(),
        case["output_sha256"]
    );
    let rgba = native::codec_image::decoded(&path).unwrap().rgba;
    assert_eq!(rgba.dimensions(), (100, 100));
    assert_eq!(
        native::native_raster::digest_hex(rgba.as_raw()),
        case["rgba_sha256"]
    );
    path
}
pub fn fingerprint(path: &Path) -> Value {
    native::codec_image::fingerprint_typed(path).unwrap()
}
pub fn insert(cat: &mut native::catalog::Catalog, path: &Path, extra: Value) -> String {
    let mut item = fingerprint(path);
    let key = item["content_key"].as_str().unwrap().to_owned();
    item["fmt"] = json!("static");
    item["file_path"] = json!(path);
    for (k, v) in extra.as_object().unwrap() {
        item[k] = v.clone();
    }
    assert!(cat.insert(&item, UTC).unwrap());
    key
}
struct Near {
    cat: native::catalog::Catalog,
    f: Fixture,
    a: PathBuf,
    b: PathBuf,
    probe: PathBuf,
    keys: Vec<String>,
}
impl Near {
    fn new(two: bool) -> Self {
        let f = Fixture::new();
        let a = png(&f.root.join("a.png"), [245, 0, 0, 255]);
        let b = png(&f.root.join("b.png"), [250, 0, 0, 255]);
        let probe = png(&f.root.join("probe.png"), [248, 0, 0, 255]);
        let mut cat = f.catalog();
        let mut keys = vec![insert(&mut cat, &a, json!({}))];
        if two {
            keys.push(insert(&mut cat, &b, json!({})));
        }
        Self {
            f,
            cat,
            a,
            b,
            probe,
            keys,
        }
    }
    fn near(&self, probe: &Path, fail_b: bool, unknown_probe: bool) -> Result<NearMatch, String> {
        let fp = fingerprint(probe);
        let items = self.cat.all(None)?;
        runtime().block_on(native::reconcile::near_catalog(
            &items,
            "static",
            if unknown_probe {
                None
            } else {
                fp["phash"].as_u64()
            },
            fp["content_key"].as_str().unwrap(),
            async |item: &Value| {
                let path = Path::new(item["file_path"].as_str().unwrap());
                Ok(if fail_b && path == self.b {
                    None
                } else {
                    native::codec_image::same_image(path, probe)
                })
            },
        ))
    }
    fn unhash(&self) {
        Connection::open(self.f.data().join("catalog.db"))
            .unwrap()
            .execute("UPDATE items SET phash=NULL", [])
            .unwrap();
    }
}
// tests.test_reconcile_uniqueness.UniqueRequiresExcludingEveryRival.test_two_readable_rivals_are_ambiguous
#[test]
fn two_readable_rivals_are_ambiguous() {
    let n = Near::new(true);
    assert_eq!(
        n.near(&n.probe, false, false).unwrap(),
        NearMatch::Ambiguous
    );
}
// tests.test_reconcile_uniqueness.UniqueRequiresExcludingEveryRival.test_deleting_one_rivals_file_does_not_make_the_other_unique
#[test]
fn missing_rival_does_not_promote_survivor_to_unique() {
    let n = Near::new(true);
    std::fs::remove_file(&n.b).unwrap();
    assert_eq!(
        n.near(&n.probe, false, false).unwrap(),
        NearMatch::Undecidable
    );
}
// tests.test_reconcile_uniqueness.UniqueRequiresExcludingEveryRival.test_a_rival_that_fails_to_compare_blocks_uniqueness_too
#[test]
fn unknown_comparison_blocks_uniqueness() {
    let n = Near::new(true);
    assert_eq!(
        n.near(&n.probe, true, false).unwrap(),
        NearMatch::Undecidable
    );
}
// tests.test_reconcile_uniqueness.UniqueRequiresExcludingEveryRival.test_one_verified_and_no_rival_is_still_unique
#[test]
fn sole_verified_candidate_remains_unique() {
    let n = Near::new(false);
    assert_eq!(
        n.near(&n.a, false, false).unwrap(),
        NearMatch::Unique(n.keys[0].clone())
    );
}
// tests.test_reconcile_uniqueness.UniqueRequiresExcludingEveryRival.test_a_true_negative_is_still_a_negative
#[test]
fn proven_negative_remains_missing() {
    let n = Near::new(false);
    let blue = png(&n.f.root.join("blue.png"), [0, 0, 255, 255]);
    assert_eq!(n.near(&blue, false, false).unwrap(), NearMatch::Missing);
}
// tests.test_reconcile_uniqueness.UniqueRequiresExcludingEveryRival.test_an_item_with_no_stored_hash_is_compared_not_skipped
#[test]
fn unhashed_candidate_is_compared_and_excluded() {
    let n = Near::new(false);
    n.unhash();
    let blue = png(&n.f.root.join("blue.png"), [0, 0, 255, 255]);
    assert_eq!(n.near(&blue, false, false).unwrap(), NearMatch::Missing);
}
// tests.test_reconcile_uniqueness.UniqueRequiresExcludingEveryRival.test_an_unhashed_item_we_cannot_read_is_a_live_rival
#[test]
fn missing_unhashed_candidate_remains_live_rival() {
    let n = Near::new(false);
    n.unhash();
    std::fs::remove_file(&n.a).unwrap();
    let blue = png(&n.f.root.join("blue.png"), [0, 0, 255, 255]);
    assert_eq!(n.near(&blue, false, false).unwrap(), NearMatch::Undecidable);
}
// tests.test_reconcile_uniqueness.UniqueRequiresExcludingEveryRival.test_an_unhashed_item_can_still_be_verified_as_the_match
#[test]
fn unhashed_match_can_still_be_verified() {
    let n = Near::new(false);
    n.unhash();
    assert_eq!(
        n.near(&n.a, false, false).unwrap(),
        NearMatch::Unique(n.keys[0].clone())
    );
}
// tests.test_reconcile_uniqueness.UniqueRequiresExcludingEveryRival.test_a_failed_candidate_enumeration_is_undecidable
#[test]
fn failed_candidate_enumeration_cannot_authorize_attribution() {
    let mut n = Near::new(false);
    let db = Connection::open(n.f.data().join("catalog.db")).unwrap();
    db.execute("UPDATE items SET emojis='{invalid'", [])
        .unwrap();
    let error = n.near(&n.a, false, false).unwrap_err();
    assert!(!error.is_empty());
    let bytes = std::fs::read(&n.a).unwrap();
    let bot = Server::new(move |r| {
        if r.verb == "GET" {
            Reply::Bytes(200, bytes.clone())
        } else {
            assert_eq!(r.method(), "getFile");
            Reply::ok(json!({"file_path":"probe.png"}))
        }
    });
    let tg = bot.telegram();
    let rt = runtime();
    let media = native::media_native_adapter::NativeMediaAdapter {
        executable: n.f.binary.clone(),
        root: n.f.root.clone(),
        scratch: n.f.data().join("tmp/native-codec"),
    };
    let mut publisher = native::publication::Publisher {
        tg: &tg,
        cat: &mut n.cat,
        media: &media,
        data: n.f.data(),
        runtime: &rt,
        config: None,
    };
    assert!(
        publisher
            .resolve(&json!({"file_id":"fid","file_unique_id":"enumeration-probe"}))
            .is_err()
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM seen_files", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM publications", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
// tests.test_reconcile_uniqueness.UniqueRequiresExcludingEveryRival.test_an_undecidable_probe_is_not_a_miss
#[test]
fn undecidable_probe_is_not_missing() {
    let n = Near::new(false);
    assert_eq!(n.near(&n.a, false, true).unwrap(), NearMatch::Undecidable);
}
// tests.test_reconcile_uniqueness.NothingAmbiguousIsEverWritten.test_an_unprovable_attribution_writes_no_fuid
#[test]
fn resolver_unknown_rival_writes_no_seen_or_publication_after_reopen() {
    let mut n = Near::new(true);
    std::fs::remove_file(&n.b).unwrap();
    let bytes = std::fs::read(&n.probe).unwrap();
    let bot = Server::new(move |r| {
        if r.verb == "GET" {
            Reply::Bytes(200, bytes.clone())
        } else {
            assert_eq!(r.method(), "getFile");
            Reply::ok(json!({"file_path":"probe.png"}))
        }
    });
    let tg = bot.telegram();
    let media = native::media_native_adapter::NativeMediaAdapter {
        executable: n.f.binary.clone(),
        root: n.f.root.clone(),
        scratch: n.f.data().join("tmp/native-codec"),
    };
    let rt = runtime();
    let mut publisher = native::publication::Publisher {
        tg: &tg,
        cat: &mut n.cat,
        media: &media,
        data: n.f.data(),
        runtime: &rt,
        config: None,
    };
    assert!(
        publisher
            .resolve(&json!({"file_unique_id":"fuid-probe","file_id":"fid"}))
            .is_err()
    );
    drop(n.cat);
    let db = Connection::open(n.f.data().join("catalog.db")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM seen_files WHERE file_unique_id='fuid-probe'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM publications", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
