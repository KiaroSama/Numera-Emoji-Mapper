//! Synthetic installed converter protocol; the production watchdog owns every process here.
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
    assert!(matches!(variant, "idle" | "wall"));
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
    super::persist(
        &root.join("ready"),
        format!("{}\n{}", std::process::id(), child.id()).as_bytes(),
    );
    if variant == "idle" {
        super::hold();
    }
    // Marker changes model useful progress only; no timing delay stands in for readiness.
    let mut sequence = 0_u64;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        std::fs::write(output.join(".svg_cur"), format!("source-{sequence}.svg")).unwrap();
        sequence += 1;
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("watchdog failed to stop progressing converter");
}
