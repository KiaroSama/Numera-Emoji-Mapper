//! A refused durable replacement preserves prior state and cleans only its own staging file.
use _native::atomic;
use serde_json::json;
use std::path::PathBuf;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn invalid_destination_preserves_contents_and_reaps_staging() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("logs")
        .join(format!("atomic-refusal-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let scratch = Scratch(root);
    let destination = scratch.0.join("state.json");
    std::fs::create_dir(&destination).unwrap();
    let prior = destination.join("keep.json");
    std::fs::write(&prior, b"previous durable bytes").unwrap();
    assert!(atomic::write_json(&destination, &json!({"replacement":true})).is_err());
    assert_eq!(std::fs::read(&prior).unwrap(), b"previous durable bytes");
    assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 1);
}

#[cfg(windows)]
#[test]
fn occupied_state_preserves_previous_file_and_reaps_staging() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("logs")
        .join(format!("atomic-occupied-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let scratch = Scratch(root);
    let destination = scratch.0.join("state.json");
    std::fs::write(&destination, b"previous durable bytes").unwrap();
    // Allow readers/writers, but not deleting/replacing this exact opened file.
    let occupied = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1 | 2)
        .open(&destination)
        .unwrap();
    assert!(atomic::write_json(&destination, &json!({"replacement":true})).is_err());
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"previous durable bytes"
    );
    assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 1);
    drop(occupied);
    atomic::write_json(&destination, &json!({"replacement":true})).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&destination).unwrap()).unwrap(),
        json!({"replacement":true})
    );
}
