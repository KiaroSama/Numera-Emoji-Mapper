use crate::support::*;
use numera_runtime::{
    catalog::Catalog,
    media_adapter::MediaCodec,
    publication::Publisher,
    reconcile::{self, NearMatch},
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
struct ReconcileFixture {
    // Close SQLite and its OS leases before removing fixture files on Windows.
    cat: Catalog,
    paths: Vec<PathBuf>,
    root: Scratch,
}
impl ReconcileFixture {
    fn new(colors: &[[u8; 4]]) -> Self {
        let root = Scratch::new();
        let mut cat = Catalog::open(&root.0.join("catalog.db"), UTC, &root.0).unwrap();
        let paths=colors.iter().enumerate().map(|(i,c)|{let path=png(&root.0.join(format!("item{i}.png")),*c);let fp=runtime().block_on(Images.call(json!({"operation":"fingerprint","source":path}))).unwrap();cat.insert(&json!({"content_key":fp["content_key"],"fmt":"static","file_path":path,"phash":fp["phash"]}),UTC).unwrap();path}).collect();
        Self { root, cat, paths }
    }
    fn key(&self, n: usize) -> String {
        numera_runtime::codec_image::fingerprint_typed(&self.paths[n]).unwrap()["content_key"]
            .as_str()
            .unwrap()
            .into()
    }
    fn near(&self, probe: &Path, fmt: &str) -> NearMatch {
        let fp = runtime()
            .block_on(Images.call(json!({"operation":"fingerprint","source":probe})))
            .ok();
        let items = self.cat.all(Some(fmt)).unwrap();
        runtime()
            .block_on(reconcile::near_catalog(
                &items,
                fmt,
                fp.as_ref().and_then(|v| v["phash"].as_u64()),
                fp.as_ref()
                    .and_then(|v| v["content_key"].as_str())
                    .unwrap_or(""),
                async |item: &Value| {
                    let p = Path::new(item["file_path"].as_str().unwrap());
                    if !p.is_file() {
                        Ok(None)
                    } else {
                        Ok(Images
                            .call(json!({"operation":"same_image","source":p,"other":probe}))
                            .await
                            .ok()
                            .and_then(|v| v.as_bool()))
                    }
                },
            ))
            .unwrap()
    }
    fn resolve(&mut self, probe: &Path) -> Result<Option<String>, String> {
        let mut model = Model::ordinary();
        model
            .bodies
            .insert("served".into(), std::fs::read(probe).unwrap());
        let bot = Bot::new(model);
        let tg = bot.telegram();
        let rt = runtime();
        let result = Publisher {
            tg: &tg,
            cat: &mut self.cat,
            media: &Images,
            data: self.root.0.clone(),
            runtime: &rt,
            config: None,
        }
        .resolve(&json!({"file_unique_id":"served","file_id":"f-served"}));
        assert_eq!(bot.edit(|m| m.downloaded.clone()), vec!["served"]);
        let tmp = self.root.0.join("tmp");
        assert!(!tmp.exists() || files(&tmp).is_empty());
        result
    }
}

// Original: tests.test_reconcile_identity.NearCatalogMatchVerifiesBeforeItAttributes.test_a_candidate_that_cannot_be_examined_is_undecidable
#[test]
fn nearcatalogmatchverifiesbeforeitattributes_test_a_candidate_that_cannot_be_examined_is_undecidable()
 {
    let f = ReconcileFixture::new(&[[255, 0, 0, 255]]);
    std::fs::remove_file(&f.paths[0]).unwrap();
    let blue = png(&f.root.0.join("blue.png"), [0, 0, 255, 255]);
    assert_eq!(f.near(&blue, "static"), NearMatch::Undecidable);
}

// Original: tests.test_reconcile_identity.NearCatalogMatchVerifiesBeforeItAttributes.test_a_foreign_image_of_the_same_shape_is_not_our_item
#[test]
fn nearcatalogmatchverifiesbeforeitattributes_test_a_foreign_image_of_the_same_shape_is_not_our_item()
 {
    let f = ReconcileFixture::new(&[[255, 0, 0, 255]]);
    let blue = png(&f.root.0.join("blue.png"), [0, 0, 255, 255]);
    let a = numera_runtime::codec_image::fingerprint_typed(&f.paths[0]).unwrap();
    let b = numera_runtime::codec_image::fingerprint_typed(&blue).unwrap();
    assert_eq!(a["phash"], b["phash"]);
    assert_eq!(f.near(&blue, "static"), NearMatch::Missing);
}

// Original: tests.test_reconcile_identity.NearCatalogMatchVerifiesBeforeItAttributes.test_a_genuine_re_encode_of_our_art_still_resolves
#[test]
fn nearcatalogmatchverifiesbeforeitattributes_test_a_genuine_re_encode_of_our_art_still_resolves() {
    let f = ReconcileFixture::new(&[[255, 0, 0, 255]]);
    let webp = f.root.0.join("red_reencoded.webp");
    let image = image::open(&f.paths[0]).unwrap().to_rgba8();
    let encoded = numera_runtime::native_raster::encode_webp(
        &image,
        numera_runtime::native_raster::WebpOptions {
            lossless: false,
            quality: 88.0,
            ..Default::default()
        },
    )
    .unwrap();
    std::fs::write(&webp, encoded).unwrap();
    assert_ne!(
        numera_runtime::codec_image::fingerprint_typed(&webp).unwrap()["content_key"],
        json!(f.key(0))
    );
    assert_eq!(f.near(&webp, "static"), NearMatch::Unique(f.key(0)));
}

// Original: tests.test_reconcile_identity.NearCatalogMatchVerifiesBeforeItAttributes.test_an_empty_catalog_is_a_proven_miss
#[test]
fn nearcatalogmatchverifiesbeforeitattributes_test_an_empty_catalog_is_a_proven_miss() {
    let f = ReconcileFixture::new(&[]);
    let blue = png(&f.root.0.join("blue.png"), [0, 0, 255, 255]);
    assert_eq!(f.near(&blue, "static"), NearMatch::Missing);
}

// Original: tests.test_reconcile_identity.NearCatalogMatchVerifiesBeforeItAttributes.test_an_undecodable_raster_is_undecidable_not_a_miss
#[test]
fn nearcatalogmatchverifiesbeforeitattributes_test_an_undecodable_raster_is_undecidable_not_a_miss()
{
    let f = ReconcileFixture::new(&[]);
    let p = f.root.0.join("broken.png");
    let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
    b.extend_from_slice(&b"corrupt".repeat(8));
    std::fs::write(&p, b).unwrap();
    assert_eq!(f.near(&p, "static"), NearMatch::Undecidable);
}

// Original: tests.test_reconcile_identity.NearCatalogMatchVerifiesBeforeItAttributes.test_animated_keeps_its_old_meaning
#[test]
fn nearcatalogmatchverifiesbeforeitattributes_test_animated_keeps_its_old_meaning() {
    let f = ReconcileFixture::new(&[]);
    let p = f.root.0.join("x.tgs");
    let mut bytes = vec![0x1f, 0x8b];
    bytes.extend_from_slice(&[b'0'; 40]);
    std::fs::write(&p, bytes).unwrap();
    assert_eq!(f.near(&p, "animated"), NearMatch::Missing);
}

// Original: tests.test_reconcile_identity.NearCatalogMatchVerifiesBeforeItAttributes.test_our_own_file_still_resolves
#[test]
fn nearcatalogmatchverifiesbeforeitattributes_test_our_own_file_still_resolves() {
    let f = ReconcileFixture::new(&[[255, 0, 0, 255]]);
    assert_eq!(f.near(&f.paths[0], "static"), NearMatch::Unique(f.key(0)));
}

// Original: tests.test_reconcile_identity.NearCatalogMatchVerifiesBeforeItAttributes.test_two_genuine_matches_are_ambiguous_not_a_pick
#[test]
fn nearcatalogmatchverifiesbeforeitattributes_test_two_genuine_matches_are_ambiguous_not_a_pick() {
    let f = ReconcileFixture::new(&[[200, 0, 0, 255], [206, 0, 0, 255]]);
    let probe = png(&f.root.0.join("probe.png"), [203, 0, 0, 255]);
    assert_eq!(f.cat.all(None).unwrap().len(), 2);
    assert_eq!(f.near(&probe, "static"), NearMatch::Ambiguous);
}

// Original: tests.test_reconcile_identity.TheResolverActsOnEachAnswerDifferently.test_a_foreign_sticker_resolves_to_nothing
#[test]
fn theresolveractsoneachanswerdifferently_test_a_foreign_sticker_resolves_to_nothing() {
    let mut f = ReconcileFixture::new(&[[255, 0, 0, 255]]);
    let blue = png(&f.root.0.join("blue.png"), [0, 0, 255, 255]);
    assert_eq!(f.resolve(&blue).unwrap(), None);
    assert_eq!(
        f.cat
            .dispatch(json!({"operation":"seen","file_unique_id":"served"}))
            .unwrap(),
        Value::Null
    );
}

// Original: tests.test_reconcile_identity.TheResolverActsOnEachAnswerDifferently.test_ambiguity_stops_the_run_instead_of_guessing
#[test]
fn theresolveractsoneachanswerdifferently_test_ambiguity_stops_the_run_instead_of_guessing() {
    let mut f = ReconcileFixture::new(&[[200, 0, 0, 255], [206, 0, 0, 255]]);
    let probe = png(&f.root.0.join("probe.png"), [203, 0, 0, 255]);
    assert!(
        f.resolve(&probe)
            .unwrap_err()
            .contains("more than one catalog item")
    );
    assert_eq!(
        f.cat
            .dispatch(json!({"operation":"seen","file_unique_id":"served"}))
            .unwrap(),
        Value::Null
    );
}

// Original: tests.test_reconcile_identity.TheResolverActsOnEachAnswerDifferently.test_an_unexaminable_candidate_stops_the_run_too
#[test]
fn theresolveractsoneachanswerdifferently_test_an_unexaminable_candidate_stops_the_run_too() {
    let mut f = ReconcileFixture::new(&[[255, 0, 0, 255]]);
    std::fs::remove_file(&f.paths[0]).unwrap();
    let blue = png(&f.root.0.join("blue.png"), [0, 0, 255, 255]);
    assert!(
        f.resolve(&blue)
            .unwrap_err()
            .contains("unexamined catalog rival")
    );
    assert_eq!(
        f.cat
            .dispatch(json!({"operation":"seen","file_unique_id":"served"}))
            .unwrap(),
        Value::Null
    );
}

// Original: tests.test_reconcile_identity.TheResolverActsOnEachAnswerDifferently.test_our_own_sticker_resolves_to_our_key
#[test]
fn theresolveractsoneachanswerdifferently_test_our_own_sticker_resolves_to_our_key() {
    let mut f = ReconcileFixture::new(&[[255, 0, 0, 255]]);
    let path = f.paths[0].clone();
    let key = f.key(0);
    assert_eq!(f.resolve(&path).unwrap(), Some(key.clone()));
    assert_eq!(
        f.cat
            .dispatch(json!({"operation":"seen","file_unique_id":"served"}))
            .unwrap(),
        json!(key)
    );
}

// Original: tests.test_reconcile_identity.TheThreeNonKeyAnswersStayApart.test_they_are_distinct_values
#[test]
fn thethreenonkeyanswersstayapart_test_they_are_distinct_values() {
    assert_ne!(NearMatch::Missing, NearMatch::Ambiguous);
    assert_ne!(NearMatch::Missing, NearMatch::Undecidable);
    assert_ne!(NearMatch::Ambiguous, NearMatch::Undecidable);
}
