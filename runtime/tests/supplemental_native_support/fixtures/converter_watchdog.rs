// Retained Rust fixture from native/tests/fixtures/converter_watchdog.rs; source SHA256 9598a87d8931f46d071424957756dca901a5fdd411c87f71b7bd03caf80914bb.
//! Synthetic installed converter protocol; the production watchdog owns every process here.
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn publish_marker(output: &Path, sequence: u64, pause_after_open: bool) {
    let staged = output.join(".svg_cur.next");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)
        .unwrap();
    let contents = format!("source-{sequence}.svg");
    if pause_after_open {
        // A kill at this cutpoint must preserve the last complete published marker.
        super::persist(&output.join("marker-update-opened"), contents.as_bytes());
        super::hold();
    }
    file.write_all(contents.as_bytes()).unwrap();
    file.sync_all().unwrap();
    drop(file);
    std::fs::rename(staged, output.join(".svg_cur")).unwrap();
}

pub fn run(args: &[String], variant: &str) {
    assert_eq!(args.len(), 5);
    assert_eq!(args[0], "make-emoji-pngs");
    assert_eq!(args[1], "--in");
    assert_eq!(args[3], "--out");
    let root = std::env::current_dir().unwrap();
    assert_eq!(Path::new(&args[2]), root.join("input with spaces"));
    let output = root.join("output café");
    assert_eq!(Path::new(&args[4]), output);
    assert_eq!(
        std::fs::read(root.join("input with spaces/sentinel")).unwrap(),
        b"caller source bytes"
    );
    std::fs::create_dir(&output).unwrap();
    super::persist(&root.join("argv.txt"), args.join("\n").as_bytes());
    println!("converter café 😀");
    eprintln!("fixture stderr");
    std::io::stdout().flush().unwrap();
    if variant == "exit" {
        super::persist(
            &root.join("ready"),
            std::process::id().to_string().as_bytes(),
        );
        std::process::exit(3);
    }
    assert!(matches!(variant, "idle" | "wall" | "wall-staged"));
    let ready = root.join("descendant-ready");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--descendant")
        .arg(&ready)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let child = command.spawn().unwrap();
    let expected = child.id().to_string();
    let deadline = Instant::now() + Duration::from_secs(5);
    while std::fs::read_to_string(&ready).ok().as_deref() != Some(expected.as_str()) {
        assert!(
            Instant::now() < deadline,
            "converter descendant readiness expired"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    if variant != "idle" {
        // Readiness acknowledges a complete marker before any interruptible update.
        publish_marker(&output, 0, false);
    }
    super::persist(
        &root.join("ready"),
        format!("{}\n{}", std::process::id(), child.id()).as_bytes(),
    );
    if variant == "idle" {
        super::hold();
    }
    // Marker changes model useful progress only; no timing delay stands in for readiness.
    let mut sequence = 1_u64;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        publish_marker(&output, sequence, variant == "wall-staged" && sequence == 2);
        sequence += 1;
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("watchdog failed to stop progressing converter");
}
