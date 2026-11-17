//! Original writer-spy contract delegates each observed call to the actual atomic writer.
use crate::baseline_coin_state_support as support;
use serde_json::json;
use support::{Root, noise};
// tests.test_rebuild_dedup_plan.TruncatedPlanFailsClosed.test_the_plan_and_the_report_are_written_atomically
#[test]
fn test_the_plan_and_the_report_are_written_atomically() {
    let root = Root::new();
    let images = root.0.join("emoji");
    std::fs::create_dir(&images).unwrap();
    std::fs::write(images.join("aaa.png"), noise("aaa")).unwrap();
    std::fs::write(root.0.join("currency-emoji-inventory.md"), "ticker: aaa\n").unwrap();
    let mut writes = vec![];
    super::build_with_persist(&root.0, &images, |path, value| {
        writes.push(path.to_owned());
        crate::atomic::write_json(path, value)
    })
    .unwrap();
    writes.sort();
    let mut expected = vec![
        root.0.join("rebuild_dedup_plan.json"),
        root.0.join("shared_logo_groups.json"),
    ];
    expected.sort();
    assert_eq!(writes, expected);
    let plan = super::load(&root.0).unwrap();
    assert_eq!(
        plan.as_array()
            .unwrap()
            .iter()
            .map(|group| group["rep"].clone())
            .collect::<Vec<_>>(),
        vec![json!("aaa")]
    );
    assert!(std::fs::read_dir(&root.0).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
}
