//! Watchdog's public deadline policy; fake elapsed values avoid waiting two minutes.
use _native::tooling_convert;
use std::path::Path;
use std::time::Duration;

#[test]
fn relative_watchdog_paths_bind_to_package_not_callers_cwd() {
    let root = Path::new(if cfg!(windows) {
        "C:/package root"
    } else {
        "/package root"
    });
    assert_eq!(
        tooling_convert::resolve_path(root, Path::new("logs/stdout")),
        root.join("logs/stdout")
    );
    assert_eq!(tooling_convert::resolve_path(root, root), root);
}

#[test]
fn damaged_progress_marker_refuses_instead_of_reading_unbounded_input() {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../logs/tmp");
    std::fs::create_dir_all(&parent).unwrap();
    let directory = parent.join(format!("tooling-marker-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let directory = Scratch(directory);
    let marker = directory.0.join(".svg_cur");
    assert!(tooling_convert::marker_state(&marker).unwrap().is_none());
    std::fs::write(&marker, b"source.svg").unwrap();
    let first = tooling_convert::marker_state(&marker).unwrap();
    std::fs::write(&marker, b"next-source.svg").unwrap();
    assert!(first != tooling_convert::marker_state(&marker).unwrap());
    std::fs::write(&marker, vec![b'x'; 65537]).unwrap();
    assert!(tooling_convert::marker_state(&marker).is_err());
    std::fs::remove_file(&marker).unwrap();
    std::fs::create_dir(&marker).unwrap();
    assert!(tooling_convert::marker_state(&marker).is_err());
}

#[test]
fn native_conversion_marker_idle_and_wall_exit_codes_keep_source_precedence() {
    for (elapsed, idle, code) in [
        (0, 0, None),
        (119, 119, None),
        (121, 120, Some(124)),
        (3600, 0, Some(125)),
        (3600, 120, Some(125)),
        (3599, 119, None),
    ] {
        assert_eq!(
            tooling_convert::deadline_code(Duration::from_secs(elapsed), Duration::from_secs(idle)),
            code
        );
    }
}
