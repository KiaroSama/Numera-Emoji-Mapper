//! A competing maintenance thread must remain excluded at the durable plan replacement boundary.
use crate::{catalog::Catalog, panel_save};
use serde_json::json;
use std::cell::Cell;
use std::path::Path;
thread_local! { static CHECK: Cell<bool> = const { Cell::new(false) }; }
pub(crate) fn before_replace(path: &Path) {
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
fn maintenance_is_excluded_until_plan_replacement_and_catalog_close() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("logs")
        .join(format!("save-lease-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
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
    std::fs::remove_dir_all(&root).unwrap();
    result.unwrap();
}
