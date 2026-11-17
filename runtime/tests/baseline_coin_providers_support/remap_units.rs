//! Actual whole remap run must reject short math results before canonical persistence.
use crate as rt;
use crate::baseline_coin_provider_support as support;
use std::collections::BTreeMap;
use support::*;
// tests.test_remap_ids.MainApplyTest.test_a_short_result_from_nearest_raises_instead_of_mapping_fewer
#[test]
fn test_a_short_result_from_nearest_raises_instead_of_mapping_fewer() {
    let f = Remap::new();
    f.local("btc", &block([200, 20, 20, 255]));
    f.local("eth", &block([20, 200, 20, 255]));
    let blobs = BTreeMap::from([
        ("a".into(), block([200, 20, 20, 255])),
        ("b".into(), block([20, 200, 20, 255])),
    ]);
    let server = Server::new(remap_script(&["a", "b"], &blobs, &[]));
    let mut reached = false;
    let result = super::run_with_nearest(
        &f.files,
        &server.telegram(),
        rt::coin_remap_policy::Options {
            apply: true,
            max_distance: Some(100.0),
            min_margin: None,
            incomplete_sets: 0,
            failed_images: 0,
        },
        &f.root.log(),
        |local, live| {
            reached = true;
            let mut actual = rt::coin_remap_math::nearest(local, live)?;
            assert_eq!(actual.indices.len(), 2);
            actual.indices.truncate(1);
            actual.squared.truncate(1);
            actual.second.truncate(1);
            Ok(actual)
        },
    );
    assert!(
        reached,
        "injection must run after actual live/local signatures"
    );
    assert!(
        result
            .unwrap_err()
            .contains("parallel match arrays disagree")
    );
    assert!(!f.files.output.exists(), "short map must not be written");
    assert!(!f.files.candidates.exists());
    assert_eq!(doc(&f.files.cache)["sigs"].as_object().unwrap().len(), 2);
    let requests = server.finish();
    assert_eq!(requests.iter().filter(|r| r.method == "getFile").count(), 2);
    assert!(requests.iter().all(|r| !matches!(
        r.method.as_str(),
        "addStickerToSet" | "replaceStickerInSet" | "createNewStickerSet"
    )));
}

// tests.test_remap_ids.MainApplyTest.test_apply_refuses_to_overwrite_canonical_map_from_incomplete_data
#[test]
fn test_apply_refuses_to_overwrite_canonical_map_from_incomplete_data() {
    use std::sync::{Arc, Mutex};
    let f = Remap::new();
    write(&f.files.output, &serde_json::json!({"btc":"GOOD-OLD"}));
    f.local("btc", &block([200, 20, 20, 255]));
    let mut script = vec![list_ids(&["a", "b"])];
    download(&mut script, "fid-a", &block([200, 20, 20, 255]));
    script.push(reply(
        "getFile",
        ok(serde_json::json!({"file_path":"fid-b.png"})),
    ));
    for _ in 0..5 {
        script.push(fault("GET fid-b.png"));
    }
    let server = Server::new(script);
    let waits = Arc::new(Mutex::new(vec![]));
    let seen = waits.clone();
    let tg = server.telegram().with_download_clock(Arc::new(move |wait| {
        seen.lock().unwrap().push(wait.as_secs())
    }));
    let code = rt::coin_remap::run(
        &f.files,
        &tg,
        rt::coin_remap_policy::Options {
            apply: true,
            max_distance: Some(100.0),
            min_margin: None,
            incomplete_sets: 0,
            failed_images: 0,
        },
        &f.root.log(),
    )
    .unwrap();
    assert_eq!(code, 3);
    assert_eq!(doc(&f.files.output), serde_json::json!({"btc":"GOOD-OLD"}));
    assert_eq!(doc(&f.files.candidates), serde_json::json!({"btc":"a"}));
    assert!(!f.files.output.with_extension("prebroken.json").exists());
    assert_eq!(*waits.lock().unwrap(), vec![3, 6, 9, 12]);
    let requests = server.finish();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "GET fid-b.png")
            .count(),
        5
    );
    assert_eq!(requests.iter().filter(|r| r.method == "getFile").count(), 2);
    let error = doc(&f.files.cache)["errors"]["b"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!error.is_empty());
    assert!(!error.contains(TOKEN));
}
