//! Private, test-only write-boundary observations; no production test API is exported.
use crate as runtime;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
#[allow(dead_code)]
#[path = "mod.rs"]
mod support;

enum Action {
    Maintenance(Arc<Mutex<Vec<&'static str>>>),
    WriteFailure,
}
static ACTIONS: OnceLock<Mutex<BTreeMap<PathBuf, Action>>> = OnceLock::new();
fn actions() -> &'static Mutex<BTreeMap<PathBuf, Action>> {
    ACTIONS.get_or_init(Default::default)
}
pub(crate) fn before_replace(path: &Path) {
    let action = actions().lock().unwrap().remove(path);
    match action {
        Some(Action::Maintenance(observed)) => {
            let data = path.parent().unwrap().to_path_buf();
            let contender = std::thread::spawn(move || {
                let state = if runtime::ownership::Ownership::acquire(
                    &data,
                    runtime::ownership::Mode::Maintenance,
                    support::UTC,
                )
                .is_err()
                {
                    "blocked"
                } else {
                    "entered"
                };
                observed.lock().unwrap().push(state);
            });
            contender.join().unwrap();
        }
        Some(Action::WriteFailure) => {
            assert!(!path.exists());
            std::fs::create_dir(path).unwrap();
            std::fs::write(path.join("foreign"), b"retain obstruction").unwrap();
        }
        None => {}
    }
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_plan_write_keeps_catalog_lease_and_blocks_maintenance
#[test]
fn intent_plan_write_keeps_catalog_lease_and_blocks_maintenance() {
    let f = support::Fixture::intent();
    let observed = Arc::new(Mutex::new(Vec::new()));
    actions()
        .lock()
        .unwrap()
        .insert(f.plan_path.clone(), Action::Maintenance(observed.clone()));
    assert_eq!(f.save().0, 200);
    assert_eq!(*observed.lock().unwrap(), vec!["blocked"]);
    assert!(!actions().lock().unwrap().contains_key(&f.plan_path));
}
// OriginalID: test_panel_intent_persistence.PanelIntentPersistence.test_plan_write_failure_is_not_acknowledged_and_exact_retry_completes
#[test]
fn intent_plan_write_failure_is_not_acknowledged_and_exact_retry_completes() {
    let f = support::Fixture::intent();
    actions()
        .lock()
        .unwrap()
        .insert(f.plan_path.clone(), Action::WriteFailure);
    let request = json!({"known":f.keys,"excluded":[f.keys[1]],"packs":[[f.keys[0],2]]});
    let (status, response) = f.post("/api/save", request.clone());
    assert_eq!(status, 503);
    assert_ne!(response.get("ok"), Some(&json!(true)));
    assert_eq!(
        std::fs::read(f.plan_path.join("foreign")).unwrap(),
        b"retain obstruction"
    );
    std::fs::remove_file(f.plan_path.join("foreign")).unwrap();
    std::fs::remove_dir(&f.plan_path).unwrap();
    assert_eq!(f.post("/api/save", request).0, 200);
    let plan = f.plan();
    assert!(
        plan["excluded"]
            .as_array()
            .unwrap()
            .contains(&json!(f.keys[1]))
    );
    assert_eq!(runtime::intent::targets(&plan).unwrap()[&f.keys[0]], 2);
    assert!(!actions().lock().unwrap().contains_key(&f.plan_path));
}

// Original-main Rust: native/src/panel_save_tests.rs @9c3cbb43305b.
// This is separate from the1516 original Python identities above.
#[test]
fn maintenance_is_excluded_until_plan_replacement_and_catalog_close() {
    let scratch = support::Scratch::new();
    let root = &scratch.0;
    let mut cat =
        runtime::catalog::Catalog::open(&root.join("catalog.db"), "fixture", root).unwrap();
    cat.insert(
        &json!({"content_key":"s:fixture","fmt":"static","file_path":"./art.png"}),
        "fixture",
    )
    .unwrap();
    let path = root.join("pack_plan.json");
    let observed = Arc::new(Mutex::new(Vec::new()));
    actions()
        .lock()
        .unwrap()
        .insert(path.clone(), Action::Maintenance(observed.clone()));
    let saved = runtime::panel_save::save(
        &mut cat,
        &json!({"known":["s:fixture"],"excluded":[],"packs":[["s:fixture",2]]}),
        &json!({"items":[],"published":[],"published_sets":{},"show_published":false,"keep_sets":{},"branded":false}),
        &path,
        "fixture",
    ).unwrap();
    assert_eq!(saved["status"], 200);
    assert_eq!(*observed.lock().unwrap(), vec!["blocked"]);
    assert!(!actions().lock().unwrap().contains_key(&path));
    let plan = runtime::panel_save::read_plan(&path).unwrap();
    assert_eq!(runtime::intent::targets(&plan).unwrap()["s:fixture"], 2);
    // A separate contender remains blocked even after save returns, until the catalog closes.
    let contender = root.clone();
    assert!(
        std::thread::spawn(move || runtime::ownership::Ownership::acquire(
            &contender,
            runtime::ownership::Mode::Maintenance,
            "fixture"
        )
        .is_err())
        .join()
        .unwrap()
    );
    drop(cat);
    let contender = root.clone();
    assert!(
        std::thread::spawn(move || runtime::ownership::Ownership::acquire(
            &contender,
            runtime::ownership::Mode::Maintenance,
            "fixture"
        )
        .is_ok())
        .join()
        .unwrap()
    );
}
