//! Physical per-run sources survive unknown intent and promote only their recorded identity.
use serde_json::json;
use std::path::PathBuf;

#[test]
fn separate_runs_matching_promotion_and_pending_source_retention_are_physical() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let scratch = root
        .join("logs/tmp")
        .join(format!("provider staging {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let published = scratch.join("logos/emoji");
    let mut first = _native::coin_provider_stage::Stage::new(&published).unwrap();
    let mut second = _native::coin_provider_stage::Stage::new(&published).unwrap();
    assert_ne!(first.directory(), second.directory());
    let source = root.join("tests/fixtures/native/static-codec/alpha-edge.png");
    let bytes = std::fs::read(&source).unwrap();
    let staged = first.source("aaa").unwrap();
    std::fs::write(&staged, &bytes).unwrap();
    let hash = _native::codec_image::source_dhash(&staged).unwrap();
    std::fs::create_dir_all(&published).unwrap();
    let destination = published.join("aaa.png");
    std::fs::write(&destination, b"previous published source").unwrap();
    assert!(!first.promote(&staged, "aaa", hash ^ u64::MAX).unwrap());
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"previous published source"
    );
    let other = first.source("bbb").unwrap();
    std::fs::write(&other, &bytes).unwrap();
    first.clean(Some(&staged)).unwrap();
    assert!(staged.is_file() && !other.exists());
    let directory = first.directory().to_path_buf();
    drop(first);
    assert!(
        staged.is_file(),
        "unknown intent's only source must survive owner exit"
    );
    let mut later = _native::coin_provider_stage::Stage::new(&published).unwrap();
    assert!(later.promote(&staged, "aaa", hash).unwrap());
    assert_eq!(std::fs::read(&destination).unwrap(), bytes);
    assert!(!staged.exists());
    assert!(later.source("../aaa").is_err());
    assert!(later.source("AAA").is_err());
    let absent = json!({"source_path":staged,"key":"aaa"});
    assert_eq!(later.for_intent(&absent).unwrap(), destination);
    later.clean(None).unwrap();
    second.clean(None).unwrap();
    drop(later);
    drop(second);
    std::fs::remove_dir(directory).unwrap();
    std::fs::remove_file(destination).unwrap();
    std::fs::remove_dir(published).unwrap();
    std::fs::remove_dir(scratch.join("logos/.incoming")).unwrap();
    std::fs::remove_dir(scratch.join("logos")).unwrap();
    std::fs::remove_dir(scratch).unwrap();
}
