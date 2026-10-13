//! Frozen plan persistence uses independently pinned source pixel hashes and grouping outputs.
use serde_json::{Value, json};
use std::path::PathBuf;
#[test]
fn real_images_group_once_and_corrupt_frozen_plan_is_not_regenerated() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin plan disk {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let images = scratch.join("images");
        std::fs::create_dir(&images).unwrap();
        let image_fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native/coin-audit-image.json"
        ))
        .unwrap();
        let case = &image_fixture["cases"][2];
        let source = root.join(case["input"].as_str().unwrap());
        std::fs::copy(&source, images.join("aaa.png")).unwrap();
        std::fs::copy(&source, images.join("aaabsc.png")).unwrap();
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
        let expected = json!([{"rep":"aaabsc","tickers":["aaabsc","aaa"],"kw":"BSC, Alpha","hash":case["expected"]["hash"]}]);
        let plan = _native::coin_plan_disk::build(&scratch, &images).unwrap();
        assert_eq!(plan, expected);
        assert_eq!(_native::coin_plan_disk::load(&scratch).unwrap(), expected);
        let groups = scratch.join("shared_logo_groups.json");
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&groups).unwrap()).unwrap(),
            json!({"aaabsc":["aaabsc","aaa"]})
        );
        let frozen = scratch.join("rebuild_dedup_plan.json");
        let before = std::fs::read(&groups).unwrap();
        std::fs::write(&frozen, b"[{\"rep\":").unwrap();
        assert!(_native::coin_plan_disk::load(&scratch).is_err());
        assert!(_native::coin_plan_disk::build(&scratch, &images).is_err());
        assert_eq!(std::fs::read(&frozen).unwrap(), b"[{\"rep\":");
        assert_eq!(std::fs::read(&groups).unwrap(), before);
        assert!(_native::coin_plan_disk::validate(&json!([{"rep":"aaa"}])).is_err());
        assert!(_native::coin_plan_disk::validate(&json!([])).is_err());
        assert!(
            std::fs::read_dir(&scratch).unwrap().all(|p| !p
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
        );
    }));
    let cleaned = std::fs::remove_dir_all(&scratch);
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    cleaned.unwrap();
}
