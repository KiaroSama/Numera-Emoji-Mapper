//! Exact original keyword-writer failure identity, exercised inside its private Rust module.
use serde_json::Map;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        loop {
            let path = std::env::temp_dir().join(format!(
                "numera-baseline-keywords-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("create keyword failure scratch: {e}"),
            }
        }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove owned keyword scratch");
    }
}
// tests.test_coin_keywords.KeywordsFileHasOneWriter.test_a_crash_before_the_swap_leaves_the_old_file_intact
#[test]
fn test_a_crash_before_the_swap_leaves_the_old_file_intact() {
    const OLD: &str =
        "ticker,name,format,file,keywords\nbtc,Bitcoin,svg,logos/svg/btc.svg,\"btc, Bitcoin\"\n";
    let scratch = Scratch::new();
    let csv = scratch.0.join("keywords.csv");
    std::fs::write(&csv, OLD).unwrap();
    let result = super::write_with(&Map::new(), &csv, |path, bytes| {
        crate::atomic::write_bytes_with(path, bytes, std::fs::File::sync_all, |_, _| {
            Err(std::io::Error::other("interrupted"))
        })
    });
    assert!(
        result.is_err(),
        "the injected swap failure must escape the keyword writer"
    );
    assert_eq!(std::fs::read_to_string(&csv).unwrap(), OLD);
    let temps = std::fs::read_dir(&scratch.0)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "tmp"))
        .collect::<Vec<_>>();
    assert!(
        temps.is_empty(),
        "the half-written temp file must not be left behind: {temps:?}"
    );
}
