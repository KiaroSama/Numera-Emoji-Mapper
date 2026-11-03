//! Pinned original failure/overlap assertions against the same production writer.
use crate::atomic::{write_bytes_with, write_json};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs::File;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

fn fixture(label: &str) -> (PathBuf, PathBuf) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent = crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let root = parent.join(format!(
        "{label} {} {}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("state.json");
    std::fs::write(&path, b"{\"old\":true}").unwrap();
    std::fs::write(root.join("state.json.tmp"), b"pre-existing unrelated bytes").unwrap();
    (root, path)
}

fn preserved(root: &Path, path: &Path) {
    assert_eq!(std::fs::read(path).unwrap(), b"{\"old\":true}");
    members(root);
}

fn members(root: &Path) {
    assert_eq!(
        std::fs::read(root.join("state.json.tmp")).unwrap(),
        b"pre-existing unrelated bytes"
    );
    let names = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        names,
        BTreeSet::from(["state.json".into(), "state.json.tmp".into()])
    );
}

#[test]
fn serialization_failure_preserves_previous_state_and_foreign_temporary() {
    struct Unsupported;
    impl serde::Serialize for Unsupported {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeMap;
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry("valid", &true)?;
            Err(serde::ser::Error::custom("unsupported fixture object"))
        }
    }
    let (root, path) = fixture("atomic serialization failure é");
    let error = write_json(&path, &Unsupported).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Other);
    assert_eq!(error.to_string(), "unsupported fixture object");
    preserved(&root, &path);
    let previous = json!({"done":["a"]});
    write_json(&path, &previous).unwrap();
    assert!(write_json(&path, &Unsupported).is_err());
    let retained: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(retained, previous);
    members(&root);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn sync_and_replace_failures_preserve_previous_state_and_remove_only_owned_staging() {
    for sync_failure in [true, false] {
        let (root, path) = fixture("atomic original injected failure");
        let mut synced = 0;
        let mut replaced = 0;
        let error = write_bytes_with(
            &path,
            b"{\"new\":true}",
            |file| {
                synced += 1;
                let owned = std::fs::read_dir(&root)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .filter(|entry| {
                        entry
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with(".state.json-")
                    })
                    .collect::<Vec<_>>();
                assert_eq!(owned.len(), 1);
                assert_eq!(std::fs::read(&owned[0]).unwrap(), b"{\"new\":true}");
                assert_eq!(std::fs::read_dir(&root).unwrap().count(), 3);
                if sync_failure {
                    Err(io::Error::other("disk failure"))
                } else {
                    file.sync_all()
                }
            },
            |_, _| {
                replaced += 1;
                Err(io::Error::other("destination busy"))
            },
        )
        .unwrap_err();
        assert_eq!(synced, 1);
        assert_eq!(replaced, usize::from(!sync_failure));
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(
            error.to_string(),
            if sync_failure {
                "disk failure"
            } else {
                "destination busy"
            }
        );
        preserved(&root, &path);
        if !sync_failure {
            let previous = json!({"base":"pk","sets":[],"sent":["first"]});
            write_json(&path, &previous).unwrap();
            let before = std::fs::read(&path).unwrap();
            let replacement =
                serde_json::to_vec(&json!({"base":"pk","sets":[],"sent":["second"]})).unwrap();
            let mut attempts = 0;
            let error = write_bytes_with(
                &path,
                &replacement,
                File::sync_all,
                |staged, destination| {
                    attempts += 1;
                    assert_eq!(destination, path);
                    assert_eq!(std::fs::read(staged).unwrap(), replacement);
                    assert_eq!(std::fs::read(destination).unwrap(), before);
                    Err(io::Error::other("boom"))
                },
            )
            .unwrap_err();
            assert_eq!(attempts, 1);
            assert_eq!(error.to_string(), "boom");
            assert_eq!(std::fs::read(&path).unwrap(), before);
            let saved: Value = serde_json::from_slice(&before).unwrap();
            assert_eq!(saved["sent"], json!(["first"]));
            members(&root);

            let old_csv = b"ticker,name,format,file,keywords\nbtc,Bitcoin,svg,logos/svg/btc.svg,\"btc, Bitcoin\"\n";
            std::fs::write(&path, old_csv).unwrap();
            let error = write_bytes_with(
                &path,
                b"ticker,name,format,file,keywords\r\n",
                File::sync_all,
                |staged, destination| {
                    assert_eq!(std::fs::read(destination).unwrap(), old_csv);
                    assert_eq!(
                        std::fs::read(staged).unwrap(),
                        b"ticker,name,format,file,keywords\r\n"
                    );
                    Err(io::Error::other("interrupted"))
                },
            )
            .unwrap_err();
            assert_eq!(error.to_string(), "interrupted");
            assert_eq!(std::fs::read(&path).unwrap(), old_csv);
            members(&root);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(windows)]
#[test]
fn two_transient_windows_refusals_retry_the_same_owned_staging_to_completion() {
    let (root, path) = fixture("atomic transient Windows refusal");
    let mut sources = Vec::new();
    write_bytes_with(
        &path,
        b"{\"new\":true}",
        File::sync_all,
        |source, destination| {
            assert_eq!(destination, path);
            assert_eq!(std::fs::read(source).unwrap(), b"{\"new\":true}");
            sources.push(source.to_path_buf());
            if sources.len() <= 2 {
                Err(io::Error::from_raw_os_error(
                    windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED as i32,
                ))
            } else {
                std::fs::rename(source, destination)
            }
        },
    )
    .unwrap();
    assert_eq!(sources.len(), 3);
    assert!(sources.iter().all(|source| *source == sources[0]));
    assert_eq!(std::fs::read(&path).unwrap(), b"{\"new\":true}");
    members(&root);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn writers_hold_distinct_complete_staging_before_either_replaces_the_destination() {
    let (root, path) = fixture("atomic deterministic overlap");
    let bodies = [
        json!({"writer":1,"value":"A".repeat(10_000)}),
        json!({"writer":2,"value":"B".repeat(1000)}),
    ];
    let bytes = bodies
        .iter()
        .map(|body| serde_json::to_vec(body).unwrap())
        .collect::<Vec<_>>();
    std::thread::scope(|scope| {
        let (staged, observations) = std::sync::mpsc::channel();
        let (completed, results) = std::sync::mpsc::channel();
        let mut releases = Vec::new();
        for body in &bytes {
            let (release, proceed) = std::sync::mpsc::channel();
            releases.push(release);
            let staged = staged.clone();
            let completed = completed.clone();
            let path = &path;
            scope.spawn(move || {
                let mut first = true;
                let result = write_bytes_with(path, body, File::sync_all, |source, destination| {
                    if first {
                        staged
                            .send(source.to_path_buf())
                            .map_err(io::Error::other)?;
                        proceed
                            .recv_timeout(Duration::from_secs(5))
                            .map_err(io::Error::other)?;
                        first = false;
                    }
                    std::fs::rename(source, destination)
                });
                completed.send(result).unwrap();
            });
        }
        drop(staged);
        drop(completed);
        let sources = (0..2)
            .map(|_| observations.recv_timeout(Duration::from_secs(5)).unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(sources.len(), 2);
        let actual = sources
            .iter()
            .map(|source| {
                assert_eq!(source.parent(), path.parent());
                serde_json::from_slice::<Value>(&std::fs::read(source).unwrap()).unwrap()
            })
            .collect::<Vec<_>>();
        for body in &bodies {
            assert!(actual.contains(body));
        }
        preserved_before_release(&root, &path, &sources);
        for release in releases {
            release.send(()).unwrap();
        }
        for _ in 0..2 {
            results
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
        }
    });
    let final_value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(bodies.contains(&final_value));
    members(&root);
    std::fs::remove_dir_all(root).unwrap();
}

fn preserved_before_release(root: &Path, path: &Path, staging: &BTreeSet<PathBuf>) {
    assert_eq!(std::fs::read(path).unwrap(), b"{\"old\":true}");
    assert_eq!(
        std::fs::read(root.join("state.json.tmp")).unwrap(),
        b"pre-existing unrelated bytes"
    );
    let actual = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<BTreeSet<_>>();
    let mut expected = staging.clone();
    expected.extend([path.to_path_buf(), root.join("state.json.tmp")]);
    assert_eq!(actual, expected);
}
