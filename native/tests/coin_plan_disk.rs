//! Frozen plan persistence uses independently pinned source pixel hashes and grouping outputs.
use serde_json::{Value, json};
use std::path::Path;

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

#[test]
fn real_images_group_once_and_corrupt_frozen_plan_is_not_regenerated() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture_root = tooling_fixture::Root::new("coin frozen plan");
    let scratch = &fixture_root.0;
    let images = scratch.join("images");
    std::fs::create_dir(&images).unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-plan-disk.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "d5fe1edbd9ecf37cc59eada51b2108a756d49edece5b1fd47db4a1b325ec5bd3"
    );
    let source = root.join(fixture["input"].as_str().unwrap());
    use sha2::Digest;
    let bytes = std::fs::read(&source).unwrap();
    let hash = sha2::Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(hash, fixture["input_sha256"]);
    for name in ["aaa.png", "aaabsc.png"] {
        std::fs::write(images.join(name), &bytes).unwrap();
    }
    std::fs::write(images.join("empty.png"), b"").unwrap();
    std::fs::write(images.join("bad.png"), b"not an image").unwrap();
    std::fs::write(
        scratch.join("currency-emoji-inventory.md"),
        b"ticker: aaabsc\n",
    )
    .unwrap();
    std::fs::write(
        scratch.join("keywords.csv"),
        b"ticker,keywords\naaa,Alpha\naaabsc,BSC\n",
    )
    .unwrap();
    let expected = fixture["plan"].clone();
    let plan = _native::coin_plan_disk::build(scratch, &images).unwrap();
    assert_eq!(plan, expected);
    assert_eq!(_native::coin_plan_disk::load(scratch).unwrap(), expected);
    let groups = scratch.join("shared_logo_groups.json");
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&groups).unwrap()).unwrap(),
        fixture["writes"][1][1]
    );
    let frozen = scratch.join("rebuild_dedup_plan.json");
    let before = std::fs::read(&groups).unwrap();
    std::fs::write(&frozen, b"[{\"rep\":").unwrap();
    assert!(_native::coin_plan_disk::load(scratch).is_err());
    assert!(_native::coin_plan_disk::build(scratch, &images).is_err());
    assert_eq!(std::fs::read(&frozen).unwrap(), b"[{\"rep\":");
    assert_eq!(std::fs::read(&groups).unwrap(), before);
    assert!(_native::coin_plan_disk::validate(&json!([{"rep":"aaa"}])).is_err());
    assert!(_native::coin_plan_disk::validate(&json!([])).is_err());
    assert!(
        std::fs::read_dir(scratch).unwrap().all(|p| !p
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp"))
    );
    assert_eq!(std::fs::read(images.join("aaa.png")).unwrap(), bytes);
    assert_eq!(std::fs::read(images.join("aaabsc.png")).unwrap(), bytes);
    fixture_root.clean().unwrap();
}
