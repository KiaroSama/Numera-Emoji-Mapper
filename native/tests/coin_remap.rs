//! Guarded replacement preserves the actual old map and never writes during dry/refused work.
use _native::coin_remap_policy::{Match, Options, decide};
use serde_json::{Value, json};
use std::path::PathBuf;
#[test]
fn durable_remap_keeps_backup_and_refused_candidates_separate_from_canonical_map() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_owned();
    let scratch = root
        .join("logs/tmp")
        .join(format!("coin remap writes {}", std::process::id()));
    std::fs::create_dir(&scratch).unwrap();
    let output = scratch.join("ticker_to_id.json");
    let candidate = scratch.join("candidate.json");
    let old = b"{\"aaa\":\"old\",\"other\":\"keep only in backup\"}\n";
    std::fs::write(&output, old).unwrap();
    let matches = [Match {
        ticker: "aaa".into(),
        cid: "new".into(),
        distance: 0.0,
        margin: 200.0,
    }];
    let options = Options {
        apply: false,
        max_distance: Some(100.0),
        min_margin: None,
        incomplete_sets: 0,
        failed_images: 0,
    };
    let dry = decide(&matches, options).unwrap();
    _native::coin_remap::persist(&scratch, &output, &candidate, &dry).unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), old);
    assert!(!candidate.exists());
    let refused = decide(
        &matches,
        Options {
            apply: true,
            incomplete_sets: 1,
            ..options
        },
    )
    .unwrap();
    _native::coin_remap::persist(&scratch, &output, &candidate, &refused).unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), old);
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&candidate).unwrap()).unwrap(),
        json!({"aaa":"new"})
    );
    assert!(!output.with_extension("prebroken.json").exists());
    let admitted = decide(
        &matches,
        Options {
            apply: true,
            ..options
        },
    )
    .unwrap();
    let lock =
        _native::locks::Lease::acquire(&scratch.join(".locks/canonical_map.lock"), "fixture")
            .unwrap();
    assert!(_native::coin_remap::persist(&scratch, &output, &candidate, &admitted).is_err());
    assert_eq!(std::fs::read(&output).unwrap(), old);
    drop(lock);
    _native::coin_remap::persist(&scratch, &output, &candidate, &admitted).unwrap();
    assert_eq!(
        std::fs::read(output.with_extension("prebroken.json")).unwrap(),
        old
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(&output).unwrap()).unwrap(),
        json!({"aaa":"new"})
    );
    _native::coin_remap::persist(&scratch, &output, &candidate, &admitted).unwrap();
    assert_eq!(
        std::fs::read(output.with_extension("prebroken.json")).unwrap(),
        old
    );
    assert!(!std::fs::read_dir(&scratch).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".video-output-")
    }));
    std::fs::remove_dir_all(scratch).unwrap();
}
