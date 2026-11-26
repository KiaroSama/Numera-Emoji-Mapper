//! Preserve prior compiler evidence across PID reuse without adopting old storage.
use crate::retained_directory;
use crate::tooling_fixture::Root;
use std::{collections::BTreeSet, fs, path::Path};

#[cfg(windows)]
use junction::create as link_directory;
#[cfg(unix)]
use std::os::unix::fs::symlink as link_directory;

fn remove_link(path: &Path) {
    #[cfg(windows)]
    junction::delete(path).unwrap();
    #[cfg(unix)]
    fs::remove_file(path).unwrap();
}

#[test]
fn compiler_storage_preserves_occupied_pid_names_and_allocates_exclusively() {
    for namespace in ["supplemental-codec-worker", "runner-observer"] {
        for kind in ["directory", "file", "link"] {
            let root = Root::new(&format!("compiler allocation {namespace} {kind}"));
            let parent = root.0.join("logs/tmp");
            fs::create_dir_all(&parent).unwrap();
            let legacy = parent.join(format!("{namespace}-{}", std::process::id()));
            let target = root.0.join("retained target");
            match kind {
                "directory" => {
                    fs::create_dir(&legacy).unwrap();
                    fs::write(legacy.join("sentinel"), b"earlier compiler evidence").unwrap();
                }
                "file" => fs::write(&legacy, b"earlier compiler evidence").unwrap(),
                "link" => {
                    fs::create_dir(&target).unwrap();
                    fs::write(target.join("sentinel"), b"earlier compiler evidence").unwrap();
                    link_directory(&target, &legacy).unwrap();
                }
                _ => unreachable!(),
            }
            let old_type = fs::symlink_metadata(&legacy).unwrap().file_type();
            let old_link = (kind == "link").then(|| fs::read_link(&legacy).unwrap());
            assert_eq!(
                fs::create_dir(&legacy).unwrap_err().kind(),
                std::io::ErrorKind::AlreadyExists,
                "the legacy PID-only allocation must reproduce the collision"
            );
            let fresh = std::thread::scope(|scope| {
                (0..4)
                    .map(|_| {
                        scope.spawn(|| retained_directory::allocate(&root.0, namespace).unwrap())
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|thread| thread.join().unwrap())
                    .collect::<BTreeSet<_>>()
            });
            assert_eq!(
                fresh.len(),
                4,
                "independent allocations never share storage"
            );
            for directory in &fresh {
                assert_ne!(directory, &legacy);
                assert_eq!(directory.parent().unwrap(), parent);
                assert!(fs::symlink_metadata(directory).unwrap().is_dir());
                assert!(fs::read_dir(directory).unwrap().next().is_none());
                fs::write(directory.join("compiler.log"), b"retained new diagnostics").unwrap();
            }
            let later = retained_directory::allocate(&root.0, namespace).unwrap();
            assert!(!fresh.contains(&later));
            for directory in fresh {
                assert_eq!(
                    fs::read(directory.join("compiler.log")).unwrap(),
                    b"retained new diagnostics",
                    "returned compiler storage must outlive its allocation helper"
                );
            }
            let evidence = if kind == "file" {
                legacy.clone()
            } else {
                legacy.join("sentinel")
            };
            assert_eq!(fs::read(evidence).unwrap(), b"earlier compiler evidence");
            assert_eq!(fs::symlink_metadata(&legacy).unwrap().file_type(), old_type);
            if let Some(link) = old_link {
                assert_eq!(fs::read_link(&legacy).unwrap(), link);
                assert_eq!(
                    fs::read(target.join("sentinel")).unwrap(),
                    b"earlier compiler evidence"
                );
                remove_link(&legacy);
            }
            root.clean().unwrap();
        }
    }
}

#[test]
fn compiler_storage_refuses_invalid_or_linked_parents_without_repair() {
    let root = Root::new("compiler parent refusal");
    let logs = root.0.join("logs");
    fs::create_dir(&logs).unwrap();
    let parent = logs.join("tmp");
    fs::write(&parent, b"existing parent is a file").unwrap();
    for namespace in ["supplemental-codec-worker", "runner-observer"] {
        assert!(retained_directory::allocate(&root.0, namespace).is_err());
        assert_eq!(fs::read(&parent).unwrap(), b"existing parent is a file");
    }
    fs::remove_file(&parent).unwrap();
    let target = root.0.join("physical target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("sentinel"), b"unmodified link target").unwrap();
    link_directory(&target, &parent).unwrap();
    let link = fs::read_link(&parent).unwrap();
    for namespace in ["supplemental-codec-worker", "runner-observer"] {
        assert!(retained_directory::allocate(&root.0, namespace).is_err());
        assert_eq!(fs::read_link(&parent).unwrap(), link);
        assert_eq!(
            fs::read(target.join("sentinel")).unwrap(),
            b"unmodified link target"
        );
        assert_eq!(fs::read_dir(&target).unwrap().count(), 1);
    }
    remove_link(&parent);
    root.clean().unwrap();
}
