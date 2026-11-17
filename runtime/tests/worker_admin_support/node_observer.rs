//! Explicit external-CLI stand-in, compiled by real rustc and entered through normal Rust main.
//! Records only metadata/checksums of never-issued synthetic stdin, never raw values.
use std::{
    fs::OpenOptions,
    hash::Hasher,
    io::{Read, Seek, Write},
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};
fn quoted(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|s| s == "--owned-descendant") {
        loop {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let root = PathBuf::from(
        std::env::var_os("NUMERA_TEST_WORKER_OBSERVER_ROOT").expect("owned observer root"),
    );
    assert_eq!(std::env::current_dir().unwrap(), root);
    assert_eq!(args.len(), 5);
    assert!(args[1].ends_with("wrangler.js"));
    assert_eq!(args[2], "secret");
    assert_eq!(args[3], "put");
    let key = &args[4];
    #[cfg(unix)]
    let mut input = {
        use std::os::fd::AsFd;
        std::fs::File::from(std::io::stdin().as_fd().try_clone_to_owned().unwrap())
    };
    #[cfg(windows)]
    let mut input = {
        use std::os::windows::io::AsHandle;
        std::fs::File::from(std::io::stdin().as_handle().try_clone_to_owned().unwrap())
    };
    let regular = input.metadata().unwrap().is_file();
    #[cfg(unix)]
    let unlinked = {
        use std::os::unix::fs::MetadataExt;
        input.metadata().unwrap().nlink() == 0
    }
    .to_string();
    #[cfg(windows)]
    let unlinked = "null".to_owned();
    let seekable = input.stream_position().is_ok();
    let mut bytes = vec![];
    input.read_to_end(&mut bytes).unwrap();
    let mut checksum = std::collections::hash_map::DefaultHasher::new();
    checksum.write(&bytes);
    let fingerprint = checksum.finish();
    let record = format!(
        "{{\"key\":{},\"argv\":[{}],\"bytes\":{},\"checksum\":\"{:016x}\",\"regular\":{},\"seekable\":{},\"unlinked\":{},\"pid\":{}}}\n",
        quoted(key),
        args.iter().map(|a| quoted(a)).collect::<Vec<_>>().join(","),
        bytes.len(),
        fingerprint,
        regular,
        seekable,
        unlinked,
        std::process::id()
    );
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("observer-metadata.jsonl"))
        .unwrap()
        .write_all(record.as_bytes())
        .unwrap();
    if std::env::var("WRANGLER_WRITE_LOGS").as_deref() != Ok("false") {
        std::fs::write(
            root.join("unexpected-log-enabled.txt"),
            "external logger was not disabled",
        )
        .unwrap();
    }
    assert_eq!(
        std::env::var("WRANGLER_SEND_METRICS").as_deref(),
        Ok("false")
    );
    if key == "COIN_WEBHOOK_SECRET" {
        if let Some(path) = std::env::var_os("NUMERA_TEST_WORKER_EDIT_ENV") {
            OpenOptions::new()
                .append(true)
                .open(path)
                .unwrap()
                .write_all(b"UNRELATED_CONCURRENT_EDIT=preserve-me\n")
                .unwrap();
        }
    }
    // A careless real CLI can echo stdin; production must discard both streams.
    let _ = std::io::stdout().write_all(&bytes);
    let _ = std::io::stderr().write_all(&bytes);
    if std::env::var("NUMERA_TEST_WORKER_FAIL_KEY").as_deref() == Ok(key.as_str()) {
        std::process::exit(19);
    }
    if std::env::var("NUMERA_TEST_WORKER_BLOCK_KEY").as_deref() == Ok(key.as_str()) {
        let child = Command::new(std::env::current_exe().unwrap())
            .arg("--owned-descendant")
            .stdin(Stdio::inherit())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        std::fs::write(root.join("descendant.pid"), child.id().to_string()).unwrap();
        #[cfg(target_os = "linux")]
        {
            let stat = std::fs::read_to_string(format!("/proc/{}/stat", child.id())).unwrap();
            let birth = stat
                .rsplit_once(") ")
                .unwrap()
                .1
                .split_whitespace()
                .nth(19)
                .unwrap();
            std::fs::write(root.join("descendant.birth"), birth).unwrap();
        }
        loop {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
