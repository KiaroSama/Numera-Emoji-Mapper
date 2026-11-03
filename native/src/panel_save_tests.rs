//! A competing maintenance thread must remain excluded at the durable plan replacement boundary.
use crate::{catalog::Catalog, panel_save};
use serde_json::json;
use std::cell::Cell;
use std::path::Path;
thread_local! {
    static CHECK: Cell<bool> = const { Cell::new(false) };
    static OBSTRUCT: Cell<bool> = const { Cell::new(false) };
}
pub(crate) fn before_replace(path: &Path) {
    if OBSTRUCT.replace(false) {
        std::fs::rename(path, path.with_file_name("retained-plan.json")).unwrap();
        std::fs::create_dir(path).unwrap();
        std::fs::write(path.join("foreign"), b"retain obstruction").unwrap();
    }
    if CHECK.get() {
        let data = path.parent().unwrap().to_path_buf();
        assert!(
            std::thread::spawn(move || crate::ownership::Ownership::acquire(
                &data,
                crate::ownership::Mode::Maintenance,
                "fixture"
            )
            .is_err())
            .join()
            .unwrap(),
            "save released its catalog lease before replacing the plan"
        );
    }
}
#[test]
fn original_plan_write_failure_preserves_evidence_and_exact_retry_completes() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent = crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let root = parent.join(format!("save-write-refusal-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("pack_plan.json");
    let result = std::panic::catch_unwind(|| {
        let mut cat = Catalog::open(&root.join("catalog.db"), "fixture", project).unwrap();
        cat.insert(
            &json!({"content_key":"s:fixture","fmt":"static","file_path":"./art.png"}),
            "fixture",
        )
        .unwrap();
        let original = json!({"version":1,"moves":[],"held":[],"targets":[]});
        crate::atomic::write_json(&path, &original).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let request =
            json!({"known":["s:fixture"],"excluded":["s:fixture"],"packs":[["s:fixture",2]]});
        let options = json!({"keep_sets":{},"show_published":false,"branded":false});
        OBSTRUCT.set(true);
        assert!(panel_save::save(&mut cat, &request, &options, &path, "fixture").is_err());
        assert!(!OBSTRUCT.get());
        assert_eq!(
            std::fs::read(root.join("retained-plan.json")).unwrap(),
            bytes
        );
        assert_eq!(
            std::fs::read(path.join("foreign")).unwrap(),
            b"retain obstruction"
        );
        assert_eq!(std::fs::read_dir(&path).unwrap().count(), 1);
        assert!(
            !cat.get("s:fixture").unwrap().unwrap()["included"]
                .as_bool()
                .unwrap()
        );
        std::fs::remove_file(path.join("foreign")).unwrap();
        std::fs::remove_dir(&path).unwrap();
        std::fs::rename(root.join("retained-plan.json"), &path).unwrap();
        let saved = panel_save::save(&mut cat, &request, &options, &path, "fixture").unwrap();
        assert_eq!(saved["status"], 200);
        assert_eq!(saved["body"]["ok"], true);
        assert_eq!(saved["body"]["excluded"], 1);
        let plan = panel_save::read_plan(&path).unwrap();
        assert_eq!(plan["targets"], json!([["s:fixture", 2]]));
        assert_eq!(plan["excluded"], json!(["s:fixture"]));
        drop(cat);
        let reopened = Catalog::open(&root.join("catalog.db"), "fixture", project).unwrap();
        assert_eq!(reopened.all(None).unwrap().len(), 1);
        assert_eq!(
            reopened.get("s:fixture").unwrap().unwrap()["included"],
            false
        );
    });
    OBSTRUCT.set(false);
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    assert!(!crate::process_owner::cleanup_pending(&root));
    for entry in std::fs::read_dir(&root).unwrap() {
        let name = entry.unwrap().file_name();
        assert!(
            ["catalog.db", "pack_plan.json", ".maintenance.lock"]
                .iter()
                .any(|allowed| name == *allowed)
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn maintenance_is_excluded_until_plan_replacement_and_catalog_close() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent = crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let root = parent.join(format!("save-lease-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let result = std::panic::catch_unwind(|| {
        let mut cat = Catalog::open(&root.join("catalog.db"), "fixture", &root).unwrap();
        cat.insert(
            &json!({"content_key":"s:fixture","fmt":"static","file_path":"./art.png"}),
            "fixture",
        )
        .unwrap();
        CHECK.set(true);
        let saved = panel_save::save(&mut cat, &json!({"known":["s:fixture"],"excluded":[],"packs":[["s:fixture",2]]}),
            &json!({"items":[],"published":[],"published_sets":{},"show_published":false,"keep_sets":{},"branded":false}),
            &root.join("pack_plan.json"), "fixture").unwrap();
        CHECK.set(false);
        assert_eq!(saved["status"], 200);
        let plan = panel_save::read_plan(&root.join("pack_plan.json")).unwrap();
        assert_eq!(crate::intent::targets(&plan).unwrap()["s:fixture"], 2);
        drop(cat);
        let data = root.clone();
        assert!(
            std::thread::spawn(move || crate::ownership::Ownership::acquire(
                &data,
                crate::ownership::Mode::Maintenance,
                "fixture"
            )
            .is_ok())
            .join()
            .unwrap()
        );
    });
    CHECK.set(false);
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    assert!(!crate::process_owner::cleanup_pending(&root));
    std::fs::remove_dir_all(&root).unwrap();
}
