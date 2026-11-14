//! A refused durable replacement preserves prior state and cleans only its own staging file.
use _native::atomic;
use serde_json::json;
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

fn original_source() {
    let inventory: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .unwrap();
    assert_eq!(
        inventory["baseline_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    let sources = inventory["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["source"] == "tests/test_atomic_state_files.py")
        .collect::<Vec<_>>();
    assert_eq!(sources.len(), 1);
    assert_eq!(
        sources[0]["sha256"],
        "47eb6571bb14466fc25cd932449099c59da70107a2848c92f3e9fbadf16bc462"
    );
}

fn assert_members(root: &Path) {
    let names = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        names,
        BTreeSet::from(["state.json".into(), "state.json.tmp".into()])
    );
    assert_eq!(
        std::fs::read(root.join("state.json.tmp")).unwrap(),
        b"pre-existing unrelated bytes"
    );
}

#[test]
fn original_foreign_temporary_utf8_and_existing_permissions_survive_replacement() {
    original_source();
    let root = Root::new("atomic original ownership é");
    let path = root.0.join("state.json");
    std::fs::write(&path, b"{\"old\":true}").unwrap();
    std::fs::write(
        root.0.join("state.json.tmp"),
        b"pre-existing unrelated bytes",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    atomic::write_json(&path, &json!({"new":"سلام"})).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(text).unwrap(),
        json!({"new":"سلام"})
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
            0o644
        );
    }
    assert_members(&root.0);
    root.clean().unwrap();
}

#[test]
fn original_basic_json_writes_leave_only_the_target_and_replace_its_contents() {
    let root = Root::new("atomic original basic JSON");
    let path = root.0.join("state.json");
    for value in [json!({"done":["a","b"]}), json!({"n":1}), json!({"n":2})] {
        atomic::write_json(&path, &value).unwrap();
        let actual: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(actual, value);
        let members = std::fs::read_dir(&root.0)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(members.as_slice(), std::slice::from_ref(&path));
    }
    root.clean().unwrap();
}

#[test]
fn original_concurrent_bodies_publish_complete_json_and_preserve_foreign_temporary() {
    original_source();
    let root = Root::new("atomic concurrent ownership");
    let path = root.0.join("state.json");
    std::fs::write(&path, b"{\"old\":true}").unwrap();
    std::fs::write(
        root.0.join("state.json.tmp"),
        b"pre-existing unrelated bytes",
    )
    .unwrap();
    let bodies = [
        json!({"writer":1,"value":"A".repeat(10_000)}),
        json!({"writer":2,"value":"B".repeat(1000)}),
    ];
    std::thread::scope(|scope| {
        let (ready, started) = std::sync::mpsc::channel();
        let (completed, results) = std::sync::mpsc::channel();
        let mut starts = Vec::new();
        for body in &bodies {
            let (start, begin) = std::sync::mpsc::channel();
            starts.push(start);
            let ready = ready.clone();
            let completed = completed.clone();
            let path = &path;
            scope.spawn(move || {
                ready.send(()).unwrap();
                begin.recv_timeout(Duration::from_secs(5)).unwrap();
                completed.send(atomic::write_json(path, body)).unwrap();
            });
        }
        drop(ready);
        drop(completed);
        for _ in 0..2 {
            started.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        for start in starts {
            start.send(()).unwrap();
        }
        for _ in 0..2 {
            results
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
        }
    });
    let actual: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(bodies.contains(&actual));
    assert_members(&root.0);
    // A simultaneous start does not prove overlap at the original pre-rename interception point.
    root.clean().unwrap();
}

#[test]
fn invalid_destination_preserves_contents_and_reaps_staging() {
    let scratch = Root::new("atomic replacement refusal");
    let destination = scratch.0.join("state.json");
    std::fs::create_dir(&destination).unwrap();
    let prior = destination.join("keep.json");
    std::fs::write(&prior, b"previous durable bytes").unwrap();
    assert!(atomic::write_json(&destination, &json!({"replacement":true})).is_err());
    assert_eq!(std::fs::read(&prior).unwrap(), b"previous durable bytes");
    assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 1);
    scratch.clean().unwrap();
}

#[cfg(windows)]
#[test]
fn occupied_state_preserves_previous_file_and_reaps_staging() {
    use std::os::windows::fs::OpenOptionsExt;
    original_source();
    let scratch = Root::new("atomic occupied destination");
    let destination = scratch.0.join("state.json");
    std::fs::write(&destination, b"{\"old\":true}").unwrap();
    std::fs::write(
        scratch.0.join("state.json.tmp"),
        b"pre-existing unrelated bytes",
    )
    .unwrap();
    // Allow readers/writers, but not deleting/replacing this exact opened file.
    let occupied = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1 | 2)
        .open(&destination)
        .unwrap();
    assert!(atomic::write_json(&destination, &json!({"replacement":true})).is_err());
    assert_eq!(std::fs::read(&destination).unwrap(), b"{\"old\":true}");
    assert_members(&scratch.0);
    drop(occupied);
    atomic::write_json(&destination, &json!({"replacement":true})).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&destination).unwrap()).unwrap(),
        json!({"replacement":true})
    );
    assert_members(&scratch.0);
    scratch.clean().unwrap();
}
